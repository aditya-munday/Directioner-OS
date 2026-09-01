# Directioner-OS — Consolidated Deferred & Known-Incomplete Items

Consolidated register of explicitly deferred, scoped-out, or known-incomplete items across Phases 0 through 3.1.

---

## 1. Native Layered RPM Packaging (Phase 0 / Phase 3) [COMPLETED & VERIFIED]
- **Resolution**: Created Fedora 42 RPM spec file `packaging/rpm/directioner-ai-engineer.spec` and build automation `packaging/build-rpm.sh`. Successfully generates binary RPM `directioner-ai-engineer-0.1.0-1.x86_64.rpm` containing all 7 binaries (`ai-capabilityd`, `ai-worklog`, `ai-killswitch`, `ai-worldmodel`, `ai-engineer`, `ai-cockpit`, `ai-health`), systemd units, and configuration. Verified via local `rpmbuild` execution and CI VM integration testing.

## 2. Autonomous Semantic Code Synthesis Without Compiler Suggestions (Phase 2) [COMPLETED & VERIFIED]
- **Resolution**: Implemented AST symbol-graph-driven semantic code synthesis engine in `CodeEngine` (`synthesize_semantic_patch`). Resolves complex compilation diagnostics lacking compiler suggestions:
  - `E0425` / `E0412`: Cross-references workspace `WorkspaceSymbolGraph` across all crates to automatically synthesize missing `use <crate>::<module>::<Symbol>;` import statements at file headers.
  - Typo Identifier Correction: Computes fuzzy Levenshtein distance against indexed workspace symbol definitions within edit distance $\le 2$.
  - `E0599`: Trait discovery resolving unimported trait methods and injecting corresponding `use <trait_path>;`.
  - `E0308`: Type coercion synthesis (`.as_str()`, `.to_string()`, `.as_path()`, `PathBuf::from(...)`, `Some(...)`).
- **Verification**: Verified across unit & integration tests in `control-plane/tests/autonomous_code_synthesis.rs`.

## 3. Incremental Live Filesystem Watcher for Symbol Graph (Phase 2) [COMPLETED & VERIFIED]
- **Resolution**: Implemented `IncrementalSymbolGraph` in `symbol_graph.rs` with per-file mtime and file size caching, $O(1)$ single-file delta indexing, and integrated it directly into `CodeEngine`.

## 4. Dynamic Live Package Unlayering Without Reboot (Phase 1 / Phase 3) [COMPLETED & VERIFIED]
- **Resolution**: Implemented live dynamic unlayering and layering execution logic in `capability-layer/src/handler.rs` (`cap_package_remove` and `cap_package_install`). On OSTree immutable deployments, executes dynamic live application (`rpm-ostree apply-live`) with graceful fallback to staged reboot deployment (`status: "staged_pending_reboot"`, `deployment_deferred: true`) if system state or kernel constraints prevent runtime unlayering.
- **Verification**: Verified via test suite in `capability-layer/tests/live_package_unlayering.rs`.

## 5. Phase 3.1 Closed-Loop Self-Healing Disabled by Default (Phase 3.1) [ENFORCED INVARIANT]
- **Current State**: Shipped configuration (`/etc/ai-engineer/self-healing.toml`) and binary defaults have `enabled = false`.
- **Enforced Invariant**: Default out-of-the-box autonomous action without operator enablement is intentionally prohibited by safety policy.
