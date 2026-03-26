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
                // HT (tab)
                let next_tab = (self.buffer.cursor.col + 8) & !7;
                self.buffer.cursor.col = next_tab.min(self.buffer.cols - 1);
            }
            0x0A..=0x0C => {
                // LF, VT, FF
                self.buffer.newline();
            }
            0x0D => self.buffer.carriage_return(), // CR
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let params: Vec<u16> = params.iter().map(|p| p[0]).collect();

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
            'm' => {
                // SGR - Select Graphic Rendition
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
            _ => {
                debug!(action = %action, ?params, "Unhandled CSI sequence");
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn unhook(&mut self) {}
    fn put(&mut self, _byte: u8) {}
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
