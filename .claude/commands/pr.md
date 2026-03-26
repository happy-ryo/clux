現在のフィーチャーブランチからPull Requestを作成する。

## 手順

1. **事前チェック**
   - 現在のブランチがmainでないことを確認
   - `cargo fmt --check` でフォーマット確認
   - `cargo clippy --all-targets -- -D warnings` でlint確認
   - `cargo test --all` でテスト確認
   - `cargo build` でビルド確認
   - いずれか失敗したら修正してから再実行

2. **差分確認**
   - `git diff main...HEAD` で全変更内容を確認
   - `git log main..HEAD --oneline` でコミット一覧確認

3. **PR作成**
   - ブランチ名からIssue番号を抽出
   - リモートにプッシュ: `git push -u origin <ブランチ名>`
   - `gh pr create` でPR作成:
     - タイトル: 変更内容を簡潔に (70文字以内)
     - ボディ: `## Summary` + 変更の箇条書き + `## Test plan` + `Closes #<Issue番号>`
   - PR URLをユーザーに報告

## PRボディテンプレート
```
## Summary
- <変更内容1>
- <変更内容2>

## Test plan
- [ ] <テスト項目>

Closes #<Issue番号>

🤖 Generated with [Claude Code](https://claude.ai/code)
```

## 注意
- CIチェックが全て通ることを確認してからPRを作成する
- PR作成後、マージはユーザーの判断に委ねる
