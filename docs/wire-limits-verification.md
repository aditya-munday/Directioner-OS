# Verification Record: Socket Wire Limits (TODO 001)

## Scope
- Area: `ai-capabilityd` Unix socket IPC framing
- Plan Item: **TODO 001** (Wave 1, Gate: `OPEN`)
- Issues: #16 (Spec), #17 (Impl), #18 (Verification)
- Pull Requests:
  - PR-1: [#19](https://github.com/aditya-munday/Directioner-OS/pull/19) (`plan/001-wire-limits-1`)
  - PR-2: [#20](https://github.com/aditya-munday/Directioner-OS/pull/20) (`plan/001-wire-limits-2`)
  - PR-3: `plan/001-wire-limits-3`

## Subtasks Verification

| Subtask | Requirement | Implementation | Evidence |
|---|---|---|---|
| 1.1 | Document pre-fix framing vulnerability | `BufRead::read_line` had unbounded buffer growth on long lines | Confirmed pre-fix in `server.rs` |
| 1.2 | Hard max request size (64 KiB default) | `read_bounded_line` consumes buffer incrementally and halts at `max_request_bytes` without allocation | `test_wire_limit_oversized_payload_rejected_with_structured_error` (ok) |
| 1.3 | Per-connection read timeout (5s default) | `stream.set_read_timeout(Some(config.read_timeout))` with structured `Timeout` response | `test_wire_limit_read_timeout_slow_client` (ok) |
| 1.4 | Max concurrent connections cap (64 default) | `active_connections` atomic counter; excess connections rejected immediately with structured error | `test_wire_limit_max_concurrent_connections_cap` (ok) |
| 1.5 | Structured error responses | Returns typed `CapabilityResponse::Error` without panics | Validated JSON error shapes in all tests |

## Stress Test Results

### 1. 10 MB Garbage Line Test
- **Action**: Streamed 160 × 64 KiB (10 MiB) chunks of garbage bytes into the socket without newline.
- **Result**: Server rejected payload upon hitting 64 KiB limit without unbounded heap allocation. Returned `CapabilityResponse::Error` with message `request payload exceeds maximum allowed size of 65536 bytes`.
- **Verdict**: PASS (`test_stress_10mb_garbage_rejection`).

### 2. Never-Ending Stalled Client Test
- **Action**: Malicious client connected and streamed incomplete JSON prefix `{"never_ending": "streaming...` then idled indefinitely.
- **Concurrently**: Normal client connected and dispatched `Ping` request.
- **Result**: Normal client received `CapabilityResponse::Success` in 2ms. Malicious connection timed out cleanly and was closed by server without blocking listener.
- **Verdict**: PASS (`test_stress_never_ending_line_daemon_stays_responsive`).

### 3. Connection Flood Cap Test
- **Action**: Connected client pool exceeding configured `max_concurrent_connections`.
- **Result**: Excess client received immediate error `server connection limit of 2 exceeded` and socket was closed cleanly.
- **Verdict**: PASS (`test_wire_limit_max_concurrent_connections_cap`).
