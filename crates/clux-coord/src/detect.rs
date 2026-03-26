use std::path::{Path, PathBuf};

use tracing::info;

use crate::error::Result;

/// Patterns that indicate Claude Code is running in a pane.
/// Claude Code outputs its banner on startup.
const DETECTION_PATTERNS: &[&str] = &[
    "╭─",     // Claude Code TUI top border
    "claude", // claude command invocation
];

/// Minimum number of characters to buffer before attempting detection.
const MIN_BUFFER_LEN: usize = 32;

/// Tracks whether Claude Code has been detected in a pane's output.
#[derive(Debug)]
pub struct ClaudeDetector {
    /// Rolling buffer of recent output text for pattern matching.
    buffer: String,
    /// Whether Claude Code has been detected in this pane.
    pub detected: bool,
    /// Whether the MCP config has been injected for this pane.
    pub config_injected: bool,
}

impl ClaudeDetector {
    pub fn new() -> Self {
        Self {
            buffer: String::with_capacity(256),
            detected: false,
            config_injected: false,
        }
    }

    /// Feed terminal output text for detection.
    /// Returns `true` if Claude Code was newly detected (first time).
    pub fn feed(&mut self, text: &str) -> bool {
        if self.detected {
            return false;
        }

        self.buffer.push_str(text);

        // Trim buffer to avoid unbounded growth
        if self.buffer.len() > 1024 {
            let start = self.buffer.len() - 512;
            self.buffer = self.buffer[start..].to_string();
        }

        if self.buffer.len() < MIN_BUFFER_LEN {
            return false;
        }

        let lower = self.buffer.to_lowercase();
        for pattern in DETECTION_PATTERNS {
            if lower.contains(pattern) {
                self.detected = true;
                info!(pattern, "Claude Code detected in pane output");
                return true;
            }
        }

        false
    }

    /// Reset detection state (e.g., when a pane is reused).
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.detected = false;
        self.config_injected = false;
    }
}

impl Default for ClaudeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ClaudeSettings {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: serde_json::Map<String, serde_json::Value>,
    #[serde(flatten)]
    other: serde_json::Map<String, serde_json::Value>,
}

/// Inject MCP server configuration into `.claude/settings.local.json`.
///
/// If `cwd` is provided, writes to `{cwd}/.claude/settings.local.json`.
/// Otherwise falls back to the user's home directory.
pub fn inject_mcp_config(cwd: Option<&Path>, mcp_port: u16) -> Result<PathBuf> {
    let base = if let Some(cwd) = cwd {
        cwd.to_path_buf()
    } else {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    };

    let settings_dir = base.join(".claude");
    std::fs::create_dir_all(&settings_dir)?;

    let settings_path = settings_dir.join("settings.local.json");

    // Read existing settings or start fresh
    let mut settings: ClaudeSettings = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| ClaudeSettings {
            mcp_servers: serde_json::Map::new(),
            other: serde_json::Map::new(),
        })
    } else {
        ClaudeSettings {
            mcp_servers: serde_json::Map::new(),
            other: serde_json::Map::new(),
        }
    };

    // Add/update the clux-coord MCP server entry
    let server_config = serde_json::json!({
        "url": format!("http://127.0.0.1:{mcp_port}/mcp")
    });
    settings
        .mcp_servers
        .insert("clux-coord".to_string(), server_config);

    let json = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&settings_path, json)?;

    info!(?settings_path, "MCP config injected for Claude Code");
    Ok(settings_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_claude_code_banner() {
        let mut d = ClaudeDetector::new();
        // Not enough data yet
        assert!(!d.feed("short"));
        assert!(!d.detected);

        // Feed enough text with the pattern
        assert!(d.feed("some output before ╭─ Claude Code v1.0 and more text after it"));
        assert!(d.detected);
    }

    #[test]
    fn detect_claude_command() {
        let mut d = ClaudeDetector::new();
        assert!(d.feed("PS C:\\Users\\test> claude some-argument and more text here"));
        assert!(d.detected);
    }

    #[test]
    fn no_double_detection() {
        let mut d = ClaudeDetector::new();
        assert!(d.feed("running claude code in this terminal pane right now"));
        // Second call should not re-detect
        assert!(!d.feed("claude again"));
    }

    #[test]
    fn reset_allows_redetection() {
        let mut d = ClaudeDetector::new();
        d.feed("running claude code in this terminal pane right now");
        assert!(d.detected);
        d.reset();
        assert!(!d.detected);
        assert!(d.feed("running claude code in this terminal pane right now"));
    }

    #[test]
    fn inject_mcp_config_creates_file() {
        let tmp = std::env::temp_dir().join("clux-test-inject");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let path = inject_mcp_config(Some(&tmp), 19836).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed["mcpServers"]["clux-coord"]["url"],
            "http://127.0.0.1:19836/mcp"
        );

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn inject_mcp_config_preserves_existing() {
        let tmp = std::env::temp_dir().join("clux-test-inject-existing");
        let _ = std::fs::remove_dir_all(&tmp);
        let claude_dir = tmp.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();

        // Write existing settings
        let existing = serde_json::json!({
            "mcpServers": {
                "other-server": { "command": "other" }
            },
            "someKey": "someValue"
        });
        std::fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        inject_mcp_config(Some(&tmp), 19836).unwrap();

        let content = std::fs::read_to_string(claude_dir.join("settings.local.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Both servers should exist
        assert!(parsed["mcpServers"]["other-server"].is_object());
        assert!(parsed["mcpServers"]["clux-coord"].is_object());
        // Other keys preserved
        assert_eq!(parsed["someKey"], "someValue");

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
