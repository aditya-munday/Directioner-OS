# Directioner-OS Architecture

> *"The differentiator is not that the OS can talk. It is that the OS employs an engineer."*

Directioner-OS is an immutable, Fedora-bootc-derived operating system engineered with an integrated, multi-agent AI control-plane. The architecture enforces complete determinism, strict privilege boundaries, auditable execution, and zero raw shell access for automated agents.

---

## 1. System Topology & Architectural Layers

Directioner-OS is organized into six distinct, strictly bounded layers:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       6. CONVERSATIONAL OPERATOR LAYER                      │
│   ai-engineer ask / chat | IntentParser | Model Backends (Local / Cloud)   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ PlanDraft (Draft Only)
┌──────────────────────────────────────▼──────────────────────────────────────┐
│                    5. MULTI-AGENT CONTROL-PLANE PIPELINE                    │
│   PlanDraftValidator ──► PolicyEngine ──► Reviewer (Safety Invariants)     │
│   Planner (Simulation & Rollback Synthesis) ──► Implementer Execution      │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ Typed RPC & Telemetry
┌──────────────────────────────────────▼──────────────────────────────────────┐
│                   4. WORLD MODEL & OBSERVABILITY COCKPIT                     │
│   SQLite Graph (Packages, Services, Graph Stats) | HealthModel | Cockpit    │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ Unix Domain Socket (/run/ai-capabilityd.sock)
┌──────────────────────────────────────▼──────────────────────────────────────┐
│                     3. TYPED CAPABILITY DAEMON LAYER                         │
│   ai-capabilityd | ScopedPath Sandbox | Append-Only Audit Stream (JSONL)    │
│   Hardware / File Killswitch | Pre-Execution Invariant Gates                │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ System API (systemd D-Bus, sysctl, dnf5)
┌──────────────────────────────────────▼──────────────────────────────────────┐
│                  2. FEDORA BOOTC IMMUTABLE BASE LAYER                       │
│   OSTree Deployments | SELinux Enforcing | Polkit | Greenboot Watchdog      │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ Bare Metal / KVM Hypervisor
┌──────────────────────────────────────▼──────────────────────────────────────┐
│                        1. HARDWARE & KERNEL RUNTIME                         │
│   Linux Kernel | GRUB2 Bootloader (Boot Slot Pinning) | Storage & Network   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. The Typed Capability Model

Automated agents and control-plane services never execute arbitrary shell commands (`/bin/sh`, `bash`, `exec`). Every system interaction is mediated through the **Capability Layer** (`capability-layer`), exposing a strictly typed RPC protocol over a local Unix domain socket (`/run/ai-capabilityd.sock`).

### 2.1 Core Capability Variants
* **`FileRead` / `FileWrite`**: Scoped filesystem mutations within authorized roots (`ScopedPath`), with automatic parent directory resolution and atomic writes.
* **`PackageInstall` / `PackageRemove`**: Package management actions mediated through `dnf5` with transactional metadata resolution and reverse dependency auditing.
* **`ServiceStart` / `ServiceStop` / `ServiceRestart`**: Systemd unit lifecycle control with protected system daemon enforcement.
* **`KernelParamSet`**: Dynamic `/proc/sys` parameter inspection and persistence in `/etc/sysctl.d/99-ai-engineer.conf`, pre-capturing prior values for rollback.
* **`KernelModuleLoad` / `KernelModuleUnload`**: Allowlisted runtime kernel module management with reference-count safety validation.
* **`BootSlotPin`**: Hardware and bootloader watchdog stabilization below user-space.

### 2.2 Safety Invariants of the Capability Daemon
1. **Audit-Before-Execute**: Every capability request is monotonically sequenced and committed to an append-only JSONL audit stream (`worklog.jsonl`) **prior** to performing the underlying kernel or system action.
2. **Hardware / File Killswitch**: A lock-file (`ai-capabilityd.lock`) is verified on every single dispatch. When engaged, all mutations are instantly halted with a hard fail-closed error.
3. **Path Scoping**: Absolute paths outside the configured sandbox root are rejected before reaching filesystem syscalls.

---

## 3. Multi-Agent Pipeline & Control-Plane Services

The control plane (`control-plane`) decouples intent comprehension, blast-radius policy evaluation, independent safety review, and execution.

```
                  ┌──────────────────────┐
                  │ Operator User Intent │
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │     IntentParser     │
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │  PlanDraftValidator  │ (Layer 1-4 Schema & Drift Gates)
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │     PolicyEngine     │ (Blast Radius Scoring 0-100)
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │   Reviewer (Agent)   │ (Independent Invariant Audit)
                  └──────────┬───────────┘
                             │
            ┌────────────────┴────────────────┐
     [Fatal / Rejected]                [Approved / Plan]
            │                                 │
            ▼                                 ▼
   Refusal Diagnostic                Compensating Rollback Synthesis
   Returned to User                           │
                                              ▼
                                     Implementer Dispatch
                                  (Step-by-Step Verification)
```

