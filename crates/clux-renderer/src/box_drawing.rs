//! Programmatic rendering of Unicode box-drawing characters (U+2500–U+257F).
//!
//! Instead of relying on font glyphs (which suffer from hinting artifacts and
//! fallback font metric mismatches), box-drawing characters are rendered as
//! geometric quads that perfectly align to the terminal cell grid.
//!
//! This approach is used by Alacritty, Kitty, `WezTerm`, and other major terminals.

use crate::cell_renderer::CellInstance;

/// Line thickness as a fraction of cell width.
const LINE_THICKNESS: f32 = 0.12;

/// Minimum line thickness in pixels.
const MIN_THICKNESS: f32 = 1.0;

/// Returns `true` if the character is a box-drawing character we handle.
pub fn is_box_drawing(c: char) -> bool {
    ('\u{2500}'..='\u{257F}').contains(&c)
}

/// Cell geometry for box-drawing calculations.
struct Cell {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    /// Line thickness
    thick: f32,
    /// Center X for vertical lines
    cx: f32,
    /// Center Y for horizontal lines
    cy: f32,
    /// Foreground color
    fg: [f32; 3],
}

impl Cell {
    #[allow(clippy::many_single_char_names)]
    fn new(x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, b: f32) -> Self {
        let thick = (w * LINE_THICKNESS).max(MIN_THICKNESS);
        Self {
            x,
            y,
            w,
            h,
            thick,
            cx: x + (w - thick) / 2.0,
            cy: y + (h - thick) / 2.0,
            fg: [r, g, b],
        }
    }

    fn quad(&self, qx: f32, qy: f32, qw: f32, qh: f32) -> CellInstance {
        CellInstance::background(qx, qy, qw, qh, self.fg[0], self.fg[1], self.fg[2])
    }

    /// Full horizontal line through cell center.
    fn hfull(&self) -> CellInstance {
        self.quad(self.x, self.cy, self.w, self.thick)
    }

    /// Full vertical line through cell center.
    fn vfull(&self) -> CellInstance {
        self.quad(self.cx, self.y, self.thick, self.h)
    }

    /// Horizontal line from center to right edge.
    fn hright(&self) -> CellInstance {
        self.quad(self.cx, self.cy, self.w - (self.cx - self.x), self.thick)
    }

    /// Horizontal line from left edge to center.
    fn hleft(&self) -> CellInstance {
        self.quad(self.x, self.cy, self.cx - self.x + self.thick, self.thick)
    }

    /// Vertical line from center to bottom edge.
    fn vdown(&self) -> CellInstance {
        self.quad(self.cx, self.cy, self.thick, self.h - (self.cy - self.y))
    }

    /// Vertical line from top edge to center.
    fn vup(&self) -> CellInstance {
        self.quad(self.cx, self.y, self.thick, self.cy - self.y + self.thick)
    }
}

/// Render a box-drawing character as `CellInstance` quads.
///
/// Returns an empty vec for unrecognized characters.
#[allow(clippy::too_many_arguments, clippy::many_single_char_names)]
pub fn render(
    c: char,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    g: f32,
    b: f32,
) -> Vec<CellInstance> {
    let cell = Cell::new(x, y, w, h, r, g, b);

    match c {
        // ─ Light Horizontal
        '\u{2500}' => vec![cell.hfull()],
        // ━ Heavy Horizontal
        '\u{2501}' => {
            let t2 = cell.thick * 2.0;
            vec![cell.quad(x, y + (h - t2) / 2.0, w, t2)]
        }
        // │ Light Vertical
        '\u{2502}' => vec![cell.vfull()],
        // ┃ Heavy Vertical
        '\u{2503}' => {
            let t2 = cell.thick * 2.0;
            vec![cell.quad(x + (w - t2) / 2.0, y, t2, h)]
        }
        // ┌╭ Down and Right (arc variant renders same with quads)
        '\u{250C}' | '\u{256D}' => vec![cell.hright(), cell.vdown()],
        // ┐╮ Down and Left
        '\u{2510}' | '\u{256E}' => vec![cell.hleft(), cell.vdown()],
        // └╰ Up and Right
        '\u{2514}' | '\u{2570}' => vec![cell.hright(), cell.vup()],
        // ┘╯ Up and Left
        '\u{2518}' | '\u{256F}' => vec![cell.hleft(), cell.vup()],
        // ├ Vertical and Right
        '\u{251C}' => vec![cell.vfull(), cell.hright()],
        // ┤ Vertical and Left
        '\u{2524}' => vec![cell.vfull(), cell.hleft()],
        // ┬ Down and Horizontal
        '\u{252C}' => vec![cell.hfull(), cell.vdown()],
        // ┴ Up and Horizontal
        '\u{2534}' => vec![cell.hfull(), cell.vup()],
        // ┼ Vertical and Horizontal
        '\u{253C}' => vec![cell.hfull(), cell.vfull()],
        // ═ Double Horizontal
        '\u{2550}' => {
            let gap = cell.thick;
            vec![
                cell.quad(x, cell.cy - gap, w, cell.thick),
                cell.quad(x, cell.cy + gap, w, cell.thick),
            ]
        }
        // ║ Double Vertical
        '\u{2551}' => {
            let gap = cell.thick;
            vec![
                cell.quad(cell.cx - gap, y, cell.thick, h),
                cell.quad(cell.cx + gap, y, cell.thick, h),
            ]
        }
        // ╴ Light Left (half)
        '\u{2574}' => vec![cell.quad(x, cell.cy, w / 2.0, cell.thick)],
        // ╵ Light Up (half)
        '\u{2575}' => vec![cell.quad(cell.cx, y, cell.thick, h / 2.0)],
        // ╶ Light Right (half)
        '\u{2576}' => vec![cell.quad(x + w / 2.0, cell.cy, w / 2.0, cell.thick)],
        // ╷ Light Down (half)
        '\u{2577}' => vec![cell.quad(cell.cx, y + h / 2.0, cell.thick, h / 2.0)],

        // Unhandled: fall through to font glyph
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_box_drawing_range() {
        assert!(is_box_drawing('─'));
        assert!(is_box_drawing('│'));
        assert!(is_box_drawing('╭'));
        assert!(is_box_drawing('╯'));
        assert!(!is_box_drawing('A'));
        assert!(!is_box_drawing(' '));
    }

    #[test]
    fn horizontal_line_produces_one_quad() {
        let quads = render('─', 0.0, 0.0, 10.0, 20.0, 1.0, 1.0, 1.0);
        assert_eq!(quads.len(), 1);
    }

    #[test]
    fn corner_produces_two_quads() {
        let quads = render('┌', 0.0, 0.0, 10.0, 20.0, 1.0, 1.0, 1.0);
        assert_eq!(quads.len(), 2);
    }

    #[test]
    fn cross_produces_two_quads() {
        let quads = render('┼', 0.0, 0.0, 10.0, 20.0, 1.0, 1.0, 1.0);
        assert_eq!(quads.len(), 2);
    }

    #[test]
    fn unknown_box_char_returns_empty() {
        // U+2504 is box drawings light triple dash horizontal - not handled
        let quads = render('\u{2504}', 0.0, 0.0, 10.0, 20.0, 1.0, 1.0, 1.0);
        assert!(quads.is_empty());
    }

    #[test]
    fn arc_chars_produce_quads() {
        for c in ['╭', '╮', '╯', '╰'] {
            let quads = render(c, 0.0, 0.0, 10.0, 20.0, 1.0, 1.0, 1.0);
            assert_eq!(quads.len(), 2, "Expected 2 quads for {c}");
        }
    }
}
