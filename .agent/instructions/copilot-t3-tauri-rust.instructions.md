---
applyTo: "**/*.rs"
description: Best practices for using GitHub Copilot in gtaurus_server (Rust server for T3 + Tauri app).
---

# Copilot Instructions: T3 + Tauri + Rust Server

## Design Principles

- Keep APIs predictable and version-friendly.
- Treat all client input as untrusted.
- Keep failure paths explicit and recoverable.

## Coding Expectations

- Prefer small functions with single responsibility.
- Use typed enums/structs for protocol payloads and states.
- Avoid hidden side effects in async tasks.
- Keep locks short-lived; prefer message passing where practical.

## Contract and Error Rules

- Preserve backward compatibility for websocket payloads when possible.
- Validate payload shape and required fields before dispatch.
- Return structured error categories (validation, protocol, transport, internal).
- Include context in logs without leaking secrets.

## Verification Rules

- Add regression tests for every bug fix.
- Verify reconnect and partial failure behavior for transport changes.
- Run `cargo fmt`, `cargo clippy`, `cargo test`, and `cargo check` before finalizing.
