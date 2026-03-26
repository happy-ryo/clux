# clux - Development Guide

## Project

Windows向けAIエージェント協調型ターミナルマルチプレクサ (Rust製)

- リポジトリ: https://github.com/happy-ryo/clux
- アーキテクチャ: docs/ARCHITECTURE.md

## Development Workflow

**mainブランチへの直接push禁止。必ず以下のフローで進める。**

```
/start <Issue番号>  →  実装  →  /ci  →  /pr  →  レビュー/マージ  →  /done
```

### Slash Commands

| コマンド | 説明 |
|---------|------|
| `/start <番号>` | Issueの実装を開始 (ブランチ作成、タスク計画) |
| `/pr` | 現在のブランチからPR作成 (CIチェック込み) |
| `/done` | Issue完了後の後片付け (ブランチ削除、次Issue提案) |
| `/status` | プロジェクト全体の進捗表示 |
| `/ci` | ローカルで全CIチェック実行 |
| `/lint` | clippy + fmt + deny チェック |
| `/build` | cargo build |
| `/test` | cargo test |

### Git Hooks

- `scripts/pre-commit`: コミット前にfmt + clippy + build を自動実行
- hooks設定: `git config core.hooksPath scripts`

### Branch Naming

- `feature/issue-<番号>-<短い説明>` (例: `feature/issue-2-conpty-wrapper`)

### Commit Message

- 英語で簡潔に (why > what)
- `Closes #<番号>` でIssue自動クローズ
- `Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>`

## Tech Stack

- Rust (edition 2024)
- ConPTY (Windows terminal API)
- wgpu + winit (GPU rendering)
- vte (VT parser)
- cosmic-text (font shaping)

## CI

- clippy (pedantic) -- `cargo clippy --all-targets -- -D warnings`
- rustfmt -- `cargo fmt --check`
- cargo-deny -- ライセンス/セキュリティ監査
- GitHub Actions: `.github/workflows/ci.yml`

## Workspace Structure

```
crates/
  clux/           # メインバイナリ
  clux-terminal/  # ConPTY, VTパーサー, バッファ
  clux-renderer/  # wgpu GPU描画
```
