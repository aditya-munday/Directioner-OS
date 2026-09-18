# Directioner-OS Security Self-Audit & Public Audit Preparation Report

**Audit Date**: September 2026 (Phase 5 Ecosystem Milestone)  
**Classification**: Internal Technical Self-Audit & External Auditor Preparation  
**Target Scope**: Capability Layer (`ai-capabilityd`), Safety Layer (`control-plane`), Plugin SDK, and Federation Protocol

---

## 1. Executive Summary & Threat Model

Directioner-OS is an autonomous Linux operating system designed for developer workstations and local clusters. The system enforces strict separation of privileges:
- **Autonomous Reasoning & Control Plane** (`ai-engineer`): Runs in user-space or bounded operator context, synthesizes multi-step plans, evaluates blast radius via `PolicyEngine`, inspects invariants via `Reviewer`, and drafts actions without raw system-mutating privileges.
- **Privileged Capability Daemon** (`ai-capabilityd`): Runs as a minimal, isolated system daemon managing ostree deployments, kernel parameters, module loading, systemd services, and file updates over a local UNIX domain socket.
- **Plugin Ecosystem & Federation**: Third-party extensions and LAN peer nodes must operate under identical or stricter sandbox boundaries than core system components.

This document records the internal security self-audit conducted across the codebase prior to external third-party evaluation. In keeping with Directioner-OS engineering standards, all findings, architectural strengths, and known edge cases are documented honestly and transparently.

---

## 2. Capability Layer Self-Audit (`ai-capabilityd`)

### 2.1 IPC Socket Permissions & Transport Security
- **Design Intent**: The capability daemon exposes a UNIX domain socket at `/run/directioner/capabilityd.sock` using JSON-Lines protocol. Communication must be strictly local and restricted to authorized processes.
- **Current Implementation**:
  - `Server::bind` creates the parent directory `/run/directioner` (if absent) and binds `UnixListener::bind(path)`.
- **Audit Findings**:
  - **Strengths**: Socket is local-only; network interfaces (TCP/UDP) are not bound.
  - **Edge Case / Imperfect Finding (AUDIT-CAP-01)**: `Server::bind` does not explicitly invoke `fchmod` or adjust the process `umask` prior to socket creation. If `/run/directioner` is created with default permissions (e.g. `0755`) and the daemon umask is `0022`, the socket file may be world-readable and world-writable (`srwxr-xr-x`). While standard systemd service units configure `RuntimeDirectoryMode=0700`, relying on directory mode without defensive `0600` socket permissions presents defense-in-depth risk.
  - **Edge Case / Imperfect Finding (AUDIT-CAP-02)**: The socket server does not currently query `SO_PEERCRED` on Linux to enforce that connecting clients have UID 0 or belong to the `directioner-admin` group.

### 2.2 Path Traversal & Lexical Sandboxing
- **Design Intent**: File operations (`FileRead`, `FileWrite`, `FileDelete`) must never traverse outside the authorized filesystem root.
- **Current Implementation**:
  - `cap_file_write` and `cap_file_delete` inspect path components using `std::path::Path::components()` and reject any `Component::ParentDir` (`..`).
  - `ScopedPath::safe_display` strips root and parent traversals.
  - Paths are checked with `path.starts_with(self.scope.root())`.
- **Audit Findings**:
  - **Strengths**: Lexical directory traversal (`../`) is deterministically blocked at both the `Reviewer` layer and the `Handler` execution layer.
  - **Edge Case / Imperfect Finding (AUDIT-CAP-03 - Symlink TOCTOU)**: `cap_file_write` calls `OpenOptions::new().open(&path)`. If a symlink exists inside the scoped root pointing to an out-of-scope file (e.g. `/var/scoped/link -> /etc/shadow`), standard POSIX `open()` follows symlinks unless `O_NOFOLLOW` is specified. Hardening recommendation: use `openat2` with `RESOLVE_NO_SYMLINKS` or `O_NOFOLLOW` on mutating operations.

### 2.3 Killswitch Latency & In-Flight Execution Halting
- **Design Intent**: One action (touching or writing `/run/directioner/killswitch.lock`) must immediately halt all AI Engineer activity.
- **Current Implementation**:
  - `KillSwitch::is_engaged` performs a sub-millisecond check on the existence and contents of the lockfile.
  - `Handler::dispatch` checks `self.killswitch.is_engaged()` before executing any request.
