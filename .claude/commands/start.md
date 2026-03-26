Issue の実装を開始する。引数にIssue番号を指定する。

## 手順

1. **Issue内容の確認**
   - `gh issue view <番号> --repo happy-ryo/clux` でIssue内容を取得
   - タスクの概要と完了条件を把握する

2. **ブランチ作成**
   - mainブランチを最新にする: `git checkout main && git pull`
   - フィーチャーブランチを作成: `git checkout -b feature/issue-<番号>-<短い説明>`
   - ブランチ名は英語小文字ケバブケース (例: `feature/issue-2-conpty-wrapper`)

3. **作業計画**
   - Issueのタスクリストを元にTaskCreateで作業タスクを作成
   - 依存関係があればTaskUpdateでblockedByを設定

4. **ユーザーへの報告**
   - Issue内容の要約
   - 作成したブランチ名
   - 作業計画の概要

## 注意
- mainブランチでは作業しない
- 既にフィーチャーブランチがある場合はそれを使う
- 引数がない場合は `gh issue list --repo happy-ryo/clux --state open` でオープンなIssue一覧を表示し、どれに取り組むか確認する
