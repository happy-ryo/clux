#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const fn black() -> Self {
        Self::new(0, 0, 0)
    }
    pub const fn white() -> Self {
        Self::new(255, 255, 255)
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::white()
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CellAttrs {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::white(),
            bg: Color::black(),
            attrs: CellAttrs::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CursorPos {
    pub col: usize,
    pub row: usize,
}

/// Saved cursor state for save/restore operations.
#[derive(Debug, Clone)]
pub struct SavedCursor {
    pub pos: CursorPos,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttrs,
}

impl Default for SavedCursor {
    fn default() -> Self {
        Self {
            pos: CursorPos::default(),
            fg: Color::white(),
            bg: Color::black(),
            attrs: CellAttrs::default(),
        }
    }
}

/// Default scrollback buffer capacity in lines.
const DEFAULT_SCROLLBACK_MAX: usize = 10_000;

/// The terminal screen buffer.
pub struct TerminalBuffer {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Vec<Cell>>,
    pub cursor: CursorPos,
    pub default_fg: Color,
    pub default_bg: Color,
    current_fg: Color,
    current_bg: Color,
    current_attrs: CellAttrs,

    // Scroll region (top inclusive, bottom inclusive, 0-indexed)
    pub scroll_top: usize,
    pub scroll_bottom: usize,

    // Scrollback buffer
    pub scrollback: Vec<Vec<Cell>>,
    pub scrollback_max: usize,

    // Cursor save/restore
    saved_cursor: SavedCursor,

    // Alternate screen buffer
    alternate_cells: Option<Vec<Vec<Cell>>>,
    alternate_cursor: Option<CursorPos>,
    /// True when the alternate screen is active.
    pub in_alternate_screen: bool,

    // Insert mode
    pub insert_mode: bool,

    // Tab stops
    pub tab_stops: Vec<bool>,

    // Window title (set via OSC)
    pub title: String,

    /// Current scroll offset (0 = at the bottom / live view).
    /// Positive values scroll back into history.
    pub scroll_offset: usize,

    /// Whether the cursor should be displayed (DECTCEM).
    pub cursor_visible: bool,
}

impl TerminalBuffer {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        let tab_stops = init_tab_stops(cols);
        Self {
            cols,
            rows,
            cells,
            cursor: CursorPos::default(),
            default_fg: Color::white(),
            default_bg: Color::black(),
            current_fg: Color::white(),
            current_bg: Color::black(),
            current_attrs: CellAttrs::default(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            scrollback: Vec::new(),
            scrollback_max: DEFAULT_SCROLLBACK_MAX,
            saved_cursor: SavedCursor::default(),
            alternate_cells: None,
            alternate_cursor: None,
            in_alternate_screen: false,
            insert_mode: false,
            tab_stops,
            title: String::new(),
            scroll_offset: 0,
            cursor_visible: true,
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let mut new_cells = vec![vec![Cell::default(); cols]; rows];
        let copy_rows = self.rows.min(rows);
        let copy_cols = self.cols.min(cols);
        for (new_row, old_row) in new_cells.iter_mut().zip(self.cells.iter()).take(copy_rows) {
            for (new_cell, old_cell) in new_row.iter_mut().zip(old_row.iter()).take(copy_cols) {
                new_cell.clone_from(old_cell);
            }
        }
        self.cells = new_cells;
        self.cols = cols;
        self.rows = rows;
        self.cursor.col = self.cursor.col.min(cols.saturating_sub(1));
        self.cursor.row = self.cursor.row.min(rows.saturating_sub(1));
        self.scroll_bottom = rows.saturating_sub(1);
        self.tab_stops = init_tab_stops(cols);
    }

    pub fn put_char(&mut self, c: char) {
        if self.cursor.col < self.cols && self.cursor.row < self.rows {
            if self.insert_mode {
                // Shift characters right from cursor position
                let row = self.cursor.row;
                let col = self.cursor.col;
                // Remove last cell, insert a blank at cursor position
                self.cells[row].pop();
                self.cells[row].insert(col, Cell::default());
            }
            self.cells[self.cursor.row][self.cursor.col] = Cell {
                c,
                fg: self.current_fg,
                bg: self.current_bg,
                attrs: self.current_attrs,
            };
            self.cursor.col += 1;
            if self.cursor.col >= self.cols {
                self.cursor.col = 0;
                self.newline();
            }
        }
    }

    pub fn newline(&mut self) {
        if self.cursor.row == self.scroll_bottom {
            self.scroll_up();
        } else if self.cursor.row + 1 < self.rows {
            self.cursor.row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
    }

    /// Index: move cursor down one line. If at the bottom of the scroll region,
    /// scroll the region up. If below the scroll region, just move down.
    pub fn index(&mut self) {
        if self.cursor.row == self.scroll_bottom {
            self.scroll_up();
        } else if self.cursor.row + 1 < self.rows {
            self.cursor.row += 1;
        }
    }

    /// Reverse Index: move cursor up one line. If at the top of the scroll region,
    /// scroll the region down. If above the scroll region, just move up.
    pub fn reverse_index(&mut self) {
        if self.cursor.row == self.scroll_top {
            self.scroll_down();
        } else if self.cursor.row > 0 {
            self.cursor.row -= 1;
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    /// Scroll the scroll region up by one line.
    /// The top line of the region is moved to scrollback (if in main screen and region is full
    /// screen) and a blank line is inserted at the bottom of the region.
    pub fn scroll_up(&mut self) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;

        // Move the top line to scrollback only when scrolling the full screen (not alternate)
        if !self.in_alternate_screen && top == 0 {
            let removed = self.cells.remove(top);
            self.scrollback.push(removed);
            if self.scrollback.len() > self.scrollback_max {
                self.scrollback.remove(0);
            }
            self.cells.insert(bottom, vec![Cell::default(); self.cols]);
        } else {
            self.cells.remove(top);
            self.cells.insert(bottom, vec![Cell::default(); self.cols]);
        }
    }

    /// Scroll the scroll region down by one line.
    /// A blank line is inserted at the top of the region and the bottom line is removed.
    pub fn scroll_down(&mut self) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        self.cells.remove(bottom);
        self.cells.insert(top, vec![Cell::default(); self.cols]);
    }

    /// Erase `n` characters starting at the cursor position (ECH).
    /// Does not move the cursor.
    pub fn erase_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        if row < self.rows {
            let end = (col + n).min(self.cols);
            for c in col..end {
                self.cells[row][c] = Cell::default();
            }
        }
    }

    pub fn clear_screen(&mut self) {
        self.cells = vec![vec![Cell::default(); self.cols]; self.rows];
        self.cursor = CursorPos::default();
    }

    pub fn erase_in_line(&mut self, mode: u16) {
        match mode {
            0 => {
                // Clear from cursor to end of line
                for c in self.cursor.col..self.cols {
                    self.cells[self.cursor.row][c] = Cell::default();
                }
            }
            1 => {
                // Clear from start of line to cursor
                for c in 0..=self.cursor.col.min(self.cols - 1) {
                    self.cells[self.cursor.row][c] = Cell::default();
                }
            }
            2 => {
                // Clear entire line
                self.cells[self.cursor.row] = vec![Cell::default(); self.cols];
            }
            _ => {}
        }
    }

    pub fn erase_in_display(&mut self, mode: u16) {
        match mode {
            0 => {
                // Clear from cursor to end of screen
                self.erase_in_line(0);
                for r in (self.cursor.row + 1)..self.rows {
                    self.cells[r] = vec![Cell::default(); self.cols];
                }
            }
            1 => {
                // Clear from start of screen to cursor
                for r in 0..self.cursor.row {
                    self.cells[r] = vec![Cell::default(); self.cols];
                }
                self.erase_in_line(1);
            }
            2 | 3 => {
                self.clear_screen();
            }
            _ => {}
        }
    }

    pub fn set_cursor_pos(&mut self, row: usize, col: usize) {
        self.cursor.row = row.min(self.rows.saturating_sub(1));
        self.cursor.col = col.min(self.cols.saturating_sub(1));
    }

    pub fn set_fg(&mut self, color: Color) {
        self.current_fg = color;
    }

    pub fn set_bg(&mut self, color: Color) {
        self.current_bg = color;
    }

    pub fn reset_attrs(&mut self) {
        self.current_fg = self.default_fg;
        self.current_bg = self.default_bg;
        self.current_attrs = CellAttrs::default();
    }

    pub fn set_bold(&mut self, on: bool) {
        self.current_attrs.bold = on;
    }

    pub fn set_italic(&mut self, on: bool) {
        self.current_attrs.italic = on;
    }

    pub fn set_underline(&mut self, on: bool) {
        self.current_attrs.underline = on;
    }

    pub fn set_inverse(&mut self, on: bool) {
        self.current_attrs.inverse = on;
    }

    // --- Scroll region ---

    /// Set the scroll region (DECSTBM). Both `top` and `bottom` are 0-indexed inclusive.
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let top = top.min(self.rows.saturating_sub(1));
        let bottom = bottom.min(self.rows.saturating_sub(1));
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        }
        // Move cursor to home position after setting scroll region
        self.cursor.row = 0;
        self.cursor.col = 0;
    }

    // --- Cursor save/restore ---

    pub fn save_cursor(&mut self) {
        self.saved_cursor = SavedCursor {
            pos: self.cursor,
            fg: self.current_fg,
            bg: self.current_bg,
            attrs: self.current_attrs,
        };
    }

    pub fn restore_cursor(&mut self) {
        let saved = self.saved_cursor.clone();
        self.cursor = saved.pos;
        self.current_fg = saved.fg;
        self.current_bg = saved.bg;
        self.current_attrs = saved.attrs;
        // Clamp to valid range
        self.cursor.row = self.cursor.row.min(self.rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(self.cols.saturating_sub(1));
    }

    // --- Alternate screen buffer ---

    /// Enter the alternate screen buffer (used by fullscreen apps like vim, less).
    pub fn enter_alternate_screen(&mut self) {
        if self.in_alternate_screen {
            return;
        }
        self.in_alternate_screen = true;
        // Save current main screen cells and cursor
        let main_cells = std::mem::replace(
            &mut self.cells,
            vec![vec![Cell::default(); self.cols]; self.rows],
        );
        self.alternate_cells = Some(main_cells);
        self.alternate_cursor = Some(self.cursor);
        self.cursor = CursorPos::default();
    }

    /// Exit the alternate screen buffer and restore the main screen.
    pub fn exit_alternate_screen(&mut self) {
        if !self.in_alternate_screen {
            return;
        }
        self.in_alternate_screen = false;
        if let Some(main_cells) = self.alternate_cells.take() {
            self.cells = main_cells;
        }
        if let Some(main_cursor) = self.alternate_cursor.take() {
            self.cursor = main_cursor;
        }
    }

    // --- Insert/delete lines ---

    /// Insert `n` blank lines at the current cursor row, scrolling lines below down.
    /// Operates within the scroll region.
    pub fn insert_lines(&mut self, n: usize) {
        let row = self.cursor.row;
        if row < self.scroll_top || row > self.scroll_bottom {
            return;
        }
        for _ in 0..n {
            if self.scroll_bottom < self.cells.len() {
                self.cells.remove(self.scroll_bottom);
            }
            self.cells.insert(row, vec![Cell::default(); self.cols]);
        }
        self.cursor.col = 0;
    }

    /// Delete `n` lines at the current cursor row, scrolling lines below up.
    /// Operates within the scroll region.
    pub fn delete_lines(&mut self, n: usize) {
        let row = self.cursor.row;
        if row < self.scroll_top || row > self.scroll_bottom {
            return;
        }
        for _ in 0..n {
            if row < self.cells.len() {
                self.cells.remove(row);
            }
            self.cells
                .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
        }
        self.cursor.col = 0;
    }

    // --- Insert/delete characters ---

    /// Insert `n` blank characters at the cursor position, shifting existing characters right.
    pub fn insert_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        if row >= self.rows || col >= self.cols {
            return;
        }
        for _ in 0..n {
            if self.cells[row].len() > self.cols.saturating_sub(1) {
                self.cells[row].pop();
            }
            self.cells[row].insert(col, Cell::default());
        }
        // Ensure row length stays at cols
        self.cells[row].truncate(self.cols);
    }

    /// Delete `n` characters at the cursor position, shifting remaining characters left.
    pub fn delete_chars(&mut self, n: usize) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        if row >= self.rows || col >= self.cols {
            return;
        }
        for _ in 0..n {
            if col < self.cells[row].len() {
                self.cells[row].remove(col);
            }
            self.cells[row].push(Cell::default());
        }
        self.cells[row].truncate(self.cols);
    }

    // --- Tab stops ---

    /// Set a tab stop at the current cursor column.
    pub fn set_tab_stop(&mut self) {
        if self.cursor.col < self.tab_stops.len() {
            self.tab_stops[self.cursor.col] = true;
        }
    }

    /// Clear the tab stop at the current cursor column.
    pub fn clear_tab_stop(&mut self) {
        if self.cursor.col < self.tab_stops.len() {
            self.tab_stops[self.cursor.col] = false;
        }
    }

    /// Clear all tab stops.
    pub fn clear_all_tab_stops(&mut self) {
        self.tab_stops.fill(false);
    }

    /// Move cursor to the next tab stop (or end of line).
    pub fn next_tab_stop(&mut self) {
        let start = self.cursor.col + 1;
        for i in start..self.cols {
            if self.tab_stops[i] {
                self.cursor.col = i;
                return;
            }
        }
        // No tab stop found, go to last column
        self.cursor.col = self.cols.saturating_sub(1);
    }

    // --- Window title ---

    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    // --- Scroll offset (user-initiated scrollback viewing) ---

    /// Scroll the viewport up (into scrollback history) by `n` lines.
    pub fn scroll_view_up(&mut self, n: usize) {
        let max_offset = self.scrollback.len();
        self.scroll_offset = (self.scroll_offset + n).min(max_offset);
    }

    /// Scroll the viewport down (toward present) by `n` lines.
    pub fn scroll_view_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Reset scroll offset to show the live view.
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    /// Return the visible lines for rendering, accounting for scroll offset.
    /// When `scroll_offset == 0`, this returns the live screen buffer.
    /// When scrolled back, it blends scrollback history with the top of the screen.
    pub fn visible_lines(&self) -> Vec<&Vec<Cell>> {
        if self.scroll_offset == 0 {
            return self.cells.iter().collect();
        }

        let sb_len = self.scrollback.len();
        let offset = self.scroll_offset.min(sb_len);

        let mut lines: Vec<&Vec<Cell>> = Vec::with_capacity(self.rows);

        // How many scrollback lines are visible
        let sb_visible = offset.min(self.rows);
        let sb_start = sb_len.saturating_sub(offset);

        for line in self.scrollback.iter().skip(sb_start).take(sb_visible) {
            lines.push(line);
        }

        // Fill remaining rows from the live screen buffer
        let screen_lines = self.rows.saturating_sub(sb_visible);
        for line in self.cells.iter().take(screen_lines) {
            lines.push(line);
        }

        lines
    }
}

