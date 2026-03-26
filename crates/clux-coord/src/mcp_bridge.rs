use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::post;
use axum::{Json, Router};
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::broker::Broker;
use crate::error::Result;
use crate::protocol::{
    JsonRpcRequest, JsonRpcResponse, McpToolInfo, PeerMessage, TaskRequest, TaskStatus,
};

/// Shared state accessible to all MCP request handlers.
pub struct McpState {
    pub broker: Arc<Broker>,
    /// Pane context provider: maps `pane_id` to visible text content.
    /// Updated by the main app whenever terminal buffers change.
    pub pane_contexts: RwLock<HashMap<u64, String>>,
}

/// Start the MCP HTTP server on the given port.
/// Returns the actual bound address.
pub async fn start_server(state: Arc<McpState>, port: u16) -> Result<SocketAddr> {
    let app = Router::new()
        .route("/mcp", post(handle_mcp_request))
        .with_state(state);

    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| crate::error::CoordError::Server(e.to_string()))?;
    let bound = listener
        .local_addr()
        .map_err(|e| crate::error::CoordError::Server(e.to_string()))?;

    info!(%bound, "MCP server listening");

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!(%e, "MCP server error");
        }
    });

    Ok(bound)
}

async fn handle_mcp_request(
    axum::extract::State(state): axum::extract::State<Arc<McpState>>,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    tracing::debug!(method = %req.method, "MCP request received");
    let response = match req.method.as_str() {
        "initialize" => handle_initialize(&req),
        "tools/list" => handle_tools_list(&req),
        "tools/call" => handle_tools_call(&state, &req).await,
        _ => JsonRpcResponse::error(req.id, -32601, format!("Method not found: {}", req.method)),
    };
    Json(response)
}

fn handle_initialize(req: &JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::success(
        req.id.clone(),
        serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "clux-coord",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )
}

fn handle_tools_list(req: &JsonRpcRequest) -> JsonRpcResponse {
    let tools = build_tool_definitions();
    JsonRpcResponse::success(req.id.clone(), serde_json::json!({ "tools": tools }))
}

async fn handle_tools_call(state: &McpState, req: &JsonRpcRequest) -> JsonRpcResponse {
    let tool_name = req.params.get("name").and_then(|v| v.as_str());
    let args = req
        .params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let Some(tool_name) = tool_name else {
        return JsonRpcResponse::error(req.id.clone(), -32602, "Missing 'name' parameter");
    };

    let result = match tool_name {
        "clux_list_peers" => tool_list_peers(state),
        "clux_send_message" => tool_send_message(state, &args),
        "clux_read_messages" => tool_read_messages(state, &args),
        "clux_broadcast" => tool_broadcast(state, &args),
        "clux_get_pane_context" => tool_get_pane_context(state, &args).await,
        "clux_set_status" => tool_set_status(state, &args),
        "clux_request_task" => tool_request_task(state, &args),
        _ => Err(format!("Unknown tool: {tool_name}")),
    };

    match result {
        Ok(content) => JsonRpcResponse::success(
            req.id.clone(),
            serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": content
                }]
            }),
        ),
        Err(e) => JsonRpcResponse::success(
            req.id.clone(),
            serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": e
                }],
                "isError": true
            }),
        ),
    }
}

// --- Tool implementations ---

fn tool_list_peers(state: &McpState) -> std::result::Result<String, String> {
    let peers = state.broker.list_peers(None).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&peers).map_err(|e| e.to_string())
}

fn tool_send_message(
    state: &McpState,
    args: &serde_json::Value,
) -> std::result::Result<String, String> {
    let from = args
        .get("from_peer")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'from_peer'")?;
    let to = args
        .get("to_peer")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'to_peer'")?;
    let message = args.get("message").ok_or("Missing 'message'")?;

    let msg = PeerMessage {
        id: None,
        from_peer: from.to_string(),
        to_peer: Some(to.to_string()),
        body: message.clone(),
        timestamp: None,
    };
    let id = state.broker.send_message(&msg).map_err(|e| e.to_string())?;
    Ok(format!("Message sent (id: {id})"))
}

