use crate::buffer::{Color, TerminalBuffer};
use tracing::debug;
use vte::{Params, Perform};

/// ANSI color palette (basic 8 + bright 8).
const ANSI_COLORS: [Color; 16] = [
    Color::new(0, 0, 0),       // 0: Black
    Color::new(170, 0, 0),     // 1: Red
    Color::new(0, 170, 0),     // 2: Green
    Color::new(170, 170, 0),   // 3: Yellow
    Color::new(0, 0, 170),     // 4: Blue
    Color::new(170, 0, 170),   // 5: Magenta
    Color::new(0, 170, 170),   // 6: Cyan
    Color::new(170, 170, 170), // 7: White
    Color::new(85, 85, 85),    // 8: Bright Black
    Color::new(255, 85, 85),   // 9: Bright Red
    Color::new(85, 255, 85),   // 10: Bright Green
    Color::new(255, 255, 85),  // 11: Bright Yellow
    Color::new(85, 85, 255),   // 12: Bright Blue
    Color::new(255, 85, 255),  // 13: Bright Magenta
    Color::new(85, 255, 255),  // 14: Bright Cyan
    Color::new(255, 255, 255), // 15: Bright White
];

/// Wraps a `TerminalBuffer` and implements the `vte::Perform` trait.
pub struct VtHandler<'a> {
    pub buffer: &'a mut TerminalBuffer,
}

impl<'a> VtHandler<'a> {
    pub fn new(buffer: &'a mut TerminalBuffer) -> Self {
        Self { buffer }
    }
}

