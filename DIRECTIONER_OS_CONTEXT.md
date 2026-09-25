# Directioner-OS: Complete Technical Architecture & System Overview

> *"The differentiator is not that the OS can talk. It is that the OS employs an engineer."*

---

## 1. Project Overview & Foundation

**Directioner-OS** is an AI-native Linux operating system constructed by layering configuration, packaging, and custom Rust services on top of a stock Fedora bootc (OSTree) image. 

### Core Legal & Engineering Rule
We **never fork or patch Fedora's own source code**. All modifications exist exclusively as layered configurations, systemd units, custom RPM packages, and dedicated control-plane services. This follows the identical technical and legal model used by distributions like Bazzite, Nobara, and Pop!_OS, as well as proprietary driver packaging on Linux.

### Business & Operational Model
- **Base OS**: Free and closed-source base operating system distribution.
- **AI Integration**: AI-driven control-plane capabilities become paid after a defined usage rate limit. *(Note: Cloud billing and entitlement infrastructure are not yet built; see Section 7).*

---

## 2. Architecture: What Actually Exists Today (Verified in Code & CI)

Every component described in this section is implemented in the repository, covered by unit/integration tests, and verified in automated CI runs under headless QEMU virtual machines.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                             USER / OPERATOR                                 │
│        Interactive Shell, `ai-engineer` CLI, `ai-cockpit` Terminal HUD      │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ Intent / Diagnostic Query
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                      SAFETY KERNEL & CONTROL PLANE                          │
│                                                                             │
│   ┌──────────────┐     ┌──────────────┐     ┌──────────────┐                │
│   │   PLANNER    │ ──> │ POLICY ENGINE│ ──> │   REVIEWER   │                │
│   │ Step & Roll- │     │ Blast Radius │     │ Invariants & │                │
│   │ back Synthesis     │ Scoring (0-100)    │ Red Lines    │                │
│   └──────────────┘     └──────────────┘     └──────────────┘                │
│                                                     │                       │
│                                    Approved Steps   ▼                       │
│   ┌──────────────┐     ┌───────────────────────────────────┐                │
│   │ WORLD MODEL  │ <── │           IMPLEMENTER             │                │
│   │ SQLite Graph │     │     Step-by-step dispatcher       │                │
│   └──────────────┘     └─────────────────┬─────────────────┘                │
└──────────────────────────────────────────┼──────────────────────────────────┘
                                           │ Unix Domain Socket (/run/ai-capabilityd/ai-capabilityd.sock)
                                           │ Typed JSON-Lines RPC
                                           ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         PRIVILEGED CAPABILITY LAYER                         │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                 ai-capabilityd (Passive Daemon)                     │   │
│   │  • SO_PEERCRED UID Verification  • Explicit 0600 Permissions        │   │
│   │  • ScopedPath Sandbox            • Process Group SIGKILL Killswitch │   │
│   └──────────┬──────────────────────────────────────────────┬───────────┘   │
│              │                                              │               │
│              ▼                                              ▼               │
│   ┌──────────────────────┐                       ┌──────────────────────┐   │
│   │ Append-Only Work Log │                       │ System Adapters      │   │
│   │ (/var/log/worklog)   │                       │ ostree, systemd,     │   │
│   └──────────────────────┘                       │ sysctl, kmod, DNF5   │   │
│                                                  └──────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Phase 0: Immutable Base, Capability Daemon & Killswitch
- **Base OS Image**: Fedora 42 bootc with SELinux in Enforcing mode. Defined in [`image/Containerfile`](image/Containerfile), packaged via `bootc-image-builder` into a bootable QCOW2 image, and verified via automated QEMU headless VM boots in GitHub Actions CI.
- **Privileged Capability Daemon (`ai-capabilityd`)**:
  - Passive Unix domain socket listener at `/run/ai-capabilityd/ai-capabilityd.sock`.
  - Dispatches ~18 strictly typed `CapabilityKind` variants (file reads/writes within sandbox, package dry-run/install/remove, service status/restart/stop, kernel sysctl tuning, kernel module load/unload, boot slot pinning, and bootc rollback).
  - **Socket Permissions (AUDIT-CAP-01)**: Explicitly sets `0o600` permissions on the socket file node upon binding, overriding ambient umask without altering parent directory permissions (`/tmp` mode `1777` preserved).
  - **Peer Credential Verification (AUDIT-CAP-02)**: Queries `SO_PEERCRED` on every incoming connection. When running as root, `ServerConfig::default()` strictly authorizes only UID 0 by default. Day-one desktop usability is provided via `DIRECTIONER_ALLOWED_UIDS=1000` in [`image/configs/systemd/ai-capabilityd.service`](image/configs/systemd/ai-capabilityd.service) and `/etc/default/ai-capabilityd`.
