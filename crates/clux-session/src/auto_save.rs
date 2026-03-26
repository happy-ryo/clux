use std::time::{Duration, Instant};

use tracing::debug;

/// Debounced auto-saver that coalesces rapid layout changes into a single
/// save operation after a configurable quiet period (default: 60 seconds).
///
/// Usage:
/// 1. Call [`AutoSaver::notify_change`] whenever a layout-affecting event
///    occurs (pane split, close, tab create/close).
/// 2. Call [`AutoSaver::poll_save`] in the render/event loop. When it returns
///    `true`, the caller should perform the actual save.
pub struct AutoSaver {
    debounce: Duration,
    dirty: bool,
    last_change: Option<Instant>,
}

impl AutoSaver {
    /// Create an `AutoSaver` with the given debounce duration.
    #[must_use]
    pub fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            dirty: false,
            last_change: None,
        }
    }

    /// Create an `AutoSaver` with the default 60-second debounce.
    #[must_use]
    pub fn default_debounce() -> Self {
        Self::new(Duration::from_secs(60))
    }

    /// Signal that a layout change has occurred.
    pub fn notify_change(&mut self) {
        self.dirty = true;
        self.last_change = Some(Instant::now());
        debug!("Auto-save change notified, timer reset");
    }

    /// Check whether enough time has passed since the last change to trigger
    /// a save. Returns `true` exactly once per debounce window, then resets.
    pub fn poll_save(&mut self) -> bool {
        if !self.dirty {
            return false;
        }

        if let Some(last) = self.last_change
            && last.elapsed() >= self.debounce
        {
            self.dirty = false;
            self.last_change = None;
            debug!("Auto-save debounce elapsed, triggering save");
            return true;
        }

        false
    }

    /// Force a save on the next `poll_save` call (e.g., before shutdown).
    pub fn force_save(&mut self) {
        if self.dirty {
            self.last_change = Instant::now().checked_sub(self.debounce);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_change_means_no_save() {
        let mut saver = AutoSaver::new(Duration::from_millis(10));
        assert!(!saver.poll_save());
    }

    #[test]
    fn save_triggers_after_debounce() {
        let mut saver = AutoSaver::new(Duration::from_millis(0));
        saver.notify_change();
        // With zero debounce, should trigger immediately.
        assert!(saver.poll_save());
        // Should not trigger again without a new change.
        assert!(!saver.poll_save());
    }

    #[test]
    fn save_does_not_trigger_before_debounce() {
        let mut saver = AutoSaver::new(Duration::from_secs(60));
        saver.notify_change();
        assert!(!saver.poll_save());
    }

    #[test]
    fn force_save_triggers_on_next_poll() {
        let mut saver = AutoSaver::new(Duration::from_secs(60));
        saver.notify_change();
        assert!(!saver.poll_save());
        saver.force_save();
        assert!(saver.poll_save());
    }

    #[test]
    fn multiple_changes_reset_timer() {
        let mut saver = AutoSaver::new(Duration::from_millis(0));
        saver.notify_change();
        // Reset with a long debounce-like behavior by re-notifying.
        saver.debounce = Duration::from_secs(60);
        saver.notify_change();
        assert!(!saver.poll_save());
    }
}
