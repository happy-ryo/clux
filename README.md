# clux

Windows-native AI agent-collaborative terminal multiplexer, built in Rust.

clux is a GPU-accelerated terminal multiplexer for Windows that enables multiple Claude Code instances to coordinate with each other via an embedded MCP server.

## Features

- **Multi-pane layout** -- Binary tree-based pane splitting (horizontal/vertical)
- **Multi-tab** -- Create, switch, and close tabs
- **GPU rendering** -- wgpu-powered text rendering with glyph atlas
- **ConPTY** -- Native Windows pseudo console with passthrough mode support
- **Session persistence** -- Auto-save/restore workspace state to JSON
- **Claude Code coordination** -- Embedded MCP server for inter-agent communication
- **Auto-detection** -- Detects Claude Code launch and injects MCP config automatically
- **Configuration** -- `clux.toml` for shell, font, colors, and scrollback settings
- **Clipboard** -- Copy/paste with text selection support
- **Scrollback** -- Mouse wheel scrolling through terminal history

## Requirements

- Windows 10 version 1809+ (ConPTY support)
- Windows 11 22H2+ recommended (passthrough mode)
- GPU with DirectX 12 support

## Building

```bash
cargo build --release
```

The binary is output to `target/release/clux.exe`.

## Usage

```bash
clux
```

### Keyboard Shortcuts

All shortcuts use `Ctrl+Shift` as the modifier:

| Shortcut | Action |
|----------|--------|
| `Ctrl+Shift+H` | Split pane horizontally |
| `Ctrl+Shift+B` | Split pane vertically |
| `Ctrl+Shift+W` | Close active pane |
| `Ctrl+Shift+Arrow` | Cycle pane focus |
| `Ctrl+Shift+T` | New tab |
| `Ctrl+Shift+Q` | Close tab |
| `Ctrl+Shift+1-9` | Switch to tab N |
| `Ctrl+Shift+C` | Copy selection |
| `Ctrl+Shift+V` | Paste from clipboard |
| `Ctrl+Shift+P` | Toggle coordination panel |

### Configuration

clux reads configuration from `%APPDATA%/clux/clux.toml`. See `resources/default_config.toml` for all available options.

```toml
[shell]
default = "pwsh.exe"

[font]
family = "Consolas"
size = 14.0

[colors]
background = [0.118, 0.118, 0.180]
foreground = [0.847, 0.871, 0.914]

[scrollback]
max_lines = 10000
```

## Claude Code Coordination

clux's killer feature is enabling multiple Claude Code instances to communicate with each other.

### How It Works

1. Open multiple panes and start Claude Code in each
2. clux auto-detects Claude Code and injects MCP server configuration
3. Each Claude Code instance gets access to coordination tools
4. Agents can send messages, share context, and delegate tasks

### MCP Tools

The embedded MCP server (port 19836) provides these tools to Claude Code:

| Tool | Description |
|------|-------------|
| `clux_list_peers` | List active Claude Code panes |
| `clux_send_message` | Send a message to another pane |
| `clux_read_messages` | Read messages sent to you |
| `clux_broadcast` | Broadcast to all panes |
| `clux_get_pane_context` | Read another pane's terminal output |
| `clux_set_status` | Set your status in the status bar |
| `clux_request_task` | Request another agent to do a task |

### Coordination Panel

Press `Ctrl+Shift+P` to open the coordination panel overlay, showing:
- Active peers and their status
- Pending/active tasks
- Recent messages between agents

## Architecture

```
crates/
  clux/           # Main binary (app, config, keybindings)
  clux-terminal/  # ConPTY wrapper, VT parser, terminal buffer
  clux-renderer/  # wgpu GPU rendering, glyph atlas
  clux-layout/    # Binary tree pane/tab layout engine
  clux-session/   # Session persistence (JSON)
  clux-coord/     # MCP server, message broker, agent coordination
```

See `docs/ARCHITECTURE.md` for detailed architecture documentation.

## License

MIT
