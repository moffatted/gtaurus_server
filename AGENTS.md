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
