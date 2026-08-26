# Directioner-OS (AI-Native Linux OS)

> *"The differentiator is not that the OS can talk. It is that the OS employs an engineer."*

Directioner-OS is a Fedora-derived operating system whose resident, multi-agent
**AI Engineer** subsystem is a first-class, capability-scoped, auditable
control-plane citizen — able to plan, implement, test, review, ship, monitor,
and roll back changes across the machine, while preserving the transparency,
determinism, and user sovereignty expert Linux users require.

## How this repo is organized

```
.
├── image/                     # Phase 0.1 — bootable custom Fedora bootc image
│   ├── Containerfile          #   image definition (base == fedora-bootc:42)
│   ├── build.sh               #   wraps podman/docker build + bootc-image-builder
│   └── configs/               #   layered config: systemd units, SELinux, polkit
├── ai-engineer/               # the AI Engineer subsystem (Cargo workspace)
│   ├── capability-layer/      #   Phase 0.2/0.3/0.4 — Rust ai-capabilityd (typed API)
│   ├── worklog-cli/           #   Phase 0.3/1.4 — ai-worklog CLI (audit log reader)
│   ├── control-plane/         #   Phase 1+ — planner/implementer/reviewer services
│   ├── world-model/           #   Phase 1.1 — package + service graphs, vector index
│   └── benchmark-suite/       #   Phase 1.6 — scored AI Engineer tasks
├── vm-test/                   # disposable VM test harness (guide §4)
├── docs/                      # phase checklists and status
└── .github/workflows/         # CI: build image → qcow2 → headless VM smoke test
```

## The one rule

We write **configuration, packages, and our own services on top of Fedora** —
we never modify Fedora's source. Every privileged action the AI takes goes
through the typed **Capability Layer**, never raw shell access. Nothing touches
real hardware before it is proven in a disposable VM.

## Quick start

1. Build the image (needs `podman` or `docker`):
   ```sh
   ./image/build.sh
   ```
   (This runs in CI on every push. Locally it requires a Linux host with a
   container engine + the `bootc` image builder.)

2. Build + test the Phase 0 `ai-capabilityd` locally:
   ```sh
   cd ai-engineer
   cargo test --workspace
   ```

3. Run the daemon (dev-mode, no root required) and exercise it:
   ```sh
   cd ai-engineer
   target/debug/ai-capabilityd \
     --socket /tmp/ai-engineer/ai-capabilityd.sock \
     --audit-log /tmp/ai-engineer/worklog.jsonl \
     --lock-file /tmp/ai-engineer/ai-capabilityd.lock \
     --work-root "$HOME/ai-scopes"
   ```
   Then read what it did:
   ```sh
   target/debug/ai-worklog --log /tmp/ai-engineer/worklog.jsonl tail --lines 20
   ```

## Status

- [x] Phase 0.1 bootable image — **verified in CI**: podman build → bootc-image-builder
      (`--rootfs xfs`) → qcow2 → headless QEMU boot → SSH in → motd + systemd + bootc asserted
- [x] Phase 0.2 Capability Layer skeleton: `file.read|write`, `package.install`
      (dry-run), `service.status|restart` over a Unix socket, audit-before-execute
- [x] Phase 0.3 Audit log: append-only JSONL, `ai-worklog` CLI
- [x] Phase 0.4 Kill-switch: `ai-killswitch engage|release|status` (lock-file
      re-read per request; cross-process engage verified live)
- [x] Phase 1.1 World-model v1: SQLite package + service graphs, dnf5 repoquery /
      rpm -qa sync adapters (with `PROVIDENAME`/`PROVIDEVERSION` tags), systemctl snapshot + event-diff poller,
      `ai-worldmodel` CLI (`sync`, `stats`, `depends-on`, `requires`, `unit`, `failed-since`,
      `watch`, `--json`), work-log-staleness auto-resync — fixture-tested,
      offline queries < 10 ms
- [x] Phase 1.2 Planner + Implementer roles: multi-step structured planning, automatic compensating
      rollback generation, capability dispatch over Unix socket, and independent verification
- [x] Phase 1.3 Dry-run / simulation engine: unified line diff generation for file modifications,
      command projections for package/service actions
- [x] Phase 1.4 Work Log UI & 3-Depth Explanations: `ai-engineer explain <intent> --depth 1|2|3`
- [x] Phase 1.5 Blast-radius scoring & policy engine: 4-tier approval gates (`auto`, `notify`, `confirm`, `expert_review`)
      with World Model reverse dependency scaling and protected system daemons
- [x] Phase 1.6 AI Engineer Benchmark Suite: 7 real-world system scenarios, scored runner `ai-benchmark` (100% passing)

See `docs/phase-0-checklist.md` and `docs/phase-1-checklist.md` for detailed tracking.

## AI Engineer CLI Commands

### 1. Plan & Dry-Run
```sh
# Generate dry-run preview with unified diffs
cargo run -p control-plane --bin ai-engineer -- dry-run "configure nginx on port 8080"

# 3-Depth technical explanation
cargo run -p control-plane --bin ai-engineer -- explain "configure nginx on port 8080" --depth 2
```

### 2. Execute Benchmark Suite
```sh
cargo run -p benchmark-suite --bin ai-benchmark
```

## Reference documents

- `AI_Native_Linux_OS_Problem_Statement_v2.docx` — the product thinking (AI Engineer spec)
- `AI_Native_Linux_OS_HowTo_Build_Guide.docx` — the builder's manual and checklist
- `AI_Native_Linux_OS_Master_Plan.docx` — the legacy REQ backlog (task tracker only)
- `context_summary.md` — merged context digest

## Engineering rules

1. Never fork Fedora source — config, packages, and our own services only.
2. The AI never shells out directly — everything goes through the Capability Layer.
3. Nothing touches real hardware before VM proof (disposable VM loop, always).
4. Kill-switch before everything else — Phase 0 must finish before Phase 1.
5. Capability Layer + Safety Layer are the highest-consequence code → Rust, well tested.
6. Benchmark scoring logic is written by us — trust the numbers.
7. Local-first / full offline mode is a hard constraint, not a degraded fallback.

Guidance and requirements sourced from the four reference documents above.