fn tool_read_messages(
    state: &McpState,
    args: &serde_json::Value,
) -> std::result::Result<String, String> {
    let peer_id = args
        .get("peer_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'peer_id'")?;
    let since = args.get("since_id").and_then(serde_json::Value::as_i64);

    let messages = state
        .broker
        .read_messages(peer_id, since)
        .map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&messages).map_err(|e| e.to_string())
}

fn tool_broadcast(
    state: &McpState,
    args: &serde_json::Value,
) -> std::result::Result<String, String> {
    let from = args
        .get("from_peer")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'from_peer'")?;
    let message = args.get("message").ok_or("Missing 'message'")?;

    let id = state
        .broker
        .broadcast(from, message)
        .map_err(|e| e.to_string())?;
    Ok(format!("Broadcast sent (id: {id})"))
}

async fn tool_get_pane_context(
    state: &McpState,
    args: &serde_json::Value,
) -> std::result::Result<String, String> {
    let pane_id = args
        .get("pane_id")
        .and_then(serde_json::Value::as_u64)
        .ok_or("Missing or invalid 'pane_id'")?;

    let contexts = state.pane_contexts.read().await;
    match contexts.get(&pane_id) {
        Some(text) => Ok(text.clone()),
        None => Err(format!("No context available for pane {pane_id}")),
    }
}

fn tool_set_status(
    state: &McpState,
    args: &serde_json::Value,
) -> std::result::Result<String, String> {
    let peer_id = args
        .get("peer_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'peer_id'")?;
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'text'")?;

    state
        .broker
        .set_status(peer_id, text)
        .map_err(|e| e.to_string())?;
    Ok(format!("Status updated for {peer_id}"))
}

fn tool_request_task(
    state: &McpState,
    args: &serde_json::Value,
) -> std::result::Result<String, String> {
    let from = args
        .get("from_peer")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'from_peer'")?;
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'description'")?;
    let assignee = args.get("assignee").and_then(|v| v.as_str());

    let task = TaskRequest {
        id: None,
        description: description.to_string(),
        status: if assignee.is_some() {
            TaskStatus::Assigned
        } else {
            TaskStatus::Pending
        },
        requester: from.to_string(),
        assignee: assignee.map(String::from),
        created_at: None,
    };
    let id = state.broker.create_task(&task).map_err(|e| e.to_string())?;
    Ok(format!("Task created (id: {id})"))
}

// --- Tool definitions ---

