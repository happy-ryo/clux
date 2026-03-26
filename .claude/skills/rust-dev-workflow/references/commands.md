# Command Reference

各コマンドの詳細手順。SKILL.md から参照される。

## Table of Contents
1. [/start](#start)
2. [/pr](#pr)
3. [/done](#done)
4. [/resume](#resume)
5. [/ci](#ci)
6. [/status](#status)
7. [共通ユーティリティ](#共通ユーティリティ)

---

## /start

### 事前検証
1. `git status --porcelain` → 未コミット変更があれば停止
2. 現在 main 以外 → 「既にブランチにいます。続行or切り替え？」と確認
3. `git fetch origin` → main が最新か確認、遅れていれば pull

### 実行
1. `gh issue view <番号> --repo <REPO>` で Issue 取得 (存在/クローズ確認)
2. `git checkout main && git pull origin main`
3. Issue タイトルからブランチ名生成 → 同名あれば切り替え確認
4. `git checkout -b feature/issue-<N>-<slug>`
5. Issue のチェックリストから TaskCreate でタスク作成
6. 報告: Issue 要約、ブランチ名、タスク一覧、次ステップ

### 引数なし
`gh issue list --repo <REPO> --state open` → Milestone 別にグルーピング表示

### エラーリカバリ
- gh 失敗 → `gh auth status` 確認案内
- ブランチ作成失敗 → dirty working / 同名ブランチ確認
- 再実行 → 既存ブランチがあれば再開提案 (冪等)

---

## /pr

### 事前検証
1. main でないこと確認 (main なら `/start` を案内)
2. `git status --porcelain` → 未コミット変更なし確認
3. `git log main..HEAD --oneline` → 差分コミット存在確認
4. `gh pr list --head <branch>` → 既存 PR あれば push のみ提案

### CI チェック (自動修正付き)
順番に実行。失敗時:
- **fmt**: `cargo fmt` で自動修正 → 修正コミット → 再チェック
- **clippy**: `cargo clippy --fix --allow-dirty` を試行 → 不可なら停止
- **test**: 失敗テスト表示 → **PR 作成しない**
- **build**: エラー表示 → **PR 作成しない**

全通過後のみ続行。

### PR 作成
1. ブランチ名から Issue 番号抽出
2. `git push -u origin <branch>`
3. `gh pr create --title "<タイトル>" --body "<テンプレート>"`

テンプレート:
```
## Summary
- <箇条書き>

## Test plan
- [ ] <テスト項目>

Closes #<N>

🤖 Generated with [Claude Code](https://claude.ai/code)
```

4. 報告: PR URL、次ステップ (マージ後 `/done`)

### エラーリカバリ
- push 認証失敗 → `gh auth status` 案内
- push リモート差分 → `git pull --rebase` 提案
- PR 作成失敗 → エラー内容 + 権限確認

---

## /done

### 事前検証
1. featureブランチか確認 (main なら直近マージ PR を確認)
2. 未コミット変更なし確認

### 分岐
- **PR マージ済み**: main checkout → ブランチ削除 (`-d` のみ、`-D` 禁止) → リモート削除 → タスク完了 → Issue クローズ確認
- **PR オープン**: CI ステータス報告、レビュー待ちか修正要かを案内
- **PR 未作成**: `/pr` を案内

### 次ステップ提案
同 Phase の残り Issue を表示 → `/start <番号>` を提案

---

## /resume

### 状態推定
1. `git branch --show-current` でブランチ判定
2. **main**: 直近マージ PR 確認 → `/start` 案内
3. **feature ブランチ**:
   - ブランチ名から Issue 番号抽出
   - Issue 内容取得
   - PR 存在/状態確認
   - main との diff 確認
4. 状態→アクション:
   - PR マージ済み → `/done` 案内
   - PR オープン → CI 確認、レビュー待ち or 修正要
   - コミットあり PR なし → 続行 or `/pr`
   - コミットなし → Issue タスク表示、実装開始

### タスク復元
Issue チェックリスト → TaskCreate → git log から完了推定

---

## /ci

### チェック順序
1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test --all`
4. `cargo build`
5. `cargo deny check` (deny.toml 存在時のみ)

### 結果表示
テーブル形式で全結果サマリー。全通過 → `/pr` 案内。失敗 → 修正案内。

### 自動修正モード
「修正して」で fmt/clippy の自動修正→コミット→再チェック。

---

## /status

### 収集情報
1. Git 状態 (ブランチ、未コミット変更、リモート差分)
2. `gh api repos/<REPO>/milestones` → Milestone 進捗バー
3. `gh issue list --state open` → Phase 別 Issue
4. `gh pr list` → オープン PR + CI 状態

### 表示
プログレスバー付き Milestone 進捗 + Issue 一覧 + 推奨アクション

---

## 共通ユーティリティ

### REPO の解決
1. CLAUDE.md の `repo:` 設定を確認
2. なければ `git remote get-url origin` から `owner/repo` を抽出

### Issue 番号の抽出
ブランチ名 `feature/issue-<N>-*` から正規表現 `issue-(\d+)` で N を取得。

### CI ステップの解決
1. CLAUDE.md の `ci_steps:` 設定を確認
2. なければデフォルト: fmt → clippy → test → build → deny (deny.toml 存在時)
