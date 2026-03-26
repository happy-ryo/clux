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
}

impl TerminalBuffer {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
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
    }

    pub fn put_char(&mut self, c: char) {
        if self.cursor.col < self.cols && self.cursor.row < self.rows {
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
        if self.cursor.row + 1 >= self.rows {
            self.scroll_up();
        } else {
            self.cursor.row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
    }

    pub fn backspace(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    fn scroll_up(&mut self) {
        self.cells.remove(0);
        self.cells.push(vec![Cell::default(); self.cols]);
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
}
