# Failure-Injection Matrix & Validation Results (TODO 42)

## Overview & Execution Context
This document specifies the systematic failure-injection matrix for Directioner-OS core services (`ai-capabilityd`, `ai-control-plane`, journal, storage, timekeeping).

> **Safety Constraint (42.3)**: All failure-injection tests execute **strictly inside ephemeral virtual machines (QEMU / KVM test fixtures) or isolated hermetic simulation test harnesses**. They are NEVER run on host machines or production bare-metal environments.

---

## Failure Injection Matrix

| Case ID | Failure Mode | Target Subsystem | Expected Behavior (Written Prior to Test) | Observed Outcome | Status | Notes & Artifacts |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **FI-01** | **Kill Daemon (`SIGKILL`)** | `ai-capabilityd` / Control Plane | Systemd restarts daemon within configured restart limit; client receives socket disconnect error, transitions to degraded posture, and retries with exponential backoff without panicking. | Client detects socket EOF cleanly; systemd restarts unit; client reconnects after daemon re-binds. | **PASS** | Verified via socket reconnect harness and systemd unit watchdog. |
| **FI-02** | **Fill Disk (ENOSPC)** | Ephemeral / Target storage (`/var/log`, `/tmp`) | Control Plane detects `ENOSPC`, halts state modifications, returns structured error (`IoError`), rejects new plan executions safely, and preserves existing append-only audit trail integrity. | Writes fail cleanly with `StorageFull` / `ENOSPC`; existing worklog hashes remain uncorrupted; journal anomaly triggers. | **PASS** | Worklog integrity checked with `ai-worklog-verify`. |
| **FI-03** | **Corrupt Log File** | Worklog / Audit Journal | `ai-worklog-verify` and Control Plane detect cryptographic hash chain mismatch; state is marked quarantined/corrupted; system alerts operator without infinite crash loop. | Chain verification detects tampered hash entry at corrupt sequence index; error logged, quarantine flagged. | **PASS** | Evaluated via hash mismatch and HMAC tampering fixtures. |
| **FI-04** | **Drop Local Socket** | `/run/ai-engineer/capability.sock` | When socket file is removed or connection aborted abruptly, client logs connectivity error, drops inflight uncommitted transactions, triggers clean rollback, and enters degraded status. | Client fails fast with `BrokenPipe` / `NotFound`; control plane refuses blind execution; safe fallback initiated. | **PASS** | Verified in local socket hardening tests. |
| **FI-05** | **Clock Jump (Skew/NTP jump)** | System Clock / Timekeeper | System clock shifted forwards (+24h) or backwards (-2h). Monotonic timestamps (`Instant`) prevent negative durations or scheduler lockups; RFC3339 timestamps recorded with valid timezone; drift anomaly logged. | Monotonic timers maintain correct pacing; audit chain checks sequence monotonicity; timestamps record jump. | **PASS** | Monotonic ordering maintained across evaluation cycle. |

---

## Detailed Failure Test Scenarios & Invariant Verification

### FI-01: Daemon Kill & Recovery
- **Injection Method**: `SIGKILL -9` against running daemon process.
- **Invariants Checked**:
  1. Socket file permissions stay restricted (`0600`).
  2. No dangling zombie threads or lock leaks.
  3. Client surfaces `ClientError::ConnectionRefused` or `BrokenPipe` without panicking.

### FI-02: Disk Full (`ENOSPC`)
- **Injection Method**: Simulated write to full volume or synthetic file write exceeding quota.
- **Invariants Checked**:
  1. No partial corrupt JSON lines written to audit journal.
  2. Database rollback or atomic rename fails safely without overwriting previous valid snapshot.
  3. Control plane refuses to proceed with plans requiring disk space.

### FI-03: Corrupt Audit Log
- **Injection Method**: Bit-flip in intermediate SHA256 digest within worklog entry.
- **Invariants Checked**:
  1. Hash chain verification detects tampered hash entry.
  2. The exact offending entry index is reported.
  3. Subsequent executions flag audit violation.

### FI-04: Dropped Capability Socket
- **Injection Method**: Unlink `/run/ai-engineer/capability.sock` while plan is staged.
- **Invariants Checked**:
  1. No hang; timeout elapses and reports socket unavailability.
  2. Staged steps abort without partially executing.

### FI-05: Clock Jump
- **Injection Method**: Synthetic time-shift simulating NTP sync skew or VM resume jump.
- **Invariants Checked**:
  1. Task timers using `std::time::Instant` continue monotonically.
  2. Audit chain index increments monotonically regardless of wall-clock skew.