### 3.1 Policy Engine & Blast Radius Scoring
The `PolicyEngine` assigns every proposed action an aggregate risk score (0–100) and routes it into one of four mandatory approval tiers:
* **Tier 1: `Auto` (Score < 25)**: Idempotent inspections, non-critical service restarts, benign configuration reads.
* **Tier 2: `Notify` (Score 25–49)**: Scoped non-critical file writes, standard daemon restarts. Executes while notifying operators.
* **Tier 3: `Confirm` (Score 50–74)**: Package installations, kernel parameter tuning, multi-crate codebase modifications. Requires explicit interactive operator confirmation.
* **Tier 4: `ExpertReview` (Score 75–100)**: Critical infrastructure packages (`glibc`, `systemd`, `kernel`), security param adjustments, destructive deletions. Requires explicit confirmation with fatal review warnings.

### 3.2 Independent Safety Reviewer
The `Reviewer` runs as an adversarial safety pass over candidate plans:
* Enforces structural invariants (e.g. `/etc/sudoers.d` tampering, unapproved protected unit stops, missing rollbacks).
* Assigns finding severities (`Fatal`, `Warning`, `Info`).
* Any `Fatal` finding unconditionally rejects the plan, regardless of LLM claims.

### 3.3 Planner & Automatic Compensating Rollback
When synthesizing a plan, the `Planner` generates an exact sequence of compensating inverse steps for every forward step:
* A `FileWrite` captures the prior file contents or generates an unlinking rollback step.
* A `KernelParamSet` reads and records the current live kernel sysctl value prior to modification.
* If any step fails verification during execution, the `Implementer` halts forward progress and executes rollbacks in reverse order.

---

## 4. Control-Plane Draft Validation Pipeline

The `PlanDraftValidator` is the gatekeeper between raw LLM completions and internal system data structures. It runs four strict validation layers:

1. **Layer 1: Structural JSON Schema & Bounds Validation**:
   - Rejects missing keys, unclosed objects, deep nesting recursion bombs, overlong strings (>1024 char intent, >4096 char summary), excessive step counts (>50 steps), and out-of-range timeouts.
2. **Layer 2: AST & Typestate Validation**:
   - Matches capability payloads against strict enum variants (`#[serde(deny_unknown_fields)]`).
   - Unicode homoglyph detection and zero-width character smuggling neutralization.
3. **Layer 3: Semantic Constraints & Invariants**:
   - Rejects null bytes, path traversal attempts (`..`), and prohibited system paths.
4. **Layer 4: Pre-Flight State Freshness & Drift Detection**:
   - Computes cryptographic SHA-256 state fingerprints of targeted preconditions.
   - If live system state drifts between drafting and approval, the candidate plan is invalidated.

---

## 5. Model Backend & Conversational Architecture

Directioner-OS supports local and cloud LLM execution via a unified `LlmBackend` trait:

* **`LocalSocketBackend`**:
  - Connects to a local Unix domain socket (e.g. llama.cpp / vLLM daemon).
  - Exponential backoff retry with connection jitter.
  - Bounded memory read buffers (512 KB ceiling) to prevent memory exhaustion.
  - Clear offline operator remediation guidance when local socket is unavailable.
* **`CloudAnthropicBackend` & `CloudGeminiBackend`**:
  - Config-driven cloud routing (e.g. Claude 3.7 Sonnet, Gemini 3.7 Flash).
  - **Disabled by default** (`cloud_enabled = false`) to guarantee privacy and sovereignty.
  - Both backends produce identical structured outputs flowing through the same `IntentParser` and `PlanDraftValidator` safety chain.

---

## 6. World Model & Observability

* **World Model (`world-model`)**:
  - Maintains an embedded SQLite graph database of installed packages (`dnf5`/`rpm`), systemd unit states, and dependency edges (`pkg_requires`, `pkg_provides`).
  - Supports reverse-dependency lookups in <10ms for instant blast-radius evaluation.
  - Background event diff poller tracks state transitions (`active`, `failed`, `reloading`).
* **Multi-Signal Health Model (`health_model`)**:
  - Evaluates system health across services, boot slot stability, and package graphs.
  - Crash-loop and flapping service detection without fabricating metrics.
* **Operational Cockpit (`cockpit`)**:
  - Real-time terminal dashboard and JSON telemetry aggregator (`ai-cockpit`).

---

## 7. Autonomous Boot-Slot & Tier 1 Recovery

For critical system updates, Directioner-OS leverages Fedora bootc / OSTree primitives:
* **Greenboot Integration**: Systemd health checks execute post-boot.
* **GRUB2 Countdown**: Failed boots decrement boot attempt counters.
* **Autonomous Rollback**: If health checks fail during boot, the system rolls back to the prior known-good OSTree deployment below user-space.
