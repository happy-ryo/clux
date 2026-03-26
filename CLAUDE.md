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

新しい会話セッションで作業を再開する場合は `/resume` を実行する。

### Slash Commands

| コマンド | 説明 |
|---------|------|
| `/start <番号>` | Issueの実装を開始 (事前検証→ブランチ作成→タスク計画) |
| `/pr` | PR作成 (事前検証→CIチェック→自動修正→PR作成) |
| `/done` | 後片付け (マージ確認→ブランチ削除→次Issue提案) |
| `/resume` | 中断した作業の再開 (状態推定→タスク復元→次ステップ案内) |
| `/status` | プロジェクト進捗表示 (Milestone進捗→Issue→PR状態) |
| `/ci` | 全CIチェック実行 (fmt→clippy→test→build→deny) |
| `/lint` | clippy + fmt + deny チェック |
| `/build` | cargo build |
| `/test` | cargo test |

### コマンドの設計原則

各コマンドは以下の原則に従う:
- **事前検証**: 実行前に前提条件を全てチェックし、問題があれば是正策を提示して停止
- **エラーリカバリ**: 失敗時に自動修正を試み、不可能な場合は具体的な対処法を提示
- **状態ガイド**: 完了後に次にやるべきことを明示 (例: `/ci` 通過後 → `/pr` を案内)
- **冪等性**: 同じコマンドを再実行しても安全 (既存ブランチ/PRがあれば検出して対応)

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
