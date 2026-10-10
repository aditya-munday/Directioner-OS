# Directioner-OS Plan Status Tracking

Tracking execution of `DIRECTIONER_OS_ULTIMATE_PLAN.txt` (v2: 500 TODOs across 8 waves).

## Wave 0: Baseline Verification
- [x] PR #14 merged to `main` (commit `45af8139f50c80d5f6777c77b3d144e95d88a597`).
- [x] CI Run `37915676199` 100% green on both jobs (`cargo test` + headless QEMU VM smoke test).
- [x] Live VM capability assertions r6/r7/r8 verified (`sshd.service` restart/stop denial, canary success).
- [x] VMware serial-getty drop-in documented in `docs/vmware-verification.md`.

## Wave 1: Foundations (OPEN)

### Domain 1: Capability Daemon / IPC
- [x] **TODO 001** (`OPEN`): Wire limits on the socket protocol
  - Subtasks 1.1–1.5 completed: 64 KiB payload cap before allocation, 5s read timeout, 64 max concurrent connections, structured error responses.
  - Issues: #16 (Spec), #17 (Impl), #18 (Verification).
  - PRs: PR-1 (#19), PR-2 (#20), PR-3.
  - Verification: `docs/wire-limits-verification.md`.
- [ ] **TODO 002** (`OPEN`): Subprocess timeouts
- [ ] **TODO 003** (`RESEARCH-FIRST`): D-Bus interface study
- [ ] **TODO 004** (`DESIGN-FIRST`): Sequencing and idempotency
- [ ] **TODO 005** (`OPEN`): Atomic file operations
- [ ] **TODO 006** (`SPLIT`): Transitive unit-dependency graph
- [ ] **TODO 007** (`OPEN`): sd_notify and socket activation
- [ ] **TODO 008** (`OPEN`): Bounded output and secret scrubbing
- [ ] **TODO 009** (`RESEARCH-FIRST`): Privilege reduction
- [ ] **TODO 010** (`DESIGN-FIRST`): Interrupted-sequence detection
