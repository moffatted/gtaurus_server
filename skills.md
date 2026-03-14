# Skills Guide for gtaurus_server

## Purpose

This file defines engineering skills and quality standards for the
Rust server component in this workspace.

The focus is reliable transport, camera control orchestration,
and operational safety.

## Core Skill Areas

### 1. Server Contract Discipline

- Treat websocket and command payloads as stable contracts.
- Version payloads when behavior changes are unavoidable.
- Keep backward compatibility whenever possible.
- Prefer additive fields over breaking shape changes.

### 2. Input Validation and Boundary Safety

- Validate all inbound payloads before dispatch.
- Reject malformed or unknown commands clearly.
- Sanitize file paths and external identifiers.
- Avoid implicit defaults that hide user input errors.

### 3. Transport and Session Management

- Keep websocket session lifecycle explicit and observable.
- Handle disconnect, reconnect, and stale clients safely.
- Bound all queues and buffers used by client sessions.
- Separate protocol framing from business behavior.

### 4. Driver and Camera Orchestration

- Isolate hardware-specific behavior behind trait boundaries.
- Keep command sequencing deterministic and testable.
- Use explicit state transitions for long-running jobs.
- Preserve clear recovery behavior after partial failures.

### 5. Error Handling and Recovery

- Use structured error types with stable categories.
- Preserve causal context when mapping lower-level failures.
- Return actionable failures to clients and logs.
- Avoid panic paths in request handling workflows.

### 6. Concurrency and Lifecycle Control

- Make ownership and locking strategy explicit.
- Prefer message passing for cross-task coordination.
- Ensure graceful shutdown closes sessions and tasks cleanly.
- Use cancellation-aware loops and bounded retries.

### 7. Observability and Operations

- Emit structured logs at command receive and completion.
- Include correlation ids for multi-step job workflows.
- Distinguish expected client errors from server faults.
- Keep startup validation strict with actionable diagnostics.

### 8. Testing Strategy

- Unit tests for parsing, validation, and state transitions.
- Integration tests for websocket flow and command handling.
- Transport tests for timeout and retry semantics.
- Regression tests for every production incident.

## Coding Standards

- Keep modules focused: ws, driver, camera, and runtime concerns.
- Avoid mixing large refactors with behavior changes.
- Favor explicit names and stable public interfaces.
- Keep clippy warnings low and fix meaningful ones before merge.
- After creating or editing any .md file, run markdownlint CLI and fix all warnings.

## Definition of Done Checklist

- Contract changes are documented and versioned as needed.
- Input validation covers all new entry points.
- Errors are structured, contextual, and user-actionable.
- Tests prove the new behavior and key failure paths.
- Logs and diagnostics support troubleshooting in production.
- Formatting, linting, and tests pass.

## Pre-Merge Review Heuristics

- Can malformed client input cause undefined behavior?
- Are reconnect and partial-failure paths deterministic?
- Is job progress and failure state observable in logs?
- Can this behavior be validated without physical hardware?
- Does the change preserve existing client compatibility?
