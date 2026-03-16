# Job Resume Strategy

This document describes the strategy for pausing and resuming a G-code streaming
job in the Gtaurus Server, including the job lifecycle, the WebSocket API contract,
and a recommended UI flow for frontend clients.

---

## Job Lifecycle State Machine

A streaming job passes through the following states. Transitions are triggered by
WebSocket commands sent from the frontend client or by the streaming thread itself
when it reaches the end of the file.

```text
                        ┌─────────────────────────────────────┐
                        │                                     │
                        ▼                                     │
                   ┌─────────┐   stream_local_gcode      ┌──────────┐
                   │  idle   │ ─────────────────────────► │ running  │
                   └─────────┘                            └──────────┘
                        ▲                                  │        │
                        │                                  │        │
              job ends  │          pause_job               ▼        │
           (completed / │     ┌──────────────────────► ┌────────┐  │
            cancelled)  │     │                        │ paused │  │
                        │     │    resume_job          └────────┘  │
                        │     │ ◄──────────────────────────┘       │
                        │     │                                     │
                   ┌───────────────┐   cancel_job                  │
                   │  cancelled /  │ ◄─────────────────────────────┘
                   │  completed    │
                   └───────────────┘
```

### State Descriptions

| State       | Description                                                               |
|-------------|---------------------------------------------------------------------------|
| `idle`      | No job is running. The server is ready to accept a new job.               |
| `running`   | G-code lines are being streamed to the machine one by one.                |
| `paused`    | Streaming is suspended. The machine receives a hardware Feed Hold (`!`).  |
| `completed` | All lines have been sent successfully. Transitions to `idle`.             |
| `cancelled` | The job was aborted. A hardware Soft Reset (`Ctrl-X`) is issued.          |

---

## WebSocket API Reference

All commands are sent as JSON payloads over the WebSocket connection on port `9001`.

### Commands

#### `stream_local_gcode` — Start (or restart) a job

```json
{
  "command": "stream_local_gcode",
  "args": {
    "filePath": "/path/to/file.gcode",
    "startLine": 0
  }
}
```

