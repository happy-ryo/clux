現在のフィーチャーブランチからPull Requestを作成する。

## 事前検証

以下を全てチェックし、問題があれば是正策を提示:

1. **ブランチ確認**: 現在のブランチがmainでないこと
   - mainにいる → 「mainブランチではPRを作成できません。`/start <番号>` でIssueの作業を開始してください」
2. **未コミット変更**: `git status --porcelain` で未コミット変更がないか
   - 変更あり → 「未コミットの変更があります。コミットしてからPRを作成してください」
3. **コミット存在確認**: `git log main..HEAD --oneline` でmainとの差分コミットがあるか
   - なし → 「mainとの差分がありません。先に実装をコミットしてください」
4. **既存PR確認**: `gh pr list --repo happy-ryo/clux --head <ブランチ名>` でPRが既に存在するか
   - 既存あり → 「既にPR #<番号> が存在します。更新しますか？」と確認し、pushのみ実行

## CIチェック (自動修正付き)

順番に実行し、失敗したら自動修正を試みる:

1. **フォーマット**: `cargo fmt --check`
   - 失敗 → `cargo fmt` で自動修正 → 修正をコミット → 再チェック
2. **Lint**: `cargo clippy --all-targets -- -D warnings`
   - 失敗 → `cargo clippy --fix --allow-dirty --all-targets` で自動修正を試行
   - 自動修正不可 → 問題箇所を表示して手動修正を依頼、**PRは作成しない**
3. **テスト**: `cargo test --all`
   - 失敗 → テスト名と失敗理由を表示、**PRは作成しない**
4. **ビルド**: `cargo build`
   - 失敗 → エラー内容を表示、**PRは作成しない**

全チェック通過後のみ次のステップに進む。

## 差分分析

- `git diff main...HEAD --stat` で変更ファイル一覧
- `git log main..HEAD --oneline` でコミット一覧
- 変更の性質を分析 (新機能/バグ修正/リファクタリング/テスト追加)

## PR作成

1. **Issue番号抽出**: ブランチ名 `feature/issue-<N>-*` からNを抽出
2. **リモートプッシュ**: `git push -u origin <ブランチ名>`
3. **PR作成**: `gh pr create` で以下のフォーマットを使用:
   - タイトル: 変更内容を簡潔に (70文字以内、英語)
   - ボディ: 下記テンプレート
   - Milestone: Issueと同じMilestoneを設定

```
gh pr create --title "<タイトル>" --body "$(cat <<'EOF'
## Summary
- <変更内容の箇条書き>

## Test plan
- [ ] <テスト項目>

Closes #<Issue番号>

🤖 Generated with [Claude Code](https://claude.ai/code)
EOF
)"
```

4. **結果報告**:
   ```
   ## PR作成完了
   **URL**: <PR URL>
   **タイトル**: <タイトル>
   **Closes**: #<Issue番号>

   CIの結果を待ち、問題なければマージしてください。
   マージ後は `/done` で後片付けできます。
   ```

## エラーリカバリ

- push失敗 (認証) → `gh auth status` で状態確認を案内
- push失敗 (リモート差分) → `git pull --rebase origin <ブランチ>` を提案
- PR作成失敗 → エラー内容を表示、`gh` の認証・権限を確認
