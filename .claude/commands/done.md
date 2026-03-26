現在のIssue/PRの作業を完了し、後片付けをする。

## 手順

1. **状態確認**
   - 現在のブランチとPR状態を確認
   - マージ済みかどうか確認: `gh pr status --repo happy-ryo/clux`

2. **マージ済みの場合**
   - mainに切り替え: `git checkout main && git pull`
   - フィーチャーブランチを削除: `git branch -d <ブランチ名>`
   - 関連するTaskを全てcompletedに更新

3. **未マージの場合**
   - PRが作成済みか確認
   - 未作成なら `/pr` の実行を提案
   - 作成済みならマージ待ちであることを報告

4. **次のステップ提案**
   - `gh issue list --repo happy-ryo/clux --state open --milestone "<現在のPhase>"` で同Phaseの残りIssueを確認
   - 次に取り組むべきIssueを提案

## 注意
- フォースデリートは使わない (`git branch -D` 禁止)
- mainブランチは絶対に削除しない
