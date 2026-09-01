# Phase 4.0.1 — Policy-Governed Conversational Operator (Draft-Only) Checklist

Status tracking against master architecture specifications and Phase 4 Proposal ([`docs/phase-4-proposal.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/phase-4-proposal.md)).

Legend: `[x]` done and verified · `[~]` scaffolded, in review · `[ ]` not started

---

## Phase 4.0.1 Scoped Milestones (Draft-Only Operator)

### 4.0.1.1 Model Communication & Conversation State Architecture (`control-plane::conversational`)
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
- **What this actually verifies**: End-to-end conversational intake engine that safely converts unconstrained natural language model outputs into strictly typed, schema-validated, and drift-checked `Plan` drafts while rejecting all adversarial injections. ✔ (verified in CI run `33493043975`)

---

### 4.0.1.2 Cryptographic Execution Gate & In-Memory Validation Model (`control-plane::conversational::PlanExecutionGate`)
- [x] Cryptographic 16-hex state fingerprinting (`PlanExecutionGate::compute_plan_hash(&plan) -> String`) across intent, summary, steps, capabilities, and rollback triggers
- [x] In-memory plan state transition logic: draft plans start at `PlanStatus::Draft` and evaluate against `PlanExecutionGate::approve_plan(plan, expected_hash)`
- [x] Reviewer rejection hard-gating: plans with Reviewer `Rejected` verdicts (or `Fatal` findings) fail closed and emit `GateVerdict::Rejected`
- [x] Tier-based human confirmation requirement: `ApprovalTier::Confirm` and `ApprovalTier::ExpertReview` require `requires_human_confirmation: true`
- [x] Strictly passive CLI preview surface (`ai-engineer ask` and `ai-engineer chat`):
  - Prints `PlanStatus::Draft`, unified line diff projections, and Reviewer invariant audits
  - Banner: `[!] DRAFT ONLY — CONVERSATIONAL EXECUTION NOT WIRED IN PHASE 4.0.1`
  - Zero execution dispatch or `CapabilityClient` calls reachable from conversational commands
- [x] Benchmark suite expansion to 22 scenarios (Scenario 22: `conversational_execution_gate`), 100.0% pass rate
- **What this actually verifies**: Pure in-memory cryptographic gate and confirmation calculation ensuring conversational queries remain strictly draft-only with zero execution authority. ✔ (verified in CI run `33493043975`)

