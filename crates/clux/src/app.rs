use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tracing::info;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use clux_layout::pane::{PaneId, Rect};
use clux_layout::tab::Tab;
use clux_layout::tree::Direction;
use clux_renderer::pipeline::RenderPipeline;
use clux_session::auto_save::AutoSaver;
use clux_session::state::{PaneSnapshot, SessionState, TabState};
use clux_terminal::buffer::TerminalBuffer;
use clux_terminal::conpty::ConPty;
use clux_terminal::input::key_event_to_bytes;
use clux_terminal::resize::ResizeDebouncer;
use clux_terminal::terminal_size::{
    DEFAULT_CELL_HEIGHT, DEFAULT_CELL_WIDTH, pixel_size_to_terminal_size,
};

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 30;

struct PaneState {
    terminal: ConPty,
    buffer: TerminalBuffer,
    vt_parser: vte::Parser,
}

const SESSION_NAME: &str = "default";

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<RenderPipeline>,
    tabs: Vec<Tab>,
    active_tab: usize,
    panes: HashMap<PaneId, PaneState>,
    resize_debouncer: ResizeDebouncer,
    auto_saver: AutoSaver,
    cell_width: f32,
    cell_height: f32,
    scale_factor: f64,
    modifiers: ModifiersState,
    cursor_position: (f64, f64),
    /// Global pane ID counter (shared across all tabs).
    next_global_pane_id: PaneId,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            tabs: Vec::new(),
            active_tab: 0,
            panes: HashMap::new(),
            resize_debouncer: ResizeDebouncer::new(50),
            auto_saver: AutoSaver::default_debounce(),
            cell_width: DEFAULT_CELL_WIDTH,
            cell_height: DEFAULT_CELL_HEIGHT,
            scale_factor: 1.0,
            modifiers: ModifiersState::empty(),
            cursor_position: (0.0, 0.0),
            next_global_pane_id: 0,
        }
    }

    fn tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    fn create_tab(&mut self, name: impl Into<String>) {
        let pane_id = self.next_global_pane_id;
        self.next_global_pane_id += 1;
        let tab = Tab::with_pane_id(name, pane_id);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.spawn_pane_for_id(pane_id, DEFAULT_COLS, DEFAULT_ROWS);
        self.auto_saver.notify_change();
        info!(tab_index = self.active_tab, pane_id, "New tab created");
    }

    fn close_active_tab(&mut self) {
        if self.tabs.len() <= 1 {
            info!("Cannot close the last tab");
            return;
        }
        // Collect pane IDs from the tab being closed
        let viewport = self.viewport();
        let pane_ids: Vec<PaneId> = self.tabs[self.active_tab]
            .all_pane_rects(viewport)
            .into_iter()
            .map(|(id, _)| id)
            .collect();

        // Remove all panes (ConPty auto-cleanup via Drop)
        for id in &pane_ids {
            self.panes.remove(id);
        }

        let closed_idx = self.active_tab;
        self.tabs.remove(closed_idx);
        // Adjust active_tab index
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.auto_saver.notify_change();
        info!(closed = closed_idx, active = self.active_tab, "Tab closed");
        self.resize_all_panes();
    }

    fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() && index != self.active_tab {
            self.active_tab = index;
            info!(active = index, "Switched to tab");
            self.resize_all_panes();
        }
    }

    fn spawn_pane_for_id(&mut self, pane_id: PaneId, cols: u16, rows: u16) {
        match ConPty::spawn(cols, rows, "pwsh.exe") {
            Ok(terminal) => {
                info!(pane_id, "Spawned ConPTY for pane");
                self.panes.insert(
                    pane_id,
                    PaneState {
                        terminal,
                        buffer: TerminalBuffer::new(cols as usize, rows as usize),
                        vt_parser: vte::Parser::new(),
                    },
                );
            }
            Err(e) => {
                tracing::error!(pane_id, "Failed to spawn terminal: {e}");
            }
        }
    }

    fn close_active_pane(&mut self) {
        if self.tab().pane_count() <= 1 {
            info!("Cannot close the last pane");
            return;
        }
        let pane_id = self.tab().active_pane;
        if self.tab_mut().close_pane(pane_id) {
            self.panes.remove(&pane_id);
            self.auto_saver.notify_change();
            info!(pane_id, "Pane closed");
        }
    }

    fn split_active(&mut self, direction: Direction) {
        let new_id = self.next_global_pane_id;
        self.next_global_pane_id += 1;
        self.tab_mut().split_active_with_id(direction, 0.5, new_id);
        info!(new_pane_id = new_id, ?direction, "Split active pane");
        self.spawn_pane_for_id(new_id, DEFAULT_COLS, DEFAULT_ROWS);
        self.auto_saver.notify_change();
        self.resize_all_panes();
    }

    fn process_terminal_output(&mut self) {
        for pane in self.panes.values_mut() {
            while let Some(data) = pane.terminal.try_read() {
                clux_terminal::vt_parser::process_bytes(
                    &mut pane.vt_parser,
                    &mut pane.buffer,
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

    fn viewport(&self) -> Rect {
        if let Some(ref window) = self.window {
            let size = window.inner_size();
            Rect::new(0.0, 0.0, size.width as f32, size.height as f32)
        } else {
            Rect::new(0.0, 0.0, 800.0, 600.0)
        }
    }

    /// Build a serializable snapshot of the current workspace state.
    fn build_session_state(&self) -> SessionState {
        let tabs = self.tabs.iter().map(TabState::from).collect();
        let viewport = self.viewport();
        let panes = self
            .tabs
            .iter()
            .flat_map(|tab| tab.all_pane_rects(viewport))
            .filter_map(|(pane_id, rect)| {
                self.panes.get(&pane_id).map(|_pane| {
                    let (cols, rows) = pixel_size_to_terminal_size(
                        rect.width as u32,
                        rect.height as u32,
                        self.cell_width,
                        self.cell_height,
                        self.scale_factor,
                    );
                    PaneSnapshot {
                        pane_id,
                        cwd: None, // CWD discovery is not yet implemented
                        shell: "pwsh.exe".to_string(),
                        cols,
                        rows,
                    }
                })
            })
            .collect();

        SessionState {
            name: SESSION_NAME.to_string(),
            active_tab: self.active_tab,
            tabs,
            panes,
        }
    }

    /// Perform an auto-save if the debounce timer has elapsed.
    fn try_auto_save(&mut self) {
        if self.auto_saver.poll_save() {
            let state = self.build_session_state();
            if let Err(e) = clux_session::store::save(&state) {
                tracing::error!("Auto-save failed: {e}");
            }
        }
    }

    fn resize_all_panes(&mut self) {
        let viewport = self.viewport();
        // Only resize panes in the active tab
        for (pane_id, rect) in self.tab().all_pane_rects(viewport) {
            if let Some(pane) = self.panes.get_mut(&pane_id) {
                let (cols, rows) = pixel_size_to_terminal_size(
                    rect.width as u32,
                    rect.height as u32,
                    self.cell_width,
                    self.cell_height,
                    self.scale_factor,
                );
                let _ = pane.terminal.resize(cols, rows);
                pane.buffer.resize(cols as usize, rows as usize);
            }
        }
    }

    /// Check if a key event is a management shortcut (Ctrl+Shift+...).
    /// Returns true if the event was handled.
    fn handle_shortcut(&mut self, event: &winit::event::KeyEvent) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let ctrl_shift = self
            .modifiers
            .contains(ModifiersState::CONTROL | ModifiersState::SHIFT);
        if !ctrl_shift {
            return false;
        }

        match &event.logical_key {
            // Pane shortcuts
            Key::Character(c) if c.as_str() == "V" || c.as_str() == "v" => {
                self.split_active(Direction::Vertical);
                true
            }
            Key::Character(c) if c.as_str() == "H" || c.as_str() == "h" => {
                self.split_active(Direction::Horizontal);
                true
            }
            Key::Character(c) if c.as_str() == "W" || c.as_str() == "w" => {
                self.close_active_pane();
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.tab_mut().cycle_focus(true);
                info!(active = self.tab().active_pane, "Focus cycled forward");
                true
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.tab_mut().cycle_focus(false);
                info!(active = self.tab().active_pane, "Focus cycled backward");
                true
            }
            // Tab shortcuts
            Key::Character(c) if c.as_str() == "T" || c.as_str() == "t" => {
                let name = format!("tab-{}", self.tabs.len());
                self.create_tab(name);
                self.resize_all_panes();
                true
            }
            Key::Character(c) if c.as_str() == "Q" || c.as_str() == "q" => {
                self.close_active_tab();
                true
            }
            // Ctrl+Shift+1..9 for tab switching
            Key::Character(c) => {
                if let Some(digit) = c.as_str().chars().next().and_then(|ch| ch.to_digit(10))
                    && (1..=9).contains(&digit)
                {
                    self.switch_tab((digit - 1) as usize);
                    return true;
                }
                false
            }
            _ => false,
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

        self.scale_factor = window.scale_factor();

        let renderer = pollster::block_on(RenderPipeline::new(Arc::clone(&window)))
            .expect("Failed to initialize GPU renderer");

        info!(
            scale_factor = self.scale_factor,
            "Window and GPU renderer initialized"
        );

        self.renderer = Some(renderer);
        self.window = Some(window);

        // Create the initial tab with its first pane
        self.create_tab("main");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Window close requested");
                // Force-save session before exiting.
                self.auto_saver.notify_change();
                self.auto_saver.force_save();
                self.try_auto_save();
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(ref mut renderer) = self.renderer {
                    renderer.resize(physical_size.width, physical_size.height);
                }
                self.resize_all_panes();
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

                if let Some(ref window) = self.window {
                    let size = window.inner_size();
                    if let Some(ref mut renderer) = self.renderer {
                        renderer.resize(size.width, size.height);
                    }
                }
                self.resize_all_panes();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.process_terminal_output();
                self.try_auto_save();

                if self.resize_debouncer.poll().is_some() {
                    self.resize_all_panes();
                }

                if let Some(ref mut renderer) = self.renderer {
                    let bg = wgpu::Color {
                        r: 0.118,
                        g: 0.118,
                        b: 0.180,
                        a: 1.0,
                    };
                    // TODO: build CellInstance list from all pane buffers + tab bar
                    if let Err(e) = renderer.render_frame(bg, &[]) {
                        tracing::error!("Render error: {e}");
                    }
                }

                self.request_redraw();
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = (position.x, position.y);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let viewport = self.viewport();
                let (x, y) = self.cursor_position;
                if self.tab_mut().focus_at(x as f32, y as f32, viewport) {
                    info!(
                        active = self.tab().active_pane,
                        "Focus changed via mouse click"
                    );
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.handle_shortcut(&event) {
                    return;
                }

                if event.state == ElementState::Pressed
                    && let Some(pane) = self.panes.get(&self.tab().active_pane)
                {
                    if let Some(bytes) = key_event_to_bytes(&event, self.modifiers) {
                        pane.terminal.write(&bytes);
                    } else if let Some(ref text) = event.text {
                        pane.terminal.write(text.as_bytes());
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
