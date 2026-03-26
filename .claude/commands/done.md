現在のIssue/PRの作業を完了し、後片付けをする。

## 事前検証

1. **現在のブランチ確認**: featureブランチにいるか
   - mainにいる → 直近でマージされたPRを確認して後片付けを提案
2. **未コミット変更**: 残っていないか確認
   - あり → 「未コミットの変更があります。コミットまたは破棄してください」

## PR/マージ状態の確認

1. **PR検索**: `gh pr list --repo happy-ryo/clux --head <ブランチ名> --state all --json number,state,title,mergedAt`
2. 状態に応じて分岐:

### PRがマージ済みの場合
1. mainに切り替え: `git checkout main && git pull origin main`
2. フィーチャーブランチ削除: `git branch -d <ブランチ名>` (安全な削除のみ)
   - 削除失敗 → 「未マージのコミットがあります」と警告。`-D` は使わない。
3. リモートブランチ削除: `git push origin --delete <ブランチ名>` (既に削除済みならスキップ)
4. 完了タスクを全てcompletedに更新
5. Issue確認: `gh issue view <番号> --repo happy-ryo/clux --json state` でクローズ済みか確認
   - 未クローズ → 「PRに `Closes #<番号>` が含まれていない可能性があります。手動クローズしますか？」

### PRがオープンの場合
- 「PR #<番号> はまだオープンです。レビュー/マージ待ちです」と報告
- CIステータスを確認: `gh pr checks <番号> --repo happy-ryo/clux`
- CI失敗中 → 失敗内容を表示し、修正を提案

### PRが未作成の場合
- 「PRがまだ作成されていません。`/pr` でPRを作成してください」と案内

## 次のステップ提案

1. 同Phaseの残りIssue確認:
   ```
   gh issue list --repo happy-ryo/clux --state open --json number,title,milestone
   ```
2. Milestone別にグルーピングして表示
3. 次に取り組むべきIssueを提案: 「次は `/start <番号>` で開始できます」

## 報告フォーマット
```
## 完了: Issue #<番号> - <タイトル>
- ブランチ `<ブランチ名>` を削除しました
- PR #<PR番号> はマージ済みです

### 残りのIssue (<Phase名>)
- #<番号>: <タイトル>
- #<番号>: <タイトル>

次は `/start <番号>` で次のIssueに着手できます。
```