- **Append-Only Work Log (`ai-worklog`)**: Cryptographic JSON-Lines audit stream (`/var/lib/ai-engineer/worklog.jsonl` or `/var/log/live-worklog.jsonl`). Every request, caller UID, parameters, and outcome are logged before execution.
- **Hardware / File Killswitch (`ai-killswitch`)**:
  - Non-caching lockfile mechanism (`/run/ai-capabilityd/ai-capabilityd.lock`).
  - Child subprocesses spawned by the daemon are placed in dedicated process groups (`cmd.process_group(0)`).
  - The daemon polls the lockfile every 10ms during command execution. When engaged, it immediately terminates in-flight subprocesses via `killpg(SIGKILL)`.
  - **Real Tested Timing**: Subprocess termination verified at **~20ms** in live tests (verified in [`ai-engineer/capability-layer/tests/audit_hardening.rs::test_audit_cap_04_killswitch_terminates_inflight_subprocess`](ai-engineer/capability-layer/tests/audit_hardening.rs)).

---

### Phase 1: Resident AI Engineer Subsystem & Policy Pipeline
- **Separation of Duties**:
  - **Planner**: Converts high-level user intent into an ordered sequence of typed capability steps, automatically computing an exact **compensating rollback step** for every mutating operation.
  - **PolicyEngine**: Calculates quantitative **blast radius** scores (`0` to `100`) based on target filesystem paths, service criticality, reverse package dependencies, and crate dispersion.
  - **Reviewer**: Independent adversarial safety auditor. Enforces hard red lines: blocks modifications to `/etc/sudoers.d`, prevents stopping critical daemons (`systemd-udevd`, `dbus`, `sshd`), blocks path traversal (`..`), checks for symlink redirections, and verifies 100% rollback coverage.
  - **Implementer**: Connects to `ai-capabilityd` over the Unix socket and dispatches steps one at a time. If any step fails or verification rejects the outcome, execution immediately halts and executes compensating rollback steps in reverse order.
  - **Auditor**: Re-reads the worklog to independently confirm execution integrity.
- **The 4 Autonomy Tiers**:
  - **Tier 0: Auto** (Score 0–15): Low-risk read-only diagnostics or isolated cache cleanup. Executed silently.
  - **Tier 1: Notify** (Score 16–35): Minor mutations. Executed with a desktop notification.
  - **Tier 2: Confirm** (Score 36–60): Standard service or configuration mutations. Pauses for interactive user confirmation.
  - **Tier 3: Expert Review** (Score 61–100): High-impact actions (vital daemons, kernel parameters, core packages). Requires explicit administrator approval with full blast-radius breakdown.
- **3-Depth Explanations (`ai-engineer explain`)**:
  - **Depth 1**: Executive plain-English summary.
  - **Depth 2**: Technical step sequence with unified file diffs and affected units.
  - **Depth 3**: Full diagnostic graph with reverse dependencies, blast radius metrics, and rollback projections.
- **Benchmark Suite (`ai-benchmark`)**: Scored test harness verifying 12 real-world system maintenance tasks (12/12 passing, 100.0%).

---

### Phase 2: World Model & Operational Cockpit
- **World Model (`ai-worldmodel`)**: Embedded SQLite database (`worldmodel.db`) providing sub-10ms offline system state intelligence:
  - Synchronized package inventory and reverse dependencies via DNF5 line protocol (`PROVIDENAME` / `PROVIDEVERSION`).
  - Real-time systemd unit state snapshots and diff tracking (`failed-since`).
  - Stored compatibility profiles for Proton/Wine applications.
