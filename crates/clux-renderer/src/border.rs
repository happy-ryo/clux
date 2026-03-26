/// Renderer for pane borders and separators.
///
/// This is a stub implementation for Phase 1. Full border rendering
/// (colored rectangles between panes) will be implemented when the
/// pane management system is ready.
pub struct BorderRenderer {
    _private: (),
}

impl BorderRenderer {
    /// Create a new border renderer (currently a no-op).
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Render pane borders. Currently a no-op stub.
    pub fn render(&self, _render_pass: &mut wgpu::RenderPass<'_>) {
        // Will be implemented when pane splitting is added.
    }
}

impl Default for BorderRenderer {
    fn default() -> Self {
        Self::new()
    }
}
