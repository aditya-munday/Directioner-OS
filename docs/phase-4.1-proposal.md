# Phase 4.1 Design Proposal: Policy-Governed Conversational Execution Wiring

> **Status**: WORDS-FIRST ARCHITECTURAL DESIGN PROPOSAL  
> **Target Subsystems**: `control-plane::conversational`, `ai-engineer::chat`, `ai-engineer::ask`  
> **Permanent Safety Invariant**: Zero changes to `policy.rs`, `reviewer.rs`, `self_healing.rs`, `incident_response.rs`, or `CapabilityKind` enum.

---

## 1. Executive Summary & Objective

In Phase 4.0.1, Directioner-OS established the **Draft-Only Conversational Operator**:
1. Pluggable `ModelBackend` (Local UNIX domain socket, with cloud failovers disabled by default).
2. 4-layer validation pipeline (`PlanDraftValidator`): Markdown JSON extraction, strict schema deserialization (`#[serde(deny_unknown_fields)]`), semantic capability validation, and SQLite World Model pre-flight state drift checks.
3. In-memory `PlanExecutionGate` computing 16-hex cryptographic state fingerprints (`compute_plan_hash`) and gating plans on Reviewer verdicts and blast-radius tiers.
4. Draft-only CLI preview surface with zero reachable dispatch to `CapabilityClient`.

**Phase 4.1 Objective**: Connect the validated `Plan` drafts emitted by the conversational operator to the real execution pipeline (`Implementer`, `Verifier`, and `CapabilityClient` -> `ai-capabilityd`), governed by a strict **Human-in-the-Loop Confirmation Protocol**.

---

## 2. Core Execution Invariants & Gating Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│                         OPERATOR TERMINAL                              │
│       ai-engineer ask "Restart broken worker" --apply                  │
│       or interactive: ai-engineer chat -> "apply"                      │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   PHASE 4.0.1 VALIDATION PIPELINE                      │
│                                                                        │
│  1. Ground in SQLite World Model & Cockpit Snapshot                    │
│  2. Synthesize Typed Plan Draft (14 Primitives Only)                   │
│  3. PlanDraftValidator (4-Layer Schema & Entity Drift Check)           │
│  4. PlanSimulator -> Unified Line Diff Projections                     │
│  5. PolicyEngine -> Blast Radius Score & Approval Tier                 │
│  6. Reviewer -> Audit Invariants (Paths, Protected Units, Packages)    │
│  7. Compute Cryptographic Plan Hash (16-Hex SHA-256 Fingerprint)       │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│               PHASE 4.1 PLAN EXECUTION GATE (GATING LOGIC)             │
│                                                                        │
│  Case 1: Reviewer Verdict == Rejected / Fatal Finding                  │
│          ➔ Hard Halt. Execution forbidden under all flags.             │
│                                                                        │
│  Case 2: KillSwitch Engaged (/run/ai-killswitch.lock)                  │
│          ➔ Hard Halt. Execution silenced before socket dispatch.       │
│                                                                        │
│  Case 3: ApprovalTier::Auto (< 20 blast score)                         │
│          ➔ Authorized with explicit `--apply` flag or interactive 'y'. │
│                                                                        │
│  Case 4: ApprovalTier::Notify (20..40 blast score)                     │
│          ➔ Authorized with `--apply`, logs pre-dispatch intent to      │
│            live worklog, proceeds to execution & verification.         │
│                                                                        │
│  Case 5: ApprovalTier::Confirm / ExpertReview (>= 40 blast score)      │
│          ➔ Interactive: Requires explicit terminal `y/N` prompt.       │
│          ➔ Non-interactive: Requires `--apply --confirm-hash <HASH>`.  │
│          ➔ Injected text / prompt mimicry has zero authority.          │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ [Passes All Gates]
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                     EXISTING CONTROL PLANE EXECUTION                   │
│                                                                        │
│  1. Implementer::execute_plan(plan, client)                            │
│  2. Step-by-step dispatch to ai-capabilityd over local domain socket   │
│  3. Verifier verification & automatic compensating rollback on failure │
│  4. 3-stage append-only audit trail logging in live worklog            │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Detailed Component Specifications

### 3.1 CLI Interface Updates (`ai-engineer.rs`)

1. **`ai-engineer ask <QUERY>`**:
   - Without `--apply`: Prints Plan summary, Reviewer audit, unified diffs, blast score, and `PlanStatus::Draft` (exact Phase 4.0.1 behavior).
   - With `--apply`:
     - If tier is `Auto` or `Notify`: Evaluates gate, executes via `Implementer`, runs `Verifier`, and reports final status.
     - If tier is `Confirm` or `ExpertReview`:
       - If terminal is interactive (TTY): Prompts `Apply this plan? [y/N]: `. Requires exact `y` or `yes`.
       - If non-interactive (script/CI): Requires `--confirm-hash <16_HEX_HASH>`. Mismatched hash refuses execution.

2. **`ai-engineer chat` (Interactive Multi-Turn Session)**:
   - When the operator inputs `apply`, `execute`, or `run`:
     - Evaluates the current active `Plan` draft through `PlanExecutionGate`.
     - Displays confirmation prompt: `Plan [hash] requires [Tier] approval. Confirm execution? [y/N]: `.
     - Upon confirmation, invokes `Implementer` and prints live execution step progress.

### 3.2 Security & Robustness Protections

1. **Anti-Hallucination & Anti-Breakout**:
   - Model cannot emit commands or instructions that bypass the gate; the execution trigger is handled exclusively by Rust CLI runtime logic reading `io::stdin` or CLI flags, never by model text generation.
2. **State Drift Verification Prior to Dispatch**:
   - Immediately before `Implementer::execute_plan`, `PlanDraftValidator::validate_state_drift` is re-checked against the live SQLite World Model to ensure no concurrent package/service mutations occurred during operator deliberation.
3. **Atomic Rollback Preservation**:
   - Any failure in step execution or subsequent `Verifier` check triggers automatic compensating rollback steps (e.g. restoring previous file contents, stopping restarted units) as established in Phase 1.

---

## 4. Proposed Implementation Plan & Verification Strategy

### Step 1: `control-plane::conversational` Execution Integration
- Add `PlanExecutionGate::dispatch_execution` helper that takes a validated `Plan`, `CapabilityClient`, `&mut Implementer`, and `&Verifier`, enforcing all gating rules.

### Step 2: CLI Wiring in `ai-engineer.rs`
- Connect `--apply`, `-y`, and `--confirm-hash` flags in `ai-engineer ask`.
- Connect `apply` / `confirm` interactive loop commands in `ai-engineer chat`.

### Step 3: Comprehensive Integration & Adversarial Test Suite
- Add integration test suite testing:
  1. Dry-run vs `--apply` execution against a mock/live daemon socket.
  2. Prompt-injection refusal (e.g. model response attempting to inject `--apply` or `CONFIRMED` into text).
  3. Hash mismatch refusal for `Confirm` / `ExpertReview` tiers.
  4. Killswitch hard-halt during active conversational execution.
  5. Rollback on verification failure in conversational flow.

---

## 5. Review & Approval Checklist

Before implementing code:
- [ ] User review and approval of the Phase 4.1 execution-wiring design proposal.
- [ ] Confirmation that standing safety invariants (zero changes to `policy.rs`, `reviewer.rs`, `self_healing.rs`, `incident_response.rs`, and `CapabilityKind`) remain strictly enforced.
