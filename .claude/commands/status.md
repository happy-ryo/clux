プロジェクト全体の進捗状況を表示する。

## 手順

1. **Git状態**
   - 現在のブランチ
   - 未コミットの変更があるか
   - リモートとの差分

2. **GitHub Issue/Milestone進捗**
   - 各Milestoneのオープン/クローズIssue数を表示:
     ```
     gh api repos/happy-ryo/clux/milestones --jq '.[] | "\(.title): \(.open_issues) open / \(.closed_issues) closed"'
     ```
   - 現在のPhaseのオープンIssue一覧

3. **PR状態**
   - オープンなPR一覧: `gh pr list --repo happy-ryo/clux`

4. **レポート**
   - 全体進捗をフェーズごとにまとめて報告
   - 次にやるべきことを提案
