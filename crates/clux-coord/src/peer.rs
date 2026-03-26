use std::sync::Arc;

use crate::broker::Broker;
use crate::error::Result;
use crate::protocol::PeerInfo;

/// Maximum age (seconds) before a peer is considered stale.
const PEER_TIMEOUT_SECS: i64 = 120;

/// Manages peer lifecycle (registration, heartbeat, discovery).
pub struct PeerManager {
    broker: Arc<Broker>,
}

impl PeerManager {
    pub fn new(broker: Arc<Broker>) -> Self {
        Self { broker }
    }

    /// Register a new peer for the given pane.
    pub fn register(&self, peer_id: &str, pane_id: u64) -> Result<()> {
        self.broker.register_peer(peer_id, pane_id)
    }

    /// Unregister a peer when a pane is closed.
    pub fn unregister(&self, peer_id: &str) -> Result<()> {
        self.broker.unregister_peer(peer_id)
    }

    /// Update heartbeat for a peer.
    pub fn heartbeat(&self, peer_id: &str) -> Result<()> {
        self.broker.heartbeat(peer_id)
    }

    /// List active peers (within timeout window).
    pub fn list_active(&self) -> Result<Vec<PeerInfo>> {
        self.broker.list_peers(Some(PEER_TIMEOUT_SECS))
    }

    /// List all registered peers regardless of heartbeat.
    pub fn list_all(&self) -> Result<Vec<PeerInfo>> {
        self.broker.list_peers(None)
    }

    /// Set status text displayed in the status bar.
    pub fn set_status(&self, peer_id: &str, text: &str) -> Result<()> {
        self.broker.set_status(peer_id, text)
    }

    /// Set current working directory for a peer.
    pub fn set_cwd(&self, peer_id: &str, cwd: &str) -> Result<()> {
        self.broker.set_cwd(peer_id, cwd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager() -> PeerManager {
        let broker = Arc::new(Broker::in_memory().unwrap());
        PeerManager::new(broker)
    }

    #[test]
    fn register_and_list() {
        let pm = test_manager();
        pm.register("p1", 0).unwrap();
        pm.register("p2", 1).unwrap();
        let peers = pm.list_all().unwrap();
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn unregister_removes_peer() {
        let pm = test_manager();
        pm.register("p1", 0).unwrap();
        pm.unregister("p1").unwrap();
        assert!(pm.list_all().unwrap().is_empty());
    }

    #[test]
    fn status_update() {
        let pm = test_manager();
        pm.register("p1", 0).unwrap();
        pm.set_status("p1", "compiling").unwrap();
        let peers = pm.list_all().unwrap();
        assert_eq!(peers[0].status_text.as_deref(), Some("compiling"));
    }
}
