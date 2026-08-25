# Directioner-OS — Full Context Summary

## Project Identity

**Name**: AI-Native Linux OS (working name: Directioner-OS)  
**Owner**: Aditya  
**Base**: Fedora Linux (fork/ostree image, NOT Fedora source modification)  
**Date**: August 2026  
**Approach**: Configuration & layering only — write your own services on top of Fedora  

---

## The Core Thesis (Problem Statement v2)

> *"The differentiator is not that the OS can talk. It is that the OS employs an engineer."*

Current Linux distros treat AI as an app (chatbot, shell helper). This project treats **AI as a systems-level control-plane citizen** — capable of plan, execute, verify, and rollback multi-step system changes with the same rigor a senior systems engineer would apply. Simultaneously solving Linux's three decade-old pain points: driver/hardware setup friction, Windows app/game compatibility, CLI-complexity, and tribal-knowledge support model.

---

## Formal Problem Statement

Design and build a Fedora-derived Linux OS where an embedded, multi-agent **AI Engineer subsystem** is a first-class, capability-scoped, auditable control-plane citizen — able to plan, implement, test, review, ship, monitor, and roll back changes across the entire software and configuration surface — while preserving transparency, determinism, and user sovereignty that expert Linux users require.

---

## Reference Architecture (10 Layers)

| # | Layer | Key Contents |
|---|-------|-------------|
| 10 | **Experience** | Desktop shell, launcher, Work Log UI, voice, widgets |
| 9 | **AI Interaction** | NL, voice, visual context, intent parsing, 3-depth explanations |
| 8 | **AI Control Plane** | Multi-agent orchestration, planning, verification, memory |
| 7 | **Capability Layer** | Typed APIs: files, packages, services, network, hardware, boot, snapshots |
| 6 | **Safety Layer** | Auth, capability tokens, approval gates, blast-radius scoring, audit log, rollback |
| 5 | **System Intelligence** | Live world-model: package/service/config/code graphs, health model, event stream |
| 4 | **Compatibility Layer** | Wine/Proton, containers, legacy shims |
| 3 | **Platform Layer** | dnf5/rpm-ostree, D-Bus, SELinux/polkit, PipeWire, Wayland, btrfs |
| 2 | **Kernel Layer** | Linux kernel, eBPF introspection points, drivers, modules |
| 1 | **Firmware/Hardware** | UEFI, ACPI, TPM, GPU, CPU |

---

## The AI Engineer — Core Design

### Multi-Agent Organization (not one monolithic model)
| Agent Role | Authority | Can Execute? |
|-----------|-----------|-------------|
| **Planner** | Turns intent → multi-step plan + blast-radius estimate | ❌ Read-only |
| **Implementer** | Executes approved plan through Capability Layer | ✅ Scoped |
| **Test/Verification** | Independent; confirms actual outcome matches intent | ✅ Test-only |
| **Security & Policy** | Evaluates plan against capability/policy model; can veto | ✅ Gate-only |
| **Reviewer** | Independent second model pass over actual diff | ✅ Review-only |
| **Release/Rollout** | Staged rollout strategy, owns rollback trigger conditions | ✅ Scoped |
| **Incident Response** | Pre-approved narrow authority: auto-rollback on hard thresholds | ✅ Emergency |

> On constrained hardware: all roles served sequentially by one local model with strict role-switch prompting. On capable hardware: genuinely independent parallel passes.

### The System World-Model (most critical new infrastructure)
- **Package & Dependency Graph** — live graph from dnf5/rpm-ostree transaction hooks
- **Service Graph** — systemd units, deps, state, restart history, resource usage
- **Configuration Graph** — versioned view of every AI-touchable config file, semantic diffs
- **Filesystem Change Journal** — append-only, AI-attributed changes
- **Code Graph** — call/import/build-target graph for opted-in project dirs
- **Hardware & Driver Inventory** — kernel modules, firmware, GPU/CPU topology
- **Event Stream** — unified journald + dmesg + systemd + AI action logs on one timeline
- **Health Model** — per-subsystem score (boot, network, graphics, storage, thermal, packages)

Storage: **local SQLite + embedded graph store + local vector index** — fully offline capable.

### Autonomous Engineering Workflow (every action)
1. **Intent Capture** (NL / GUI / voice / alert / scheduled policy)
2. **Planning** — Planner Agent vs. current world-model
3. **Policy Evaluation** — Security & Policy Agent assigns approval tier: `auto / notify / confirm / expert-review`
4. **Preview** — user sees plan + diffs + rollback plan
5. **Dry-Run / Simulation** — projected diff before real execution
6. **Execution** — Implementer via Capability Layer, smallest safe increments, checkpointed
7. **Independent Verification** — Test/Verification Agent: real checks (service active, port listening, tests pass)
8. **Independent Review** — Reviewer Agent re-examines actual diff
9. **Recording** — world-model + change journal + Work Log updated
10. **Monitoring** — Health Model watches for regressions; triggers Incident Response if needed

### Safety & Capability Token Model
- Every action = typed capability (e.g., `package.install`, `service.restart`, `kernel.module.load`, `file.write[scope]`) — never raw shell access
- Tokens: short-lived, scoped to a specific plan, revocable
- Blast-radius score determines approval gate
- SELinux domains enforce the token model at kernel/LSM level (second line of defense)
- Immutable, append-only audit log
- Signed "do not touch" list — enforced below the AI layer

---

## Build Checklist (HowTo Guide — Phase-by-Phase)

