---
name: rust-dev-workflow
description: Rust プロジェクトの GitHub Issue ベース開発ワークフローを管理する Skill。Issue 開始、ブランチ作成、CI チェック、PR 作成、完了処理、作業再開を一貫したフローで実行する。Rust の cargo/clippy/fmt、GitHub CLI (gh)、Git を統合し、事前検証・エラーリカバリ・状態ガイドを備える。`/start`、`/pr`、`/done`、`/resume`、`/ci`、`/status` のコマンド群で構成。Rust プロジェクトで Issue 駆動の開発を行う場合、ブランチ管理やPR作成の手順を標準化したい場合、CI チェックをローカルで一括実行したい場合に使う。
---

# Rust Dev Workflow

GitHub Issue ベースの Rust プロジェクト開発ワークフロー。

## 設定

このSkillを使うプロジェクトでは、CLAUDE.md に以下の設定セクションを追加する:

```markdown
## Workflow Config
- repo: <owner>/<repo>  (例: happy-ryo/clux)
- default_shell: pwsh.exe  (任意)
- ci_steps:
  - cargo fmt --check
  - cargo clippy --all-targets -- -D warnings
  - cargo test --all
  - cargo build
  - cargo deny check  (deny.toml がある場合のみ)
```

設定がない場合は、git remote から repo を推定し、標準的な CI ステップを使う。

## ワークフロー全体像

```
/start <Issue番号>
    ↓
  実装 (コミットを重ねる)
    ↓
/ci (ローカル CI チェック)
    ↓ 全通過
/pr (PR 作成)
    ↓ レビュー & マージ
/done (後片付け + 次 Issue 提案)
```

新しいセッションで作業再開する場合は `/resume` を最初に実行する。

## コマンド設計原則

全コマンドに共通する原則。Skillを別プロジェクトに導入する際もこれに従う:

1. **事前検証**: 前提条件を全てチェックし、問題があれば是正策を提示して停止
2. **エラーリカバリ**: 失敗時に自動修正を試み、不可能な場合は具体的な対処法を提示
3. **状態ガイド**: 完了後に次のステップを明示
4. **冪等性**: 同じコマンドを再実行しても安全

## コマンド詳細

各コマンドの詳細は `references/commands.md` を参照。

### /start <Issue番号>
Issue の実装を開始する。事前に未コミット変更・ブランチ状態を検証し、featureブランチを作成してタスク計画を立てる。引数なしなら未解決 Issue 一覧を表示。

### /pr
現在のブランチから PR を作成する。事前に全 CI チェックを実行し、fmt/clippy は自動修正を試みる。チェックが全通過したときのみ PR を作成。

### /done
作業完了の後片付け。PR のマージ状態を確認し、ブランチ削除と次 Issue の提案を行う。

### /resume
中断した作業を再開する。Git 状態から作業フェーズを推定し、タスクリストを復元して次のステップを案内。

### /ci
ローカル CI パイプラインを実行。fmt → clippy → test → build → (deny) の順でチェックし、結果サマリーを表示。

### /status
プロジェクト全体の進捗を表示。Milestone 進捗、Issue 一覧、PR 状態を統合表示。

### /lint, /build, /test
個別の CI チェック。失敗時に修正提案を行う。

## ブランチ命名規則

```
feature/issue-<番号>-<ケバブケースの説明>
```

例: `feature/issue-42-add-tab-support`

## コミットメッセージ規則

- 英語で簡潔に (why > what)
- PR 最終コミットまたは PR ボディに `Closes #<番号>` を含める
- AI 協調の場合: `Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>`

## 前提ツール

- `git` (2.x+)
- `cargo` (Rust toolchain)
- `gh` (GitHub CLI、認証済み)
- `cargo-deny` (任意、deny.toml がある場合)
- `clippy`, `rustfmt` (rustup components)
