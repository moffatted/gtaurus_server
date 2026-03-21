# GTaurus Server API Documentation

## Scope

This document reflects the currently implemented WebSocket invoke API in the standalone server.
Source of truth: `src/ws_server.rs` (`handle_invoke` routing).

## Transport and Envelope

Clients send JSON frames of this shape:

```json
{
  "type": "invoke",
  "id": "req-123",
  "cmd": "get_connection_status",
  "args": {}
}
```

Server replies with:

Success:

```json
{
  "type": "response",
  "id": "req-123",
  "payload": { "...": "..." }
}
```

Error:

```json
{
  "type": "response",
  "id": "req-123",
  "error": "message"
}
```

## Server Events

Machine RX event (broadcast):

```json
{
  "type": "event",
  "event": "fluidnc://rx",
  "payload": "<Idle|MPos:0.000,0.000,0.000|FS:0,0>"
}
```

Job status event (broadcast):

```json
{
  "type": "event",
  "event": "job://status",
  "payload": {
    "status": "running",
    "currentLine": 120,
    "totalLines": 2048
  }
}
```

## Implemented Commands

### Connection

- `list_serial_ports`
  - args: `{}`
  - returns: `string[]`

- `get_connection_status`
  - args: `{}`
  - returns: status string, for example: `"Disconnected"`, `"Serial: COM3"`, `"WiFi: 192.168.1.10:23"`

- `send_gcode`
  - args: `{ "cmd": "G0 X10 Y10" }`
  - returns: `null`

- `send_realtime`
  - args: `{ "byte": 24 }` (`24 == 0x18` soft reset)
  - returns: `null`

- `connect_serial`
  - args accepted:
    - `{ "port_name": "COM3", "baud_rate": 115200 }`
    - `{ "portName": "COM3", "baudRate": 115200 }`
  - returns: success message string

- `connect_telnet`
  - args accepted:
    - `{ "host": "192.168.1.120", "ws_port": 23 }`
    - `{ "host": "192.168.1.120", "wsPort": 23 }`
  - returns: success message string

- `disconnect`
  - args: `{}`
  - returns: `null`

- `resume_auto_connect`
  - args: `{}`
  - returns: `"Auto-connect resumed"`

### Local File Operations

- `ensure_dir_exists`
  - args: `{ "path": "C:/jobs" }`

- `list_local_files`
  - args: `{ "path": "C:/jobs" }`
  - returns: array of `{ name, size, modified }`

- `read_local_file`
  - args: `{ "path": "C:/jobs", "filename": "part.nc" }`
  - returns: file contents as string

- `save_local_file`
  - args: `{ "path": "C:/jobs", "filename": "part.nc", "content": "..." }`

- `delete_local_file`
  - args: `{ "path": "C:/jobs", "filename": "part.nc" }`

- `copy_to_storage`
  - args: `{ "sourcePath": "C:/tmp/part.nc", "destDir": "C:/jobs" }`

- `get_home_dir`
  - args: `{}`
  - returns: home directory string

- `validate_gcode_file`
  - args: `{ "path": "C:/jobs/part.nc" }`
  - returns: boolean

### Job Control and Streaming

- `stream_local_gcode`
  - args:

```json
{
  "path": "C:/jobs/part.nc",
  "feedRateOverride": 900.0,
  "startLine": 1
}
```

- `get_job_status`
  - args: `{}`
  - returns:

```json
{
  "status": "running",
  "filePath": "C:/jobs/part.nc",
  "currentLine": 42,
  "totalLines": 2048
}
```

- `pause_job`
- `resume_job`
- `cancel_job`
  - args for each: `{}`

### Camera / G-code / Surfacing

- `get_camera_settings`
  - args: `{ "configPath": "...optional..." }`

- `set_camera_settings`
  - args:

```json
{
  "configPath": "...optional...",
  "updates": {
    "brightness": 10,
    "contrast": 5
  }
}
```

- `parse_gcode_file`
  - args accepted:
    - `{ "path": "C:/jobs/part.nc" }`
    - `{ "filePath": "C:/jobs/part.nc" }`

- `generate_surfacing_toolpath`
  - args: see `src/ws_surfacing.rs`; both camelCase and snake_case variants are accepted for several fields.

### Checkpoint

- `save_checkpoint`
  - args:

```json
{
  "checkpoint": { "...": "JobCheckpoint fields" },
  "savePath": "C:/jobs/part.resume.json"
}
```

- `load_checkpoint`
  - args accepted:
    - `{ "checkpointPath": "C:/jobs/part.resume.json" }`
    - `{ "path": "C:/jobs/part.resume.json" }`
    - `{ "filePath": "C:/jobs/part.resume.json" }`

- `get_resume_checkpoint_path`
  - args accepted:
    - `{ "path": "C:/jobs/part.nc" }`
    - `{ "filePath": "C:/jobs/part.nc" }`

- `compute_file_hash`
  - args accepted:
    - `{ "path": "C:/jobs/part.nc" }`
    - `{ "filePath": "C:/jobs/part.nc" }`

## Notes

- Unknown commands return: `"Command <cmd> not implemented in standalone server"`.
- This server has no built-in authentication; deploy behind network controls.

## Client Quickstart

Below is a minimal end-to-end flow using the exact wire format.

1. Query connection status

```json
{
  "type": "invoke",
  "id": "1",
  "cmd": "get_connection_status",
  "args": {}
}
```

2. Connect (serial)

```json
{
  "type": "invoke",
  "id": "2",
  "cmd": "connect_serial",
  "args": {
    "portName": "COM3",
    "baudRate": 115200
  }
}
```

3. Start streaming a local file

```json
{
  "type": "invoke",
  "id": "3",
  "cmd": "stream_local_gcode",
  "args": {
    "path": "C:/jobs/part.nc",
    "feedRateOverride": 900.0,
    "startLine": 1
  }
}
```

4. Pause and resume

```json
{
  "type": "invoke",
  "id": "4",
  "cmd": "pause_job",
  "args": {}
}
```

```json
{
  "type": "invoke",
  "id": "5",
  "cmd": "resume_job",
  "args": {}
}
```

5. Save checkpoint during/after run

```json
{
  "type": "invoke",
  "id": "6",
  "cmd": "save_checkpoint",
  "args": {
    "checkpoint": { "checkpoint_id": "job-001", "file_path": "C:/jobs/part.nc" },
    "savePath": "C:/jobs/part.resume.json"
  }
}
```

6. Disconnect

```json
{
  "type": "invoke",
  "id": "7",
  "cmd": "disconnect",
  "args": {}
}
```
