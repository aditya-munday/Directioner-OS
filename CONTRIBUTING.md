# Contributing to Directioner-OS

Thank you for contributing to Directioner-OS! Directioner-OS is built on a foundation of rigorous software safety, determinism, and auditable automation.

Before contributing, please read our [Architecture Guide](ARCHITECTURE.md) to understand the system design and layer separation.

---

## 1. Core Principles & Safety Invariants

Every contribution must strictly adhere to the project's non-negotiable safety rules:

1. **No Raw Shell Access**: Never introduce raw shell commands (`/bin/sh`, `exec`, `spawn_shell`) for automated agents. All machine modifications must flow through typed, discrete RPC capabilities.
2. **Audit Before Execute**: Every mutation must be committed to the append-only JSONL audit stream before execution begins.
3. **Mandatory Compensating Rollback**: Every mutating capability must have a corresponding, deterministic inverse rollback capability.
4. **Independent Safety Review**: Execution authority is decoupled from intent parsing and planning. The `Reviewer` and `PolicyEngine` evaluate candidate plans independently.
5. **Fail-Closed by Default**: If a capability, schema, or configuration is malformed, unrecognized, or disabled, the system must refuse execution with structured diagnostics.
6. **No Source Modifications to Fedora**: We build services, packages, and configuration on top of Fedora bootc — we never modify upstream Fedora base sources directly.

---

## 2. How to Safely Add a New Capability (Step-by-Step)

Adding a new system capability requires updating every layer of the validation and execution pipeline. Follow this 9-step workflow:

### Step 1: Capability Specification & Blast Radius Assessment
Define:
- The exact action and its payload fields.
- What prior system state must be captured before execution to enable complete rollback.
- What failure modes exist (e.g. timeout, lock contention, permission error).
- What risk level (0–100) and approval tier (`Auto`, `Notify`, `Confirm`, `ExpertReview`) the capability belongs to.

### Step 2: Define the Capability RPC Type in `capability-layer`
In `ai-engineer/capability-layer/src/lib.rs`:
- Add a new variant to `CapabilityKind`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum CapabilityKind {
    // ... existing variants
    CustomAction {
        target: String,
        param: u32,
        approved: bool,
    },
}
```
- Ensure `#[serde(deny_unknown_fields)]` is preserved to prevent payload smuggling.

### Step 3: Implement the Execution Handler in `capability-layer`
In `ai-engineer/capability-layer/src/handler.rs`:
- Implement the capability execution logic inside `Handler::execute_request`.
- Verify the `KillSwitch` state before dispatch.
- Enforce path sandboxing via `ScopedPath` if interacting with the filesystem.
- Return structured `CapabilityResponse::Success` or `CapabilityResponse::Denied`.

### Step 4: Update the Policy Engine Blast Radius Scoring
In `ai-engineer/control-plane/src/policy.rs`:
- Add a scoring rule in `PolicyEngine::evaluate_capability`:
```rust
CapabilityKind::CustomAction { target, .. } => {
    // Compute blast radius score (0-100)
    let score = if is_critical_target(target) { 80 } else { 30 };
    let tier = ApprovalTier::from_score(score);
    BlastRadiusReport { score, tier, factors: vec!["Custom action modifier".into()] }
}
```

### Step 5: Add Reviewer Invariant Checks
In `ai-engineer/control-plane/src/reviewer.rs`:
- Define safety invariants for the new capability in `Reviewer::review_plan`.
- Flag prohibited targets with `FindingSeverity::Fatal` (blocks plan execution).
- Flag risky configurations with `FindingSeverity::Warning`.

### Step 6: Implement Simulation & Dry-Run Projections
In `ai-engineer/control-plane/src/simulation.rs`:
- Implement `Simulator::simulate_step` to project what the action will do without executing it (e.g. file line diffs, configuration deltas).

### Step 7: Update the Planner & Compensating Rollback Generator
In `ai-engineer/control-plane/src/planner.rs`:
- Update `Planner` intent synthesis to produce the new capability.
- In `Planner::generate_rollback_step`, implement the exact compensating inverse capability.

### Step 8: Update the Draft Validator & Intent Parser
In `ai-engineer/control-plane/src/draft_validator.rs`:
- Add semantic validation rules to `PlanDraftValidator::validate_draft`.
- Ensure path sanitization and boundary constraints are enforced.

### Step 9: Write Comprehensive Tests & Benchmark Scenarios
Every new capability requires:
1. **Unit & Integration Tests** in `capability-layer/tests/` and `control-plane/tests/`.
2. **Adversarial Injection Tests** in `control-plane/tests/adversarial_injection_suite.rs` (verifying role spoofing, parameter fuzzing, and type confusion are rejected).
3. **Rollback Verification Test** (executing a live failure and asserting the system returns to its original state).
4. **Benchmark Scenario** in `benchmark-suite/src/scenarios/` registered in `ai-benchmark`.

---

## 3. Development Workflow & Testing Standards

### Prerequisites
- Rust 1.80+ (stable toolchain)
- `cargo`, `rustfmt`, `clippy`
- `podman` or `docker` (for container and image builds)
- `qemu-system-x86_64` (for VM testing)

### Local Verification Checklist
Before submitting any PR or requesting sign-off, run the full verification pipeline:

1. **Format Check**:
   ```sh
   cargo fmt --all -- --check
   ```
2. **Clippy Lint Pass** (Zero-tolerance on warnings):
   ```sh
   cargo clippy --workspace --all-targets -- -D warnings -A missing-docs
   ```
3. **Full Test Suite Pass**:
   ```sh
   cargo test --workspace
   ```
4. **AI Engineer Benchmark Suite Pass** (Must achieve 100.0% pass rate):
   ```sh
   cargo run -p benchmark-suite --bin ai-benchmark
   ```
5. **Disposable VM Smoke Test** (For image/packaging modifications):
   ```sh
   ./image/build.sh
   ./vm-test/run-vm-test.sh
   ```

---

## 4. Pull Request & Review Process

All changes submitted to Directioner-OS undergo a structured review process:

1. **Branch Isolation**: Feature work must occur on dedicated phase/feature branches. Direct pushes to `main` are restricted.
2. **Boundary Validation**: The reviewer verifies that no execution wiring or capability dispatch was added to unverified modules.
3. **Adversarial Audit**: New features handling external input (LLM outputs, telemetry streams, user commands) must include tests proving adversarial payloads fail closed.
4. **Scorecard Verification**: The benchmark runner `ai-benchmark` must demonstrate 0 regressions across all standard scenarios.
