use std::sync::Arc;

use anyhow::Result;
use tracing::info;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use clux_renderer::pipeline::RenderPipeline;
use clux_terminal::buffer::TerminalBuffer;
use clux_terminal::conpty::ConPty;
use clux_terminal::input::key_event_to_bytes;
use clux_terminal::resize::ResizeDebouncer;
use clux_terminal::terminal_size::{
    DEFAULT_CELL_HEIGHT, DEFAULT_CELL_WIDTH, pixel_size_to_terminal_size,
};

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 30;

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<RenderPipeline>,
    terminal: Option<ConPty>,
    buffer: TerminalBuffer,
    vt_parser: vte::Parser,
    resize_debouncer: ResizeDebouncer,
    /// Logical cell width in pixels (before DPI scaling).
    cell_width: f32,
    /// Logical cell height in pixels (before DPI scaling).
    cell_height: f32,
    /// Current DPI scale factor from the OS.
    scale_factor: f64,
    modifiers: ModifiersState,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            terminal: None,
            buffer: TerminalBuffer::new(DEFAULT_COLS as usize, DEFAULT_ROWS as usize),
            vt_parser: vte::Parser::new(),
            resize_debouncer: ResizeDebouncer::new(50),
            cell_width: DEFAULT_CELL_WIDTH,
            cell_height: DEFAULT_CELL_HEIGHT,
            scale_factor: 1.0,
            modifiers: ModifiersState::empty(),
        }
    }

    fn process_terminal_output(&mut self) {
        if let Some(ref terminal) = self.terminal {
            while let Some(data) = terminal.try_read() {
                clux_terminal::vt_parser::process_bytes(
                    &mut self.vt_parser,
                    &mut self.buffer,
                    &data,
                );
            }
        }
    }

    fn request_redraw(&self) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }

    /// Recalculate terminal dimensions from physical pixel size and schedule
    /// a debounced resize for `ConPTY` and the terminal buffer.
    fn schedule_terminal_resize(&mut self, physical_width: u32, physical_height: u32) {
        let (cols, rows) = pixel_size_to_terminal_size(
            physical_width,
            physical_height,
            self.cell_width,
            self.cell_height,
            self.scale_factor,
        );
        self.resize_debouncer.request(cols, rows);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes().with_title("clux - Terminal Multiplexer");
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

        // Capture the initial DPI scale factor from the window
        self.scale_factor = window.scale_factor();

        // Initialize renderer
        let renderer = pollster::block_on(RenderPipeline::new(Arc::clone(&window)))
            .expect("Failed to initialize GPU renderer");

        info!(
            scale_factor = self.scale_factor,
            "Window and GPU renderer initialized"
        );

        // Spawn terminal
        match ConPty::spawn(DEFAULT_COLS, DEFAULT_ROWS, "pwsh.exe") {
            Ok(pty) => {
                info!("ConPTY terminal spawned");
                self.terminal = Some(pty);
            }
            Err(e) => {
                tracing::error!("Failed to spawn terminal: {}", e);
            }
        }

        self.renderer = Some(renderer);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Window close requested");
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(ref mut renderer) = self.renderer {
                    renderer.resize(physical_size.width, physical_size.height);
                }
                self.schedule_terminal_resize(physical_size.width, physical_size.height);
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let old_factor = self.scale_factor;
                self.scale_factor = scale_factor;
                info!(
                    old_factor,
                    new_factor = scale_factor,
                    "DPI scale factor changed"
                );

                // Recalculate terminal size with the new scale factor using
                // the current physical window size.
                if let Some(ref window) = self.window {
                    let size = window.inner_size();
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.resize(size.width, size.height);
                    }
                    self.schedule_terminal_resize(size.width, size.height);
                }

                // TODO: rebuild glyph atlas at new DPI for sharper rendering
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.process_terminal_output();

                // Check for pending resize
                if let Some((cols, rows)) = self.resize_debouncer.poll() {
                    if let Some(ref terminal) = self.terminal {
                        let _ = terminal.resize(cols, rows);
                    }
                    self.buffer.resize(cols as usize, rows as usize);
                }

                if let Some(ref mut renderer) = self.renderer {
                    let bg = wgpu::Color {
                        r: 0.118,
                        g: 0.118,
                        b: 0.180,
                        a: 1.0,
                    };
                    // TODO: build CellInstance list from terminal buffer
                    if let Err(e) = renderer.render_frame(bg, &[]) {
                        tracing::error!("Render error: {}", e);
                    }
                }

                // Request continuous redraws for terminal output
                self.request_redraw();
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == winit::event::ElementState::Pressed
                    && let Some(ref terminal) = self.terminal
                {
                    // Try special-key / modifier mapping first
                    if let Some(bytes) = key_event_to_bytes(&event, self.modifiers) {
                        terminal.write(&bytes);
                    } else if let Some(ref text) = event.text {
                        // Fall back to plain text input
                        terminal.write(text.as_bytes());
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn run() -> Result<()> {
    info!("Starting clux");
    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
