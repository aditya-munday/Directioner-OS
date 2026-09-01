# Directioner-OS — AI Engineer Benchmark Suite (v1.0.0)

Consolidated catalog of the 22 real-world OS engineering benchmark scenarios executing under [`ai-benchmark`](file:///home/aditya_munday/projects/Directioner-OS/ai-engineer/benchmark-suite/src/main.rs).

---

## Benchmark Scenario Matrix (22/22 Passing, 100.0%)

| # | Scenario Name | Category | Primary Safety & Correctness Invariant Tested | Typical Time |
| :-: | :--- | :--- | :--- | :-: |
| **1** | `webserver_setup` | Lifecycle | Synthesizes multi-step nginx setup, simulates diffs, executes capability write, and verifies active port binding | ~800 ms |
| **2** | `service_remediation` | Remediation | Inspects degraded systemd service, plans compensating restart, and verifies recovered health state | ~50 ms |
| **3** | `dependency_audit` | World Model | Audits reverse package dependencies in SQLite World Model and assigns accurate quantitative blast-radius scores | ~5 ms |
| **4** | `file_write_compensating_rollback` | Rollback | Injects mid-plan file write failure and verifies automatic execution of reverse compensating rollback steps | ~25 ms |
| **5** | `package_service_compensating_rollback` | Rollback | Validates live package and service rollback staging across multi-step transaction boundaries | ~1800 ms |
| **6** | `policy_gate_enforcement` | Policy | Verifies that protected units (`sshd`, `systemd-journald`) and critical paths (`/etc/sudoers.d`) are rejected | ~25 ms |
| **7** | `killswitch_hard_halt` | KillSwitch | Proves that engaging `/run/ai-killswitch.lock` instantly hard-halts all capability dispatches | ~25 ms |
| **8** | `worklog_auditability` | Auditing | Proves 100% of capability dispatches are monotonically sequenced in append-only JSONL worklog | ~35 ms |
| **9** | `multi_crate_repair` | CodeEngine | Cross-crate AST symbol graph localization, multi-diagnostic repair patch synthesis | ~25 ms |
| **10** | `multi_crate_rollback` | CodeEngine | Multi-crate atomic rollback restoration upon compilation or verification failure | ~25 ms |
| **11** | `auto_approve_policy_refusal` | Policy | Verifies that `--auto-approve` flag is strictly refused for high-risk `Confirm` / `ExpertReview` tiers | < 1 ms |
| **12** | `cockpit_telemetry` | Observability | Real-time cockpit aggregation across World Model, Work Log, and Health Model telemetry | ~25 ms |
| **13** | `conversational_intent_parsing` | Conversational | Grounded translation of natural language queries into strictly typed `Plan` draft representations | < 1 ms |
| **14** | `conversational_multiturn_refinement` | Conversational | Multi-turn conversational plan refinement (e.g. changing ports or adding steps) with invariant re-auditing | < 1 ms |
| **15** | `conversational_cloud_failover` | Conversational | Fails closed by default when cloud escalation is unconfigured, preserving strict local-first security | < 1 ms |
| **16** | `conversational_local_fallback` | Conversational | Local domain socket communication fallback with actionable operator diagnostic guidance | < 1 ms |
| **17** | `conversational_adversarial_rejection` | Conversational | Neutralizes adversarial prompt injections, role-mimicry, and bracket breakout attempts | < 1 ms |
| **18** | `conversational_state_drift` | Conversational | Re-checks pre-flight SQLite World Model entity freshness to invalidate drifted plan drafts | < 1 ms |
| **19** | `conversational_safety_boundary` | Conversational | Prohibits unallowlisted capabilities or unreviewed dangerous actions from conversational synthesis | < 1 ms |
| **20** | `conversational_context_compaction` | Conversational | Bounded FIFO conversation window auto-compacts older turns while preserving system grounding facts | < 1 ms |
| **21** | `conversational_corrupted_model_output` | Conversational | Truncated or malformed model JSON outputs fail closed gracefully through `PlanDraftValidator` | < 1 ms |
| **22** | `conversational_execution_gate` | Conversational | Cryptographic 16-hex plan state fingerprinting, Reviewer veto gating, and human confirmation enforcement | < 1 ms |

---

## Execution Command

```sh
cargo run -p benchmark-suite --bin ai-benchmark
```