- **Operational Cockpit (`ai-cockpit`)**: Real-time ANSI terminal dashboard and `--json` exporter reporting CPU load, memory utilization, World Model sync status, active alerts, and live Work Log tails.
- **Symbol Graph & Diagnostic Repair**:
  - Incremental AST symbol indexer for Rust and workspace projects.
  - `ai-engineer diagnose`: Ingests compiler diagnostic errors (rustc/gcc), localizes root causes across crate boundaries, and generates targeted unified diff patches.
  - `ai-engineer fix`: Validates patches within `ScopedPath` boundaries, refusing execution if compiler suggestions are ambiguous or absent.

---

### Phase 3: Closed-Loop Resilience & Self-Healing
- **Tier 1 Boot Recovery (Real Hardware/Watchdog Mechanism)**:
  - Uses GRUB2 boot-counting (`boot_counter=3`) and Greenboot required health checks (`01_directioner_milestone.sh`).
  - Deployment 0 is pinned via `boot_slot.pin`.
  - If a newly staged deployment fails to achieve system milestones on reboot, Greenboot exhausts its retry counter and GRUB2 **automatically falls back to the pinned good deployment below user-space**.
  - Verified under live QEMU VM testing with SELinux Enforcing (Assertion 9 in CI).
- **Tier 2 Controlled Remediation**:
  - Runtime and persistent sysctl tuning (`KernelParamSet`).
  - Dynamic kernel module loading and unloading (`KernelModuleLoad`/`KernelModuleUnload`) with kernel refcount protection (`ModuleRefcountNonZero` guard) and pre-captured rollback state.
- **Self-Healing Engine (Phase 3.1)**:
  - Closed-loop event monitor with multi-layer circuit breakers, per-entity attempt limits (max 3 consecutive attempts), global velocity throttling (max 5 actions/hour), and 30-second anti-flap suppression.
  - **Guaranteed Disabled by Default**: Ships with `enabled = false` in `/etc/ai-engineer/self-healing.toml`.
  - **Tier Ceiling**: Autonomous execution is strictly capped at the Notify tier; Confirm and Expert Review proposals are held for user confirmation.

---

### Phase 4: Conversational Operator & Diagnostic Tooling
- **Conversational Operator (`ai-engineer ask` / `ai-engineer chat`)**:
  - Natural language intent parsing into structured multi-step plans.
  - Grounded by a 4-layer `PlanDraftValidator` and evaluated by `PolicyEngine`/`Reviewer`.
  - **STRICTLY DRAFT-ONLY**: Ends at a printed preview. **No execution dispatch exists or is authorized**. Wiring `PlanExecutionGate` to `CapabilityClient` or `Implementer` is physically omitted from the code.
- **Hardware Diagnostic Tool (`ai-hwcheck`)**:
  - **DIAGNOSTIC-ONLY**: Inspects and reports system hardware status without modifying drivers or guaranteeing out-of-the-box operation.
  - Inspects PCI WiFi chipsets (Broadcom BCM43xx, Realtek, Intel).
  - Inspects USB biometric fingerprint sensors (checks `libfprint` vs proprietary `libfprint-tod`).
  - Inspects ACPI sleep/wake capabilities (`s2idle` vs S3, NVMe APST status).
- **Application Compatibility Profiles (`ai-worldmodel compat`)**: SQLite-backed tracker for Proton/Wine environment overrides (DXVK, VKD3D).

---

### Phase 5: Plugin SDK & Federation
- **Plugin SDK**:
  - Manifest format (`PluginManifest`) with SHA-256 canonical hash verification.
  - Sandboxed execution context (`PluginContext`): structurally routes third-party capabilities through the same 5-stage policy and reviewer gate. Plugins have zero direct access to raw capability handles.
- **Multi-Machine Federation**:
  - Read-only LAN telemetry exchange (`SignedFederatedSnapshot`).
  - **Zero Remote Execution**: No remote capability dispatch, no remote shell, and no inbound mutating sockets exist in the network protocol.