- `filePath` *(required)*: Absolute path to the G-code file on the host machine.
- `startLine` *(optional, default `0`)*: The **raw file line number** (1-indexed) to
  begin streaming from. `0` is a sentinel meaning "start from the beginning" and behaves
  identically to `1`. Any value ≥ 2 skips all preceding file lines. Note that
  `startLine` counts every line in the file (including blank lines and comments), while
  `currentLine` in `job://status` events counts only active (non-blank, non-comment)
  lines. These two values are not directly interchangeable; see
  [Partial Resume](#partial-resume-restart-from-a-specific-line) for guidance.

#### `get_job_status` — Poll current job state

```json
{ "command": "get_job_status", "args": {} }
```

Response payload:

```json
{
  "status": "running",
  "filePath": "/path/to/file.gcode",
  "currentLine": 42,
  "totalLines": 200
}
```

#### `pause_job` — Pause a running job

```json
{ "command": "pause_job", "args": {} }
```

Issues a hardware Feed Hold (`!`) to the machine and halts line delivery.

#### `resume_job` — Resume a paused job

```json
{ "command": "resume_job", "args": {} }
```

Issues a hardware Cycle Start (`~`) and continues streaming from the next line.

#### `cancel_job` — Cancel a running or paused job

```json
{ "command": "cancel_job", "args": {} }
```

Clears the pause flag, sets the cancel flag, and issues a hardware Soft Reset
(`Ctrl-X`) to flush the machine's motion buffer immediately.

### Real-Time Events (`job://status`)

While a job is active, the server broadcasts progress events to every subscribed
WebSocket client:

```json
{
  "type": "event",
  "event": "job://status",
  "payload": {
    "status": "running",
    "currentLine": 42,
    "totalLines": 200
  }
}
```

The `status` field contains one of: `running`, `paused`, `cancelled`, `completed`.

---

## Recommended UI Flow

The following diagram shows the recommended sequence of user interactions and
server events from the perspective of a frontend client.

```text
  User Action                  Frontend UI                      Server Event
  ───────────────────────────────────────────────────────────────────────────

  [Open Job Panel]
        │
        ├──── send: get_job_status ──────────────────────────────────────────►
        │◄─── response: { status, currentLine, totalLines } ─────────────────
        │
        │     Render progress bar at currentLine / totalLines
        │
  [Select File]
        │
        ├──── send: stream_local_gcode { filePath, startLine: 0 } ──────────►
        │                                                    ◄── job://status
        │     status = "running"                                { running, 0 }
        │     Show: [Pause] [Cancel] buttons
        │     Update progress bar on each job://status event
        │
  [Pause]
        │
        ├──── send: pause_job ───────────────────────────────────────────────►
        │                                                    ◄── job://status
        │     status = "paused"                                 { paused, N }
        │     Show: [Resume] [Cancel] buttons
        │     Show: "Paused at line N of M"
        │
  [Resume]
        │
        ├──── send: resume_job ──────────────────────────────────────────────►
        │                                                    ◄── job://status
        │     status = "running"                               { running, N }
        │     Show: [Pause] [Cancel] buttons
        │     Continue updating progress bar
        │
  [Cancel]
        │
        ├──── send: cancel_job ──────────────────────────────────────────────►
        │                                                    ◄── job://status
        │     status = "cancelled"                          { cancelled, N }
        │     Show: [Resume from line N] [Start Over] buttons
        │     Remember lastLine = N (active lines sent) for display
        │
  [Resume from line N]          (restart cancelled job from last position)
        │
        ├──── send: stream_local_gcode { filePath, startLine: rawFileLine } ►
        │          (rawFileLine must be tracked separately — see Partial Resume)
        │                                                    ◄── job://status
        │     status = "running"                            { running, ... }
        │     Show: [Pause] [Cancel] buttons
        │
  [Job completes]
        │                                                    ◄── job://status
        │     status = "completed"                          { completed, M }
        │     Show: "Job complete!" notification
        │     Show: [Start Over] button
```

### UI Component State Summary

| Server Status | Primary Button | Secondary Button | Progress Bar         |
|---------------|----------------|------------------|----------------------|
| `idle`        | Start          | —                | Hidden               |
| `running`     | Pause          | Cancel           | Visible              |
| `paused`      | Resume         | Cancel           | Visible (static)     |
| `cancelled`   | Resume from N  | Start Over       | Visible (static)     |
| `completed`   | Start Over     | —                | Full (100%)          |

---

## Partial Resume (Restart from a Specific Line)

The `startLine` parameter in `stream_local_gcode` enables mid-file restarts.
This is useful when a job is cancelled after a tool change or power interruption
and the operator wants to skip already-completed work.

### `startLine` vs `currentLine` — Key Distinction

| Value         | What it counts                                                          |
|---------------|-------------------------------------------------------------------------|
| `startLine`   | Raw file line number (1-indexed, includes blank lines and comments)     |
| `currentLine` | Active lines sent (non-blank, non-comment lines only)                   |

Because these two values count different things, a frontend **cannot** pass
`currentLine` directly as `startLine` and expect a correct resume position in
files that contain blank lines or comments. Frontends that need accurate mid-file
restart capability should track the raw file line number of the last dispatched
command independently.

For G-code files with no blank lines or comment-only lines, `currentLine` and
the corresponding raw file line number coincide and the resume is exact.

### Operator Considerations

- The machine must be homed and the work coordinate origin re-established before
  resuming from a mid-file line. Otherwise the tool may move to unexpected positions.
- Lines skipped by `startLine` include all setup commands (feed rate, spindle speed,
  absolute/relative mode). The operator or the frontend must ensure the machine is
  in the correct modal state before restarting.
- The server counts only *active* lines (non-empty, non-comment) toward `currentLine`
  in `job://status` events. `totalLines` is also an active line count.