### Phase 0 — Foundation (start here, no exceptions)
| Item | What | Done When |
|------|------|-----------|
| 0.1 | Bootable custom Fedora image (Containerfile + CI) | Fresh VM boots, all packages/configs present |
| 0.2 | **Capability Layer skeleton** (Rust: `ai-capabilityd` Unix socket API: `file.read`, `file.write`, `package.install`, `service.restart`, `service.status`) | `package.install` goes through service, audit log entry created |
| 0.3 | Audit log (append-only SQLite or JSONL) | `ai-worklog show --today` prints every action |
| 0.4 | **Kill-switch** (`ai-killswitch engage` → stops all ai-* services within 1 second) | Exercised mid-flight and confirmed working |

### Phase 1 — Single-Agent AI Engineer
| Item | What | Done When |
|------|------|-----------|
| 1.1 | World-model v1: package + service graphs | Offline query: "what depends on openssl" works |
| 1.2 | Planner + Implementer roles (one model, two prompts) | "install nginx on port 8080" → readable plan → correct execution |
| 1.3 | Dry-run / simulation | Every package install + config write shows preview + requires confirmation |
| 1.4 | Work Log UI (CLI first: `ai-worklog tail`, `ai-worklog show <id>`) | User can answer "what did AI change today" without raw logs |
| 1.5 | Blast-radius scoring + approval gates (rule table first, weighted score later) | Destructive action always requires confirmation, provably |
| 1.6 | AI Engineer Benchmark Suite v1 (5–10 VM scenarios, scored) | Single number "7/10 passing" trackable |

### Phase 2 — Multi-Agent + Code Engineering
- Full 5-agent roles as separate services
- Code graph (rust-analyzer, ctags, pyright)
- Build/test/debugger integration (structured output: `cargo test --message-format=json`, `pytest --json-report`)
- Reviewer Agent as independent pass

### Phase 3 — Self-Healing + Kernel Engineering
- Health Model + Incident Response Agent
- A/B kernel boot slots (ostree deployment pinning)
- Kernel/module build support + DKMS

### Phase 4 — Compatibility + Desktop Polish
- AI-tuned Proton/Wine profiles (local benchmark-based)
- Accessibility personalization
- Gaming: Steam-first, game mode, frame-time monitoring

### Phase 5 — Ecosystem
- Plugin SDK / skill marketplace
- Multi-machine federation (local network, read-only Health Model sharing)
- Public security audit of Safety/Capability layers

---

## Tooling & Tech Stack

| Purpose | Tool |
|---------|------|
| Image build | `podman` + `bootc` + `bootc-image-builder` |
| OS base | `quay.io/fedora/fedora-bootc:42` |
| Package mgmt | `dnf5` / `rpm-ostree` |
| Capability Layer impl | **Rust** (`ai-capabilityd`) — Unix socket API |
| Agent logic prototyping | **Python** (uv/venv) before hardening in Rust |
| World-model storage | SQLite + embedded graph store + local vector index |
| VM testing | QEMU/KVM (`qemu-system-x86_64`), disposable qcow2 snapshots |
| Desktop shell | GNOME (primary), KDE Plasma (second target) |
| CI | podman build → boot qcow2 headlessly → SSH smoke test |

---

## Key Engineering Rules

1. **Never fork Fedora source** — config, packages, and your own services only
2. **AI never shells out directly** — all actions go through the Capability Layer
3. **Nothing touches real hardware before VM proof** — disposable VM loop always
4. **Kill-switch before everything else** — Phase 0 must be complete before Phase 1
5. **Capability Layer and Safety Layer** = highest-consequence code → write yourself in Rust, test thoroughly
6. **Benchmark Suite** = write scoring logic yourself (trust the numbers)
7. **Local-first** = full offline mode is a hard constraint, not a degraded fallback

---

## Sequencing Advice (solo builder)

**Build yourself:**
- Capability Layer (0.2) + Safety Layer pieces (0.3, 0.4, 1.5)
- Benchmark Suite scenarios (1.6) + scoring logic

**Delegate to AI (Claude Code, etc.):**
- Boilerplate: Containerfile variations, systemd unit files, CLI arg parsing
- Work Log UI first version
- Adapter code for well-documented tools (dnf5 JSON parsing, journald queries)

**Realistic first milestone:** Machine running bootc image → type "install nginx" in CLI → see plan + diff → confirm → execute through Capability Layer → logged in Work Log → kill the service → Incident Response scaffolding says it's down. That's Phases 0 + most of 1.

---

## Repo Layout

```
ai-os/
├── image/
│   ├── Containerfile              # OS image definition
│   ├── configs/                   # systemd units, /etc overlays, policies
│   │   ├── systemd/
│   │   ├── selinux/
│   │   └── polkit/
│   └── build.sh
├── ai-engineer/
│   ├── control-plane/             # planner/implementer/reviewer/etc services
│   ├── capability-layer/          # typed API adapters (dnf5, systemd, files...)
│   ├── world-model/               # local graph store + vector index
│   ├── worklog-ui/                # desktop widget + CLI
│   └── benchmark-suite/           # scored test tasks
├── vm-test/                       # disposable VM test harness
└── docs/
```

---

## Success Metrics

- AI Engineer Benchmark Suite score trend (release over release)
- % of AI-proposed changes passing independent verification without rollback
- MTTR from detected regression → Incident Response rollback (target: seconds)
- % of AI actions fully explainable and auditable (target: 100% by construction)
- Boot-to-desktop + idle overhead vs. stock Fedora (target: within 5%)
- Real compatibility coverage of defined top-N Windows apps/games (measured, not claimed)

---

## Open Questions (from Problem Statement Appendix B)

1. Exact model-routing thresholds (local small model vs. larger review pass cutoffs)
2. Desktop shell final decision (GNOME vs. KDE vs. both)
3. Legal/licensing review before any Android/web-app packaging compatibility work
4. Governance model for third-party skills in the Section 5.9 marketplace (review process, signing authority, revocation)