fn build_tool_definitions() -> Vec<McpToolInfo> {
    vec![
        McpToolInfo {
            name: "clux_list_peers".into(),
            description: "List all active Claude Code panes in the terminal multiplexer".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        McpToolInfo {
            name: "clux_send_message".into(),
            description: "Send a structured message to another Claude Code pane".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "from_peer": { "type": "string", "description": "Your peer ID" },
                    "to_peer": { "type": "string", "description": "Target peer ID" },
                    "message": { "description": "Message content (any JSON value)" }
                },
                "required": ["from_peer", "to_peer", "message"]
            }),
        },
        McpToolInfo {
            name: "clux_read_messages".into(),
            description: "Read messages sent to you, optionally since a given message ID".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "peer_id": { "type": "string", "description": "Your peer ID" },
                    "since_id": { "type": "integer", "description": "Only return messages after this ID" }
                },
                "required": ["peer_id"]
            }),
        },
        McpToolInfo {
            name: "clux_broadcast".into(),
            description: "Broadcast a message to all Claude Code panes".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "from_peer": { "type": "string", "description": "Your peer ID" },
                    "message": { "description": "Message content (any JSON value)" }
                },
                "required": ["from_peer", "message"]
            }),
        },
        McpToolInfo {
            name: "clux_get_pane_context".into(),
            description: "Get the visible terminal content of another pane (read-only)".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pane_id": { "type": "integer", "description": "Target pane ID" }
                },
                "required": ["pane_id"]
            }),
        },
        McpToolInfo {
            name: "clux_set_status".into(),
            description: "Set your status text displayed in the status bar".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "peer_id": { "type": "string", "description": "Your peer ID" },
                    "text": { "type": "string", "description": "Status text to display" }
                },
                "required": ["peer_id", "text"]
            }),
        },
        McpToolInfo {
            name: "clux_request_task".into(),
            description: "Request another agent to perform a task".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "from_peer": { "type": "string", "description": "Your peer ID" },
                    "description": { "type": "string", "description": "Task description" },
                    "assignee": { "type": "string", "description": "Peer ID to assign the task to (optional)" }
                },
                "required": ["from_peer", "description"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> Arc<McpState> {
        Arc::new(McpState {
            broker: Arc::new(Broker::in_memory().unwrap()),
            pane_contexts: RwLock::new(HashMap::new()),
        })
    }

    #[tokio::test]
    async fn initialize_response() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            method: "initialize".into(),
            params: serde_json::Value::Null,
        };
        let resp = handle_initialize(&req);
        assert!(resp.result.is_some());
        let result = resp.result.unwrap();
        assert_eq!(result["serverInfo"]["name"], "clux-coord");
    }

    #[tokio::test]
    async fn tools_list_has_all_tools() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            method: "tools/list".into(),
            params: serde_json::Value::Null,
        };
        let resp = handle_tools_list(&req);
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        assert_eq!(tools.len(), 7);
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"clux_list_peers"));
        assert!(names.contains(&"clux_send_message"));
        assert!(names.contains(&"clux_read_messages"));
        assert!(names.contains(&"clux_broadcast"));
        assert!(names.contains(&"clux_get_pane_context"));
        assert!(names.contains(&"clux_set_status"));
        assert!(names.contains(&"clux_request_task"));
    }

    #[test]
    fn tool_list_peers_empty() {
        let state = test_state();
        let result = tool_list_peers(&state).unwrap();
        let peers: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert!(peers.is_empty());
    }

    #[test]
    fn tool_send_and_read_message() {
        let state = test_state();
        state.broker.register_peer("a", 0).unwrap();
        state.broker.register_peer("b", 1).unwrap();

        let args = serde_json::json!({
            "from_peer": "a",
            "to_peer": "b",
            "message": {"text": "hello"}
        });
        tool_send_message(&state, &args).unwrap();

        let read_args = serde_json::json!({ "peer_id": "b" });
        let result = tool_read_messages(&state, &read_args).unwrap();
        let msgs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn tool_broadcast_message() {
        let state = test_state();
        state.broker.register_peer("a", 0).unwrap();

        let args = serde_json::json!({
            "from_peer": "a",
            "message": "hello all"
        });
        let result = tool_broadcast(&state, &args).unwrap();
        assert!(result.contains("Broadcast sent"));
    }

    #[tokio::test]
    async fn tool_get_pane_context_found() {
        let state = test_state();
        state
            .pane_contexts
            .write()
            .await
            .insert(0, "$ ls\nfoo bar".into());

        let args = serde_json::json!({ "pane_id": 0 });
        let result = tool_get_pane_context(&state, &args).await.unwrap();
        assert!(result.contains("$ ls"));
    }

    #[tokio::test]
    async fn tool_get_pane_context_not_found() {
        let state = test_state();
        let args = serde_json::json!({ "pane_id": 99 });
        let result = tool_get_pane_context(&state, &args).await;
        assert!(result.is_err());
    }

    #[test]
    fn tool_set_status_ok() {
        let state = test_state();
        state.broker.register_peer("a", 0).unwrap();

        let args = serde_json::json!({ "peer_id": "a", "text": "building" });
        let result = tool_set_status(&state, &args).unwrap();
        assert!(result.contains("Status updated"));
    }

    #[test]
    fn tool_request_task_ok() {
        let state = test_state();
        let args = serde_json::json!({
            "from_peer": "a",
            "description": "run cargo test",
            "assignee": "b"
        });
        let result = tool_request_task(&state, &args).unwrap();
        assert!(result.contains("Task created"));
    }

    #[tokio::test]
    async fn unknown_method_returns_error() {
        let state = test_state();
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            method: "unknown/method".into(),
            params: serde_json::Value::Null,
        };
        let Json(resp) = handle_mcp_request(axum::extract::State(state), Json(req)).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
