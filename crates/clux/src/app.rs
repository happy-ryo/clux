use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tracing::{info, warn};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use clux_coord::broker::Broker;
use clux_coord::detect::ClaudeDetector;
use clux_coord::mcp_bridge::McpState;
use clux_coord::panel::CoordPanel;
use clux_layout::pane::{PaneId, Rect};
use clux_layout::tab::Tab;
use clux_layout::tree::Direction;
use clux_renderer::cell_renderer::CellInstance;
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

use crate::config::Config;
use crate::selection::{CellCoord, Selection, extract_text, pixel_to_cell};

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 30;

/// Number of lines to scroll per mouse wheel tick.
const SCROLL_LINES_PER_TICK: usize = 3;

/// Default port for the embedded MCP server.
const MCP_DEFAULT_PORT: u16 = 19836;

/// Height of the tab bar in pixels.
const TAB_BAR_HEIGHT: f32 = 24.0;

struct PaneState {
    terminal: ConPty,
    buffer: TerminalBuffer,
    vt_parser: vte::Parser,
    /// Detects Claude Code launch in this pane's output.
    claude_detector: ClaudeDetector,
}

const SESSION_NAME: &str = "default";

/// A pending glyph to resolve from the atlas after collecting all cells.
struct GlyphRequest {
    px: f32,
    py: f32,
    c: char,
    fg_r: f32,
    fg_g: f32,
    fg_b: f32,
}

struct App {
    config: Config,
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
    /// Current text selection state.
    selection: Option<Selection>,
    /// Whether the left mouse button is currently held for drag-selection.
    mouse_dragging: bool,
    /// Clipboard handle (lazily initialized).
    clipboard: Option<arboard::Clipboard>,
    /// Shared MCP coordination state.
    mcp_state: Option<Arc<McpState>>,
    /// Coordination panel overlay.
    coord_panel: Option<CoordPanel>,
    /// Tokio runtime for the MCP server.
    tokio_rt: Arc<tokio::runtime::Runtime>,
    /// Cached cell instances for rendering (rebuilt only when dirty).
    cached_cells: Vec<CellInstance>,
    /// Whether the cell cache needs rebuilding.
    cells_dirty: bool,
    /// Frame counter for throttling expensive operations.
    frame_count: u64,
    /// Whether the cursor is currently visible (for blinking).
    cursor_visible: bool,
}

impl App {
    fn new(config: Config, tokio_rt: Arc<tokio::runtime::Runtime>) -> Self {
        Self {
            config,
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
            selection: None,
            mouse_dragging: false,
            clipboard: None,
            mcp_state: None,
            coord_panel: None,
            tokio_rt,
            cached_cells: Vec::new(),
            cells_dirty: true,
            frame_count: 0,
            cursor_visible: true,
        }
    }

    /// Initialize the MCP coordination server.
    fn start_mcp_server(&mut self) {
        let db_path = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("clux")
            .join("coord.db");

        match Broker::open(&db_path) {
            Ok(broker) => {
                let state = Arc::new(McpState {
                    broker: Arc::new(broker),
                    pane_contexts: tokio::sync::RwLock::new(HashMap::new()),
                });
                let state_clone = Arc::clone(&state);
                self.tokio_rt.spawn(async move {
                    match clux_coord::mcp_bridge::start_server(state_clone, MCP_DEFAULT_PORT).await
                    {
                        Ok(addr) => info!(%addr, "MCP coordination server started"),
                        Err(e) => tracing::error!("Failed to start MCP server: {e}"),
                    }
                });
                self.coord_panel = Some(CoordPanel::new(Arc::clone(&state.broker)));
                self.mcp_state = Some(state);
            }
            Err(e) => {
                tracing::error!("Failed to initialize coordination broker: {e}");
            }
        }
    }

    /// Register a pane as a peer in the coordination system.
    fn register_pane_peer(&self, pane_id: PaneId) {
        if let Some(ref state) = self.mcp_state {
            let peer_id = format!("pane-{pane_id}");
            if let Err(e) = state.broker.register_peer(&peer_id, pane_id) {
                tracing::error!(pane_id, "Failed to register peer: {e}");
            }
        }
    }

