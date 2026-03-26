use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crossbeam_channel::{Receiver, Sender, bounded};
use tracing::{debug, info};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROCESS_INFORMATION, STARTUPINFOEXW, UpdateProcThreadAttribute,
};
use windows::core::PWSTR;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConPtyError {
    #[error("Failed to create pipes: {0}")]
    PipeCreation(#[source] windows::core::Error),
    #[error("Failed to create pseudo console: {0}")]
    ConsoleCreation(#[source] windows::core::Error),
    #[error("Failed to create process: {0}")]
    ProcessCreation(#[source] windows::core::Error),
    #[error("Failed to resize pseudo console: {0}")]
    Resize(#[source] windows::core::Error),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, ConPtyError>;

const PSEUDOCONSOLE_RESIZE_QUIRK: u32 = 0x2;
const PSEUDOCONSOLE_WIN32_INPUT_MODE: u32 = 0x4;

/// Wrapper to make HANDLE Send (HANDLE is a raw pointer, but we manage
/// thread safety through our channel-based architecture).
struct SendableHandle(HANDLE);
unsafe impl Send for SendableHandle {}

/// A `ConPTY` session managing a pseudo console and child process.
pub struct ConPty {
    console: HPCON,
    process_info: PROCESS_INFORMATION,
    input_tx: Sender<Vec<u8>>,
    output_rx: Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
    read_thread: Option<thread::JoinHandle<()>>,
    write_thread: Option<thread::JoinHandle<()>>,
}

impl ConPty {
    /// Create a new `ConPTY` session with the given size and shell command.
    pub fn spawn(cols: u16, rows: u16, shell: &str) -> Result<Self> {
        unsafe { Self::spawn_inner(cols, rows, shell) }
    }

    unsafe fn spawn_inner(cols: u16, rows: u16, shell: &str) -> Result<Self> {
        let (pty_input_read, pty_input_write) = create_pipe()?;
        let (pty_output_read, pty_output_write) = create_pipe()?;

        let size = COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        let flags = PSEUDOCONSOLE_RESIZE_QUIRK | PSEUDOCONSOLE_WIN32_INPUT_MODE;
        let console = unsafe {
            CreatePseudoConsole(size, pty_input_read, pty_output_write, flags)
                .map_err(ConPtyError::ConsoleCreation)?
        };

        unsafe {
            let _ = CloseHandle(pty_input_read);
            let _ = CloseHandle(pty_output_write);
        }

        let process_info = unsafe { spawn_process(console, shell)? };

        let shutdown = Arc::new(AtomicBool::new(false));
        let (output_tx, output_rx) = bounded(256);
        let (input_tx, input_rx) = bounded::<Vec<u8>>(256);

        let read_shutdown = Arc::clone(&shutdown);
        let read_handle = SendableHandle(pty_output_read);
        let read_thread = thread::Builder::new()
            .name("conpty-read".into())
            .spawn(move || {
                let h = read_handle; // move entire SendableHandle into closure
                read_loop(h.0, output_tx, read_shutdown);
            })
            .expect("failed to spawn read thread");

        let write_shutdown = Arc::clone(&shutdown);
        let write_handle = SendableHandle(pty_input_write);
        let write_thread = thread::Builder::new()
            .name("conpty-write".into())
            .spawn(move || {
                let h = write_handle; // move entire SendableHandle into closure
                write_loop(h.0, input_rx, write_shutdown);
            })
            .expect("failed to spawn write thread");

        info!(cols, rows, shell, "ConPTY session created");

        Ok(ConPty {
            console,
            process_info,
            input_tx,
            output_rx,
            shutdown,
            read_thread: Some(read_thread),
            write_thread: Some(write_thread),
        })
    }

    /// Send input bytes to the terminal.
    pub fn write(&self, data: &[u8]) {
        let _ = self.input_tx.send(data.to_vec());
    }

    /// Try to receive output bytes from the terminal (non-blocking).
    pub fn try_read(&self) -> Option<Vec<u8>> {
        self.output_rx.try_recv().ok()
    }

    /// Get the output receiver for use in select loops.
    pub fn output_receiver(&self) -> &Receiver<Vec<u8>> {
        &self.output_rx
    }

    /// Resize the pseudo console.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let size = COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        unsafe {
            ResizePseudoConsole(self.console, size).map_err(ConPtyError::Resize)?;
        }
        debug!(cols, rows, "ConPTY resized");
        Ok(())
    }
}

impl Drop for ConPty {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        drop(self.input_tx.clone());

        unsafe {
            ClosePseudoConsole(self.console);
        }

        if let Some(handle) = self.read_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.write_thread.take() {
            let _ = handle.join();
        }

        unsafe {
            let _ = CloseHandle(self.process_info.hProcess);
            let _ = CloseHandle(self.process_info.hThread);
        }

        info!("ConPTY session closed");
    }
}

fn create_pipe() -> Result<(HANDLE, HANDLE)> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    unsafe {
        CreatePipe(&raw mut read, &raw mut write, None, 0).map_err(ConPtyError::PipeCreation)?;
    }
    Ok((read, write))
}

const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x0002_0016;

unsafe fn spawn_process(console: HPCON, shell: &str) -> Result<PROCESS_INFORMATION> {
    let mut attr_list_size: usize = 0;
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &raw mut attr_list_size) };

    let mut attr_list_buf = vec![0u8; attr_list_size];
    let attr_list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_list_buf.as_mut_ptr().cast());
    unsafe {
        InitializeProcThreadAttributeList(Some(attr_list), 1, None, &raw mut attr_list_size)
            .map_err(ConPtyError::ProcessCreation)?;
    }

    unsafe {
        UpdateProcThreadAttribute(
            attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            Some(console.0 as *const _),
            size_of::<HPCON>(),
            None,
            None,
        )
        .map_err(ConPtyError::ProcessCreation)?;
    }

    let mut startup_info = STARTUPINFOEXW::default();
    startup_info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup_info.lpAttributeList = attr_list;

    let mut process_info = PROCESS_INFORMATION::default();
    let mut cmd: Vec<u16> = shell.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        CreateProcessW(
            None,
            Some(PWSTR(cmd.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            None,
            None,
            &raw const startup_info.StartupInfo,
            &raw mut process_info,
        )
        .map_err(ConPtyError::ProcessCreation)?;

        DeleteProcThreadAttributeList(attr_list);
    }

    Ok(process_info)
}

fn read_loop(handle: HANDLE, tx: Sender<Vec<u8>>, shutdown: Arc<AtomicBool>) {
    let mut buf = vec![0u8; 4096];
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let mut bytes_read: u32 = 0;
        let ok = unsafe {
            windows::Win32::Storage::FileSystem::ReadFile(
                handle,
                Some(&mut buf),
                Some(&raw mut bytes_read),
                None,
            )
        };
        match ok {
            Ok(()) if bytes_read > 0 => {
                let data = buf[..bytes_read as usize].to_vec();
                if tx.send(data).is_err() {
                    break;
                }
            }
            _ => break,
        }
    }
    unsafe {
        let _ = CloseHandle(handle);
    }
}

fn write_loop(handle: HANDLE, rx: Receiver<Vec<u8>>, shutdown: Arc<AtomicBool>) {
    while let Ok(data) = rx.recv() {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let mut written: u32 = 0;
        let _ = unsafe {
            windows::Win32::Storage::FileSystem::WriteFile(
                handle,
                Some(&data),
                Some(&raw mut written),
                None,
            )
        };
    }
    unsafe {
        let _ = CloseHandle(handle);
    }
}
