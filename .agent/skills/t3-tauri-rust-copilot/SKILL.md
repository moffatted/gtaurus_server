---
name: t3-tauri-rust-copilot
description: Use when implementing or reviewing gtaurus_server changes that affect websocket contracts, driver orchestration, or Tauri frontend compatibility.
---

# t3-tauri-rust-copilot

Workflow skill for developing `gtaurus_server` safely with GitHub Copilot in a T3 + Tauri + Rust ecosystem.

## Use This Skill When

- Changing websocket message formats or handlers.
- Modifying driver/camera orchestration behavior.
- Fixing reliability bugs (disconnect, timeout, retries, cancellation).
- Updating behavior consumed by the Tauri frontend.

## Required Workflow

1. Map impacted modules and public contract surfaces.
2. Preserve compatibility or document explicit breaking changes.
3. Keep domain logic and transport logic separated.
4. Add or update regression tests for changed behavior.
5. Run fmt, clippy, test, and check before handoff.

## Quality Gates

- No panics in request handling path.
- Errors include actionable context.
- Concurrency behavior is deterministic under failure.
- Client-visible contract changes are documented.
