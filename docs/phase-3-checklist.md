# Phase 3 — Safety, Self-Healing & Resilience Checklist

Status tracking against the build guide (`AI_Native_Linux_OS_HowTo_Build_Guide.docx`, §4 Phase 3) and master architecture specification.

Legend: `[x]` done and verified · `[~]` scaffolded, needs CI/VM to verify · `[ ]` not started

---

## 3.0 Tier 1 Autonomous Boot-Slot Pinning & Watchdog Mechanics

### 3.0.1 Genuine A/B Boot Slot Pinning & Tier 1 Automatic Rollback
- [x] Boot slot pinning capability (`boot_slot.pin`) targeting deployment 0 (`ostree admin pin 0`) with audit tracking
- [x] Standard `greenboot` health check integration with image-baked required check (`/etc/greenboot/check/required.d/01_directioner_milestone.sh`)
- [x] Deterministic systemd unit graph ordering (`Before=multi-user.target`, `WantedBy=multi-user.target`) for milestone verification
- [x] 10s bounded polling defense-in-depth in health check script
- [x] Autonomous Tier 1 boot-watchdog recovery: GRUB2 `boot_counter` decrementing (`3 -> 2 -> 1 -> rollback`), ostree rollback switch on retry exhaustion, and automatic reboot into pinned deployment 0
- [x] Verified zero manual `bootc.rollback` capability calls invoked during failure/recovery window (rollback executes strictly below user-space at the bootloader/watchdog layer)
- [x] Verified post-boot observation event (`boot_watchdog.tier1_rollback_observed`) recorded to persistent Work Log (`/var/log/live-worklog.jsonl`)
- [x] Verified under live QEMU ostree guest with full **SELinux Enforcing** mode active (`getenforce -> Enforcing`, `audit: enforcing=1`)
- **What this actually verifies**: Real GRUB2/greenboot boot-counter retry countdown and autonomous ostree deployment fallback triggered by a genuine non-zero exiting systemd service drop-in override, verified in live headless QEMU guest with SELinux enforcing. This is a true hardware/kernel-level resilience test, not a scripted simulation or label matching mock. ✔ (verified in CI run `33250142691`)

---

## 3.0.2 Health Model v1 (`control-plane::health_model`)
- [ ] System health scoring engine aggregating kernel, systemd, and service telemetry
- [ ] Multi-signal health state evaluation
- [ ] Integration with Operational Cockpit and Diagnostic CLI

---

## 3.1 Self-Healing Loop & Automated Remediation
- [ ] Tier 2 Automated Remediation Agent
- [ ] Automated incident containment and service remediation
- [ ] Bounded remediation attempts with fallback to operator confirmation
