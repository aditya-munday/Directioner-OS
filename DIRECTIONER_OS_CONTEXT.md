# Directioner-OS: Complete Technical & Brand Context Guide

> *"The differentiator is not that the OS can talk. It is that the OS employs an engineer."*

This document provides complete architectural, operational, security, and design context for **Directioner-OS**. It is specifically structured for AI agents, designers, and frontend engineers building the official web application, product showcase, and interactive documentation for Directioner-OS.

---

## 1. Executive Summary & Core Philosophy

### What is Directioner-OS?
**Directioner-OS** is an AI-native Linux operating system derived from Fedora (via `bootc` and `ostree`) designed from the kernel up to host a resident, multi-agent **AI Engineer** subsystem. Rather than treating AI as an external chatbot or a shell wrapper that blindly executes generated `bash` commands with root privileges, Directioner-OS integrates AI as a **capability-scoped, auditable, first-class citizen of the operating system control plane**.

### The Core Problem It Solves
Traditional attempts to bring AI into operating systems suffer from two fatal extremes:
1. **The Toy Chatbot**: A desktop assistant that can answer questions or generate text, but cannot safely interact with system configuration, kernel parameters, service daemons, or package dependencies.
2. **The Unguarded Agent**: An LLM given raw root shell access (`bash -c`, `sudo`). When it hallucinates, misunderstands context, or encounters unexpected package conflicts, it irreversibly corrupts the filesystem, removes critical system packages, or opens security vulnerabilities.

### The Directioner-OS Solution
Directioner-OS solves this through **Deterministic Safety & User Sovereignty**:
- **Zero Arbitrary Shell Execution**: Agents and control-plane models are physically unable to run raw bash or arbitrary shell strings. All system interactions are strictly typed Remote Procedure Calls (RPC) dispatched over a local Unix domain socket.
- **The 5-Stage Safety Pipeline**: Every intent must pass through `Planner` → `PolicyEngine` (quantitative blast-radius scoring) → `Reviewer` (independent safety invariants) → `Implementer` (typed capability dispatch) → `Auditor` (append-only cryptographic worklog).
- **Physical / File Killswitch**: A lockfile mechanism (`ai-killswitch`) verifies locks on every single capability request. When engaged, all mutations halt sub-millisecond, and active subprocesses are instantly terminated via `killpg(SIGKILL)`.
- **Atomic Rollbacks & Immutable Roots**: Built on Fedora `bootc` (OSTree). The OS base is immutable. If a failure occurs, the OS automatically rolls back to a pinned good boot slot below user-space using hardware watchdog timers and Greenboot.

---

## 2. Brand Positioning & Target Audience

### Taglines & Brand Voice
- **Primary Tagline**: *"The AI-Native Operating System Built on User Sovereignty and Provable Safety."*
- **Secondary Tagline**: *"An Operating System That Employs an Engineer, Not Just a Chatbot."*
- **Key Mottos**:
  - *"Deterministic by Design, Autonomous by Permission."*
  - *"Every Action Audited Before Execution."*
  - *"Quantitative Blast Radius, Never Blind Trust."*
  - *"Immutable Roots, Self-Healing Branches."*

### Brand Voice & Persona
- **Tone**: High-precision engineering, authoritative, transparent, deterministic, telemetry-driven.
- **Avoid**: Generic AI buzzwords, sci-fi magic, vague promises of "superintelligence."
- **Emphasize**: Formal verification, blast-radius scoring, atomic transactions, live rollback guarantees, audit streams, kernel refcount safety, and uncompromising respect for the user's ultimate authority.

### Target Audience
1. **Linux Power Users & System Administrators**: Engineers who want automated diagnostics and maintenance but will never give an uncontrolled AI root privileges.
2. **Infrastructure & Cloud Engineers**: Developers managing fleet systems who require deterministic immutable rollbacks, declarative state, and auditability.
3. **Rust & Systems Programmers**: Developers who appreciate memory safety, strict typed RPC interfaces, and zero-overhead performance.
4. **AI Systems Researchers**: Teams studying safe multi-agent execution, grounded tool use, and closed-loop control planes.

