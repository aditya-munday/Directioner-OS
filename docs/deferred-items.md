# Directioner-OS — Consolidated Deferred & Known-Incomplete Items

Consolidated register of explicitly deferred, scoped-out, or known-incomplete items across Phases 0 through 3.1.

---

## 1. Native Layered RPM Packaging (Phase 0 / Phase 3)
- **Current State**: Control plane binaries (`ai-capabilityd`, `ai-worklog`, `ai-killswitch`, `ai-worldmodel`, `ai-engineer`, `ai-cockpit`, `ai-health`) are compiled via cargo workspace and deployed into `/var/lib/ai-bin/` and `/usr/lib/systemd/system/` via CI/container image copy.
- **Deferred Item**: Packaging these binaries and service units into `.rpm` spec files for native `dnf5` layered package management and distribution on Fedora OSTree.

## 2. Autonomous Semantic Code Synthesis Without Compiler Suggestions (Phase 2)
- **Current State**: `CodeEngine` applies structured machine suggestions provided by `rustc` JSON output (`cargo check --message-format=json`). If compiler diagnostics lack machine suggestions (e.g. non-trivial lifetime mismatches, complex algorithmic defects), the CLI cleanly refuses (`exit 1`) with `"Could not synthesize automated remediation plan from diagnostics (no machine-applicable compiler suggestions available)"`.
- **Deferred Item**: Generating novel, semantically valid Rust code out of whole cloth for complex compilation errors lacking compiler suggestions is deferred for a dedicated scoped design.

## 3. Incremental Live Filesystem Watcher for Symbol Graph (Phase 2)
- **Current State**: `WorkspaceSymbolGraph` indexes the workspace on-demand during `diagnose` and `fix` CLI invocations.
- **Deferred Item**: Maintaining an in-memory background filesystem watcher with real-time incremental AST cache updates.

## 4. Dynamic Live Package Unlayering Without Reboot (Phase 1 / Phase 3)
- **Current State**: `PackageRemove` stages unlayering in ostree deployments (`dnf5 remove`), which takes effect upon next system boot, because direct runtime package unlinking on an immutable OSTree rootfs is forbidden.
- **Deferred Item**: Live runtime dynamic package unlinking without reboot.

## 5. Phase 3.1 Closed-Loop Self-Healing Disabled by Default (Phase 3.1)
- **Current State**: Shipped configuration (`/etc/ai-engineer/self-healing.toml`) and binary defaults have `enabled = false`.
- **Deferred Item**: Default out-of-the-box autonomous action without operator enablement is intentionally prohibited by safety policy.
