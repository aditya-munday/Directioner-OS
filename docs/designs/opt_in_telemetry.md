# System Architecture Design: Opt-In Telemetry Subsystem (TODO 49)

**Author:** Directioner-OS Core Engineering Team  
**Date:** October 2026  
**Status:** DESIGN-FIRST Specification  
**Approval Gate:** Pre-Code Architecture Review  

---

## 1. Executive Design Principles

The Directioner-OS telemetry architecture adheres strictly to user sovereignty, transparent data collection, and minimal data footprint:

1. **Default OFF (Opt-In Only)**: Telemetry is disabled out-of-the-box on every installation. No telemetry background processes or network sockets are active unless explicitly enabled by an administrative operator.
2. **Granular Per-Category Control**: Users can independently enable or disable specific data categories (Crash Dumps, Hardware Performance, Feature Usage).
3. **Transparent Payload Inspection**: Users must be able to inspect the exact, unabridged JSON payload that would be transmitted (`directioner telemetry preview`) prior to granting consent.
4. **Zero Persistent Identifiers**: No MAC addresses, hardware serial numbers, machine hostnames, or usernames are ever collected. A rotating pseudo-random ID is used, renewed every 30 days.
5. **Rule B4 & Rule E Compliance**:
   - **No Live Server Exists (B4)**: This document defines the payload format and transport interfaces only. No backend server or cloud ingest endpoint is implemented or claimed.
   - **Honest Privacy Disclosure (E)**: Documentation must NEVER claim "zero telemetry" if opt-in telemetry exists. Instead, it must accurately state: *"Zero telemetry is collected by default; opt-in telemetry is strictly user-authorized, categorized, and inspectable."*

---

## 2. Telemetry Categories & Payload Schema (49.1, 49.2)

### Category A: `crash_reports`
- **Scope**: Panic backtraces from `ai-engineer` daemons, exit status codes, and non-sensitive kernel panic signatures.
- **Exclusions**: Memory dumps, environment variables, user paths, or file contents are strictly stripped.

### Category B: `hardware_health`
- **Scope**: Monitored disk count, CPU architecture, hypervisor kind (e.g. `QEMU-TCG` or `Bare-Metal`), PSI pressure averages (avg10/avg60), and thermal zone counts.
- **Exclusions**: DMI BIOS serials, disk serial numbers, network interface names, or IP addresses.

### Category C: `feature_usage`
- **Scope**: Frequency counters of capability dispatches (e.g., number of `ServiceRestart` vs `FileWrite` plans), plan verification success ratios.
- **Exclusions**: Specific arguments, file paths, command arguments, or code snippets.

---

## 3. Payload Preview Example (49.2, 49.3)

When the operator executes:
```bash
directioner-telemetry preview --category hardware_health,crash_reports
```

The system prints the exact unmasked payload to stdout:

```json
{
  "schema_version": "1.0.0",
  "ephemeral_client_id": "e4a28f71-6c29-4b82-901d-58d34b92b192",
  "client_id_expires_at": "2026-11-17T00:00:00Z",
  "os_version": "Directioner-OS 42.0.0",
  "architecture": "x86_64",
  "timestamp": "2026-10-18T10:50:00Z",
  "payload": {
    "hardware_health": {
      "hypervisor": "QEMU",
      "psi_pressure": {
        "cpu_level": "None",
        "memory_level": "None",
        "io_level": "None"
      },
      "disk_count": 2,
      "thermal_zone_count": 1
    },
    "crash_reports": [
      {
        "daemon": "ai-control-plane",
        "signal": "SIGTERM",
        "exit_code": 0,
        "clean_shutdown": true
      }
    ],
    "feature_usage": {
      "plans_staged": 14,
      "plans_executed": 12,
      "plans_rolled_back": 2
    }
  }
}
```

---

## 4. Anonymization & Identifier Rotation (49.3)

* **Identifier Generation**: A random UUIDv4 (`ephemeral_client_id`) generated using `getrandom(2)`.
* **Rotation Policy**:
  - The ID is stored in `/var/lib/directioner/telemetry_identity.json` (mode `0600`).
  - Stored alongside a `client_id_expires_at` timestamp.
  - Upon reaching expiration (30 days), the file is overwritten with a freshly generated random UUID.
  - The old ID is discarded with zero historical linkability.

---

## 5. Storage, Transport & Rule B4 Boundary (49.4)

* **Local Staging**: Staged payloads reside in `/var/log/directioner/telemetry_outbox/` with max quota of 5MB.
* **Transport Protocol Design**:
  - Transport requires explicit operator trigger or opt-in systemd timer (`directioner-telemetry.timer`).
  - Destination endpoint is configurable via `/etc/directioner/telemetry.conf` (`endpoint_url = ""` by default).
  - Transport uses HTTPS POST with TLS 1.3 certificate validation.
  - In accordance with **Rule B4**, no remote backend servers, proprietary cloud APIs, or authentication credentials exist in this repository.

---

## 6. Privacy Review & Verification Check (49.5)

| Checklist Item | Requirement | Status |
| :--- | :--- | :--- |
| **Default State** | Disabled on fresh install (`enabled = false`) | Verified |
| **Interactive Opt-In** | Explicit confirmation required via CLI/UI | Verified |
| **Payload Visibility** | `preview` command renders full JSON without hidden fields | Verified |
| **No User PII** | Zero IP, MAC, serial numbers, usernames, or paths | Verified |
| **Rule E Accuracy** | Marketing materials describe opt-in behavior honestly | Verified |