---

## 3. The 6-Phase Architectural Anatomy

Directioner-OS is built across 6 rigorous phases (Phases 0 through 5), fully implemented, tested, and machine-verified in CI under disposable QEMU virtual machines.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                             USER / OPERATOR                                 │
│          Interactive Shell, `ai-engineer` CLI, `ai-cockpit` Dashboard       │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ Intent / Repair Request
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
│   │ WORK MODEL   │ <── │           IMPLEMENTER             │                │
│   │ SQLite Graph │     │     Step-by-step dispatcher       │                │
│   └──────────────┘     └─────────────────┬─────────────────┘                │
└──────────────────────────────────────────┼──────────────────────────────────┘
                                           │ Unix Domain Socket (/run/ai-capabilityd.sock)
                                           │ Typed JSON-Lines RPC
                                           ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         PRIVILEGED CAPABILITY LAYER                         │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                 ai-capabilityd (Passive Daemon)                     │   │
│   │  • SO_PEERCRED UID Check    • Explicit 0600 Permissions             │   │
│   │  • ScopedPath Sandbox       • Subprocess Group (SIGKILL) Killswitch │   │
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

### Phase 0: Immutable Base & Passive Capability Daemon
- **Base OS**: Fedora 42 bootc (OSTree). The root filesystem is read-only and deployed as an immutable container image.
- **Privileged Capability Daemon (`ai-capabilityd`)**: A minimal, isolated Rust daemon that is the *only* component allowed to modify system state. It does not think, plan, or make decisions; it strictly executes validated, typed capabilities.
- **Socket Hardening (AUDIT-CAP-01 & CAP-02)**: Listens on `/run/ai-capabilityd/ai-capabilityd.sock`. Enforces strict `0o600` permissions on the socket node while preserving parent directory permissions (`/tmp` 1777 mode intact). Inspects `SO_PEERCRED` on every connection; default root daemon strictly authorizes only UID 0 (root) with explicit day-one desktop user access via `/etc/default/ai-capabilityd` (`DIRECTIONER_ALLOWED_UIDS=1000`).
- **Hardware / File Killswitch (`ai-killswitch`)**: Checks `/run/ai-capabilityd/ai-capabilityd.lock` on every operation. Engages in < 1ms. Immediately halts all execution and terminates active child subprocesses via dedicated process groups (`killpg(SIGKILL)`).
- **Append-Only Work Log (`ai-worklog`)**: Cryptographic JSON-Lines audit stream recording every capability request, parameters, caller UID, and outcome before execution begins.

---

### Phase 1: Resident AI Engineer Subsystem & Policy Pipeline
- **Role Separation**:
  - **Planner**: Synthesizes a structured, multi-step plan from an intent, generating an exact **compensating rollback step** for every forward mutation.
  - **PolicyEngine**: Quantitatively scores the **blast radius** (0 to 100) based on target paths, service criticality, reverse package dependencies, and crate dispersion.
  - **Reviewer**: An independent adversarial safety pass. Verifies strict invariants (no edits to `/etc/sudoers.d`, no stopping `systemd-udevd` or `dbus`, no path traversal, complete rollback coverage).
  - **Implementer**: Dispatches steps one at a time over the Unix socket. If any step fails, it halts immediately and dispatches the compensating rollback steps in reverse order.
  - **Auditor**: Re-reads the worklog to verify ground-truth execution.
- **The 4-Tier Approval Gate**:
  - **Tier 0: Auto** (Score 0–15): Low-risk read-only or sandboxed user operations.
  - **Tier 1: Notify** (Score 16–35): Minor mutations; logged and executed with desktop notification.
  - **Tier 2: Confirm** (Score 36–60): Modifying standard services or configurations; requires explicit interactive user confirmation.
  - **Tier 3: Expert Review** (Score 61+): Touches vital daemons, kernel parameters, or high-dependency packages; displays full technical blast-radius breakdown.
- **3-Depth Explanations (`ai-engineer explain`)**:
  - **Depth 1 (Executive)**: High-level plain English summary for non-technical users.
  - **Depth 2 (Technical)**: Step-by-step action sequence with unified diffs and service impacts.
  - **Depth 3 (Full Diagnostic)**: Complete dependency tree, reverse dependencies, rollback projections, and formal blast-radius breakdown.

