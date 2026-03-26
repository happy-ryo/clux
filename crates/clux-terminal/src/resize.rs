use std::time::{Duration, Instant};

/// Manages debounced resize operations for `ConPTY`.
///
/// `ConPTY` resize is asynchronous and can produce garbled output
/// during transitions. This debouncer ensures we only send a resize
/// after the user has stopped resizing for a threshold period.
pub struct ResizeDebouncer {
    threshold: Duration,
    pending: Option<PendingResize>,
}

struct PendingResize {
    cols: u16,
    rows: u16,
    requested_at: Instant,
}

impl ResizeDebouncer {
    pub fn new(threshold_ms: u64) -> Self {
        Self {
            threshold: Duration::from_millis(threshold_ms),
            pending: None,
        }
    }

    /// Request a resize. Returns None immediately; call `poll` to check
    /// if the debounce period has elapsed.
    pub fn request(&mut self, cols: u16, rows: u16) {
        self.pending = Some(PendingResize {
            cols,
            rows,
            requested_at: Instant::now(),
        });
    }

    /// Check if a pending resize is ready to be applied.
    /// Returns Some((cols, rows)) if the debounce period has elapsed.
    pub fn poll(&mut self) -> Option<(u16, u16)> {
        if let Some(ref pending) = self.pending
            && pending.requested_at.elapsed() >= self.threshold
        {
            let result = (pending.cols, pending.rows);
            self.pending = None;
            return Some(result);
        }
        None
    }

    /// Check if there is a pending resize.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}