impl Perform for VtHandler<'_> {
    fn print(&mut self, c: char) {
        self.buffer.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => self.buffer.backspace(), // BS
            0x09 => {
                // HT (tab) - use tab stop table
                self.buffer.next_tab_stop();
            }
            0x0A..=0x0C => {
                // LF, VT, FF
                self.buffer.newline();
            }
            0x0D => self.buffer.carriage_return(), // CR
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let params: Vec<u16> = params.iter().map(|p| p[0]).collect();

        // Handle DEC private mode sequences (intermediates contains '?')
        if intermediates.first() == Some(&b'?') {
            self.handle_dec_private_mode(&params, action);
            return;
        }

        match action {
            'A' => {
                // Cursor Up
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.buffer.cursor.row = self.buffer.cursor.row.saturating_sub(n);
            }
            'B' => {
                // Cursor Down
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.buffer.cursor.row = (self.buffer.cursor.row + n).min(self.buffer.rows - 1);
            }
            'C' => {
                // Cursor Forward
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.buffer.cursor.col = (self.buffer.cursor.col + n).min(self.buffer.cols - 1);
            }
            'D' => {
                // Cursor Back
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.buffer.cursor.col = self.buffer.cursor.col.saturating_sub(n);
            }
            'H' | 'f' => {
                // Cursor Position
                let row = params.first().copied().unwrap_or(1).max(1) as usize - 1;
                let col = params.get(1).copied().unwrap_or(1).max(1) as usize - 1;
                self.buffer.set_cursor_pos(row, col);
            }
            'J' => {
                // Erase in Display
                let mode = params.first().copied().unwrap_or(0);
                self.buffer.erase_in_display(mode);
            }
            'K' => {
                // Erase in Line
                let mode = params.first().copied().unwrap_or(0);
                self.buffer.erase_in_line(mode);
            }
            'L' => {
                // Insert Lines
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.buffer.insert_lines(n);
            }
            'M' => {
                // Delete Lines
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.buffer.delete_lines(n);
            }
            '@' => {
                // Insert Characters
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.buffer.insert_chars(n);
            }
            'P' => {
                // Delete Characters
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.buffer.delete_chars(n);
            }
            'g' => {
                // Clear Tab Stop
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.buffer.clear_tab_stop(),
                    3 => self.buffer.clear_all_tab_stops(),
                    _ => {}
                }
            }
            'r' => {
                // DECSTBM - Set Scroll Region
                let top = params.first().copied().unwrap_or(1).max(1) as usize - 1;
                let bottom = params
                    .get(1)
                    .copied()
                    .unwrap_or(self.buffer.rows as u16)
                    .max(1) as usize
                    - 1;
                self.buffer.set_scroll_region(top, bottom);
            }
            's' => {
                // Save Cursor Position
                self.buffer.save_cursor();
            }
            'u' => {
                // Restore Cursor Position
                self.buffer.restore_cursor();
            }
            'm' => self.handle_sgr(&params),
            _ => {
                debug!(action = %action, ?params, "Unhandled CSI sequence");
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => {
                // DECSC - Save Cursor
                self.buffer.save_cursor();
            }
            b'8' => {
                // DECRC - Restore Cursor
                self.buffer.restore_cursor();
            }
            b'H' => {
                // HTS - Set Tab Stop at current position
                self.buffer.set_tab_stop();
            }
            _ => {
                debug!(byte = byte, "Unhandled ESC dispatch");
            }
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        // OSC Ps ; Pt ST - params[0] is Ps, params[1] is Pt
        let ps = params[0];
        match ps {
            // OSC 0 / 1 / 2 - Set window title
            b"0" | b"1" | b"2" => {
                if let Some(title_bytes) = params.get(1)
                    && let Ok(title) = std::str::from_utf8(title_bytes)
                {
                    self.buffer.set_title(title.to_string());
                }
            }
            _ => {
                debug!(?ps, "Unhandled OSC sequence");
            }
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn unhook(&mut self) {}
    fn put(&mut self, _byte: u8) {}
}

impl VtHandler<'_> {
    /// Handle SGR (Select Graphic Rendition) sequences.
    fn handle_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.buffer.reset_attrs();
            return;
        }
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.buffer.reset_attrs(),
                1 => self.buffer.set_bold(true),
                3 => self.buffer.set_italic(true),
                4 => self.buffer.set_underline(true),
                7 => self.buffer.set_inverse(true),
                22 => self.buffer.set_bold(false),
                23 => self.buffer.set_italic(false),
                24 => self.buffer.set_underline(false),
                27 => self.buffer.set_inverse(false),
                // Standard foreground colors
                c @ 30..=37 => {
                    self.buffer.set_fg(ANSI_COLORS[(c - 30) as usize]);
                }
                39 => self.buffer.set_fg(self.buffer.default_fg),
                // Standard background colors
                c @ 40..=47 => {
                    self.buffer.set_bg(ANSI_COLORS[(c - 40) as usize]);
                }
                49 => self.buffer.set_bg(self.buffer.default_bg),
                // Bright foreground
                c @ 90..=97 => {
                    self.buffer.set_fg(ANSI_COLORS[(c - 90 + 8) as usize]);
                }
                // Bright background
                c @ 100..=107 => {
                    self.buffer.set_bg(ANSI_COLORS[(c - 100 + 8) as usize]);
                }
                // 256 color / TrueColor
                38 => {
                    if let Some(color) = parse_extended_color(&params[i..]) {
                        self.buffer.set_fg(color.0);
                        i += color.1;
                        continue;
                    }
                }
                48 => {
                    if let Some(color) = parse_extended_color(&params[i..]) {
                        self.buffer.set_bg(color.0);
                        i += color.1;
                        continue;
                    }
                }
                _ => {
                    debug!(param = params[i], "Unhandled SGR parameter");
                }
            }
            i += 1;
        }
    }

    /// Handle DEC private mode set/reset sequences (CSI ? Pm h/l).
    fn handle_dec_private_mode(&mut self, params: &[u16], action: char) {
        for &param in params {
            match (param, action) {
                (1049, 'h') => {
                    // Enable alternate screen buffer
                    self.buffer.save_cursor();
                    self.buffer.enter_alternate_screen();
                }
                (1049, 'l') => {
                    // Disable alternate screen buffer
                    self.buffer.exit_alternate_screen();
                    self.buffer.restore_cursor();
                }
                (4, 'h') => {
                    // Insert mode on
                    self.buffer.insert_mode = true;
                }
                (4, 'l') => {
                    // Insert mode off (replace mode)
                    self.buffer.insert_mode = false;
                }
                _ => {
                    debug!(param, action = %action, "Unhandled DEC private mode");
                }
            }
        }
    }
}