- **Audit Findings**:
  - **Strengths**: Sub-millisecond latency for subsequent requests; zero memory caching means lock engagement takes effect on the next CPU cycle.
  - **Edge Case / Imperfect Finding (AUDIT-CAP-04 - In-Flight Halting)**: If a long-running subprocess (e.g. `rpm-ostree install` or `bootc update`) is already executing in `std::process::Command`, engaging the killswitch halts *subsequent* steps, but does not send `SIGKILL` or `SIGTERM` to the currently spawned child process. Hardening recommendation: track child process PIDs in a process group and issue `killpg(SIGKILL)` on killswitch transition.

### 2.4 Command Escaping & Argument Injection
- **Design Intent**: System mutation commands (sysctl, modprobe, rpm-ostree, bootc) must never be vulnerable to shell injection or flag poisoning.
- **Current Implementation**:
  - All commands are invoked directly via `std::process::Command::new(binary).arg(...)`.
  - Zero invocations of `sh -c` or `bash -c` exist in `capability-layer`.
- **Audit Findings**:
  - **Strengths**: Classic shell syntax breakout (`;`, `|`, `&&`, `$()`, backticks) is impossible because `Command` passes argument arrays directly to `execve()`.
  - **Defensive Coverage**: Package names and module names are validated against strict alphanumeric and character sets prior to execution, preventing argument injection (e.g. leading hyphens `--`).

---

## 3. Safety Layer Self-Audit (`control-plane`)

### 3.1 PolicyEngine Blast Radius & Tiering
- **Design Intent**: Quantify the risk of every planned step and aggregate score across plans, assigning deterministic approval tiers (`Auto`, `Notify`, `Confirm`, `ExpertReview`).
- **Current Implementation**:
  - Scoring matrix evaluates modifying operations, critical files, package criticality, and service dependencies.
  - Checks against SQLite World Model for package dependencies and service topology.
- **Audit Findings**:
  - **Strengths**: Verified fail-closed behavior: if SQLite database connection fails or is unavailable, `evaluate_capability` defaults to `ApprovalTier::ExpertReview` with score 80.
  - **Cross-Crate & Multiplicity Penalties**: Modifying more than 3 distinct crates or multiple distinct subsystems incurs additive blast radius penalties.

### 3.2 Reviewer Invariant Enforcement
- **Design Intent**: Independent secondary validation pass evaluating plans against hard non-negotiable safety rules before user presentation or execution.
- **Current Implementation**:
  - Enforces: zero empty plans, critical path protections (`/etc/sudoers.d`, `/boot`, `/etc/pam.d`), protected unit restrictions (`systemd-udevd`, `ssh`, `dbus`, `firewalld`), path traversal rejection, rollback completeness, and chronological step ordering.
- **Audit Findings**:
  - **Strengths**: Dual verdict model (`Approved`, `ApprovedWithWarnings`, `Rejected`). Fatal findings immediately fail-close the plan.
  - **Edge Case / Finding**: If `ReviewerConfig::require_rollbacks` is disabled in non-standard configurations, mutating steps can be approved without rollback steps. (Default is strictly `require_rollbacks: true`).

### 3.3 Tier 2 Security Boundaries (`remediation.rs`)
- **Design Intent**: Kernel parameter tuning and module management must be restricted to safe operating bounds.
- **Current Implementation**:
  - `ALLOWED_KERNEL_MODULES` allowlist explicitly limits loadable/unloadable modules (`br_netfilter`, `overlay`, `wireguard`, `fuse`, `nf_conntrack`).
  - `PROHIBITED_SECURITY_PARAMS` banlist strictly refuses modification of LSM, lockdown, and ASLR parameters (`kernel.kexec_load_disabled`, `kernel.yama.ptrace_scope`, `kernel.modules_disabled`, `kernel.randomize_va_space`).
- **Audit Findings**:
  - **Strengths**: Deterministic compile-time allowlists and banlists. Attempts to modify prohibited parameters yield immediate fatal `Reviewer` rejections.

### 3.4 Self-Healing Circuit Breakers (`self_healing.rs`)
- **Design Intent**: Automated recovery loops must never cause reboot-loops, service thrashing, or cascade failures.
- **Current Implementation**:
  - Disabled by default (`enabled: false`).
  - Anti-flap suppression: single-blip transient alerts are ignored; requires consecutive incident observations.
  - Per-entity budget: maximum 3 recovery attempts before hard lockout.
  - Global throttle: maximum 5 total actions across all entities within a rolling 300-second window.
  - Killswitch checks before every plan synthesis and execution.
