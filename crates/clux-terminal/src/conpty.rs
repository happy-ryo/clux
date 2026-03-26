use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use std::{io, thread};

use crossbeam_channel::{Receiver, Sender, bounded};
use tracing::{debug, info, warn};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
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
const PSEUDOCONSOLE_PASSTHROUGH_MODE: u32 = 0x8;

/// Minimum Windows build number required for `PASSTHROUGH_MODE` (Win11 22H2).
const MIN_BUILD_FOR_PASSTHROUGH: u32 = 22_621;

/// Thread join timeout during shutdown (2 seconds).
const JOIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Cached result of OS version check for `PASSTHROUGH_MODE` support.
static PASSTHROUGH_SUPPORTED: LazyLock<bool> = LazyLock::new(supports_passthrough_mode);

// ---------------------------------------------------------------------------
// OS version detection via RtlGetVersion (raw FFI, no Wdk feature needed)
// ---------------------------------------------------------------------------

#[repr(C)]
struct OsVersionInfoW {
    os_version_info_size: u32,
    major_version: u32,
    minor_version: u32,
    build_number: u32,
    platform_id: u32,
    sz_csd_version: [u16; 128],
}

fn supports_passthrough_mode() -> bool {
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(lp_version_information: *mut OsVersionInfoW) -> i32;
    }

    let mut info: OsVersionInfoW = unsafe { std::mem::zeroed() };
    info.os_version_info_size = size_of::<OsVersionInfoW>() as u32;

    let status = unsafe { RtlGetVersion(&raw mut info) };
    // STATUS_SUCCESS == 0
    if status == 0 {
        let supported = info.build_number >= MIN_BUILD_FOR_PASSTHROUGH;
        if supported {
            info!(build = info.build_number, "PASSTHROUGH_MODE supported");
        } else {
            debug!(
                build = info.build_number,
                min_build = MIN_BUILD_FOR_PASSTHROUGH,
                "PASSTHROUGH_MODE not supported"
            );
        }
        supported
    } else {
        warn!(status, "RtlGetVersion failed, assuming no PASSTHROUGH_MODE");
        false
    }
}

// ---------------------------------------------------------------------------
// SendableHandle wrapper
// ---------------------------------------------------------------------------

/// Wrapper to make HANDLE Send. HANDLE is a raw pointer, but we manage
/// thread safety through our channel-based architecture where each handle
/// is used exclusively by a single thread.
struct SendableHandle(HANDLE);
unsafe impl Send for SendableHandle {}

// ---------------------------------------------------------------------------
// ConPty
// ---------------------------------------------------------------------------

/// A `ConPTY` session managing a pseudo console and child process.
pub struct ConPty {
    console: HPCON,
    process_info: PROCESS_INFORMATION,
    /// None after shutdown (taken during Drop to close the channel).
    input_tx: Option<Sender<Vec<u8>>>,
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

        let mut flags = PSEUDOCONSOLE_RESIZE_QUIRK | PSEUDOCONSOLE_WIN32_INPUT_MODE;
        if *PASSTHROUGH_SUPPORTED {
            flags |= PSEUDOCONSOLE_PASSTHROUGH_MODE;
        }

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
                let h = read_handle;
                read_loop(h.0, output_tx, read_shutdown);
            })
            .expect("failed to spawn read thread");

        let write_shutdown = Arc::clone(&shutdown);
        let write_handle = SendableHandle(pty_input_write);
        let write_thread = thread::Builder::new()
            .name("conpty-write".into())
            .spawn(move || {
                let h = write_handle;
                write_loop(h.0, input_rx, write_shutdown);
            })
            .expect("failed to spawn write thread");

        info!(cols, rows, shell, "ConPTY session created");

        Ok(ConPty {
            console,
            process_info,
            input_tx: Some(input_tx),
            output_rx,
            shutdown,
            read_thread: Some(read_thread),
            write_thread: Some(write_thread),
        })
    }

    /// Send input bytes to the terminal.
    pub fn write(&self, data: &[u8]) {
        if let Some(ref tx) = self.input_tx {
            let _ = tx.send(data.to_vec());
        }
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

    /// Join a thread handle with a timeout. If the join times out, the thread
    /// is leaked (it will be cleaned up when the process exits).
    fn join_with_timeout(handle: thread::JoinHandle<()>, timeout: Duration) {
        let (done_tx, done_rx) = bounded::<()>(1);
        let joiner = thread::spawn(move || {
            let _ = handle.join();
            let _ = done_tx.send(());
        });
        if done_rx.recv_timeout(timeout).is_err() {
            warn!(?timeout, "Thread join timed out, leaking thread");
            // Let joiner leak — the OS cleans up on process exit.
            drop(joiner);
        }
    }
}

