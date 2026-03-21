# Agent Rules

## Rust Documentation

- Use Rust doc comments (`///`) for public items and module-level docs (`//!`) where helpful for context.
- Document intent, parameters, return values, and side effects for non-trivial functions and methods.
- Include rustdoc sections like `# Errors`, `# Panics`, and `# Safety` when applicable.
- Keep examples concise and realistic; prefer examples that compile when practical.
- Update docs alongside code changes so behavior and constraints remain accurate.

## Validation & Verification

- Run `cargo test` after Rust code changes and before committing.
- Run targeted tests for touched modules when practical.
- If validation cannot be run, clearly state what was not validated and why.

## GitHub Workflow

- Default to creating a feature or bugfix branch from `main` before making code changes.
- Before switching branches, run `git status -sb` and confirm the working tree is clean, or explicitly stash/commit first.
- Never carry unstaged work across branches unintentionally; if checkout is blocked or risky, stop and resolve state first.
- Stage intentionally with explicit paths when practical (avoid broad `git add .` unless requested).
- Keep commits focused and atomic: one logical change set per commit with clear messages.
- Before merging, verify validation for touched areas (at minimum `cargo test`, plus targeted tests when practical).
- Prefer merge via feature branch history; only commit directly to `main` when explicitly requested.
- After merge/push, verify with `git status -sb` and branch tracking to confirm clean synced state.
