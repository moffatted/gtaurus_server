# GitHub Workflow Rules

## Pull Request Quality

- Keep PRs narrowly scoped and reviewable.
- Include problem statement, approach, risk, and test evidence.
- Link incidents/issues when fixing production defects.
- Require at least one human reviewer for behavior changes.

## CI Expectations

A server PR should pass:

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test`
4. `cargo check`

## Copilot Review Guidance

- Use Copilot review for broad/static feedback.
- Human review remains required for protocol, concurrency, and safety-sensitive changes.
- Ensure PR includes deterministic repro and regression test for bug fixes.
