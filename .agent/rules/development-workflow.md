# Development Workflow Rules

## Copilot-First Loop

1. Understand the request and impacted modules.
2. Read existing call sites before changing contracts.
3. Implement minimal, focused edits.
4. Validate immediately with local checks.
5. Summarize behavior impact and verification.

## Validation Commands

Run the following after meaningful edits:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo check
```

## Server Reliability Checklist

- Validate malformed websocket payload handling.
- Verify reconnect/disconnect lifecycle behavior.
- Ensure cancellation/shutdown paths do not leak tasks.
- Keep retries bounded and observable.
- Add regression tests for production bug fixes.

## Git Practice

- Do not commit unless explicitly requested.
- Keep commits atomic and concern-focused.
- Check remote state (`git fetch`) before starting large tasks.