/// Initialize tab stops at every 8 columns.
fn init_tab_stops(cols: usize) -> Vec<bool> {
    let mut stops = vec![false; cols];
    for (i, stop) in stops.iter_mut().enumerate() {
        if i > 0 && i % 8 == 0 {
            *stop = true;
        }
    }
    stops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_region_basic() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.set_scroll_region(5, 10);
        assert_eq!(buf.scroll_top, 5);
        assert_eq!(buf.scroll_bottom, 10);
        // Cursor should be homed
        assert_eq!(buf.cursor.row, 0);
        assert_eq!(buf.cursor.col, 0);
    }

    #[test]
    fn test_scroll_region_newline_scrolls_within_region() {
        let mut buf = TerminalBuffer::new(10, 5);
        // Put identifiable content
        buf.set_cursor_pos(0, 0);
        buf.put_char('A');
        buf.set_cursor_pos(1, 0);
        buf.put_char('B');
        buf.set_cursor_pos(2, 0);
        buf.put_char('C');
        buf.set_cursor_pos(3, 0);
        buf.put_char('D');
        buf.set_cursor_pos(4, 0);
        buf.put_char('E');

        // Set scroll region to rows 1-3
        buf.set_scroll_region(1, 3);
        // Move cursor to bottom of scroll region
        buf.set_cursor_pos(3, 0);
        // Newline should scroll within region
        buf.newline();

        // Row 0 should still be 'A' (outside region)
        assert_eq!(buf.cells[0][0].c, 'A');
        // Row 1 should now be 'C' (was row 2)
        assert_eq!(buf.cells[1][0].c, 'C');
        // Row 4 should still be 'E' (outside region)
        assert_eq!(buf.cells[4][0].c, 'E');
    }

    #[test]
    fn test_cursor_save_restore() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.set_cursor_pos(5, 10);
        buf.set_fg(Color::new(255, 0, 0));
        buf.set_bold(true);
        buf.save_cursor();

        // Change cursor state
        buf.set_cursor_pos(0, 0);
        buf.set_fg(Color::new(0, 255, 0));
        buf.set_bold(false);

        buf.restore_cursor();
        assert_eq!(buf.cursor.row, 5);
        assert_eq!(buf.cursor.col, 10);
    }

    #[test]
    fn test_alternate_screen() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(0, 0);
        buf.put_char('X');

        buf.enter_alternate_screen();
        assert!(buf.in_alternate_screen);
        // Alternate screen should be blank
        assert_eq!(buf.cells[0][0].c, ' ');
        // Write something on alternate
        buf.put_char('Y');

        buf.exit_alternate_screen();
        assert!(!buf.in_alternate_screen);
        // Main screen should be restored
        assert_eq!(buf.cells[0][0].c, 'X');
    }

    #[test]
    fn test_alternate_screen_idempotent() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.enter_alternate_screen();
        buf.enter_alternate_screen(); // should be no-op
        assert!(buf.in_alternate_screen);
        buf.exit_alternate_screen();
        assert!(!buf.in_alternate_screen);
        buf.exit_alternate_screen(); // should be no-op
        assert!(!buf.in_alternate_screen);
    }

    #[test]
    fn test_insert_lines() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(1, 0);
        buf.put_char('A');
        buf.set_cursor_pos(2, 0);
        buf.put_char('B');

        buf.set_cursor_pos(1, 0);
        buf.insert_lines(1);

        // Row 1 should now be blank (inserted)
        assert_eq!(buf.cells[1][0].c, ' ');
        // 'A' should have moved down to row 2
        assert_eq!(buf.cells[2][0].c, 'A');
    }

    #[test]
    fn test_delete_lines() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(1, 0);
        buf.put_char('A');
        buf.set_cursor_pos(2, 0);
        buf.put_char('B');

        buf.set_cursor_pos(1, 0);
        buf.delete_lines(1);

        // Row 1 should now be 'B' (was row 2)
        assert_eq!(buf.cells[1][0].c, 'B');
    }

    #[test]
    fn test_insert_chars() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(0, 0);
        buf.put_char('A');
        buf.put_char('B');
        buf.put_char('C');

        buf.set_cursor_pos(0, 1);
        buf.insert_chars(1);

        assert_eq!(buf.cells[0][0].c, 'A');
        assert_eq!(buf.cells[0][1].c, ' '); // inserted blank
        assert_eq!(buf.cells[0][2].c, 'B'); // shifted right
        assert_eq!(buf.cells[0][3].c, 'C'); // shifted right
    }

    #[test]
    fn test_delete_chars() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(0, 0);
        buf.put_char('A');
        buf.put_char('B');
        buf.put_char('C');

        buf.set_cursor_pos(0, 0);
        buf.delete_chars(1);

        assert_eq!(buf.cells[0][0].c, 'B');
        assert_eq!(buf.cells[0][1].c, 'C');
    }

    #[test]
    fn test_tab_stops_default() {
        let buf = TerminalBuffer::new(80, 24);
        assert!(!buf.tab_stops[0]);
        assert!(buf.tab_stops[8]);
        assert!(buf.tab_stops[16]);
        assert!(!buf.tab_stops[7]);
    }

    #[test]
    fn test_tab_stop_next() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.cursor.col = 0;
        buf.next_tab_stop();
        assert_eq!(buf.cursor.col, 8);

        buf.next_tab_stop();
        assert_eq!(buf.cursor.col, 16);
    }

    #[test]
    fn test_tab_stop_set_clear() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.cursor.col = 5;
        buf.set_tab_stop();
        assert!(buf.tab_stops[5]);

        buf.cursor.col = 0;
        buf.next_tab_stop();
        assert_eq!(buf.cursor.col, 5);

        buf.clear_tab_stop();
        assert!(!buf.tab_stops[5]);
    }

    #[test]
    fn test_clear_all_tab_stops() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.clear_all_tab_stops();
        assert!(!buf.tab_stops[8]);
        assert!(!buf.tab_stops[16]);

        // next_tab_stop should go to end of line
        buf.cursor.col = 0;
        buf.next_tab_stop();
        assert_eq!(buf.cursor.col, 79);
    }

    #[test]
    fn test_scrollback_buffer() {
        let mut buf = TerminalBuffer::new(10, 3);
        buf.set_cursor_pos(0, 0);
        buf.put_char('A');
        buf.set_cursor_pos(1, 0);
        buf.put_char('B');
        buf.set_cursor_pos(2, 0);
        buf.put_char('C');

        // Trigger scroll
        buf.set_cursor_pos(2, 0);
        buf.newline();

        // Scrollback should have the first line
        assert_eq!(buf.scrollback.len(), 1);
        assert_eq!(buf.scrollback[0][0].c, 'A');
    }

    #[test]
    fn test_scrollback_max() {
        let mut buf = TerminalBuffer::new(5, 2);
        buf.scrollback_max = 3;

        // Scroll many times to exceed max
        for i in 0..5 {
            buf.set_cursor_pos(0, 0);
            buf.put_char(char::from(b'A' + i));
            buf.set_cursor_pos(1, 0);
            buf.newline();
        }

        assert!(buf.scrollback.len() <= 3);
    }

    #[test]
    fn test_insert_mode() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(0, 0);
        buf.put_char('A');
        buf.put_char('B');
        buf.put_char('C');

        buf.insert_mode = true;
        buf.set_cursor_pos(0, 1);
        buf.put_char('X');

        // 'X' should be inserted, pushing 'B' and 'C' right
        assert_eq!(buf.cells[0][0].c, 'A');
        assert_eq!(buf.cells[0][1].c, 'X');
        assert_eq!(buf.cells[0][2].c, 'B');
        assert_eq!(buf.cells[0][3].c, 'C');
    }

    #[test]
    fn test_window_title() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.set_title("My Terminal".to_string());
        assert_eq!(buf.title, "My Terminal");
    }

    // --- Edge case tests ---

    #[test]
    fn unicode_cjk_characters() {
        let mut buf = TerminalBuffer::new(20, 5);
        buf.set_cursor_pos(0, 0);
        // CJK characters (each typically 2 cells wide, but we store per-cell)
        for c in "日本語テスト".chars() {
            buf.put_char(c);
        }
        assert_eq!(buf.cells[0][0].c, '日');
        assert_eq!(buf.cells[0][1].c, '本');
        assert_eq!(buf.cells[0][5].c, 'ト');
    }

    #[test]
    fn emoji_in_buffer() {
        let mut buf = TerminalBuffer::new(20, 5);
        buf.set_cursor_pos(0, 0);
        buf.put_char('🚀');
        buf.put_char('✨');
        buf.put_char('🎉');
        assert_eq!(buf.cells[0][0].c, '🚀');
        assert_eq!(buf.cells[0][1].c, '✨');
        assert_eq!(buf.cells[0][2].c, '🎉');
    }

    #[test]
    fn rapid_resize_does_not_panic() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.set_cursor_pos(10, 40);
        buf.put_char('X');

        // Rapid resize sequence
        for size in [(10, 5), (200, 50), (1, 1), (80, 24), (3, 3)] {
            buf.resize(size.0, size.1);
            // Cursor should be clamped
            assert!(buf.cursor.col < buf.cols);
            assert!(buf.cursor.row < buf.rows);
        }
    }

    #[test]
    fn resize_to_minimum() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.set_cursor_pos(12, 60);
        buf.put_char('Z');
        buf.resize(1, 1);
        assert_eq!(buf.cols, 1);
        assert_eq!(buf.rows, 1);
        assert_eq!(buf.cursor.col, 0);
        assert_eq!(buf.cursor.row, 0);
    }

    #[test]
    fn high_speed_output_fills_scrollback() {
        let mut buf = TerminalBuffer::new(10, 3);
        buf.scrollback_max = 100;

        // Simulate rapid output: 500 lines
        for i in 0..500u32 {
            buf.set_cursor_pos(buf.rows - 1, 0);
            let c = char::from(b'A' + (i % 26) as u8);
            buf.put_char(c);
            buf.newline();
        }

        // Scrollback should be capped at max
        assert!(buf.scrollback.len() <= buf.scrollback_max);
        assert!(!buf.scrollback.is_empty());
    }

    #[test]
    fn put_char_wraps_at_end_of_line() {
        let mut buf = TerminalBuffer::new(5, 3);
        buf.set_cursor_pos(0, 0);
        for c in "ABCDE".chars() {
            buf.put_char(c);
        }
        // After 5 chars in 5-col buffer, cursor should wrap to next line
        assert_eq!(buf.cursor.row, 1);
        assert_eq!(buf.cursor.col, 0);
        assert_eq!(buf.cells[0][4].c, 'E');
    }

    #[test]
    fn scroll_view_beyond_scrollback() {
        let mut buf = TerminalBuffer::new(10, 3);
        buf.scrollback_max = 10;
        // Add a few lines to scrollback
        for _ in 0..5 {
            buf.set_cursor_pos(2, 0);
            buf.newline();
        }
        // Try to scroll way past available scrollback
        buf.scroll_view_up(1000);
        assert!(buf.scroll_offset <= buf.scrollback.len());
    }
}
