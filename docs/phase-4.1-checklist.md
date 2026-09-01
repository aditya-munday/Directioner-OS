# Phase 4.1 — Policy-Governed Conversational Execution Wiring Checklist

Status tracking against master architecture specifications and Phase 4.1 Proposal ([`docs/phase-4.1-proposal.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/phase-4.1-proposal.md)).

Legend: `[x]` done and verified · `[~]` scaffolded, in review · `[ ]` not started

---

## Phase 4.1 Scoped Milestones (Execution Wiring & Confirmation Gating)

### 4.1.1 Cryptographic Execution Gate & Dispatch Architecture (`control-plane::conversational::PlanExecutionGate`)
- [x] Implement `PlanExecutionGate::dispatch_execution` integrating policy, review, killswitch, and drift checks
- [x] Reviewer verdict hard-gating: Plans with Reviewer `Rejected` or `Fatal` findings fail closed before socket communication
- [x] KillSwitch hard-halt: Active `/run/ai-killswitch.lock` halts execution before capability dispatch
- [x] Pre-flight SQLite World Model entity drift validation prior to socket dispatch
- [x] Atomic execution via `Implementer` and independent post-execution verification via `Verifier`
- [x] `ConversationalExecutionReport` data model capturing execution report, verification checks, and plan hash

---

### 4.1.2 Conversational CLI Wiring (`ai-engineer.rs`)
- [x] Add `--apply` flag to `ai-engineer ask` enabling capability execution pathway
- [x] Support `-y` / `--yes` and `--confirm-hash <16_HEX_HASH>` confirmation flags
- [x] Enforce interactive `y/N` terminal prompt for `Confirm` and `ExpertReview` tier plans
- [x] Add explicit `[i] To execute this proposal, rerun with: ai-engineer ask "<query>" --apply` instruction on draft proposal previews
- [x] Update usage and help documentation for `ai-engineer ask` and `ai-engineer chat`

---

### 4.1.3 Integration Testing & Verification (`tests/conversational_execution_integration.rs`)
- [x] Integration test suite validating live execution over capability daemon socket (`test_conversational_draft_vs_apply_execution_gating`)
- [x] Rejection of prompt-injection attempts and hash mismatch on Confirm-tier plans (`test_conversational_hash_mismatch_refusal_on_confirm_tier`)
- [x] Killswitch hard-halt verification before socket communication (`test_conversational_killswitch_hard_halt_before_socket_dispatch`)
- [x] Reviewer veto enforcement blocking execution dispatch (`test_conversational_reviewer_veto_blocks_execution`)
- [x] Benchmark suite (22/22 passing)
- [x] Permanent invariant check: 0 diff lines on `policy.rs`, `reviewer.rs`, `self_healing.rs`, `incident_response.rs`, `capability-layer/src/lib.rs`
