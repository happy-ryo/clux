cluxワークスペースをビルドする。

## 手順

1. `cargo build` でdebugビルド
2. 成功 → 「Build successful」と報告
3. 失敗 → コンパイルエラーを表示し、修正を試みる

`release` と引数を付けた場合は `cargo build --release` でリリースビルド。
