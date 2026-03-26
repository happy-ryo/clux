use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tracing::info;

use crate::error::{CoordError, Result};
use crate::protocol::{PeerInfo, PeerMessage, TaskRequest, TaskStatus};

/// SQLite-backed message broker for peer coordination.
pub struct Broker {
    conn: Mutex<Connection>,
}

impl Broker {
    /// Create a new broker with the database at the given path.
    pub fn open(db_path: &PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        let broker = Self {
            conn: Mutex::new(conn),
        };
        broker.init_schema()?;
        info!(?db_path, "Coordination broker initialized");
        Ok(broker)
    }

    /// Create an in-memory broker (useful for testing).
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let broker = Self {
            conn: Mutex::new(conn),
        };
        broker.init_schema()?;
        Ok(broker)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS peers (
                peer_id    TEXT PRIMARY KEY,
                pane_id    INTEGER NOT NULL,
                cwd        TEXT,
                status_text TEXT,
                last_heartbeat INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );

            CREATE TABLE IF NOT EXISTS messages (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                from_peer  TEXT NOT NULL,
                to_peer    TEXT,
                body       TEXT NOT NULL,
                timestamp  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );

            CREATE INDEX IF NOT EXISTS idx_messages_to_peer
                ON messages(to_peer);

            CREATE TABLE IF NOT EXISTS tasks (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                description TEXT NOT NULL,
                status      TEXT NOT NULL DEFAULT 'pending',
                requester   TEXT NOT NULL,
                assignee    TEXT,
                created_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );",
        )?;
        Ok(())
    }

    // --- Peer operations ---

    /// Register or update a peer.
    pub fn register_peer(&self, peer_id: &str, pane_id: u64) -> Result<()> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        conn.execute(
            "INSERT INTO peers (peer_id, pane_id, last_heartbeat)
             VALUES (?1, ?2, strftime('%s', 'now'))
             ON CONFLICT(peer_id) DO UPDATE SET
                pane_id = excluded.pane_id,
                last_heartbeat = strftime('%s', 'now')",
            rusqlite::params![peer_id, pane_id],
        )?;
        info!(peer_id, pane_id, "Peer registered");
        Ok(())
    }

    /// Remove a peer.
    pub fn unregister_peer(&self, peer_id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        conn.execute("DELETE FROM peers WHERE peer_id = ?1", [peer_id])?;
        info!(peer_id, "Peer unregistered");
        Ok(())
    }

    /// Update heartbeat timestamp for a peer.
    pub fn heartbeat(&self, peer_id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        conn.execute(
            "UPDATE peers SET last_heartbeat = strftime('%s', 'now') WHERE peer_id = ?1",
            [peer_id],
        )?;
        Ok(())
    }

    /// List all peers (optionally filter to those active within `max_age_secs`).
    pub fn list_peers(&self, max_age_secs: Option<i64>) -> Result<Vec<PeerInfo>> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        let mut stmt = if let Some(age) = max_age_secs {
            let mut s = conn.prepare(
                "SELECT peer_id, pane_id, cwd, status_text, last_heartbeat
                 FROM peers
                 WHERE last_heartbeat >= strftime('%s', 'now') - ?1
                 ORDER BY pane_id",
            )?;
            let rows = s
                .query_map([age], map_peer_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            return Ok(rows);
        } else {
            conn.prepare(
                "SELECT peer_id, pane_id, cwd, status_text, last_heartbeat
                 FROM peers ORDER BY pane_id",
            )?
        };
        let rows = stmt
            .query_map([], map_peer_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Set status text for a peer.
    pub fn set_status(&self, peer_id: &str, text: &str) -> Result<()> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        let updated = conn.execute(
            "UPDATE peers SET status_text = ?2, last_heartbeat = strftime('%s', 'now')
             WHERE peer_id = ?1",
            rusqlite::params![peer_id, text],
        )?;
        if updated == 0 {
            return Err(CoordError::PeerNotFound(peer_id.to_string()));
        }
        Ok(())
    }

    /// Set CWD for a peer.
    pub fn set_cwd(&self, peer_id: &str, cwd: &str) -> Result<()> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        conn.execute(
            "UPDATE peers SET cwd = ?2 WHERE peer_id = ?1",
            rusqlite::params![peer_id, cwd],
        )?;
        Ok(())
    }

    // --- Message operations ---

    /// Send a message to a specific peer (or broadcast if `to_peer` is `None`).
    pub fn send_message(&self, msg: &PeerMessage) -> Result<i64> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        let body_json = serde_json::to_string(&msg.body)?;
        conn.execute(
            "INSERT INTO messages (from_peer, to_peer, body)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![msg.from_peer, msg.to_peer, body_json],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Read messages for a peer, optionally since a given message ID.
    pub fn read_messages(&self, peer_id: &str, since_id: Option<i64>) -> Result<Vec<PeerMessage>> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        let since = since_id.unwrap_or(0);
        let mut stmt = conn.prepare(
            "SELECT id, from_peer, to_peer, body, timestamp
             FROM messages
             WHERE (to_peer = ?1 OR to_peer IS NULL)
               AND id > ?2
             ORDER BY id",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![peer_id, since], |row| {
                let body_str: String = row.get(3)?;
                let body: serde_json::Value =
                    serde_json::from_str(&body_str).unwrap_or(serde_json::Value::Null);
                Ok(PeerMessage {
                    id: Some(row.get(0)?),
                    from_peer: row.get(1)?,
                    to_peer: row.get(2)?,
                    body,
                    timestamp: Some(row.get(4)?),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Broadcast a message to all peers.
    pub fn broadcast(&self, from_peer: &str, body: &serde_json::Value) -> Result<i64> {
        let msg = PeerMessage {
            id: None,
            from_peer: from_peer.to_string(),
            to_peer: None,
            body: body.clone(),
            timestamp: None,
        };
        self.send_message(&msg)
    }

    // --- Task operations ---

    /// Create a new task request.
    pub fn create_task(&self, task: &TaskRequest) -> Result<i64> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        conn.execute(
            "INSERT INTO tasks (description, status, requester, assignee)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                task.description,
                task.status.to_string(),
                task.requester,
                task.assignee,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// List tasks, optionally filtered by assignee.
    pub fn list_tasks(&self, assignee: Option<&str>) -> Result<Vec<TaskRequest>> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        let mut tasks = Vec::new();
        if let Some(assignee) = assignee {
            let mut stmt = conn.prepare(
                "SELECT id, description, status, requester, assignee, created_at
                 FROM tasks WHERE assignee = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map([assignee], map_task_row)?;
            for row in rows {
                tasks.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, description, status, requester, assignee, created_at
                 FROM tasks ORDER BY id",
            )?;
            let rows = stmt.query_map([], map_task_row)?;
            for row in rows {
                tasks.push(row?);
            }
        }
        Ok(tasks)
    }

    /// Update task status.
    pub fn update_task_status(&self, task_id: i64, status: TaskStatus) -> Result<()> {
        let conn = self.conn.lock().expect("broker lock poisoned");
        conn.execute(
            "UPDATE tasks SET status = ?2 WHERE id = ?1",
            rusqlite::params![task_id, status.to_string()],
        )?;
        Ok(())
    }
}

fn map_peer_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PeerInfo> {
    Ok(PeerInfo {
        peer_id: row.get(0)?,
        pane_id: row.get(1)?,
        cwd: row.get(2)?,
        status_text: row.get(3)?,
        last_heartbeat: row.get(4)?,
    })
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRequest> {
    let status_str: String = row.get(2)?;
    let status = status_str.parse().unwrap_or(TaskStatus::Pending);
    Ok(TaskRequest {
        id: Some(row.get(0)?),
        description: row.get(1)?,
        status,
        requester: row.get(3)?,
        assignee: row.get(4)?,
        created_at: Some(row.get(5)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_broker() -> Broker {
        Broker::in_memory().unwrap()
    }

    #[test]
    fn register_and_list_peers() {
        let broker = test_broker();
        broker.register_peer("peer-1", 0).unwrap();
        broker.register_peer("peer-2", 1).unwrap();
        let peers = broker.list_peers(None).unwrap();
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].peer_id, "peer-1");
        assert_eq!(peers[1].pane_id, 1);
    }

    #[test]
    fn unregister_peer() {
        let broker = test_broker();
        broker.register_peer("peer-1", 0).unwrap();
        broker.unregister_peer("peer-1").unwrap();
        let peers = broker.list_peers(None).unwrap();
        assert!(peers.is_empty());
    }

    #[test]
    fn set_and_read_status() {
        let broker = test_broker();
        broker.register_peer("peer-1", 0).unwrap();
        broker.set_status("peer-1", "working on tests").unwrap();
        let peers = broker.list_peers(None).unwrap();
        assert_eq!(peers[0].status_text.as_deref(), Some("working on tests"));
    }

    #[test]
    fn set_status_unknown_peer() {
        let broker = test_broker();
        let result = broker.set_status("unknown", "test");
        assert!(result.is_err());
    }

    #[test]
    fn send_and_read_messages() {
        let broker = test_broker();
        broker.register_peer("a", 0).unwrap();
        broker.register_peer("b", 1).unwrap();

        let msg = PeerMessage {
            id: None,
            from_peer: "a".into(),
            to_peer: Some("b".into()),
            body: serde_json::json!({"text": "hello"}),
            timestamp: None,
        };
        broker.send_message(&msg).unwrap();

        let msgs = broker.read_messages("b", None).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].from_peer, "a");
        assert_eq!(msgs[0].body["text"], "hello");

        // Peer "a" should not see messages to "b"
        let msgs_a = broker.read_messages("a", None).unwrap();
        assert!(msgs_a.is_empty());
    }

    #[test]
    fn broadcast_reaches_all() {
        let broker = test_broker();
        broker.register_peer("a", 0).unwrap();
        broker.register_peer("b", 1).unwrap();

        broker
            .broadcast("a", &serde_json::json!({"text": "broadcast"}))
            .unwrap();

        // Broadcast (to_peer IS NULL) should be visible to both
        let msgs_a = broker.read_messages("a", None).unwrap();
        let msgs_b = broker.read_messages("b", None).unwrap();
        assert_eq!(msgs_a.len(), 1);
        assert_eq!(msgs_b.len(), 1);
    }

    #[test]
    fn read_messages_since_id() {
        let broker = test_broker();
        broker.register_peer("a", 0).unwrap();

        let id1 = broker.broadcast("a", &serde_json::json!({"n": 1})).unwrap();
        broker.broadcast("a", &serde_json::json!({"n": 2})).unwrap();

        let msgs = broker.read_messages("a", Some(id1)).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].body["n"], 2);
    }

    #[test]
    fn create_and_list_tasks() {
        let broker = test_broker();
        broker.register_peer("a", 0).unwrap();

        let task = TaskRequest {
            id: None,
            description: "run tests".into(),
            status: TaskStatus::Pending,
            requester: "a".into(),
            assignee: Some("b".into()),
            created_at: None,
        };
        let task_id = broker.create_task(&task).unwrap();
        assert!(task_id > 0);

        let all = broker.list_tasks(None).unwrap();
        assert_eq!(all.len(), 1);

        let for_b = broker.list_tasks(Some("b")).unwrap();
        assert_eq!(for_b.len(), 1);

        let for_c = broker.list_tasks(Some("c")).unwrap();
        assert!(for_c.is_empty());
    }

    #[test]
    fn update_task_status() {
        let broker = test_broker();
        let task = TaskRequest {
            id: None,
            description: "build".into(),
            status: TaskStatus::Pending,
            requester: "a".into(),
            assignee: None,
            created_at: None,
        };
        let id = broker.create_task(&task).unwrap();
        broker
            .update_task_status(id, TaskStatus::Completed)
            .unwrap();

        let tasks = broker.list_tasks(None).unwrap();
        assert_eq!(tasks[0].status, TaskStatus::Completed);
    }
}
