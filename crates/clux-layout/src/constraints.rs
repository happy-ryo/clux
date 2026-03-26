use crate::tree::Direction;

pub const MIN_PANE_COLS: u16 = 20;
pub const MIN_PANE_ROWS: u16 = 5;

#[must_use]
pub fn min_pane_width(cell_width: f32) -> f32 {
    f32::from(MIN_PANE_COLS) * cell_width
}

#[must_use]
pub fn min_pane_height(cell_height: f32) -> f32 {
    f32::from(MIN_PANE_ROWS) * cell_height
}

/// Clamp a split ratio so that both children meet minimum size requirements.
#[must_use]
pub fn clamp_ratio(ratio: f32, direction: Direction, viewport_size: f32, min_size: f32) -> f32 {
    let _ = direction; // direction is used to document intent; clamping logic is the same
    let min_ratio = min_size / viewport_size;
    let max_ratio = 1.0 - min_ratio;
    if min_ratio >= max_ratio {
        return 0.5; // viewport too small, just split evenly
    }
    ratio.clamp(min_ratio, max_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_ratio_within_range() {
        let result = clamp_ratio(0.5, Direction::Vertical, 800.0, 100.0);
        assert!((result - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn clamp_ratio_below_min() {
        let result = clamp_ratio(0.05, Direction::Vertical, 800.0, 100.0);
        assert!((result - 0.125).abs() < f32::EPSILON); // min_ratio = 100/800 = 0.125
    }

    #[test]
    fn clamp_ratio_tiny_viewport() {
        let result = clamp_ratio(0.5, Direction::Horizontal, 100.0, 60.0);
        assert!((result - 0.5).abs() < f32::EPSILON); // min_ratio=0.6 >= max_ratio=0.4
    }
}