    /// Unregister a pane peer from the coordination system.
    fn unregister_pane_peer(&self, pane_id: PaneId) {
        if let Some(ref state) = self.mcp_state {
            let peer_id = format!("pane-{pane_id}");
            if let Err(e) = state.broker.unregister_peer(&peer_id) {
                tracing::error!(pane_id, "Failed to unregister peer: {e}");
            }
        }
    }

    /// Update pane context snapshots for the MCP server.
    fn update_pane_contexts(&self) {
        let Some(ref state) = self.mcp_state else {
            return;
        };
        let mut contexts = HashMap::new();
        for (&pane_id, pane) in &self.panes {
            let lines = pane.buffer.visible_lines();
            let text: String = lines
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| cell.c)
                        .collect::<String>()
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n");
            contexts.insert(pane_id, text);
        }
        // Use blocking write since we're on the main thread
        self.tokio_rt.spawn({
            let pane_contexts = Arc::clone(state);
            async move {
                *pane_contexts.pane_contexts.write().await = contexts;
            }
        });
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
        self.register_pane_peer(pane_id);
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
            self.unregister_pane_peer(*id);
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
            self.cells_dirty = true;
            info!(active = index, "Switched to tab");
            self.resize_all_panes();
        }
    }

    fn spawn_pane_for_id(&mut self, pane_id: PaneId, cols: u16, rows: u16) {
        let shell = &self.config.shell.default;
        match ConPty::spawn(cols, rows, shell) {
            Ok(terminal) => {
                info!(pane_id, shell, "Spawned ConPTY for pane");
                let mut buffer = TerminalBuffer::new(cols as usize, rows as usize);
                buffer.scrollback_max = self.config.scrollback.max_lines;
                self.panes.insert(
                    pane_id,
                    PaneState {
                        terminal,
                        buffer,
                        vt_parser: vte::Parser::new(),
                        claude_detector: ClaudeDetector::new(),
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
            self.unregister_pane_peer(pane_id);
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
        self.register_pane_peer(new_id);
        self.auto_saver.notify_change();
        self.resize_all_panes();
    }

    fn process_terminal_output(&mut self) {
        let mut newly_detected: Vec<PaneId> = Vec::new();
        let mut any_output = false;

        for (&pane_id, pane) in &mut self.panes {
            let had_output = {
                let mut received = false;
                while let Some(data) = pane.terminal.try_read() {
                    clux_terminal::vt_parser::process_bytes(
                        &mut pane.vt_parser,
                        &mut pane.buffer,
                        &data,
                    );
                    // Feed raw output to Claude Code detector
                    if !pane.claude_detector.detected {
                        let text = String::from_utf8_lossy(&data);
                        pane.claude_detector.feed(&text);
                    }
                    received = true;
                }
                received
            };
            if had_output {
                any_output = true;
            }
            // Reset scroll offset when new output arrives (user is at live view)
            if had_output && pane.buffer.scroll_offset > 0 {
                pane.buffer.reset_scroll();
            }
            // Check if Claude Code was newly detected
            if pane.claude_detector.detected && !pane.claude_detector.config_injected {
                newly_detected.push(pane_id);
            }
        }

        if any_output {
            self.cells_dirty = true;
        }

        // Inject MCP config for newly detected Claude Code panes
        for pane_id in newly_detected {
            if let Some(pane) = self.panes.get_mut(&pane_id) {
                // Try to inject MCP config using home directory as fallback
                match clux_coord::detect::inject_mcp_config(None, MCP_DEFAULT_PORT) {
                    Ok(path) => {
                        info!(pane_id, ?path, "Injected MCP config for Claude Code");
                        pane.claude_detector.config_injected = true;
                    }
                    Err(e) => {
                        tracing::error!(pane_id, "Failed to inject MCP config: {e}");
                    }
                }
            }
        }
    }

    fn request_redraw(&self) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }

    /// Viewport for pane content (below the tab bar), in logical pixels.
    /// Width and height are clamped to at least 1.0 to prevent negative dimensions.
    fn viewport(&self) -> Rect {
        let (win_w, win_h) = self.logical_size();
        let w = win_w.max(1.0);
        let h = (win_h - TAB_BAR_HEIGHT).max(1.0);
        Rect::new(0.0, TAB_BAR_HEIGHT, w, h)
    }

    /// Full window size in logical pixels.
    fn logical_size(&self) -> (f32, f32) {
        if let Some(ref window) = self.window {
            let size = window.inner_size();
            let scale = self.scale_factor as f32;
            (size.width as f32 / scale, size.height as f32 / scale)
        } else {
            (800.0, 600.0)
        }
    }

    /// Build cell instances for the tab bar.
    fn build_tab_bar_instances(&mut self) -> Vec<CellInstance> {
        let (win_w, _) = self.logical_size();
        let bar_h = TAB_BAR_HEIGHT;
        let cell_w = self.cell_width;
        let cell_h = self.cell_height;
        let mut instances = Vec::new();

        // Tab bar background
        instances.push(CellInstance::background(
            0.0, 0.0, win_w, bar_h, 0.15, 0.15, 0.22,
        ));

        // Render tab labels
        let mut x_offset: f32 = 4.0;
        let renderer = self.renderer.as_mut().expect("renderer initialized");
        let font_size = renderer.font_size();

        for (idx, tab) in self.tabs.iter().enumerate() {
            let label = tab.name();
            let is_active = idx == self.active_tab;

            // Tab background (highlight active)
            let tab_width = (label.len() as f32 + 2.0) * cell_w;
            let (bg_r, bg_g, bg_b) = if is_active {
                (0.25, 0.25, 0.38)
            } else {
                (0.15, 0.15, 0.22)
            };
            instances.push(CellInstance::background(
                x_offset, 0.0, tab_width, bar_h, bg_r, bg_g, bg_b,
            ));

            // Tab text
            let text_y = (bar_h - cell_h) / 2.0;
            let mut char_x = x_offset + cell_w;
            for c in label.chars() {
                if let Some(glyph) = renderer.get_or_insert_glyph(c, font_size)
                    && glyph.width > 0
                    && glyph.height > 0
                {
                    let (fg_r, fg_g, fg_b) = if is_active {
                        (0.95, 0.95, 0.95)
                    } else {
                        (0.6, 0.6, 0.7)
                    };
                    instances.push(CellInstance::glyph(
                        char_x + glyph.offset_x as f32,
                        text_y + (cell_h - glyph.offset_y as f32),
                        glyph.width as f32,
                        glyph.height as f32,
                        fg_r,
                        fg_g,
                        fg_b,
                        glyph.u,
                        glyph.v,
                        glyph.uv_w,
                        glyph.uv_h,
                    ));
                }
                char_x += cell_w;
            }

            x_offset += tab_width + 2.0;
        }

        instances
    }

    /// Build the list of cell instances for GPU rendering from all visible panes.
    fn build_cell_instances(&mut self) -> Vec<CellInstance> {
        if self.renderer.is_none() {
            return Vec::new();
        }

        let viewport = self.viewport();
        let pane_rects = self.tabs[self.active_tab].all_pane_rects(viewport);
        let cell_w = self.cell_width;
        let cell_h = self.cell_height;
        // Estimate capacity: 2 instances per cell (bg + fg) for all visible cells
        let estimated = pane_rects
            .iter()
            .map(|(id, _)| {
                self.panes
                    .get(id)
                    .map_or(0, |p| p.buffer.cols * p.buffer.rows * 2)
            })
            .sum();
        let mut instances = Vec::with_capacity(estimated);
        let mut glyph_requests: Vec<GlyphRequest> = Vec::new();

        let active_pane_id = self.tabs[self.active_tab].active_pane;

        for (pane_id, rect) in &pane_rects {
            // Skip panes too small to render even one cell
            if rect.width < cell_w || rect.height < cell_h {
                continue;
            }
            let Some(pane) = self.panes.get(pane_id) else {
                continue;
            };
            let cursor_col = pane.buffer.cursor.col;
            let cursor_row = pane.buffer.cursor.row;
            let is_active = *pane_id == active_pane_id;
            // Only show cursor when at live view (not scrolled back)
            let show_cursor = is_active
                && pane.buffer.scroll_offset == 0
                && pane.buffer.cursor_visible
                && self.cursor_visible;

            let visible = pane.buffer.visible_lines();
            for (row_idx, row) in visible.iter().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    let px = rect.x + col_idx as f32 * cell_w;
                    let py = rect.y + row_idx as f32 * cell_h;

                    // Cursor: invert colors at cursor position
                    let at_cursor = show_cursor && col_idx == cursor_col && row_idx == cursor_row;

                    let (bg_r, bg_g, bg_b, fg_r, fg_g, fg_b) = if at_cursor {
                        // Invert: use foreground color as background, background as foreground
                        let fg = &cell.fg;
                        let bg = &cell.bg;
                        (
                            f32::from(fg.r) / 255.0,
                            f32::from(fg.g) / 255.0,
                            f32::from(fg.b) / 255.0,
                            f32::from(bg.r) / 255.0,
                            f32::from(bg.g) / 255.0,
                            f32::from(bg.b) / 255.0,
                        )
                    } else {
                        let bg = &cell.bg;
                        let fg = &cell.fg;
                        (
                            f32::from(bg.r) / 255.0,
                            f32::from(bg.g) / 255.0,
                            f32::from(bg.b) / 255.0,
                            f32::from(fg.r) / 255.0,
                            f32::from(fg.g) / 255.0,
                            f32::from(fg.b) / 255.0,
                        )
                    };

                    instances.push(CellInstance::background(
                        px, py, cell_w, cell_h, bg_r, bg_g, bg_b,
                    ));

                    if cell.c != ' ' {
                        glyph_requests.push(GlyphRequest {
                            px,
                            py,
                            c: cell.c,
                            fg_r,
                            fg_g,
                            fg_b,
                        });
                    }
                }
            }
        }

        Self::resolve_glyphs(
            self.renderer.as_mut().expect("checked above"),
            &glyph_requests,
            cell_h,
            &mut instances,
        );

        instances
    }

    /// Resolve glyph atlas lookups and append foreground instances.
    fn resolve_glyphs(
        renderer: &mut RenderPipeline,
        requests: &[GlyphRequest],
        cell_h: f32,
        instances: &mut Vec<CellInstance>,
    ) {
        let font_size = renderer.font_size();
        for req in requests {
            if let Some(glyph) = renderer.get_or_insert_glyph(req.c, font_size)
                && glyph.width > 0
                && glyph.height > 0
            {
                instances.push(CellInstance::glyph(
                    req.px + glyph.offset_x as f32,
                    req.py + (cell_h - glyph.offset_y as f32),
                    glyph.width as f32,
                    glyph.height as f32,
                    req.fg_r,
                    req.fg_g,
                    req.fg_b,
                    glyph.u,
                    glyph.v,
                    glyph.uv_w,
                    glyph.uv_h,
                ));
            }
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
                        rect.width.max(0.0) as u32,
                        rect.height.max(0.0) as u32,
                        self.cell_width,
                        self.cell_height,
                        1.0,
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
                // Viewport and cell dimensions are both in logical pixels,
                // so no scale_factor needed here.
                let (cols, rows) = pixel_size_to_terminal_size(
                    rect.width.max(0.0) as u32,
                    rect.height.max(0.0) as u32,
                    self.cell_width,
                    self.cell_height,
                    1.0,
                );
                let _ = pane.terminal.resize(cols, rows);
                pane.buffer.resize(cols as usize, rows as usize);
            }
        }
    }

    /// Get the pane rect for the active pane in the current viewport.
    fn active_pane_rect(&self) -> Option<Rect> {
        let viewport = self.viewport();
        let active_id = self.tab().active_pane;
        self.tab()
            .all_pane_rects(viewport)
            .into_iter()
            .find(|(id, _)| *id == active_id)
            .map(|(_, rect)| rect)
    }

    /// Convert cursor pixel position to cell coordinate relative to active pane.
    fn cursor_to_cell(&self) -> Option<CellCoord> {
        let rect = self.active_pane_rect()?;
        // cursor_position is in physical pixels; convert to logical for cell lookup
        let scale = self.scale_factor;
        Some(pixel_to_cell(
            self.cursor_position.0 / scale,
            self.cursor_position.1 / scale,
            rect.x,
            rect.y,
            self.cell_width,
            self.cell_height,
            1.0, // already in logical pixels
        ))
    }

    /// Get or create the clipboard handle.
    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(cb) => self.clipboard = Some(cb),
                Err(e) => {
                    warn!(%e, "Failed to initialize clipboard");
                    return None;
                }
            }
        }
        self.clipboard.as_mut()
    }

    /// Copy the current selection to the system clipboard.
    fn copy_selection(&mut self) {
        let Some(ref sel) = self.selection else {
            return;
        };
        if sel.is_empty() {
            return;
        }

        let active_id = self.tab().active_pane;
        let text = if let Some(pane) = self.panes.get(&active_id) {
            let (start, end) = sel.ordered();
            let visible = pane.buffer.visible_lines();
            let owned: Vec<Vec<clux_terminal::buffer::Cell>> =
                visible.into_iter().cloned().collect();
            extract_text(&owned, start, end, pane.buffer.cols)
        } else {
            return;
        };

        if text.is_empty() {
            return;
        }

        if let Some(cb) = self.clipboard() {
            if let Err(e) = cb.set_text(&text) {
                warn!(%e, "Failed to copy to clipboard");
            } else {
                info!(len = text.len(), "Copied selection to clipboard");
            }
        }
    }

    /// Paste text from the system clipboard into the active pane.
    fn paste_clipboard(&mut self) {
        let text = if let Some(cb) = self.clipboard() {
            match cb.get_text() {
                Ok(t) => t,
                Err(e) => {
                    warn!(%e, "Failed to read clipboard");
                    return;
                }
            }
        } else {
            return;
        };

        if text.is_empty() {
            return;
        }

        let active_id = self.tab().active_pane;
        if let Some(pane) = self.panes.get(&active_id) {
            pane.terminal.write(text.as_bytes());
            info!(len = text.len(), "Pasted from clipboard");
        }
    }

    /// Build a status line string for a pane.
    /// Includes agent status from the coordination broker when available.
    #[expect(
        dead_code,
        reason = "will be used when status bar rendering is connected"
    )]
    pub fn pane_status_text(&self, pane_id: PaneId) -> String {
        let shell = &self.config.shell.default;
        let title = self
            .panes
            .get(&pane_id)
            .map_or("", |p| p.buffer.title.as_str());

        // Check for agent status from coordination broker
        let agent_status = self.mcp_state.as_ref().and_then(|state| {
            let peer_id = format!("pane-{pane_id}");
            state
                .broker
                .list_peers(None)
                .ok()?
                .into_iter()
                .find(|p| p.peer_id == peer_id)?
                .status_text
        });

        let is_claude = self
            .panes
            .get(&pane_id)
            .is_some_and(|p| p.claude_detector.detected);

        let mut parts = vec![format!("[{pane_id}]")];
        if is_claude {
            parts.push("🤖".to_string());
        }
        parts.push(shell.clone());
        if let Some(status) = agent_status {
            parts.push(format!("({status})"));
        } else if !title.is_empty() {
            parts.push(format!("- {title}"));
        }
        parts.join(" ")
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
            // Copy / Paste
            Key::Character(c) if c.as_str() == "C" || c.as_str() == "c" => {
                self.copy_selection();
                true
            }
            // Note: Ctrl+Shift+V was previously used for vertical split.
            // We reassign it to paste. Vertical split moves to Ctrl+Shift+B (bar split).
            Key::Character(c) if c.as_str() == "V" || c.as_str() == "v" => {
                self.paste_clipboard();
                true
            }
            // Pane shortcuts (vertical split now uses B for "bar")
            Key::Character(c) if c.as_str() == "B" || c.as_str() == "b" => {
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
            // Coordination panel toggle
            Key::Character(c) if c.as_str() == "P" || c.as_str() == "p" => {
                if let Some(ref mut panel) = self.coord_panel {
                    panel.toggle();
                    info!(visible = panel.visible, "Coordination panel toggled");
                }
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

    /// Update drag selection when the cursor moves.
    fn handle_cursor_drag(&mut self) {
        if self.mouse_dragging
            && let Some(coord) = self.cursor_to_cell()
            && let Some(ref mut sel) = self.selection
        {
            sel.end = coord;
        }
    }

    /// Handle mouse button press/release for focus and text selection.
    /// Check if a click is in the tab bar and switch tabs accordingly.
    /// Returns true if the click was handled by the tab bar.
    fn handle_tab_bar_click(&mut self, x: f32, y: f32) -> bool {
        // Convert physical pixel coords to logical
        let scale = self.scale_factor as f32;
        let lx = x / scale;
        let ly = y / scale;
        if ly >= TAB_BAR_HEIGHT {
            return false;
        }

        let cell_w = self.cell_width;
        let mut x_offset: f32 = 4.0;

        for (idx, tab) in self.tabs.iter().enumerate() {
            let tab_width = (tab.name().len() as f32 + 2.0) * cell_w;
            if lx >= x_offset && lx < x_offset + tab_width {
                if idx != self.active_tab {
                    self.active_tab = idx;
                    self.cells_dirty = true;
                    info!(active = idx, "Tab switched via click");
                    self.resize_all_panes();
                }
                return true;
            }
            x_offset += tab_width + 2.0;
        }

        true // Click was in tab bar area even if not on a tab
    }

    fn handle_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        if button != MouseButton::Left {
            return;
        }
        if state == ElementState::Pressed {
            let (x, y) = self.cursor_position;
            // Convert physical mouse coords to logical
            let scale = self.scale_factor as f32;
            let lx = x as f32 / scale;
            let ly = y as f32 / scale;

            // Check tab bar first
            if self.handle_tab_bar_click(lx, ly) {
                return;
            }

            let viewport = self.viewport();
            if self.tab_mut().focus_at(lx, ly, viewport) {
                info!(
                    active = self.tab().active_pane,
                    "Focus changed via mouse click"
                );
            }
            if let Some(coord) = self.cursor_to_cell() {
                self.selection = Some(Selection::new(coord));
                self.mouse_dragging = true;
            }
        } else {
            self.mouse_dragging = false;
            if let Some(coord) = self.cursor_to_cell()
                && let Some(ref mut sel) = self.selection
            {
                sel.end = coord;
            }
        }
    }

    /// Handle IME composition events.
    fn handle_ime(&mut self, ime: winit::event::Ime) {
        match ime {
            winit::event::Ime::Enabled => {
                // Update IME cursor area to current cursor position
                self.update_ime_cursor_area();
            }
            winit::event::Ime::Commit(text) => {
                let active_id = self.tab().active_pane;
                if let Some(pane) = self.panes.get(&active_id) {
                    pane.terminal.write(text.as_bytes());
                    info!(len = text.len(), "IME commit forwarded to pane");
                }
                self.cells_dirty = true;
            }
            winit::event::Ime::Preedit(text, _cursor) => {
                if !text.is_empty() {
                    tracing::debug!(text, "IME preedit");
                }
                self.update_ime_cursor_area();
            }
            winit::event::Ime::Disabled => {}
        }
    }

    /// Update the IME candidate window position to the current cursor location.
    fn update_ime_cursor_area(&self) {
        let Some(ref window) = self.window else {
            return;
        };
        let Some(rect) = self.active_pane_rect() else {
            return;
        };
        let active_id = self.tab().active_pane;
        let Some(pane) = self.panes.get(&active_id) else {
            return;
        };

        let scale = self.scale_factor as f32;
        // Cursor position in logical pixels
        let cursor_x = rect.x + pane.buffer.cursor.col as f32 * self.cell_width;
        let cursor_y = rect.y + pane.buffer.cursor.row as f32 * self.cell_height;

        // Convert to physical pixels for winit
        let position =
            winit::dpi::PhysicalPosition::new((cursor_x * scale) as i32, (cursor_y * scale) as i32);
        let size = winit::dpi::PhysicalSize::new(
            (self.cell_width * scale) as u32,
            (self.cell_height * scale) as u32,
        );
        window.set_ime_cursor_area(position, size);
    }

    /// Handle keyboard input events.
    fn handle_keyboard(&mut self, event: winit::event::KeyEvent) {
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

    /// Handle mouse wheel scrolling for the active pane's scrollback.
    fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => {
                if y > 0.0 {
                    // Scroll up (into history)
                    Some((true, (y.abs() as usize) * SCROLL_LINES_PER_TICK))
                } else if y < 0.0 {
                    // Scroll down (toward present)
                    Some((false, (y.abs() as usize) * SCROLL_LINES_PER_TICK))
                } else {
                    None
                }
            }
            MouseScrollDelta::PixelDelta(pos) => {
                let line_height = f64::from(self.cell_height) * self.scale_factor;
                if line_height <= 0.0 {
                    return;
                }
                let pixel_lines = (pos.y.abs() / line_height).ceil() as usize;
                let pixel_lines = pixel_lines.max(1);
                if pos.y > 0.0 {
                    Some((true, pixel_lines))
                } else if pos.y < 0.0 {
                    Some((false, pixel_lines))
                } else {
                    None
                }
            }
        };

        if let Some((up, count)) = lines {
            let active_id = self.tab().active_pane;
            if let Some(pane) = self.panes.get_mut(&active_id) {
                if up {
                    pane.buffer.scroll_view_up(count);
                } else {
                    pane.buffer.scroll_view_down(count);
                }
                self.cells_dirty = true;
            }
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

        // Enable IME input (required for Japanese input methods like ATOK, MS-IME)
        window.set_ime_allowed(true);

        self.scale_factor = window.scale_factor();

        let renderer = pollster::block_on(RenderPipeline::new(Arc::clone(&window)))
            .expect("Failed to initialize GPU renderer");

        info!(
            scale_factor = self.scale_factor,
            cell_width = self.cell_width,
            cell_height = self.cell_height,
            "Window and GPU renderer initialized"
        );

        self.renderer = Some(renderer);
        self.window = Some(window);

        // Start the MCP coordination server
        self.start_mcp_server();

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
                    renderer.resize(physical_size.width, physical_size.height, self.scale_factor);
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
                        renderer.resize(size.width, size.height, self.scale_factor);
                    }
                }
                self.resize_all_panes();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.frame_count += 1;
                self.process_terminal_output();
                self.try_auto_save();

                // Throttle expensive MCP context updates (~every 60 frames)
                if self.frame_count.is_multiple_of(60) {
                    self.update_pane_contexts();
                }

                // Cursor blink (~every 30 frames ≈ 500ms at 60fps)
                if self.frame_count.is_multiple_of(30) {
                    self.cursor_visible = !self.cursor_visible;
                    self.cells_dirty = true;
                }

                if self.resize_debouncer.poll().is_some() {
                    self.resize_all_panes();
                    self.cells_dirty = true;
                }

                // Only rebuild cell instances when content changed
                if self.cells_dirty {
                    let mut cells = self.build_tab_bar_instances();
                    cells.append(&mut self.build_cell_instances());
                    self.cached_cells = cells;
                    self.cells_dirty = false;
                }

                if let Some(ref mut renderer) = self.renderer {
                    let bg_color = &self.config.colors.background;
                    let bg = wgpu::Color {
                        r: f64::from(bg_color[0]),
                        g: f64::from(bg_color[1]),
                        b: f64::from(bg_color[2]),
                        a: 1.0,
                    };
                    if let Err(e) = renderer.render_frame(bg, &self.cached_cells) {
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
                self.handle_cursor_drag();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(state, button);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(delta);
            }
            WindowEvent::Ime(ime) => {
                self.handle_ime(ime);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_keyboard(event);
            }
            _ => {}
        }
    }
}

pub fn run(config: Config) -> Result<()> {
    info!("Starting clux");

    info!(
        shell = %config.shell.default,
        font = %config.font.family,
        font_size = config.font.size,
        scrollback = config.scrollback.max_lines,
        "Configuration loaded"
    );

    let tokio_rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime"),
    );

    let event_loop = EventLoop::new()?;
    let mut app = App::new(config, tokio_rt);
    event_loop.run_app(&mut app)?;
    Ok(())
}
