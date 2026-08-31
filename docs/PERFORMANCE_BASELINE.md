# Directioner-OS Performance Baseline & Profiling Report

> **Baseline Profile**: Phase 4.0.1 Validation Pipeline  
> **Environment**: Linux x86_64 (64-bit multi-threaded runtime)  
> **Benchmark Harness**: `ai-engineer/control-plane/tests/performance_profiling.rs`  
> **Target Tolerance**: 0 regressions permitted exceeding 2x baseline limits

---

## 1. Executive Performance Summary

| Subsystem Component | Workload Under Test | Metric Recorded | Target Ceiling | Status |
| :--- | :--- | :--- | :--- | :--- |
| **World Model Ingestion** | 1,000 pkgs, 5,000 edges, 200 units | **91.02 ms** total | < 500.0 ms | **PASS (5.5x headroom)** |
| **Reverse-Dependency Traversal** | 500 dependent package lookups | **19.67 µs / query** | < 2,000.0 µs | **PASS (101x headroom)** |
| **Symbol Graph Initial Scan** | 100 source files across 5 crates | **3.67 ms** (400 symbols) | < 100.0 ms | **PASS (27x headroom)** |
| **Incremental Symbol Re-index** | 20 sequential single-file updates | **5.96 ms / update** | < 50.0 ms | **PASS (8.4x headroom)** |
| **PlanDraftValidator Pipeline** | 1,000 complex draft payloads (L1-4) | **61.57 µs / draft** | < 200.0 µs | **PASS (3.2x headroom)** |
| **Validator Throughput** | Continuous ingestion stream | **16,242 drafts / sec** | > 5,000 drafts/s | **PASS (3.2x headroom)** |

---

## 2. Detailed Subsystem Benchmarks

### 2.1 World Model Ingestion & Graph Traversal (`world-model`)
- **Ingestion Scale**:
  - Packages: 1,000 records with metadata (`name`, `evr`, `arch`, `summary`, `synced_at`).
  - Requires Relationships: 5,000 directed edges with version constraints (`GE 1.0`).
  - Systemd Services: 200 units with state tracking.
- **Observed Ingestion Latency**: `91.02 ms` inside a single transactional batch.
- **Reverse Dependency Resolution**: Average `19.67 µs` per query using SQLite indexed lookup (`idx_pkg_requires_dep`).

### 2.2 Workspace Symbol Graph & Incremental Watcher (`control-plane::symbol_graph`)
- **Workspace Scale**: 100 Rust source files across 5 crates (`core`, `net`, `storage`, `auth`, `crypto`), generating 400 symbol definitions (`struct`, `impl`, `fn`, `method`).
- **Initial Indexing**: Full scan, AST parsing, and graph construction completed in `3.67 ms`.
- **Incremental Sync**: When individual files are modified, mtime cache evaluation skips unchanged files and re-indexes modified files in `5.96 ms / file`.

### 2.3 Model Plan Draft Validation Pipeline (`control-plane::draft_validator`)
- **Validation Pipeline Stages**:
  1. *Layer 1*: Size bounding (512 KB) & UTF-8 character sanitization.
  2. *Layer 2*: Strict JSON typestate deserialization (`#[serde(deny_unknown_fields)]`).
  3. *Layer 3*: CapabilityKind schema & semantic boundary validation.
  4. *Layer 4*: Atomic cryptographic state fingerprinting & drift detection.
- **Latency**: `61.57 µs` per multi-step, multi-rollback draft.
- **Sustained Throughput**: `16,242 drafts / second` on a single thread.

---

## 3. Regression Tracking Invariants

Any future changes to the validation pipeline, world-model database layer, or incremental symbol graph must maintain the following maximum thresholds:

1. **Draft Validation**: Per-draft validation time must never exceed **200 µs**.
2. **Reverse-Dependency Lookup**: Single-package reverse dependency queries must never exceed **2 ms**.
3. **Incremental Re-indexing**: Single-file incremental symbol update must never exceed **50 ms**.
4. **World Model Ingestion**: 1,000-package batch sync must complete in under **500 ms**.
