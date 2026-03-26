中断した作業を再開する。新しい会話セッションの最初に使う。

## 手順

1. **Git状態の確認**
   - 現在のブランチ: `git branch --show-current`
   - 未コミット変更: `git status --porcelain`
   - 直近のコミット: `git log --oneline -5`

2. **作業状態の推定**

   ブランチ名から状態を判断:

   ### mainブランチにいる場合
   - 直近マージされたPRを確認: `gh pr list --repo happy-ryo/clux --state merged --limit 3`
   - 次に取り組むべきIssueを提案
   - → 「前回の作業が完了しているようです。`/start <番号>` で次のIssueに着手できます」

   ### featureブランチにいる場合
   - ブランチ名からIssue番号を抽出
   - Issue内容を取得: `gh issue view <番号> --repo happy-ryo/clux`
   - PRの存在確認: `gh pr list --repo happy-ryo/clux --head <ブランチ名> --json number,state`
   - mainとの差分: `git log main..HEAD --oneline`
   - 直近の変更ファイル: `git diff main --stat`

   状態に応じた案内:
   - **PR作成済み & マージ済み** → `/done` を提案
   - **PR作成済み & オープン** → CIステータス確認、レビュー待ちか修正要か報告
   - **PR未作成 & コミットあり** → 実装の続きか `/pr` でPR作成かを確認
   - **PR未作成 & コミットなし** → Issueの作業タスクを再表示、実装開始を案内

3. **TaskListの復元**
   - Issueのチェックリストを元にTaskCreateでタスクを再作成
   - `git log main..HEAD` の内容から完了済みタスクを推定しcompletedに

4. **報告**
   ```
   ## 作業再開: Issue #<番号> - <タイトル>
   **ブランチ**: `<ブランチ名>`
   **状態**: <実装中 / PR作成済み / マージ済み>

   ### 進捗
   - <完了したこと>
   - <残っていること>

   ### 次のステップ
   <具体的な推奨アクション>
   ```
