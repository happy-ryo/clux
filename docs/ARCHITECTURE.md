# clux Architecture

Windows向けAIエージェント協調型ターミナルマルチプレクサ

## Context

MacではcmuxなどAIエージェント連携を重視したモダンなターミナルアプリが登場しているが、Windowsには同等のものがない。cluxはWindows向けに、cmux風のAIエージェント(Claude Code)協調作業を核とした、Rust製GPU加速ターミナルマルチプレクサを開発するプロジェクト。

**コア差別化ポイント**: 複数ペイン/タブ間でのClaude Codeインスタンスの協調通信(MCP経由)

---

## 技術スタック

| 領域 | 技術 |
|------|------|
| 言語 | Rust |
| ターミナル | ConPTY (Windows 10 1809+) |
| レンダリング | wgpu (DX12バックエンド) + winit |
| フォント | cosmic-text |
| VTパース | vte crate |
| 協調通信 | 組み込みMCPサーバー (axum + SQLite) |
| 非同期ランタイム | tokio |

---

## プロジェクト構造

```
clux/
  Cargo.toml              # workspace root
  crates/
    clux/                  # メインバイナリ
      src/
        main.rs            # エントリポイント
        app.rs             # アプリケーションライフサイクル・イベントループ
        config.rs          # 設定読み込み
        keybindings.rs     # キーバインド処理
    clux-terminal/         # ターミナルエミュレーション
      src/
        lib.rs
        conpty.rs          # ConPTYラッパー (windows-rs)
        vt_parser.rs       # VTシーケンスパース
        buffer.rs          # 画面バッファ / セルグリッド
        resize.rs          # 防御的リサイズロジック
    clux-layout/           # ペイン・タブレイアウト管理
      src/
        lib.rs
        pane.rs            # 個別ペイン状態
        tab.rs             # タブ (ペインツリーを保持)
        tree.rs            # 二分木によるレイアウト表現
        constraints.rs     # 最小サイズ・比率制約
    clux-renderer/         # GPU加速レンダリング
      src/
        lib.rs
        atlas.rs           # グリフアトラス / フォントテクスチャキャッシュ
        pipeline.rs        # wgpuレンダーパイプライン
        cell_renderer.rs   # ターミナルセル→GPUクワッドマッピング
        border.rs          # ペイン境界線・ステータスバー
    clux-session/          # セッション永続化
      src/
        lib.rs
        state.rs           # シリアライズ可能なセッション状態
        store.rs           # JSON/SQLiteバックエンド
        restore.rs         # セッション復元
    clux-coord/            # Claude Code協調通信
      src/
        lib.rs
        broker.rs          # 組み込みメッセージブローカー
        protocol.rs        # メッセージ型・シリアライゼーション
        mcp_bridge.rs      # MCPサーバーブリッジ
        peer.rs            # ピア登録・ディスカバリ
  resources/
    shaders/               # wgslシェーダー
    default_config.toml    # デフォルト設定
```

---

## コアアーキテクチャ

### イベントループ

```
winit event → App dispatch → {
  keyboard/mouse  → 対象ペイン特定 → ConPTY stdin転送
  resize          → レイアウトツリー再計算 → 各ConPTYリサイズ
  render request  → 全ペインバッファ収集 → Renderer::draw()
  coord message   → CoordBroker::dispatch()
}
```

### ConPTYスレッドモデル

デッドロック防止のため3スレッドモデルを採用:

- **Thread 1 (Read)**: ConPTY出力パイプからリングバッファに連続読み取り
- **Thread 2 (Write)**: 書き込みキューをドレインしてConPTY入力パイプに送信
- **Main Thread**: VTパース → セルグリッド更新 → 再描画トリガー

ConPTY生成フラグ:
- `PSEUDOCONSOLE_RESIZE_QUIRK` (0x2) -- リサイズアーティファクト修正
- `PSEUDOCONSOLE_WIN32_INPUT_MODE` (0x4) -- 適切なキー入力処理
- `PSEUDOCONSOLE_PASSTHROUGH_MODE` (0x8) -- VTシーケンス直接リレー (Win11 22H2+、実行時検出)

