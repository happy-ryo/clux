Lint/フォーマット/依存監査チェックを実行する。

## 手順

1. `cargo fmt --check` でフォーマット確認
   - 失敗 → 差分を表示し、「`cargo fmt` で修正しますか？」と確認
2. `cargo clippy --all-targets -- -D warnings` でlint確認
   - 失敗 → 警告一覧を表示し、「自動修正を試みますか？」と確認
3. `cargo deny check` で依存監査
   - 失敗 → ライセンス/セキュリティ問題を報告

結果サマリー:
```
Lint Results: fmt ✓/✗ | clippy ✓/✗ | deny ✓/✗
```

「修正して」と言われたら、自動修正可能なものを全て修正する。