impl Drop for ConPty {
    fn drop(&mut self) {
        // 1. Signal shutdown to threads
        self.shutdown.store(true, Ordering::SeqCst);

        // 2. Drop the Sender to close the channel, unblocking write_loop's rx.recv()
        drop(self.input_tx.take());

        // 3. Join write thread (should exit quickly now that channel is closed)
        if let Some(handle) = self.write_thread.take() {
            Self::join_with_timeout(handle, JOIN_TIMEOUT);
        }

        // 4. Close the pseudo console — this causes ReadFile in read_loop to fail
        unsafe {
            ClosePseudoConsole(self.console);
        }

        // 5. Join read thread (should exit now that ReadFile returned an error)
        if let Some(handle) = self.read_thread.take() {
            Self::join_with_timeout(handle, JOIN_TIMEOUT);
        }

        // 6. Close process handles
        unsafe {
            let _ = CloseHandle(self.process_info.hProcess);
            let _ = CloseHandle(self.process_info.hThread);
        }

        info!("ConPTY session closed");
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

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
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
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
                    debug!("Read loop: output channel closed");
                    break;
                }
            }
            Ok(()) => {
                debug!("Read loop: zero-length read, pipe closed");
                break;
            }
            Err(e) => {
                if !shutdown.load(Ordering::Relaxed) {
                    debug!(%e, "Read loop: ReadFile error (expected during shutdown)");
                }
                break;
            }
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
        let result = unsafe {
            windows::Win32::Storage::FileSystem::WriteFile(
                handle,
                Some(&data),
                Some(&raw mut written),
                None,
            )
        };
        if let Err(e) = result {
            if !shutdown.load(Ordering::Relaxed) {
                warn!(%e, "Write loop: WriteFile error");
            }
            break;
        }
    }
    unsafe {
        let _ = CloseHandle(handle);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_detection_does_not_panic() {
        let _ = supports_passthrough_mode();
    }

    #[test]
    fn spawn_and_drop() {
        let pty = ConPty::spawn(80, 24, "powershell.exe -NoProfile -Command exit")
            .expect("failed to spawn ConPty");
        std::thread::sleep(Duration::from_millis(500));
        drop(pty);
        // If we reach here, Drop completed without deadlock
    }

    #[test]
    #[ignore = "ConPTY output goes to parent console in test context, verify with cargo run"]
    fn read_receives_output() {
        // Verify the ConPTY output pipe delivers data from the child process.
        // We spawn cmd.exe which produces a banner/prompt, so we should
        // receive some output without needing to send input.
        let pty = ConPty::spawn(80, 24, "cmd.exe").expect("failed to spawn ConPty");

        let rx = pty.output_receiver();
        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(data) => {
                    output.extend_from_slice(&data);
                    // cmd.exe should produce at least a prompt containing ">"
                    if !output.is_empty() {
                        break;
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        }

        assert!(
            !output.is_empty(),
            "Expected some output from cmd.exe, got nothing"
        );
    }

    #[test]
    fn resize_does_not_error() {
        let pty = ConPty::spawn(80, 24, "powershell.exe -NoProfile -Command exit")
            .expect("failed to spawn ConPty");
        assert!(pty.resize(120, 40).is_ok());
    }
}
