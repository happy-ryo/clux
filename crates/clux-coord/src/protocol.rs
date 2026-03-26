use serde::{Deserialize, Serialize};

/// Unique identifier for a pane/peer in the coordination system.
pub type PeerId = String;

/// Information about a registered peer (Claude Code instance in a pane).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub pane_id: u64,
    pub cwd: Option<String>,
    pub status_text: Option<String>,
    pub last_heartbeat: i64,
}

/// A message sent between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerMessage {
    pub id: Option<i64>,
    pub from_peer: PeerId,
    pub to_peer: Option<PeerId>,
    pub body: serde_json::Value,
    pub timestamp: Option<i64>,
}

/// A task request that one agent can assign to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub id: Option<i64>,
    pub description: String,
    pub status: TaskStatus,
    pub requester: PeerId,
    pub assignee: Option<PeerId>,
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Assigned => write!(f, "assigned"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "assigned" => Ok(Self::Assigned),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("unknown task status: {s}")),
        }
    }
}

/// MCP JSON-RPC 2.0 request envelope.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// MCP JSON-RPC 2.0 response envelope.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// MCP tool description for the `tools/list` response.
#[derive(Debug, Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_status_roundtrip() {
        for status in [
            TaskStatus::Pending,
            TaskStatus::Assigned,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Failed,
        ] {
            let s = status.to_string();
            let parsed: TaskStatus = s.parse().unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn json_rpc_response_success_serialization() {
        let resp =
            JsonRpcResponse::success(serde_json::json!(1), serde_json::json!({"status": "ok"}));
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["result"]["status"], "ok");
        assert!(json.get("error").is_none());
    }

    #[test]
    fn json_rpc_response_error_serialization() {
        let resp = JsonRpcResponse::error(serde_json::json!(2), -32601, "Method not found");
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"]["code"], -32601);
        assert!(json.get("result").is_none());
    }

    #[test]
    fn peer_message_serialization() {
        let msg = PeerMessage {
            id: None,
            from_peer: "peer-1".into(),
            to_peer: Some("peer-2".into()),
            body: serde_json::json!({"text": "hello"}),
            timestamp: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: PeerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from_peer, "peer-1");
        assert_eq!(parsed.to_peer.as_deref(), Some("peer-2"));
    }
}
