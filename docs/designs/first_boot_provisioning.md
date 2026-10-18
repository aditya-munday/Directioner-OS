# Design Specification: First-Boot Provisioning & Onboarding

**Author**: Directioner-OS Core Architecture  
**Date**: 2026-10-18  
**Status**: Proposal & Design Specification (Gate: DESIGN-FIRST — Awaiting Aditya Sign-off)  
**Security Invariants**: B4 (Zero Repo Secrets), B1 (Non-Delegation), Offline-First Guarantees

---

## 1. Executive Summary

This design defines the initial provisioning and onboarding workflow for fresh Directioner-OS deployments across bare-metal, QEMU, and public cloud environments.

Key architectural guarantees:
1. **Zero Hardcoded Credentials**: No default passwords, static tokens, or baked-in SSH keys are ever shipped in release images.
2. **AI-Engine Opt-In (Default OFF)**: Autonomous self-healing actions are strictly disabled until explicitly enabled by an authorized operator.
3. **Offline-First Resilience**: All core system features, telemetry collection, local diagnostics, and Work Log operations function 100% offline without external network or LLM connectivity.
4. **Secure Credential Storage**: External LLM API keys (if configured) are never placed in source code or unencrypted world-readable files.

---

## 2. Declarative Provisioning Engine Comparison

| Feature | Ignition | Cloud-Init | Anaconda Kickstart | Directioner-OS Selection |
|:---|:---|:---|:---|:---|
| **Execution Point** | Early initramfs before root pivot | Late userspace after systemd multi-user | Pre-install installer phase | **Ignition (Bare-metal) + Cloud-Init (Cloud VM)** |
| **Idempotency** | Runs exactly once on first boot; self-masks | Runs per-instance; stateful in `/var/lib/cloud` | Requires Anaconda ISO installer | **High** |
| **Bootc Native Support** | Native integration via `bootc-image-builder` | Supported via `cloud-init.service` | Heavy; incompatible with pure ostree container images | **Ignition for physical/virtual disks** |
| **Failure Behavior** | Halts boot on parse error; prevents partial state | Continues on failure; risk of partial setup | Halts install | **Ignition fail-closed guarantee** |

---

## 3. First-Boot Sequence & Workflow

```text
[Power On]
     │
     ▼
[Initramfs: Ignition / Cloud-Init Engine]
     ├── Partition /var & Encrypt with LUKS2 (TODO 36)
     ├── Create Administrative User ('directioner')
     ├── Inject Operator SSH Public Key
     └── Set Static Hostname
     │
     ▼
[Userspace: systemd-firstboot & Milestone Service]
     ├── Initialize Machine ID (/etc/machine-id)
     ├── Set Default Locale & Keymap
     └── Start Directioner Milestone Check
     │
     ▼
[First-Boot Wizard / TUI: 'directioner-setup']
     ├── Interactive Operator Prompt (Serial Console or TTY1)
     ├── 1. Display Hardware Telemetry & Hypervisor Identity (TODO 34)
     ├── 2. Prompt for AI-Engine Activation (Explicit Opt-In: Default [N])
     └── 3. Optional: Configure Local or Remote Model API Keys
```

---

## 4. AI-Engine Opt-In & Credential Storage (B4 Invariant)

### 4.1 AI Engine Default State
In conformance with safety rules, `self-healing.toml` ships with:
```toml
[general]
enabled = false # STRICT DEFAULT: Autonomous actions disabled

[remediation]
require_operator_confirmation = true
```
The first-boot wizard asks:
> *"Enable autonomous system health self-healing engine? [y/N]"*
If the operator enters `y`, the engine enables `ai-capabilityd.service` and writes `enabled = true`.

### 4.2 API Key Lifecycle & In-Memory Encryption
When remote LLM capabilities (e.g. cloud diagnostic synthesis) are configured:
1. **Never in Repository (B4)**: Release images contain zero API keys, bearer tokens, or cloud endpoint credentials.
2. **Restricted Storage**:
   - Credentials are saved to `/etc/ai-engineer/credentials.toml` with strict permissions:
     ```text
     -rw-------. 1 root root 0600 /etc/ai-engineer/credentials.toml
     ```
   - Alternatively sealed using `systemd-creds` bound to TPM2:
     ```bash
     systemd-creds encrypt --with-key=tpm2 plain.txt /etc/ai-engineer/credentials.encrypted
     ```
3. **In-Memory Sanitization**: Loaded keys are scrubbed from memory buffers upon unloading via zeroize.

---

## 5. Offline-First Architecture

Directioner-OS is designed as an autonomous, self-contained operating system:
- **Zero Internet Requirement**: The entire World Model (SQLite graph, package metadata, systemd dependency graphs), Policy Engine, Reviewer, and deterministic diagnostic state machines operate purely from local kernel telemetry.
- **Local Model Fallback**: If an on-device lightweight LLM is present, queries route to a local unix domain socket.
- **Graceful Network Degradation**: If no upstream network is available, `NetworkBlackhole` signals `NoUpstream` or `NoLink` without degrading core OS functionality.

---

## 6. Verification & Sign-off

- [x] Design specification drafted and aligned with Phase 4 roadmap.
- [ ] Sign-off by Aditya before implementation of onboarding wizard binaries.
