//! Calculates terminal dimensions (columns and rows) from pixel dimensions
//! and cell size, accounting for DPI scale factor.

/// Default cell width in pixels at 1.0 scale factor.
pub const DEFAULT_CELL_WIDTH: f32 = 8.0;

/// Default cell height in pixels at 1.0 scale factor.
pub const DEFAULT_CELL_HEIGHT: f32 = 16.0;

/// Convert pixel dimensions to terminal columns and rows.
///
/// The `scale_factor` is the DPI scale (e.g. 1.0 for 96 DPI, 1.5 for 144 DPI).
/// Cell dimensions are specified in logical pixels and scaled by the factor.
///
/// Returns `(cols, rows)`, each guaranteed to be at least 1.
#[must_use]
pub fn pixel_size_to_terminal_size(
    width_px: u32,
    height_px: u32,
    cell_width: f32,
    cell_height: f32,
    scale_factor: f64,
) -> (u16, u16) {
    let scaled_cell_w = cell_width * scale_factor as f32;
    let scaled_cell_h = cell_height * scale_factor as f32;

    // Avoid division by zero
    if scaled_cell_w <= 0.0 || scaled_cell_h <= 0.0 {
        return (1, 1);
    }

    let cols = (width_px as f32 / scaled_cell_w).floor() as u16;
    let rows = (height_px as f32 / scaled_cell_h).floor() as u16;

    (cols.max(1), rows.max(1))
}

/// Compute scaled cell dimensions given a base cell size and DPI scale factor.
///
/// Returns `(cell_width_px, cell_height_px)` in physical pixels.
#[must_use]
pub fn scaled_cell_dimensions(cell_width: f32, cell_height: f32, scale_factor: f64) -> (f32, f32) {
    (
        cell_width * scale_factor as f32,
        cell_height * scale_factor as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_terminal_size_calculation() {
        // 800x600 window with 8x16 cells at 1.0 scale
        let (cols, rows) = pixel_size_to_terminal_size(800, 600, 8.0, 16.0, 1.0);
        assert_eq!(cols, 100);
        assert_eq!(rows, 37);
    }

    #[test]
    fn dpi_scale_factor_applied() {
        // 1600x1200 physical pixels with 8x16 logical cells at 2.0 scale
        // Scaled cell: 16x32, so 1600/16=100 cols, 1200/32=37 rows
        let (cols, rows) = pixel_size_to_terminal_size(1600, 1200, 8.0, 16.0, 2.0);
        assert_eq!(cols, 100);
        assert_eq!(rows, 37);
    }

    #[test]
    fn minimum_size_enforced() {
        // Very small window should still give at least 1x1
        let (cols, rows) = pixel_size_to_terminal_size(1, 1, 8.0, 16.0, 1.0);
        assert_eq!(cols, 1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn zero_size_gives_minimum() {
        let (cols, rows) = pixel_size_to_terminal_size(0, 0, 8.0, 16.0, 1.0);
        assert_eq!(cols, 1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn zero_cell_size_gives_minimum() {
        let (cols, rows) = pixel_size_to_terminal_size(800, 600, 0.0, 0.0, 1.0);
        assert_eq!(cols, 1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn fractional_scale_factor() {
        // 1200x900 physical pixels with 8x16 logical cells at 1.5 scale
        // Scaled cell: 12x24, so 1200/12=100 cols, 900/24=37 rows
        let (cols, rows) = pixel_size_to_terminal_size(1200, 900, 8.0, 16.0, 1.5);
        assert_eq!(cols, 100);
        assert_eq!(rows, 37);
    }

    #[test]
    fn non_divisible_size_floors() {
        // 805x605 with 8x16 cells at 1.0 scale
        // 805/8 = 100.625 -> 100 cols, 605/16 = 37.8125 -> 37 rows
        let (cols, rows) = pixel_size_to_terminal_size(805, 605, 8.0, 16.0, 1.0);
        assert_eq!(cols, 100);
        assert_eq!(rows, 37);
    }

    #[test]
    fn scaled_cell_dimensions_at_1x() {
        let (w, h) = scaled_cell_dimensions(8.0, 16.0, 1.0);
        assert!((w - 8.0).abs() < f32::EPSILON);
        assert!((h - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scaled_cell_dimensions_at_2x() {
        let (w, h) = scaled_cell_dimensions(8.0, 16.0, 2.0);
        assert!((w - 16.0).abs() < f32::EPSILON);
        assert!((h - 32.0).abs() < f32::EPSILON);
    }
}
