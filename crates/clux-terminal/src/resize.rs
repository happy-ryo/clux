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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn poll_returns_none_before_threshold() {
        let mut d = ResizeDebouncer::new(100);
        d.request(80, 24);
        assert!(d.poll().is_none());
        assert!(d.has_pending());
    }

    #[test]
    fn poll_returns_size_after_threshold() {
        let mut d = ResizeDebouncer::new(10);
        d.request(120, 40);
        thread::sleep(Duration::from_millis(20));
        assert_eq!(d.poll(), Some((120, 40)));
        assert!(!d.has_pending());
    }

    #[test]
    fn later_request_replaces_earlier() {
        let mut d = ResizeDebouncer::new(10);
        d.request(80, 24);
        d.request(120, 40);
        thread::sleep(Duration::from_millis(20));
        assert_eq!(d.poll(), Some((120, 40)));
    }

    #[test]
    fn poll_after_consumed_returns_none() {
        let mut d = ResizeDebouncer::new(10);
        d.request(80, 24);
        thread::sleep(Duration::from_millis(20));
        assert!(d.poll().is_some());
        assert!(d.poll().is_none());
    }
}
