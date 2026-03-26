Run all CI lint checks (clippy, fmt, deny) on the workspace.

Steps:
1. Run `cargo clippy --all-targets` and report any warnings
2. Run `cargo fmt --check` and report any formatting issues
3. Run `cargo deny check` and report any license/advisory issues
4. Summarize: all passed or list failures

Fix any issues found automatically where possible.
