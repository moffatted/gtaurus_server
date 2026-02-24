# Gtaurus Standalone Server

The `gtaurus_server` is a standalone backend application written in Rust. It provides a WebSocket bridge between the Gtaurus web frontend and a CNC machine running FluidNC.

## Purpose

Web browsers have strict security models that prevent them from directly interacting with raw local hardware such as Serial/USB ports. While modern browsers have some experimental capabilities like WebSerial or WebUSB, they are not universally supported and can be restrictive.

To run Gtaurus as a pure web application (accessing it via browser from another device or machine) while still controlling local hardware attached to a host computer, you need a local broker.

The `gtaurus_server` acts as this broker:

- It runs locally on the machine physically connected to the CNC controller.
- It exposes a WebSocket server on `ws://0.0.0.0:9001` that the Gtaurus web frontend can connect to.
- It translates JSON payloads via WebSocket into raw hardware commands (Serial/USB or Telnet) to drive the FluidNC controller.
- It forwards real-time hardware status and responses back to the frontend.

## Technical Details

- **Language**: Rust
- **Transport Protocol**: WebSockets via `tokio-tungstenite`.
- **Port**: `9001` (by default).
- **Driver Architecture**: Implements a `FluidNCDriver` containing the same serial and network logic as the Tauri desktop application, but decoupled from the desktop windowing environment.
- **Message Format**: Uses a lightweight JSON wrapper over WebSocket:
  - Frontend to Server (`invoke`): `{ "type": "invoke", "cmd": "send_gcode", "args": { "cmd": "G0 X10" }, "id": "req_1" }`
  - Server to Frontend (`response`): `{ "type": "response", "id": "req_1", "payload": null }`
  - Server to Frontend Event (`event`): `{ "type": "event", "event": "fluidnc://rx", "payload": "..." }`

## Prerequisites

- Request cargo/rust (`rustup`) toolchain.
- Standard build tools depending on the OS (e.g. build-essential on Linux, MSVC on Windows).

## Building

To build the server for release:

```bash
cargo build --release
```

The compiled binary will be located in the `target/release/` directory.

## Running

You can run the application directly with Cargo during development:

```bash
cargo run
```

Or you can run the compiled binary:

```bash
./target/release/gtaurus_server
```

When started, you should see:

```text
Starting Gtaurus Standalone Server...
[WS] Server listening on ws://0.0.0.0:9001
```

Once it's running, you can open the Gtaurus web frontend in your browser, and it will automatically attempt to connect to this server when not running in the Tauri desktop environment.
