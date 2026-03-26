プロジェクト全体の進捗状況を表示する。

## 収集する情報

### 1. Git状態
- 現在のブランチ: `git branch --show-current`
- 未コミットの変更: `git status --porcelain`
- リモートとの差分: `git log --oneline origin/main..HEAD` (featureブランチの場合)

### 2. Milestone進捗
```bash
gh api repos/happy-ryo/clux/milestones --jq '.[] | "\(.title)|\(.open_issues)|\(.closed_issues)"'
```
各Milestoneのオープン/クローズ比率をプログレスバーで表示。

### 3. オープンIssue
```bash
gh issue list --repo happy-ryo/clux --state open --json number,title,milestone,labels
```
現在のPhase (最もIssueが多い未完了Milestone) のIssueを優先表示。

### 4. オープンPR
```bash
gh pr list --repo happy-ryo/clux --json number,title,headRefName,reviewDecision,statusCheckRollup
```
各PRのCIステータスとレビュー状態を含めて表示。

## 表示フォーマット

```
## clux Project Status

### Git
- ブランチ: `<ブランチ名>`
- 未コミット変更: <あり/なし>
- リモート差分: <N commits ahead>

### Milestones
Phase 1: Foundation     [████████░░] 80% (4/5 closed)
Phase 2: Layout         [░░░░░░░░░░]  0% (0/3 closed)
Phase 3: Session        [░░░░░░░░░░]  0% (0/2 closed)
Phase 4: Coordination   [░░░░░░░░░░]  0% (0/2 closed)
Phase 5: Refinement     [░░░░░░░░░░]  0% (0/1 closed)

### 現在のPhaseのIssue (<Phase名>)
- [ ] #<番号>: <タイトル>
- [x] #<番号>: <タイトル> (closed)

### オープンPR
- PR #<番号>: <タイトル> [CI: ✓/✗] [Review: pending/approved]

### 次のアクション
<現在の状態に応じた推奨アクションを1つ提示>
```
