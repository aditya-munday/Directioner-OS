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
- [x] Multi-signal health scoring engine aggregating real event streams from World Model (`units`, `unit_events`, `packages`, `pkg_requires`, `pkg_provides`) and Work Log audit history
- [x] Subsystem score evaluations:
  - `services`: Active state inspections, failure penalties, and rapid crash-loop transition detection ($\ge 2$ failures in window)
  - `boot`: Pinned deployment verification and Tier 1 autonomous rollback observation detection
  - `packages`: Dependency graph completeness and reverse dependency edge verification
- [x] Traceability guarantee: every deduction and score is strictly linked to underlying system events (unit names, timestamps, transition IDs, audit seqs)
- [x] Strict veracity invariant: unobserved subsystems evaluate to `Unknown` / `None`, never fabricated defaults
- [x] Scoring-only invariant: zero automated mutation or remediation capability calls dispatched by health model
- [x] `ai-health status` and `ai-health status --json` CLI query surface
- [x] Operational Cockpit (`ai-cockpit`) integration displaying composite health score and subsystem breakdowns
- [x] Tested in-guest with real crash-looping systemd service and real post-rollback boot observation
- **What this actually verifies**: Real multi-signal telemetry ingestion from SQLite World Model and JSONL Work Log detecting actual systemd service crashloops and boot watchdog recovery events without synthetic markers. ✔ (verified in CI run `33263794703`)

---

## 3.0.3 IncidentResponseAgent & Tier 1 Observation Boundary (`control-plane::incident_response`)
- [x] Formalized named `IncidentResponseAgent` component owning Tier 1 post-hoc observation and resilience posture synthesis
- [x] Explicit code-level authority boundary doc-block stating: "Tier 1 recovery is fully autonomous at the bootloader/greenboot layer and requires no action from this agent. This agent's only current responsibility is post-hoc observation and logging. It has zero write/mutating capability access as of Phase 3.0.3."
- [x] Zero capability-client imports, mutating calls, or proactive action triggers in `IncidentResponseAgent`
- [x] Sourced post-rollback observation and incident posture through `IncidentResponseAgent` in `ai-cockpit` and integration test suite
- **What this actually verifies**: Architectural formalization and structural separation of Tier 1 autonomous bootloader mechanics from agent control plane, establishing a strictly read-only post-hoc observer component with zero capability dispatch authority. ✔

---

## 3.0.4 Tier 2 Self-Healing & Remediation Loop (`control-plane::remediation`, `capability_layer`)
- [x] Typed capability primitives:
  - `KernelParamSet`: runtime `sysctl -w` and persistent drop-ins with direct `/proc/sys/` rollback value capture
  - `KernelModuleLoad`: dynamically load allowlisted modules (`br_netfilter`, `overlay`, `dummy`, `veth`, etc.)
  - `KernelModuleUnload`: dynamic `modprobe -r` protected by in-use refcount inspection (`/proc/modules`)
- [x] Structural conservative floor: all Tier 2 Phase 3 actions have a strict floor at `Notify` (score $\ge 20$, never `Auto`)
- [x] Independent Reviewer domain rules:
  - Closed module allowlist (`ALLOWED_KERNEL_MODULES`)
  - Prohibited security parameter denylist (`PROHIBITED_KERNEL_PARAMS` fatal refusal)
  - Arbitrary executable / pipe block on `kernel.core_pattern`
  - Reversible forward/compensating rollback step pair verification
- [x] In-guest verification under live ostree VM:
  - Real `KernelParamSet` forward + rollback on `net.ipv4.ip_forward`
  - Real `KernelModuleLoad` of `dummy` module with `args: ["numdummies=2"]`
  - Real Refcount-Guard denial when attempting to unload active in-use interface (`ip link set dummy0 up`)
  - Clean unload upon releasing interface (`ip link set dummy0 down`)
  - Strict refusal of prohibited parameters and unlisted modules
- **What this actually verifies**: Controlled Tier 2 kernel/module remediation with double-enforced conservative floor, deterministic rollback capture, and real kernel-enforced refcount safety guards. ✔

---

## 3.1 Self-Healing Loop & Automated Remediation
- [ ] Tier 2 Automated Incident Remediation Agent
- [ ] Automated incident containment and service remediation
- [ ] Bounded remediation attempts with fallback to operator confirmation