### レイアウト管理

二分木によるペイン分割:

```rust
enum LayoutNode {
    Leaf { pane_id: PaneId },
    Split {
        direction: Direction, // Horizontal | Vertical
        ratio: f32,           // 0.0..1.0
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}
```

最小ペインサイズ: 20列 x 5行

### Claude Code協調通信

キラーフィーチャー。組み込みMCPサーバーによるエージェント間通信。

```
[Pane 1: Claude Code] ←MCP HTTP→ [clux-coord MCP Server]
[Pane 2: Claude Code] ←MCP HTTP→ [clux-coord MCP Server]
[Pane 3: Claude Code] ←MCP HTTP→ [clux-coord MCP Server]
                                        |
                                  [Embedded Broker]
                                        |
                                  [SQLite message store]
```

#### MCPツール (Claude Codeに公開)

| ツール | 説明 |
|--------|------|
| `clux_list_peers` | アクティブなClaude Codeペイン一覧 |
| `clux_send_message(to_pane_id, message)` | 構造化メッセージ送信 |
| `clux_read_messages(since?)` | 自分宛メッセージ読み取り |
| `clux_broadcast(message)` | 全ピアへ一斉送信 |
| `clux_get_pane_context(pane_id)` | 他ペインの表示内容取得(読み取り専用) |
| `clux_set_status(text)` | ステータスバーに表示するステータス設定 |
| `clux_request_task(description)` | 他エージェントへのタスク依頼 |

#### 自動設定

ペインでClaude Code起動を検出 → `.claude/settings.local.json`にMCPサーバー設定を自動注入 → ゼロコンフィグ

#### ストレージ

`%APPDATA%\clux\coord.db` (SQLite):
- `peers` テーブル: pane_id, cwd, status_text, last_heartbeat
- `messages` テーブル: from_pane, to_pane, message_json, timestamp
- `tasks` テーブル: description, status, assigned_pane

---

## 主要依存クレート

| クレート | 用途 |
|---------|------|
| `windows` | ConPTY API, Win32バインディング (Microsoft公式) |
| `wgpu` | GPUレンダリング (DX12) |
| `winit` | ウィンドウ管理, IME |
| `vte` | VTシーケンスパース |
| `cosmic-text` | フォントシェーピング・ラスタライズ |
| `serde` + `serde_json` | シリアライゼーション |
| `rusqlite` | SQLite (協調メッセージストア) |
| `axum` | MCPサーバー用HTTPサーバー |
| `tokio` | 非同期ランタイム |
| `toml` | 設定ファイル |
| `tracing` | 構造化ロギング |
| `crossbeam-channel` | スレッド間通信 |

---

## リスクと対策

| リスク | 重大度 | 対策 |
|--------|--------|------|
| ConPTYリサイズ同期問題 | 高 | RESIZE_QUIRKフラグ + デバウンス(50ms) + "resize pending"状態バッファリング |
| GPUテキスト描画品質 | 高 | cosmic-text使用 + DPI変更時アトラス再構築 + ASCII/CJK別アトラスページ |
| MCP互換性 | 中 | 公式MCP仕様準拠 + HTTP streamable transport + グレースフルデグラデーション |
| ConPTYクローズ時デッドロック | 中 | writeパイプ先閉じ → ClosePseudoConsole → readスレッドタイムアウト付きjoin |
| winit IMEサポート | 低 | 早期テスト + 必要ならWin32 WM_IME_*メッセージへフォールバック |

---

## 参考プロジェクト

- [psmux](https://github.com/marlocarlo/psmux) -- Windows向けRust製tmux (ConPTYフラグ参考)
- [WezTerm](https://github.com/wezterm/wezterm) -- Rust製マルチプレクサ (アーキテクチャ参考)
- [Alacritty](https://github.com/alacritty/alacritty) -- GPU描画参考
- [claude-peers-mcp](https://github.com/louislva/claude-peers-mcp) -- Claude Code間通信参考