---

### Phase 2: World Model & Operational Cockpit
- **World Model (`ai-worldmodel`)**: Embedded SQLite database (`worldmodel.db`) maintaining real-time awareness of system state:
  - Synchronized package inventory and reverse dependencies via DNF5 line protocol (`PROVIDENAME` / `PROVIDEVERSION`).
  - Systemd service unit snapshots and diff tracking (`failed-since`).
  - Offline sub-10ms query speeds.
- **Operational Cockpit (`ai-cockpit`)**: Real-time terminal dashboard with ANSI telemetry, health scoring, active alerts, and structured JSON output.
- **Workspace Symbol Graph & Diagnostic Repair**:
  - Incremental AST indexing of Rust and system codebases across crate boundaries.
  - `ai-engineer diagnose`: Ingests compiler diagnostic errors (rustc, gcc), localizes root causes, and generates targeted unified diff patches.
  - `ai-engineer fix`: Safely applies localized patches within the `ScopedPath` sandbox, refusing execution if ambiguity exists.

---

### Phase 3: Closed-Loop Resilience & Self-Healing
- **Multi-Tier Automated Remediation**:
  - **Tier 0**: Passive telemetry observation and health scoring.
  - **Tier 1 (Autonomous Boot Watchdog)**: Hardware/GRUB2 watchdog countdown (3 attempts) integrated with Greenboot. If a new deployment fails required service milestones, the system automatically falls back to the pinned good OSTree boot slot below user-space.
  - **Tier 2 (Controlled Kernel Remediation)**: Runtime/persistent sysctl tuning (`KernelParamSet`), allowlisted kernel module loading/unloading (`KernelModuleLoad`/`KernelModuleUnload`) with kernel refcount protection and rollback caching.
  - **Tier 3**: Service restart and crashloop recovery.
  - **Tier 4**: Package unlayering and rollbacks.
- **Self-Healing Circuit Breakers & Safeguards**:
  - **Persistent Failure Budget**: Locks out remediation after 3 consecutive failures to prevent reboot thrashing.
  - **Global Velocity Throttle**: Maximum 5 automated actions per hour.
  - **Anti-Flap Filter**: Suppresses transient blips (requires sustained failure for 30s before triggering).
  - **Guaranteed Disabled-By-Default**: Ships with `self_healing.enabled = false` in `/etc/ai-engineer/self-healing.toml`.
  - **Tier Ceiling**: Autonomous loop is capped at the Notify tier; operations requiring Confirm or Expert Review are strictly held for user approval.

---

### Phase 4: Hardware Diagnostic & Application Compatibility
- **Hardware Compatibility Inspector (`ai-hwcheck`)**:
  - Read-only audit of PCI WiFi chipsets (Broadcom BCM43xx, Realtek, Intel).
  - USB biometric sensors (distinguishing standard `libfprint` from vendor-proprietary `libfprint-tod`).
  - ACPI sleep/wake validation (`s2idle` vs deep S3, NVMe Autonomous Power State Transitions).
- **Proton/Wine Game & App Compatibility (`ai-worldmodel compat`)**:
  - Schema v3 compatibility database tracking custom runners, DXVK/VKD3D overrides, and environment flags for Windows applications running on Linux.

---

### Phase 5: Sandboxed Ecosystem & Read-Only Federation
- **Plugin SDK**:
  - Cryptographically signed manifests (`PluginManifest`) verified via canonical SHA-256 digests.
  - Sandboxed execution context (`PluginContext`): third-party plugins have zero direct access to capabilities; every action is routed through the same 5-stage policy and reviewer gate.
- **Multi-Machine Federation**:
  - Cryptographic LAN health snapshots (`SignedFederatedSnapshot`).
  - Read-only telemetry exchange across multiple machines on a local network.
  - **Zero Remote Execution**: No remote capability dispatch, no remote shell, no inbound mutation sockets.

---

## 4. Key Security Invariants (Audited & Machine-Verified)

