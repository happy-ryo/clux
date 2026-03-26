ローカルで全CIパイプラインを実行する (GitHub Actionsと同等のチェック)。

## 事前検証

- `cargo --version` でRustツールチェインの存在確認
- 未保存の変更がある場合は警告

## チェック実行

以下を順番に実行し、各ステップの結果を記録する:

### 1. Format (`cargo fmt --check`)
- 失敗時: `cargo fmt` で自動修正するか確認
- 修正した場合は差分を表示

### 2. Lint (`cargo clippy --all-targets -- -D warnings`)
- 失敗時: 警告一覧を表示
- `cargo clippy --fix --allow-dirty --all-targets` で自動修正を試行するか確認

### 3. Test (`cargo test --all`)
- 失敗時: 失敗したテスト名と理由を表示

### 4. Build (`cargo build`)
- 失敗時: コンパイルエラーを表示

### 5. Dependency Audit (`cargo deny check`)
- 失敗時: ライセンス/セキュリティ問題を表示

## 結果レポート

全ステップ完了後、サマリーを表示:

```
## CI Results
| Check          | Status |
|----------------|--------|
| Format         | ✓ / ✗  |
| Clippy         | ✓ / ✗  |
| Test           | ✓ / ✗  |
| Build          | ✓ / ✗  |
| Dependency Audit| ✓ / ✗  |

<全て通過の場合>
全チェック通過。`/pr` でPR作成できます。

<失敗がある場合>
<N>件の問題があります。上記の問題を修正してから再度 `/ci` を実行してください。
```

## 自動修正モード

ユーザーが「修正して」と言った場合は、自動修正可能な問題 (fmt, clippy --fix) を自動修正し、
修正をコミットした上で再度チェックを実行する。