---

## 3. Absolute Operational & Security Invariants

These boundaries are enforced across the codebase and verified by automated tests:

1. **No Conversational Execution Dispatch**: `ai-engineer ask` and `ai-engineer chat` are draft-only. They cannot dispatch mutations to `ai-capabilityd`.
2. **Zero Arbitrary Shell Execution**: The Capability Layer invokes binaries directly via `std::process::Command` argument arrays. Zero calls to `/bin/sh` or `/bin/bash` exist in capability execution.
3. **Restricted Files Boundary**: Six core safety files are permanently protected and may not be modified without explicit instruction:
   - `ai-engineer/safety-kernel/src/policy.rs`
   - `ai-engineer/safety-kernel/src/reviewer.rs`
   - `ai-engineer/safety-kernel/src/self_healing.rs`
   - `ai-engineer/safety-kernel/src/incident_response.rs`
   - `ai-engineer/control-plane/src/conversational.rs`
   - `ai-engineer/capability-layer/src/lib.rs` (`CapabilityKind` enum)
4. **Symlink TOCTOU Defense**: `cap_file_write` verifies that no symlink components exist in the target path traversal and sets `O_NOFOLLOW` on file creation.
5. **Passive Capability Daemon**: `ai-capabilityd` never initiates actions or decides policy; it strictly executes validated, pre-approved requests received over its Unix socket.

---

## 4. Current Test & Verification Evidence

All claims in Section 2 are backed by real, automated tests:

- **Workspace Rust Test Suite**: 100% passing across all workspace crates:
  ```bash
  cargo test --workspace
  ```
  Verified: 23 capability tests, 4 audit hardening tests, 5 integration tests, 3 live package unlayering tests, 13 reviewer defensive tests, 7 self-healing tests, 4 performance profile tests, 10 governance guard tests, and 5 cockpit permutation tests.
- **CI Headless VM Smoke Test**: Every push to `main` and release PR executes a full headless QEMU VM boot verifying all 10 live assertions:
  - Assertion 1: MOTD and base bootc status
  - Assertion 2: SELinux Enforcing mode
  - Assertion 3: Systemd unit initialization
  - Assertion 4: Worklog append-only stream integrity
  - Assertion 5: Killswitch engagement and release
  - Assertion 6: World Model DNF5 and systemctl synchronization
  - Assertion 7: Real live compensating rollback execution in guest VM
  - Assertion 8: In-guest benchmark suite execution (12/12 scenarios pass)
  - Assertion 9: Genuine A/B boot-slot pinning and Greenboot watchdog automatic recovery
  - Assertion 10: Tier 2 kernel parameter tuning and module refcount protection

---

## 5. Local VM Run Guide: How to Boot Directioner-OS

Follow these steps to build and boot the verified image locally:

### Prerequisites
- Linux host with hardware virtualization enabled (`/dev/kvm`).
- `podman` and `qemu-system-x86_64` installed.

### 1. Build the OS Container Image
```bash
sudo podman build -t directioner-os:latest -f image/Containerfile image/
```

### 2. Generate the QCOW2 Disk Image
```bash
mkdir -p ./output
sudo podman run \
  --rm \
  --privileged \
  -v /var/lib/containers/storage:/var/lib/containers/storage \
  -v ./output:/output \
  quay.io/centos-bootc/bootc-image-builder:latest \
  --type qcow2 \
  --rootfs xfs \
  directioner-os:latest
```

### 3. Boot Headlessly in QEMU
```bash
qemu-system-x86_64 \
  -m 4096 \
  -smp 4 \
  -cpu host \
  -enable-kvm \
  -drive file=./output/qcow2/disk.qcow2,format=qcow2 \
  -net nic -net user,hostfwd=tcp::2222-:22 \
  -nographic
```