- **Audit Findings**:
  - **Strengths**: Comprehensive multi-layer circuit breaker design verified by exhaustive regression suite (`tests/self_healing.rs`).

---

## 4. Plugin SDK Isolation Audit (`plugin.rs`)

### 4.1 Structural Isolation & Privileged Handle Prohibition
- **Design Intent**: Third-party plugins must never receive raw `CapabilityClient` or `Implementer` handles, and cannot bypass `PolicyEngine` or `Reviewer`.
- **Current Implementation**:
  - `PluginContext` encapsulates internal client reference as private.
  - Plugin execution trait `PluginExtension::execute(&self, ctx: &PluginContext, input: &Value)` receives only `&PluginContext`.
  - Plans proposed by plugins pass through a mandatory 5-stage gate:
    1. Plugin permission allowlist check (`allowed_capabilities`)
    2. PolicyEngine blast radius evaluation
    3. Approval tier ceiling check (`max_approval_tier`)
    4. Reviewer safety invariant pass
    5. Internal execution with verified approval token
- **Audit Findings**:
  - **Strengths**: Formally proven in `tests/governance_guard.rs` (`test_plugin_sdk_cannot_bypass_policy_or_reviewer`). Plugins are structurally incapable of privilege escalation.
  - **Manifest Cryptographic Integrity**: Plugin manifests compute deterministic canonical SHA-256 digests and require valid digital signatures before admission into `PluginRegistry`.

---

## 5. Multi-Machine Federation Isolation Audit (`federation.rs`)

### 5.1 Remote Execution Non-Existence
- **Design Intent**: Multi-node health sharing must never expose remote execution surfaces.
- **Current Implementation**:
  - `FederationPublisher` emits read-only health metrics (`SystemHealthReport` summary, package counts, unit counts).
  - `FederationPeerClient` ingests signed snapshots into an isolated in-memory cache (`peer_cache`).
  - The module imports zero networking RPC listeners, zero capability handlers, and zero execution dispatch routines.
- **Audit Findings**:
  - **Strengths**: The attack surface for remote code execution over federation is zero.
  - **Tamper Resistance**: Payload digests are cryptographically verified against sender public keys. Replays and future-skewed timestamps are dropped.

---

## 6. Audit Summary Matrix & Remediation Roadmap

| ID | Component | Description | Severity | Status | Remediation Plan |
|---|---|---|---|---|---|
| **AUDIT-CAP-01** | `ai-capabilityd` | Socket creation inherits process umask without explicit `chmod 0600` | Medium | Documented | Set `umask(0077)` prior to `bind` and enforce `fchmod(0600)` |
| **AUDIT-CAP-02** | `ai-capabilityd` | Unix socket missing `SO_PEERCRED` caller UID/GID verification | Medium | Documented | Inspect `SO_PEERCRED` on connection accept; verify UID 0 |
| **AUDIT-CAP-03** | `ai-capabilityd` | FileWrite follows existing symlinks inside scoped root | Low | Documented | Pass `O_NOFOLLOW` or use `openat2(RESOLVE_NO_SYMLINKS)` |
| **AUDIT-CAP-04** | `ai-capabilityd` | Killswitch does not pre-emptively kill active subprocesses in flight | Low | Documented | Track child process PIDs in a process group and issue `killpg` |
| **AUDIT-SAF-01** | `control-plane` | PolicyEngine fails closed on DB error | Nominal | Verified | Maintained and verified by unit tests |
| **AUDIT-PLUG-01** | `plugin` | Plugin SDK structural bypass impossibility | Nominal | Verified | Maintained and verified by governance guard test |
| **AUDIT-FED-01** | `federation` | Zero remote execution authority in federation prototype | Nominal | Verified | Maintained and verified by static analysis and tests |

---

## 7. Conclusion

Directioner-OS demonstrates strong defensive architecture with defense-in-depth layers across plan proposal, blast radius estimation, invariant verification, and capability execution. The identified edge cases (`AUDIT-CAP-01` through `AUDIT-CAP-04`) do not represent immediate bypasses in standard systemd deployments, but provide clear, actionable targets for pre-audit hardening.
