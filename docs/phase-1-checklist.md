# Phase 1 — Single-Agent AI Engineer Checklist

Status tracking against the build guide (`AI_Native_Linux_OS_HowTo_Build_Guide.docx`, §3 Phase 1) and master plan.

Legend: `[x]` done and verified · `[~]` scaffolded, needs CI/VM to verify · `[ ]` not started

---

## 1.1 System World-Model v1 (`world-model` crate)
- [x] SQLite storage for package graph (packages, provides, requires) and service units
- [x] `rpm -qa` / `dnf5 repoquery` line-protocol sync adapter (fixed `PROVIDENAME`/`PROVIDEVERSION` tags)
- [x] `systemctl` unit status snapshot & transition event logger
- [x] Fast (<10ms) reverse-dependency graph queries (`depends-on`, `requires`)
- [x] Work Log staleness detection & auto-resync
- [x] `ai-worldmodel` CLI (`sync`, `stats`, `depends-on`, `requires`, `unit`, `failed-since`, `watch`)
- **Done when**: Offline reverse-dependency query `ai-worldmodel depends-on openssl` returns in <10ms. ✔ (verified live)

---

## 1.2 Planner + Implementer Roles (`control-plane` crate)
- [x] Multi-step structured `Plan` data model with typed capabilities and step IDs
- [x] Planner agent synthesizes plans from user intent using World Model intelligence
- [x] Automatic compensating `RollbackStep` generation for every state-modifying action
- [x] Capability Layer Unix-socket IPC client (`CapabilityClient`)
- [x] Implementer role executes plans step-by-step strictly via `ai-capabilityd`
- [x] Automatic execution halt and rollback triggering upon any step failure
- [x] Independent Test / Verification agent (`Verifier`) confirms post-execution state
- **Done when**: "install nginx on port 8080" → readable plan → correct execution & verification. ✔ (verified live)

---

## 1.3 Dry-Run & Simulation Engine (`control-plane` crate)
- [x] Unified line diff generation (`generate_unified_diff`) for all file modifications
- [x] Command projection for package installations and service restarts
- [x] Aggregate and per-step blast radius preview
- [x] `ai-engineer dry-run "<intent>"` command for visual preview before execution
- **Done when**: Every state-modifying action shows unified diff preview + requires confirmation. ✔ (verified live)

---

## 1.4 Work Log Integration & 3-Depth Explanations
- [x] 100% of capability operations committed to append-only JSONL Work Log with correlation IDs
- [x] `ai-engineer explain "<intent>" --depth <1|2|3>`:
  - **Depth 1**: 1-line plain English summary
  - **Depth 2**: Engineering design and action breakdown
  - **Depth 3**: Raw capability payloads, policy scores, and simulation diffs
- **Done when**: User can inspect and explain all changes at 3 distinct technical depths. ✔ (verified live)

---

## 1.5 Blast-Radius Scoring & Policy Engine (`control-plane::policy`)
- [x] Quantitative scoring engine (0–100 scale) based on scope, action type, and dependencies
- [x] World Model integration: scales risk based on live reverse dependency count
- [x] 4-Tier policy gates: `Auto` (<20), `Notify` (20–39), `Confirm` (40–79), `ExpertReview` (80–100)
- [x] Hard policy gates on critical system packages (`glibc`, `systemd`) and protected units (`ssh.service`)
- **Done when**: Destructive or high-impact actions provably require confirmation or expert review. ✔ (verified live)

---

## 1.6 AI Engineer Benchmark Suite (`benchmark-suite` crate)
- [x] Automated scored test harness with 7 real-world system scenarios:
  1. `webserver_setup`: Plan, preview diffs, execute nginx setup, and verify active state
  2. `service_remediation`: Inspect failed daemon, repair configuration, restart, verify
  3. `dependency_audit`: Verify `ExpertReview` tier assignment on packages with high reverse dependencies
  4. `compensating_rollback`: Fault injection mid-flight triggers automatic rollback to baseline state
  5. `policy_gate_enforcement`: Rejection of unapproved actions and protected units
  6. `killswitch_hard_halt`: Immediate execution halt (<1s) upon killswitch engagement
  7. `worklog_auditability`: 100% durable audit trail verification
- [x] `ai-benchmark` CLI with terminal scorecard table and `--json` reporting
- **Done when**: Benchmark runs with single trackable score `7/7 Passing (100.0%)`. ✔ (verified live)

---

## Verification Commands
```sh
cd ai-engineer
cargo test --workspace

# Run complete benchmark suite
cargo run -p benchmark-suite --bin ai-benchmark

# Dry-run intent
cargo run -p control-plane --bin ai-engineer -- dry-run "configure nginx on port 8080"

# 3-Depth explanation
cargo run -p control-plane --bin ai-engineer -- explain "configure nginx on port 8080" --depth 2
```
