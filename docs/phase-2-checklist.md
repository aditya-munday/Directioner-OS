# Phase 2 — Multi-Agent Engineering & Diagnostic Control Plane Checklist

Status tracking against the build guide (`AI_Native_Linux_OS_HowTo_Build_Guide.docx`, §3 Phase 2) and master plan.

Legend: `[x]` done and verified · `[~]` scaffolded, needs CI/VM to verify · `[ ]` not started

---

## 2.1 Multi-Agent Reviewer Role (`control-plane::reviewer`)
- [x] Independent safety audit and invariant validator for execution plans before implementation
- [x] Path traversal detection (`..` checking) on all target files
- [x] Critical system path modification blocks (`/etc/sudoers.d`, `/boot`, `/usr`) without `ExpertReview` tier
- [x] Destruction protection blocking core package uninstalls (`glibc`, `systemd`)
- [x] Protected systemd unit stop blocks (`ssh.service`, `systemd-journald`)
- [x] Mandatory compensating rollback completeness check for all state-mutating steps
- [x] Three-level audit verdict: `Approved`, `ApprovedWithWarnings`, `Rejected`
- **Done when**: Reviewer unconditionally audits every plan, and blocks violations with non-zero exit code. ✔ (verified live)

---

## 2.2 Workspace Symbol Indexing & Cross-Crate Localization (`control-plane::symbol_graph`, `control-plane::code_engine`)
- [x] AST/regex workspace indexer for Rust structs, enums, traits, functions, methods, and visibility
- [x] Cross-crate diagnostic localization (`CodeEngine::localize_diagnostic_root`) mapping call-site errors to upstream definition spans
- [x] Structured compiler message parser (`CodeEngine::parse_cargo_json`) extracting diagnostics, primary spans, and child machine suggestions
- [x] Multi-file coordinated patch synthesis (`CodeEngine::synthesize_cross_crate_plan`)
- **Done when**: Cross-crate compilation defects localize root causes to upstream crates and synthesize multi-step remediation plans. ✔ (verified live)

---

## 2.3 Diagnostic CLI Subcommands (`control-plane::bin::ai-engineer`)
- [x] `ai-engineer diagnose <path>`: runs `cargo check` JSON stream, cross-references workspace symbol graph, and outputs structured root-cause analysis
- [x] `ai-engineer fix <path> [--auto-approve]`: synthesizes remediation plans, runs Reviewer audit, prompts user or enforces strict policy gates on `--auto-approve`, and executes atomic rollback upon failure
- [x] Clean refusal (`exit 1`) with informative message when compiler diagnostics lack machine-applicable suggestions
- **Done when**: Real machine-applicable compiler suggestions apply column-precise patches, while non-machine suggestions cleanly refuse without generating stub code. ✔ (verified live)

---

## 2.4 Policy Engine Hardening & Crate-Boundary Dispersion (`control-plane::policy`)
- [x] Crate-boundary dispersion metric: calculates unique workspace crate roots touched by a plan
- [x] Risk calibration: $+15$ score penalty for 2 crates, $+25$ score penalty for $\ge 3$ crates
- [x] Single-crate multi-file edits stay in low-risk `Notify` tier; multi-crate patches escalate to `Confirm` tier
- [x] Reviewer advisory warning finding (`cross_crate_dispersion`) emitted when a plan touches $\ge 3$ distinct crates
- **Done when**: Multi-crate patches touching core dependencies require human confirmation. ✔ (verified live)

---

## 2.5 Phase 2-Adjacent Tooling: Operational Cockpit (`control-plane::cockpit`, `ai-cockpit`)
- [x] Telemetry aggregator (`CockpitState`) unifying SQLite World Model, append-only Work Log, and hardware killswitch state
- [x] Degraded and failed systemd unit event tracking
- [x] Point-in-time snapshot generator (`CockpitSnapshot`)
- [x] Terminal ANSI dashboard renderer (`CockpitRenderer::render_terminal`) and JSON telemetry (`ai-cockpit --json`)
- [x] Real-time 1s watch dashboard CLI (`ai-cockpit --watch`)
- **Done when**: Real-time cockpit displays system health, active plans, audit feed, and killswitch engagement. ✔ (verified live)

---

## 2.6 Extended Benchmark Suite: Scenarios 9–12 (`benchmark-suite`)
- [x] 12/12 passing benchmark scenarios (100.0% pass rate):
  1. `webserver_setup`: Plan generation, preview diffs, execution, verification
  2. `service_remediation`: Status inspection, config repair, daemon restart, verification
  3. `dependency_audit`: Reverse dependency scoring in World Model
  4. `file_write_compensating_rollback`: Forward mutation and reverse-order file rollback
  5. `package_service_compensating_rollback`: Inverse operations for package and service mutations
  6. `policy_gate_enforcement`: Protected units and critical path security boundaries
  7. `killswitch_hard_halt`: Hardware killswitch immediate lock
  8. `worklog_auditability`: 100% request auditing in append-only SQLite WAL work log
  9. `multi_crate_repair`: Cross-crate diagnostic localization and machine suggestion patch
  10. `multi_crate_rollback`: Multi-crate reverse-order atomic rollback upon post-application verification failure
  11. `auto_approve_policy_refusal`: Refusal of high-risk / `ExpertReview` operations and Fatal Reviewer findings under `--auto-approve`
  12. `cockpit_telemetry`: Health aggregation, degraded unit tracking, killswitch detection, ANSI dashboard rendering
- **Done when**: All 12 scenarios pass with 100% score in scored benchmark runner. ✔ (verified live)

---

## Explicitly Deferred / Known-Incomplete Capabilities in Phase 2
The following capabilities were intentionally deferred during Phase 2 scoping and are explicitly cataloged:
1. **Autonomous Semantic Code Synthesis**:
   - `CodeEngine` applies structured machine suggestions provided by `rustc` JSON output.
   - For compiler errors where `rustc` does *not* provide an automated machine suggestion (e.g. non-trivial lifetime mismatches, complex trait implementations, algorithmic logic errors), the CLI explicitly refuses and exits (`exit 1`) with `"Could not synthesize automated remediation plan from diagnostics (no machine-applicable compiler suggestions available)"`.
   - Generating novel, semantically correct code out of whole cloth is deferred for a dedicated scoped design.
2. **Incremental Live Symbol Graph Watcher**:
   - `WorkspaceSymbolGraph` currently indexes the workspace on-demand during diagnostic runs. Live filesystem watcher-based incremental symbol graph caching during background daemons is deferred.

See [`docs/deferred-items.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/deferred-items.md) for the project-wide consolidated register of deferred items.

---

## Verification Commands
```sh
cd ai-engineer
cargo test --workspace

# Run complete 12-scenario benchmark suite
cargo run -p benchmark-suite --bin ai-benchmark

# Test diagnostic inspection on a crate
cargo run -p control-plane --bin ai-engineer -- diagnose <path>

# Test live operational cockpit
cargo run -p control-plane --bin ai-cockpit
```