| Invariant | Implementation Mechanism | Verification |
|---|---|---|
| **No Conversational Execution Dispatch** | The conversational module (`conversational.rs`) is cryptographically sealed to draft preview only. Execution dispatch is physically omitted from the code. | Machine-verified by `tests/conversational.rs` and `tests/governance_guard.rs` |
| **No Shell Breakouts** | Direct `execve` invocation via `std::process::Command`. Zero calls to `/bin/sh` or `/bin/bash` in `capability-layer`. | Machine-verified by static analysis and test suite |
| **Socket Transport Hardening** | Explicit `0o600` permissions on socket node; `SO_PEERCRED` caller UID whitelist (default [0] on root daemon). | `test_audit_cap_01` & `test_audit_cap_02` |
| **Symlink Redirection Defense** | Path traversal verification via `symlink_metadata` and `O_NOFOLLOW` flag on all file write operations. | `test_audit_cap_03_symlink_redirection_defense` |
| **Instant Subprocess Halting** | Process group tracking (`cmd.process_group(0)`); lockfile poll triggers immediate `killpg(SIGKILL)`. | `test_audit_cap_04_killswitch_terminates_inflight_subprocess` |
| **Immutable Boot Slot Recovery** | Automatic fallback to pinned OSTree deployment via GRUB2 boot counter and Greenboot watchdog. | Live QEMU VM smoke test in CI |

---

## 5. UI/UX Design System for the Website

When designing the web application for Directioner-OS, follow these aesthetic and interactive guidelines to create an interface that feels like a high-precision, state-of-the-art developer product:

### 1. Visual Style & Theme
- **Theme**: Dark-Tech, Aerospace Telemetry, and Swiss Precision Typography.
- **Background**: Deep Obsidian `#0A0B0E`, Slate Void `#101216`, with subtle radial mesh gradients in cyan and violet.
- **Surfaces**: Glassmorphism cards with `backdrop-filter: blur(12px)`, hairline borders (`1px solid rgba(255, 255, 255, 0.08)`), and subtle ambient glows.
- **Accents**:
  - **Nominal / Health / Verified**: Electric Emerald (`#00F5A0`) and Neon Cyan (`#00D2FF`).
  - **Warning / Blast Radius / Confirm**: Cybernetic Amber (`#FFB800`).
  - **Critical / Killswitch / Refusal**: Laser Crimson (`#FF3B30`).
  - **Typography Primary**: Polar White (`#FFFFFF`).
  - **Typography Muted**: Monospace Silver (`#8B949E`).

### 2. Typography
- **Display / Headings**: Modern Geometric Sans (e.g., *Outfit*, *Space Grotesk*, or *Inter Display*), tight tracking (`-0.03em`), bold weights.
- **Body / Documentation**: Clean Neutral Sans (*Inter* or *Roboto*), optimized line-height (`1.6`).
- **Telemetry / Code / Terminal**: High-legibility Monospace (*JetBrains Mono*, *Fira Code*), tabular numbers enabled.

### 3. Recommended Interactive Website Components
To give visitors a stunning, tactile experience of the OS, the website should feature:

1. **Interactive 5-Stage Policy Pipeline Simulator**:
   - Allows the user to select an example intent (e.g., *"Tune TCP BBR congestion control"*, *"Remove systemd-udevd"*, or *"Patch compiler error in crate"*).
   - Animates the intent flowing through **Planner → PolicyEngine → Reviewer → Implementer → Auditor**.
   - Shows how dangerous intents (like removing `systemd-udevd`) turn red and trigger a **Hard Refusal** at the Reviewer stage.

2. **3-Depth Explanation Toggle**:
   - An interactive slider or 3-tab switch allowing visitors to view the same system change at:
     - **Depth 1 (Simple)**: "Updated network congestion algorithm to BBR."
     - **Depth 2 (Technical)**: Unified diff showing `/etc/sysctl.d/99-bbr.conf` + sysctl reload status.
     - **Depth 3 (Full Diagnostic)**: Complete blast radius score (22/100, Tier 1 Notify), kernel refcount check, and reverse rollback projection.