### 4. Interactive Access
Once booted, connect via SSH:
```bash
ssh -p 2222 ci@localhost
```
From the interactive guest shell, the core tools are immediately available:
```bash
# View operational telemetry
/var/lib/ai-bin/ai-cockpit --db /tmp/wm.sqlite --audit /var/log/live-worklog.jsonl

# Inspect system health
/var/lib/ai-bin/ai-health --db /tmp/wm.sqlite --worklog /var/log/live-worklog.jsonl status

# Inspect hardware diagnostics
/var/lib/ai-bin/ai-hwcheck --json

# Read audit stream
/var/lib/ai-bin/ai-worklog --log /var/log/live-worklog.jsonl tail --lines 20
```

---

## 6. Repository Layout Reference

```
Directioner-OS/
├── image/                             # Base bootc container & systemd configurations
│   ├── Containerfile                  # Layered configuration on fedora-bootc:42
│   ├── build.sh                       # Local build helper script
│   └── configs/
│       ├── systemd/                   # ai-capabilityd.service, ai-killswitch.service
│       ├── ai-engineer/               # self-healing.toml, ai-capabilityd.default
│       └── greenboot/                 # 01_directioner_milestone.sh watchdog script
├── ai-engineer/                       # Core Rust workspace
│   ├── capability-layer/              # ai-capabilityd daemon, server, handler, killswitch
│   ├── control-plane/                 # Planner, PolicyEngine, Reviewer, Implementer, Conversational
│   ├── world-model/                   # SQLite state engine, DNF5/systemctl sync adapters
│   ├── worklog-cli/                   # ai-worklog audit reader CLI
│   ├── benchmark-suite/               # ai-benchmark test harness
│   └── Cargo.toml                     # Workspace root manifest
├── packaging/                         # Packaging automation
│   ├── rpm/directioner-ai-engineer.spec
│   └── build-rpm.sh
├── docs/                              # Project specifications and audit logs
│   ├── incident-log.md                # Record of safety incidents and remediations
│   ├── security-self-audit.md         # CAP-01 through CAP-04 audit records
│   └── phase-*-checklist.md           # Original roadmap milestones
├── vm-test/                           # QEMU smoke test scripts executed during CI
└── .github/workflows/ci.yml           # Automated CI pipeline
```

---

## 7. NOT YET BUILT — FUTURE VISION

The following items are **conceptual roadmap targets and design ideas**. They **DO NOT EXIST** in the current codebase, are not built into any binary, and must never be represented as current system capabilities:

1. **Aria Voice Assistant & Spatial UI**:
   - Concept: A voice-driven multimodal assistant using openWakeWord, Silero VAD, streaming STT/TTS, and a transparent overlay window ("Canvas Mode") to render contextual graphs.
   - Status: **Not yet built.** No voice pipeline, audio capture daemon, transparent overlay compositor, or spatial window manager exists in the repository.
2. **De-Fedora-ification / Custom Distribution Identity**:
   - Concept: Replacing upstream base branding, logos, and default artwork with a standalone visual identity.
   - Status: **Not yet built.** The system currently builds directly on top of `quay.io/fedora/fedora-bootc:42`.
3. **Automated Biometric Fingerprint Setup**:
   - Concept: 1-click biometric enrollment and automated vendor driver patching.
   - Status: **Not yet built.** Hardware inspection is diagnostic-only via `ai-hwcheck`; no automated driver installation or PAM biometric integration exists.
4. **Desktop Environment & Display Tuning**:
   - Concept: Custom "Aether" glassmorphic desktop environment, native 120Hz/VRR display compositor, and zero-drain sleep automation.
   - Status: **Not yet built.** Directioner-OS currently runs as a headless server/workstation base; no custom desktop shell or display compositor is present.
5. **Commercial Cloud Billing & Entitlements**:
   - Concept: User accounts, token rate-limiting, and paid subscription tiers for advanced cloud-augmented AI planning.
   - Status: **Not yet built.** The system operates purely locally with zero account or billing infrastructure.
6. **Native AI-First App Suite**:
   - Concept: First-party file manager, system settings panel, and process manager designed with native AI telemetry integration.
   - Status: **Not yet built.** Telemetry is currently accessed via CLI tools (`ai-cockpit`, `ai-health`, `ai-worklog`).
