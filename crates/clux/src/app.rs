use std::sync::Arc;

use anyhow::Result;
use tracing::info;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use clux_renderer::pipeline::RenderPipeline;
use clux_terminal::buffer::TerminalBuffer;
use clux_terminal::conpty::ConPty;
use clux_terminal::resize::ResizeDebouncer;

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 30;

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<RenderPipeline>,
    terminal: Option<ConPty>,
    buffer: TerminalBuffer,
    vt_parser: vte::Parser,
    resize_debouncer: ResizeDebouncer,
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

        // Initialize renderer
        let renderer = pollster::block_on(RenderPipeline::new(Arc::clone(&window)))
            .expect("Failed to initialize GPU renderer");

        info!("Window and GPU renderer initialized");

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
                // TODO: calculate terminal cols/rows from pixel size
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

                if let Some(ref renderer) = self.renderer {
                    let bg = wgpu::Color {
                        r: 0.118,
                        g: 0.118,
                        b: 0.180,
                        a: 1.0,
                    };
                    if let Err(e) = renderer.render_frame(bg) {
                        tracing::error!("Render error: {}", e);
                    }
                }

                // Request continuous redraws for terminal output
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == winit::event::ElementState::Pressed
                    && let Some(ref text) = event.text
                    && let Some(ref terminal) = self.terminal
                {
                    terminal.write(text.as_bytes());
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
