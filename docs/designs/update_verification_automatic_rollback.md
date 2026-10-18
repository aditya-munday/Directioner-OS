# System Architecture Design: Update Verification with Automatic Rollback (TODO 50)

**Author:** Directioner-OS Core Engineering Team  
**Date:** October 2026  
**Status:** DESIGN-FIRST Specification  
**Approval Gate:** Pre-Code Architecture Review  

---

## 1. Executive Summary & Honest Downtime Disclosure (50.1, 50.3)

Directioner-OS employs an immutable, image-based update model powered by `bootc` (bootable container images) and `greenboot` (systemd health-check framework for transactional updates).

> **RULE E HONESTY MANDATE (50.3)**:  
> Directioner-OS **NEVER** claims "zero-downtime updates" or "seamless live kernel swapping".  
> An operating system image replacement updates the entire userspace, system libraries, and kernel. Applying a staged update **MANDATES A SYSTEM REBOOT**, resulting in predictable, bounded non-zero downtime during the reboot transition.

---

## 2. End-to-End Update Lifecycle (50.1)

```
[1. Signature Verification]
         │ (cosign / GPG validation of OCI container manifest)
         ▼
[2. Staging]
         │ (bootc switch / upgrade writes new ostree deployment slot)
         ▼
[3. Operator Scheduled Reboot]
         │ (Controlled non-zero downtime window)
         ▼
[4. Bootloader & Greenboot Trial]
         │ (GRUB boot counter decremented: 3 -> 2)
         ▼
[5. Health Check Execution]
        ╱ ╲
Pass   ╱   ╲ Fail (or Hung Boot)
      ▼     ▼
[6. Commit] [7. Automatic Rollback]
(Reset counter,     (Reboot into previous
 lock new image)     known-good deployment slot)
```

---

## 3. Greenboot Boot Counter & Rollback Mechanics (50.2)

1. **Bootloader Integration**:
   - Updates utilize Boot Loader Specification (BLS) entries.
   - When a new deployment is staged, `greenboot` initializes `boot_counter=3` in the bootloader environment variable store.
2. **Decremental Watchdog**:
   - Each boot attempt of the staged deployment decrements `boot_counter`.
   - If the system experiences a kernel panic, hardware watchdog reset, or unhandled freeze before completing health checks, `boot_counter` decrements on the subsequent reset.
3. **Rollback Trigger**:
   - If `boot_counter == 0`, the bootloader automatically selects the fallback deployment entry (the previous known-good image).
   - Once booted into the rollback slot, `greenboot` marks the failed deployment as corrupted/pinned-out, prevents retry loops, and logs a critical incident worklog entry.

---

## 4. Gating Health Checks for Deployment "Commit" (50.4)

Before `greenboot` executes `bootc commit` or marks the deployment successful, the following required checks in `/etc/greenboot/check/required.d/` must all succeed:

| Check Script | Target | Success Criterion | Failure Action |
| :--- | :--- | :--- | :--- |
| `01_systemd_posture.sh` | Core systemd init | `systemctl is-system-running` returns `running` or acceptable `degraded` (no critical system services in failed state). | Abort commit; trigger fallback. |
| `02_ai_capabilityd.sh` | AI Capability Daemon | Unix socket `/run/ai-engineer/capability.sock` active; responds to `ping` probe within 5 seconds. | Abort commit; trigger fallback. |
| `03_world_model.sh` | World Model SQLite DB | Database integrity check (`PRAGMA integrity_check;`) returns `ok`. | Abort commit; trigger fallback. |
| `04_audit_worklog.sh` | Append-only Audit Chain | `ai-worklog-verify` validates hash chain continuity. | Abort commit; trigger fallback. |

If all scripts exit with status `0`, `greenboot` executes `/usr/libexec/greenboot/greenboot-status set-status green` and clears the boot counter.

---

## 5. Comprehensive Validation Test Plan (50.5)

### Test Case A: Good Image Update (Nominal Path)
1. **Setup**: Machine running Version 42.0.0.
2. **Execution**: Stage signed Version 42.0.1 image; trigger reboot.
3. **Expected Outcome**:
   - Machine reboots into 42.0.1 slot.
   - All 4 health checks pass in required.d.
   - Greenboot marks system green; boot counter cleared.
   - `bootc status` reports 42.0.1 as the active, committed deployment.

### Test Case B: Bad Image Update (Failed Service)
1. **Setup**: Machine running Version 42.0.0.
2. **Execution**: Stage Version 42.0.1 containing a broken `ai-capabilityd` binary (fails socket bind); trigger reboot.
3. **Expected Outcome**:
   - Machine boots into 42.0.1.
   - Check `02_ai_capabilityd.sh` fails with non-zero exit code.
   - Greenboot exhausts retry attempts and forces reboot into fallback slot.
   - Machine boots back into Version 42.0.0 cleanly.
   - Incident logged in worklog with rollback provenance.

### Test Case C: Hung Boot / Kernel Lockup
1. **Setup**: Staged image with simulated kernel hang (`panic_on_oops=1` or hung initrd loop).
2. **Execution**: Trigger reboot under hardware/QEMU watchdog timer (`-watchdog i6300esb -watchdog-action reset`).
3. **Expected Outcome**:
   - System hangs before reaching userspace.
   - Watchdog timer expires (60s) and resets the VM.
   - GRUB decrements boot counter to 0 and selects the previous deployment.
   - System recovers automatically without operator intervention.
