Issue の実装を開始する。引数: Issue番号 (例: `/start 2`)

## 事前検証

実行前に以下を全てチェックし、問題があれば是正策を提示して停止する:

1. **未コミット変更の確認**: `git status --porcelain` で未コミット変更がないか確認
   - 変更あり → 「未コミットの変更があります。先にコミットまたはスタッシュしてください」と案内
2. **現在のブランチ確認**: main以外にいる場合
   - 既にfeatureブランチにいる → 「現在 `<ブランチ名>` で作業中です。このまま続けますか？新しいIssueに切り替えますか？」と確認
3. **リモート同期**: `git fetch origin` → mainが最新か確認
   - 遅れている → `git pull origin main` で更新

## 手順

1. **Issue内容の取得**
   - `gh issue view $ISSUE_NUMBER --repo happy-ryo/clux` でIssue内容を取得
   - Issueが存在しない/クローズ済みならエラー表示して停止
   - タスクの概要、完了条件、所属Milestoneを把握

2. **ブランチ作成**
   - `git checkout main && git pull origin main`
   - Issueタイトルからブランチ名を生成: `feature/issue-<番号>-<英語ケバブケース>`
   - 同名ブランチが既にある → 「既存ブランチがあります。切り替えますか？」と確認
   - `git checkout -b <ブランチ名>`

3. **作業タスクの作成**
   - Issueのチェックリスト (`- [ ]`) を解析
   - TaskCreateで各項目をタスク化
   - 依存関係があればTaskUpdateでblockedByを設定

4. **ユーザーへの報告** (以下のフォーマットで)
   ```
   ## Issue #<番号>: <タイトル>
   **Milestone**: <Milestone名>
   **ブランチ**: `<ブランチ名>`

   ### 概要
   <Issue内容の要約>

   ### 作業タスク
   1. <タスク1>
   2. <タスク2>
   ...

   ### 次のステップ
   実装を進め、区切りの良いところで `/ci` でチェック、完了したら `/pr` でPR作成。
   ```

## 引数なしの場合

`gh issue list --repo happy-ryo/clux --state open` でオープンなIssue一覧を表示。
Milestone別にグルーピングして、現在のPhaseのIssueを優先表示する。

## エラーリカバリ

- `gh` コマンド失敗 → 認証状態を確認 (`gh auth status`)、ネットワーク接続を確認
- ブランチ作成失敗 → 同名ブランチの存在確認、dirty workingの確認
- 途中で中断 → `/start` を再実行すれば、既存ブランチがあればそこから再開を提案
