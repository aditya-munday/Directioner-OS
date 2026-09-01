# Phase 4 — Policy-Governed Conversational Operator & Multi-Turn Plan Negotiation Checklist

Status tracking against master architecture specifications and Phase 4 Proposal ([`docs/phase-4-proposal.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/phase-4-proposal.md)).

Legend: `[x]` done and verified · `[~]` scaffolded, in review · `[ ]` not started

---

## 4.0 Foundation & Model Backends

### 4.0.1 Model Communication & Conversation State Architecture (`control-plane::conversational`)
- [x] Pluggable Model Backend abstraction (`ModelBackend` trait) supporting synchronous completion with timeout and error mapping
- [x] Local UNIX domain socket backend (`LocalSocketBackend`) with graceful operational instructions when socket is absent
- [x] Cloud backends (`CloudAnthropicBackend`, `CloudGeminiBackend`) disabled by default in compliance with security invariants
- [x] `ConversationSession` state manager with bounded FIFO message buffer (max 10 turns) and auto-compaction
- [x] Strict user input role-escaping sanitizing model delimiters (`<|im_start|>`, `<|im_end|>`, `[INST]`, `[/INST]`, `<system>`, `</system>`) preventing prompt breakouts
- [x] 4-layer validation pipeline (`PlanDraftValidator`):
  - Layer 1: Markdown-fenced JSON extraction (`extract_json_block`)
  - Layer 2: Strict schema deserialization with `#[serde(deny_unknown_fields)]`
  - Layer 3: Semantic structure & capability variant validation (rejecting hallucinated variants or non-existent fields)
  - Layer 4: Live SQLite World Model pre-flight state drift check (invalidating plans targeting deleted or mutated entities)
- [x] 5,000-iteration property-based fuzzing and mutation campaign (`tests/draft_validator.rs`, `tests/conversational.rs`)
- **What this actually verifies**: End-to-end conversational intake engine that safely converts unconstrained natural language model outputs into strictly typed, schema-validated, and drift-checked `Plan` drafts while rejecting all adversarial injections. ✔ (verified in CI run `33465581762`)

---

## 4.1 Policy-Governed Execution Gate & Cryptographic Confirmation Barrier

### 4.1.1 Execution Gating & Out-of-Band Approval (`control-plane::conversational::PlanExecutionGate`)
- [x] Cryptographic 16-hex state fingerprinting (`PlanExecutionGate::compute_plan_hash(&plan) -> String`) across intent, summary, steps, capabilities, and rollback triggers
- [x] Plan state transition gate: draft plans start at `PlanStatus::Draft` and can only transition to `PlanStatus::Approved` via `PlanExecutionGate::approve_plan(plan, expected_hash)`
- [x] Reviewer rejection hard-gating: plans with Reviewer `Rejected` verdicts (or `Fatal` findings) fail closed and emit `GateVerdict::Rejected`
- [x] Tier-based human confirmation requirement: `ApprovalTier::Confirm` and `ApprovalTier::ExpertReview` require `requires_human_confirmation: true`
- [x] CLI interactive confirmation wiring (`ai-engineer ask` and `ai-engineer chat`):
  - Added `--apply`, `-y/--yes`, and `--confirm-hash <HEX>` flags
  - Interactive terminal prompt `Apply this plan [hash: {:016x}] (Tier: {:?})? [y/N]: `
  - Engaged killswitch and hash tampering checks hard-block execution before any capability dispatch
  - Full execution dispatched safely via `Implementer::execute_plan` to `ai-capabilityd`
- [x] Dedicated integration test suite (`ai-engineer/control-plane/tests/conversational_execution_wiring.rs`, 5 tests)
- [x] Benchmark suite expansion to 22 scenarios (Scenario 22: `conversational_execution_gate`), 100.0% pass rate
- **What this actually verifies**: Cryptographically sound out-of-band approval barrier guaranteeing that no conversational dialogue or model hallucination can trigger autonomous state mutations on the OS without explicit human confirmation. ✔ (verified in CI run `33465581762`)

---

## 4.2 Interactive Diagnostic Dialogue & Explanations (Upcoming)

### 4.2.1 Natural Language System Telemetry & Root-Cause Diagnosis
- [ ] Grounded conversational diagnostic queries (`ai-engineer ask "Why did auditd restart this morning?"`)
- [ ] Direct telemetry synthesis consuming World Model unit state history, Work Log audit records, and Health Model score deductions
- [ ] 3-Depth interactive explanation model:
  - Depth 1: Plain-language operational summary
  - Depth 2: Engineering rationale, affected dependencies, and compensating rollback strategy
  - Depth 3: Full capability JSON, projected unified diffs, and exact blast radius risk matrix

---

## 4.3 Multi-Turn Plan Negotiation & Differential Mutation (Upcoming)

### 4.3.1 Conversational Refinement Protocol
- [ ] Multi-turn plan refinement (`"Change port to 8443 and add firewall rule"`)
- [ ] Re-running simulation, policy scoring, and reviewer audit on every conversational turn
- [ ] Differential preview displaying step additions, removals, and blast score deltas between turns
- [ ] Persistent session history tracking and audit correlation
