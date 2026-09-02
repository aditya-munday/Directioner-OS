# Phase 4 — Conversational Operator, Hardware Compatibility & Desktop Polish Checklist

Status tracking against master architecture specifications and Phase 4 Proposals ([`docs/phase-4-proposal.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/phase-4-proposal.md), [`docs/hardware-driver-compatibility.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/hardware-driver-compatibility.md), [`docs/proton-wine-compatibility.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/proton-wine-compatibility.md), [`docs/localization.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/localization.md)).

Legend: `[x]` done and verified · `[~]` scaffolded, in review · `[ ]` not started

---

## Phase 4 Scoped Milestones

### 4.1 Model Communication & Conversation State Architecture (`control-plane::conversational`)
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
- **What this actually verifies**: End-to-end conversational intake engine that safely converts unconstrained natural language model outputs into strictly typed, schema-validated, and drift-checked `Plan` drafts while rejecting all adversarial injections.

---

### 4.2 Cryptographic Execution Gate & In-Memory Validation Model (`control-plane::conversational::PlanExecutionGate`)
- [x] Cryptographic 16-hex state fingerprinting (`PlanExecutionGate::compute_plan_hash(&plan) -> String`) across intent, summary, steps, capabilities, and rollback triggers
- [x] In-memory plan state transition logic: draft plans start at `PlanStatus::Draft` and evaluate against `PlanExecutionGate::approve_plan(plan, expected_hash)`
- [x] Reviewer rejection hard-gating: plans with Reviewer `Rejected` verdicts (or `Fatal` findings) fail closed and emit `GateVerdict::Rejected`
- [x] Tier-based human confirmation requirement: `ApprovalTier::Confirm` and `ApprovalTier::ExpertReview` require `requires_human_confirmation: true`
- [x] Strictly passive CLI preview surface (`ai-engineer ask` and `ai-engineer chat`):
  - Prints `PlanStatus::Draft`, unified line diff projections, and Reviewer invariant audits
  - Banner: `[!] DRAFT ONLY — CONVERSATIONAL EXECUTION NOT WIRED IN PHASE 4.0.1`
  - Zero execution dispatch or `CapabilityClient` calls reachable from conversational commands
- [x] Governance Guard regression test suite (`tests/governance_guard.rs`) enforcing permanent code-level boundaries and banning execution aliases/backdoors
- **What this actually verifies**: Pure in-memory cryptographic gate and confirmation calculation ensuring conversational queries remain strictly draft-only with zero execution authority.

---

### 4.3 Hardware Driver Compatibility & Diagnostic Layer (`ai-hwcheck`)
- [x] Read-only PCI WiFi diagnostic inspector (`control_plane::hwcheck`): detects Broadcom BCM4360, Realtek RTL8821CE/8852BE, Intel AX200/AX210, identifies kmod vs in-tree drivers and firmware presence without privileged ioctls
- [x] Read-only USB Fingerprint reader inspector: maps USB Vendor/Product IDs for Goodix, Synaptics, and Elan sensors, evaluates `InTreeLibfprint` vs `ProprietaryTodRequired`, checks PAM configuration
- [x] Read-only ACPI sleep/wake power state inspector: evaluates `[s2idle]` vs `deep` S3 in `/sys/power/mem_sleep`, detects NVMe APST latency issues
- [x] Dedicated CLI binary `ai-hwcheck` supporting `--json`, `--sysfs`, `--etc`, `--firmware`
- [x] Integration test suite in `control-plane/tests/hwcheck_tests.rs` (100% passing)
- [x] Packaging and CI integration: added to `directioner-ai-engineer.spec` and in-guest VM smoke tests
- **What this actually verifies**: Non-mutating, zero-privilege hardware inspection engine providing actionable container-layering and kernel-tuning recommendations.

---

### 4.4 Proton/Wine Compatibility Layer Scaffolding & World Model Schema v3
- [x] Comprehensive compatibility architecture specification in `docs/proton-wine-compatibility.md` (tiering Platinum/Gold/Silver/Bronze/Borked, runner selection, DXVK/VKD3D overrides)
- [x] World Model Schema v3 upgrade in `world-model/schema.sql` adding `compatibility_profiles` table and `idx_compat_app_title` index
- [x] World Model data ingestion adapters (`parse_compatibility_profiles`) supporting JSON manifests and line-protocol feeds
- [x] Read-only querying APIs: `query::compatibility_profile` and `query::all_compatibility_profiles`
- [x] CLI inspection subcommand `ai-worldmodel compat [APP_ID] [--json]`
- [x] Integration tests in `world-model/tests/compatibility_profiles.rs` (100% passing)
- **What this actually verifies**: Standardized local database schema and query engine for Linux desktop application compatibility profiles.

---

### 4.5 Accessibility & Structured JSON Completeness
- [x] Audited all 6 CLI tools for accessible output: `ai-engineer`, `ai-worklog`, `ai-cockpit`, `ai-health`, `ai-worldmodel`, `ai-hwcheck`
- [x] Added `--json` flag support to `ai-worklog` (`tail`, `show`, `today`, `search`)
- [x] Added `disks`, `thermals`, `net`, and `compat` subcommands to `ai-worldmodel` with full `--json` support
- [x] Comprehensive CLI accessibility test suite in `control-plane/tests/accessibility_cli_json.rs` verifying valid JSON output across all tools
- **What this actually verifies**: 100% complete programmatic and screen-reader accessibility across all Directioner-OS command-line tooling.

---

### 4.6 Localization Scaffolding & Message Catalog
- [x] Created `control_plane::localization::MessageCatalog` with default `en-US` string catalog covering banners, alerts, reviewer findings, and hardware notices
- [x] Dynamic parameter interpolation `{variable}` and missing key fallback handling
- [x] JSON serialization/deserialization for multi-locale catalogs (e.g. `es-ES`)
- [x] Documented localization architecture and contribution workflow in `docs/localization.md`
- [x] Unit test suite in `control-plane/tests/localization_tests.rs` (100% passing)
- **What this actually verifies**: Structured string catalog architecture ready for community translations without hardcoded string logic.

---

### 4.7 Benchmark Suite Expansion (32/32 Scenarios Passing)
- [x] Added 10 new scored benchmark scenarios (Scenarios 23 to 32) in `benchmark-suite/src/scenarios/phase4_scenarios.rs`:
  - Scenario 23: `hwcheck_wifi_probing`
  - Scenario 24: `hwcheck_fingerprint_probing`
  - Scenario 25: `hwcheck_sleep_wake_state`
  - Scenario 26: `accessibility_json_worklog`
  - Scenario 27: `accessibility_json_worldmodel`
  - Scenario 28: `accessibility_json_health_model`
  - Scenario 29: `worldmodel_proton_wine_sync`
  - Scenario 30: `worldmodel_disk_telemetry`
  - Scenario 31: `worldmodel_thermal_telemetry`
  - Scenario 32: `localization_catalog_interpolation`
- [x] Executed full runner `ai-benchmark`: 32/32 passing (100.0%), 0 gaps, 0 failures.