/// Parse 256-color (5;n) or `TrueColor` (2;r;g;b) sequences.
/// Returns (Color, `number_of_params_consumed`) or None.
fn parse_extended_color(params: &[u16]) -> Option<(Color, usize)> {
    if params.len() < 2 {
        return None;
    }
    match params[1] {
        5 if params.len() >= 3 => {
            // 256 color
            let idx = params[2] as usize;
            let color = if idx < 16 {
                ANSI_COLORS[idx]
            } else if idx < 232 {
                // 216 color cube
                let idx = idx - 16;
                let r = (idx / 36) * 51;
                let g = ((idx / 6) % 6) * 51;
                let b = (idx % 6) * 51;
                Color::new(r as u8, g as u8, b as u8)
            } else {
                // Grayscale
                let v = ((idx - 232) * 10 + 8) as u8;
                Color::new(v, v, v)
            };
            Some((color, 3))
        }
        2 if params.len() >= 5 => {
            // TrueColor
            let r = params[2] as u8;
            let g = params[3] as u8;
            let b = params[4] as u8;
            Some((Color::new(r, g, b), 5))
        }
        _ => None,
    }
}

/// Process raw bytes through the VT parser and update the buffer.
pub fn process_bytes(parser: &mut vte::Parser, buffer: &mut TerminalBuffer, data: &[u8]) {
    let mut handler = VtHandler::new(buffer);
    parser.advance(&mut handler, data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::TerminalBuffer;

    /// Helper: process an ANSI escape string through the VT parser.
    fn process(buf: &mut TerminalBuffer, data: &str) {
        let mut parser = vte::Parser::new();
        process_bytes(&mut parser, buf, data.as_bytes());
    }

    #[test]
    fn test_cursor_save_restore_csi() {
        let mut buf = TerminalBuffer::new(80, 24);
        // Move to (3,5), save, move to (0,0), restore
        process(&mut buf, "\x1b[4;6H"); // row 3, col 5 (1-indexed: 4;6)
        process(&mut buf, "\x1b[s"); // save
        process(&mut buf, "\x1b[1;1H"); // home
        assert_eq!(buf.cursor.row, 0);
        assert_eq!(buf.cursor.col, 0);
        process(&mut buf, "\x1b[u"); // restore
        assert_eq!(buf.cursor.row, 3);
        assert_eq!(buf.cursor.col, 5);
    }

    #[test]
    fn test_cursor_save_restore_esc() {
        let mut buf = TerminalBuffer::new(80, 24);
        process(&mut buf, "\x1b[4;6H"); // row 3, col 5
        process(&mut buf, "\x1b7"); // ESC 7 - save
        process(&mut buf, "\x1b[1;1H"); // home
        process(&mut buf, "\x1b8"); // ESC 8 - restore
        assert_eq!(buf.cursor.row, 3);
        assert_eq!(buf.cursor.col, 5);
    }

    #[test]
    fn test_scroll_region_decstbm() {
        let mut buf = TerminalBuffer::new(80, 24);
        process(&mut buf, "\x1b[5;20r"); // Set scroll region rows 5-20 (1-indexed)
        assert_eq!(buf.scroll_top, 4); // 0-indexed
        assert_eq!(buf.scroll_bottom, 19);
    }

    #[test]
    fn test_insert_delete_lines_via_csi() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(1, 0);
        buf.put_char('A');
        buf.set_cursor_pos(2, 0);
        buf.put_char('B');

        // Insert 1 line at row 1
        process(&mut buf, "\x1b[2;1H"); // move to row 1 (1-indexed: 2)
        process(&mut buf, "\x1b[L"); // insert line

        assert_eq!(buf.cells[1][0].c, ' '); // blank inserted line
        assert_eq!(buf.cells[2][0].c, 'A'); // pushed down
    }

    #[test]
    fn test_delete_lines_via_csi() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(1, 0);
        buf.put_char('A');
        buf.set_cursor_pos(2, 0);
        buf.put_char('B');

        process(&mut buf, "\x1b[2;1H"); // row 1
        process(&mut buf, "\x1b[M"); // delete line

        assert_eq!(buf.cells[1][0].c, 'B'); // row 2 moved up
    }

    #[test]
    fn test_insert_chars_via_csi() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(0, 0);
        buf.put_char('A');
        buf.put_char('B');
        buf.put_char('C');

        process(&mut buf, "\x1b[1;2H"); // row 0, col 1
        process(&mut buf, "\x1b[@"); // insert 1 char

        assert_eq!(buf.cells[0][0].c, 'A');
        assert_eq!(buf.cells[0][1].c, ' ');
        assert_eq!(buf.cells[0][2].c, 'B');
    }

    #[test]
    fn test_delete_chars_via_csi() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(0, 0);
        buf.put_char('A');
        buf.put_char('B');
        buf.put_char('C');

        process(&mut buf, "\x1b[1;1H"); // row 0, col 0
        process(&mut buf, "\x1b[P"); // delete 1 char

        assert_eq!(buf.cells[0][0].c, 'B');
        assert_eq!(buf.cells[0][1].c, 'C');
    }

    #[test]
    fn test_alternate_screen_csi() {
        let mut buf = TerminalBuffer::new(10, 5);
        buf.set_cursor_pos(0, 0);
        buf.put_char('M');

        process(&mut buf, "\x1b[?1049h"); // enter alternate screen
        assert!(buf.in_alternate_screen);
        assert_eq!(buf.cells[0][0].c, ' ');

        buf.put_char('A');

        process(&mut buf, "\x1b[?1049l"); // exit alternate screen
        assert!(!buf.in_alternate_screen);
        assert_eq!(buf.cells[0][0].c, 'M');
    }

    #[test]
    fn test_insert_mode_toggle() {
        let mut buf = TerminalBuffer::new(10, 5);
        assert!(!buf.insert_mode);
        process(&mut buf, "\x1b[?4h"); // insert mode on
        assert!(buf.insert_mode);
        process(&mut buf, "\x1b[?4l"); // insert mode off
        assert!(!buf.insert_mode);
    }

    #[test]
    fn test_tab_stop_esc_h() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.clear_all_tab_stops();
        buf.cursor.col = 5;
        process(&mut buf, "\x1bH"); // ESC H - set tab stop
        assert!(buf.tab_stops[5]);
    }

    #[test]
    fn test_clear_tab_stop_csi_g() {
        let mut buf = TerminalBuffer::new(80, 24);
        // Tab stop at 8 should exist by default
        assert!(buf.tab_stops[8]);

        buf.cursor.col = 8;
        process(&mut buf, "\x1b[g"); // clear tab stop at current
        assert!(!buf.tab_stops[8]);
    }

    #[test]
    fn test_clear_all_tab_stops_csi_g3() {
        let mut buf = TerminalBuffer::new(80, 24);
        assert!(buf.tab_stops[8]);
        process(&mut buf, "\x1b[3g"); // clear all tab stops
        assert!(!buf.tab_stops[8]);
        assert!(!buf.tab_stops[16]);
    }

    #[test]
    fn test_tab_execution_uses_tab_stops() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.cursor.col = 0;
        // Execute HT (0x09)
        process(&mut buf, "\t");
        assert_eq!(buf.cursor.col, 8);
    }

    #[test]
    fn test_osc_set_title() {
        let mut buf = TerminalBuffer::new(80, 24);
        // OSC 0 ; title BEL
        process(&mut buf, "\x1b]0;My Terminal\x07");
        assert_eq!(buf.title, "My Terminal");
    }

    #[test]
    fn test_osc_set_title_osc2() {
        let mut buf = TerminalBuffer::new(80, 24);
        // OSC 2 ; title ST
        process(&mut buf, "\x1b]2;Another Title\x1b\\");
        assert_eq!(buf.title, "Another Title");
    }
}
