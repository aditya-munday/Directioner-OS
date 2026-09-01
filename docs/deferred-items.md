# Directioner-OS — Consolidated Deferred & Known-Incomplete Items

Consolidated register of explicitly deferred, scoped-out, or known-incomplete items across Phases 0 through 3.1.

---

## 1. Native Layered RPM Packaging (Phase 0 / Phase 3) [COMPLETED & VERIFIED]
- **Resolution**: Created Fedora 42 RPM spec file `packaging/rpm/directioner-ai-engineer.spec` and build automation `packaging/build-rpm.sh`. Successfully generates binary RPM `directioner-ai-engineer-0.1.0-1.x86_64.rpm` containing all 7 binaries (`ai-capabilityd`, `ai-worklog`, `ai-killswitch`, `ai-worldmodel`, `ai-engineer`, `ai-cockpit`, `ai-health`), systemd units, and configuration. Verified via local `rpmbuild` execution.

## 2. Autonomous Semantic Code Synthesis Without Compiler Suggestions (Phase 2) [DEFERRED]
- **Current State**: `CodeEngine` applies structured machine suggestions provided by `rustc` JSON output (`cargo check --message-format=json`). If compiler diagnostics lack machine suggestions (e.g. non-trivial lifetime mismatches, complex algorithmic defects), the CLI cleanly refuses (`exit 1`) with `"Could not synthesize automated remediation plan from diagnostics (no machine-applicable compiler suggestions available)"`.
- **Deferred Item**: Generating novel, semantically valid Rust code out of whole cloth for complex compilation errors lacking compiler suggestions is deferred for a dedicated scoped design.

## 3. Incremental Live Filesystem Watcher for Symbol Graph (Phase 2) [COMPLETED]
- **Resolution**: Implemented `IncrementalSymbolGraph` in `symbol_graph.rs` with per-file mtime and file size caching, $O(1)$ single-file delta indexing, and integrated it directly into `CodeEngine`.

## 4. Dynamic Live Package Unlayering Without Reboot (Phase 1 / Phase 3) [DEFERRED]
- **Current State**: `PackageRemove` stages unlayering in ostree deployments (`dnf5 remove`), which takes effect upon next system boot, because direct runtime package unlinking on an immutable OSTree rootfs is forbidden.
- **Deferred Item**: Live runtime dynamic package unlinking without reboot.

## 5. Phase 3.1 Closed-Loop Self-Healing Disabled by Default (Phase 3.1) [ENFORCED INVARIANT]
- **Current State**: Shipped configuration (`/etc/ai-engineer/self-healing.toml`) and binary defaults have `enabled = false`.
- **Enforced Invariant**: Default out-of-the-box autonomous action without operator enablement is intentionally prohibited by safety policy.
