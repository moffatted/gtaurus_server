---
trigger: always_on
---

# Tech Stack

## Runtime

- Rust stable via Cargo.
- Tokio async runtime for websocket and driver orchestration.

## Crates and Modules

- `src/main.rs`: process entry point.
- `src/ws_server.rs`: websocket transport/session handling.
- `src/driver.rs`: machine/driver orchestration layer.
- `src/camera.rs`: camera-facing operations and integration points.

## Integration Context

- This server is consumed by the Tauri desktop app and shared Rust libraries.
- Keep command/payload contracts stable for frontend compatibility.

## Engineering Direction

- Prefer explicit types and `Result`-driven error propagation.
- Validate all external input at websocket/command boundaries.
- Keep transport logic separated from domain behavior.
- Preserve backward compatibility whenever practical.
