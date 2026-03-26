Run the full CI pipeline locally (same checks as GitHub Actions).

Steps:
1. `cargo fmt --check` -- formatting check
2. `cargo clippy --all-targets` -- lint check
3. `cargo test --all` -- run tests
4. `cargo build` -- build check
5. `cargo deny check` -- dependency audit

Report a summary of all results. Fix any issues found automatically where possible.