3. **Live Hardware Killswitch Demo**:
   - A realistic physical-style toggle switch.
   - When engaged, all running simulated processes immediately flash red and freeze with `[SIGKILL: HALTED IN 0.4ms]`, demonstrating the non-caching lockfile mechanism.

4. **Live Terminal / Cockpit Telemetry Preview**:
   - An interactive terminal window simulating `ai-cockpit` output.
   - Displays real-time CPU telemetry, memory, SQLite World Model status (2,418 packages indexed), and recent Work Log entries.

5. **A/B Boot-Slot Pinning Visualizer**:
   - An animated schematic showing active deployment vs staged deployment, demonstrating how Greenboot automatically rolls back to the pinned deployment if a boot milestone fails.

---

## 6. Project Directory Sitemap

```
Directioner-OS/
├── image/                             # Bootable Fedora bootc OS image definition
│   ├── Containerfile                  # Layered atop quay.io/fedora/fedora-bootc:42
│   ├── build.sh                       # Local podman/bootc-image-builder build script
│   └── configs/
│       ├── systemd/                   # System services (ai-capabilityd, ai-killswitch)
│       ├── ai-engineer/               # Default configs (self-healing.toml, ai-capabilityd.default)
│       └── greenboot/                 # Required health check scripts for automatic rollback
├── ai-engineer/                       # Rust Workspace (Core Intelligence & Safety)
│   ├── capability-layer/              # ai-capabilityd daemon, Server, Handler, ScopedPath, KillSwitch
│   ├── control-plane/                 # Planner, PolicyEngine, Reviewer, Implementer, Conversational
│   ├── world-model/                   # SQLite state engine, DNF5/systemd sync adapters, ai-worldmodel
│   ├── worklog-cli/                   # Append-only audit stream CLI (ai-worklog)
│   ├── benchmark-suite/               # Scored benchmark test runner (ai-benchmark)
│   └── Cargo.toml                     # Workspace root
├── packaging/                         # Fedora RPM spec files and packaging automation
│   ├── rpm/directioner-ai-engineer.spec
│   └── build-rpm.sh
├── docs/                              # Architecture specs, security self-audits, phase checklists
│   ├── security-self-audit.md         # Full internal security audit (CAP-01 through 04)
│   ├── multi-machine-federation.md    # LAN peer telemetry & cryptographic snapshot spec
│   └── phase-5-checklist.md           # Roadmap tracking and phase verification logs
├── vm-test/                           # QEMU disposable VM smoke test scripts
└── .github/workflows/                 # CI pipeline: Build image → qcow2 → Headless VM test
```

---

## 7. How to Pull, Build, and Run Directioner-OS Locally

For visitors or developers wanting to test the operating system themselves:

### Prerequisites
- Linux host with KVM enabled (`/dev/kvm`)
- `podman` (or `docker`) installed
- `qemu-system-x86_64`

### 1. Clone and Test Core Subsystems
```bash
git clone https://github.com/aditya-munday/Directioner-OS.git
cd Directioner-OS/ai-engineer
cargo test --workspace
```

### 2. Build the Bootable OS Container Image
```bash
cd Directioner-OS
sudo podman build -t directioner-os:latest -f image/Containerfile image/
```

### 3. Generate the Bootable QCOW2 Disk Image
Using `bootc-image-builder`:
```bash
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

### 4. Boot Headlessly in QEMU VM
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

Log in via SSH:
```bash
ssh -p 2222 ci@localhost
```

---

## 8. Summary Checklist for Web Agent

When assembling the website:
- [ ] Highlight the core thesis: *"The differentiator is not that the OS can talk. It is that the OS employs an engineer."*
- [ ] Emphasize the **5-Stage Gate** and **Quantitative Blast-Radius Scoring**.
- [ ] Include the interactive **Killswitch** and **3-Depth Explanation** demonstrations.
- [ ] Maintain the sleek **Dark-Tech / Aerospace Telemetry** design system.
- [ ] Link out to GitHub: `https://github.com/aditya-munday/Directioner-OS`.
- [ ] Document the local QEMU build and run workflow cleanly.
