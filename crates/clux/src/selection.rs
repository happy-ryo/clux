/// A cell coordinate in the terminal buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellCoord {
    pub col: usize,
    pub row: usize,
}

/// Represents a text selection across terminal buffer cells.
#[derive(Debug, Clone)]
pub struct Selection {
    /// Where the mouse was first pressed.
    pub anchor: CellCoord,
    /// Current end of the selection (follows the cursor).
    pub end: CellCoord,
}

impl Selection {
    pub fn new(anchor: CellCoord) -> Self {
        Self {
            anchor,
            end: anchor,
        }
    }

    /// Return the selection bounds in normalized order (start <= end).
    pub fn ordered(&self) -> (CellCoord, CellCoord) {
        if self.anchor.row < self.end.row
            || (self.anchor.row == self.end.row && self.anchor.col <= self.end.col)
        {
            (self.anchor, self.end)
        } else {
            (self.end, self.anchor)
        }
    }

    /// Return `true` if the selection covers zero or more characters.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.end
    }
}

/// Convert pixel coordinates to buffer cell coordinates.
pub fn pixel_to_cell(
    px: f64,
    py: f64,
    pane_x: f32,
    pane_y: f32,
    cell_width: f32,
    cell_height: f32,
    scale_factor: f64,
) -> CellCoord {
    let scaled_cell_w = f64::from(cell_width) * scale_factor;
    let scaled_cell_h = f64::from(cell_height) * scale_factor;

    let local_x = px - f64::from(pane_x);
    let local_y = py - f64::from(pane_y);

    let col = if scaled_cell_w > 0.0 {
        (local_x / scaled_cell_w).floor().max(0.0) as usize
    } else {
        0
    };
    let row = if scaled_cell_h > 0.0 {
        (local_y / scaled_cell_h).floor().max(0.0) as usize
    } else {
        0
    };

    CellCoord { col, row }
}

/// Extract selected text from the buffer cells.
pub fn extract_text(
    cells: &[Vec<clux_terminal::buffer::Cell>],
    start: CellCoord,
    end: CellCoord,
    cols: usize,
) -> String {
    let mut result = String::new();

    if start.row == end.row {
        // Single-line selection
        if start.row < cells.len() {
            let row = &cells[start.row];
            let from = start.col.min(cols);
            let to = (end.col + 1).min(cols);
            for cell in row.iter().take(to).skip(from) {
                result.push(cell.c);
            }
        }
        return trim_trailing_spaces(&result);
    }

    // Multi-line selection
    // First line: from start.col to end of line
    if start.row < cells.len() {
        let row = &cells[start.row];
        let from = start.col.min(cols);
        for cell in row.iter().skip(from) {
            result.push(cell.c);
        }
        result = trim_trailing_spaces(&result);
        result.push('\n');
    }

    // Middle lines: full lines
    for r in (start.row + 1)..end.row {
        if r < cells.len() {
            let line: String = cells[r].iter().map(|c| c.c).collect();
            result.push_str(trim_trailing_spaces(&line).as_str());
            result.push('\n');
        }
    }

    // Last line: from start of line to end.col
    if end.row < cells.len() {
        let row = &cells[end.row];
        let to = (end.col + 1).min(cols);
        let line: String = row.iter().take(to).map(|c| c.c).collect();
        result.push_str(trim_trailing_spaces(&line).as_str());
    }

    result
}

fn trim_trailing_spaces(s: &str) -> String {
    s.trim_end_matches(' ').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_ordered_forward() {
        let sel = Selection {
            anchor: CellCoord { col: 0, row: 0 },
            end: CellCoord { col: 5, row: 2 },
        };
        let (start, end) = sel.ordered();
        assert_eq!(start, CellCoord { col: 0, row: 0 });
        assert_eq!(end, CellCoord { col: 5, row: 2 });
    }

    #[test]
    fn selection_ordered_backward() {
        let sel = Selection {
            anchor: CellCoord { col: 5, row: 2 },
            end: CellCoord { col: 0, row: 0 },
        };
        let (start, end) = sel.ordered();
        assert_eq!(start, CellCoord { col: 0, row: 0 });
        assert_eq!(end, CellCoord { col: 5, row: 2 });
    }

    #[test]
    fn pixel_to_cell_basic() {
        let coord = pixel_to_cell(80.0, 48.0, 0.0, 0.0, 8.0, 16.0, 1.0);
        assert_eq!(coord.col, 10);
        assert_eq!(coord.row, 3);
    }

    #[test]
    fn extract_text_single_line() {
        let cells = vec![vec![
            clux_terminal::buffer::Cell {
                c: 'H',
                ..Default::default()
            },
            clux_terminal::buffer::Cell {
                c: 'i',
                ..Default::default()
            },
            clux_terminal::buffer::Cell {
                c: ' ',
                ..Default::default()
            },
        ]];
        let text = extract_text(
            &cells,
            CellCoord { col: 0, row: 0 },
            CellCoord { col: 1, row: 0 },
            3,
        );
        assert_eq!(text, "Hi");
    }
}
