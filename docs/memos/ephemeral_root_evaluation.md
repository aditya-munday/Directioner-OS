# Research Memo: Architectural Evaluation of Ephemeral / Stateless Root

**Author**: Directioner-OS Core Architecture  
**Date**: 2026-10-18  
**Status**: Proposal & Research Evaluation (Gate: RESEARCH-FIRST)  
**Recommendation**: **REJECT** for Directioner-OS Phase 4 Core (Preserve standard bootc/ostree immutable root).

---

## 1. Concept Definition

An "ephemeral root" (or stateless root) mounts the root filesystem (`/`) as a temporary `tmpfs` RAM disk or volatile `overlayfs`, redirecting only designated directories (such as `/var` and `/etc`) to persistent block storage. On every reboot, changes made outside persistent mounts vanish completely.

---

## 2. Technical Evaluation & Friction with Bootc/OSTree

### 2.1 Why Bootc Already Solves the Core Problem
- `bootc` and `ostree` already provide an **immutable, read-only `/usr`** backed by content-addressable storage.
- System binaries cannot be modified by userspace processes even with root DAC permissions without explicit ostree remounting.
- Versioned rollbacks are tracked cleanly through OSTree deployment roots (`/ostree/deploy/...`).

### 2.2 Friction Points with Ephemeral Root
1. **Memory Overhead**: A `tmpfs` root requires allocating valuable RAM to hold temporary files, runtime caches, and socket paths. On resource-constrained edge or cloud instances, this wastes memory.
2. **OSTree 3-Way Merge Invalidation**: OSTree manages `/etc` through a 3-way merge across updates (old-default vs new-default vs current-local). An ephemeral root that wipes `/etc` breaks systemd machine-id and local network configuration persistence.
3. **SELinux Labeling Breakage**: Mounting an unconfined `tmpfs` over `/` frequently leads to `tmpfs_t` label mismatches, causing severe AVC denial storms on boot in Enforcing mode.
4. **Impact on the Work Log**:
   - The hash-chained Work Log (`ai-worklog`) resides in `/var/log/ai-engineer/`.
   - While `/var` is persistent, stateful local sockets (`/run/ai-capabilityd/capability.sock`) and PID tracking in volatile mounts need stable lifecycle coordination that standard ostree provides without tmpfs complexity.
5. **Rollback Ambiguity**: If root is completely ephemeral, what does "rollback" mean? In ostree, rolling back switches the kernel and immutable deployment pointer. If root is disposable, diagnosing why a prior boot failed becomes nearly impossible because debugging artifacts outside `/var` are lost.

---

## 3. Decision: REJECT

**Decision**: **REJECT** the implementation of an ephemeral `tmpfs`/volatile overlay root for Directioner-OS.

**Rationale**: The operational costs (RAM footprint, broken 3-way `/etc` merges, ostree tooling incompatibility, complex SELinux policy) far outweigh the marginal benefit over the existing read-only, cryptographically verified `composefs` + `ostree` root.

---

## 4. "Revisit If" Trigger List

Directioner-OS should re-evaluate this decision only if:
1. **Target Persona Shifts to Ephemeral Kiosks**: The system is repurposed for shared public terminals or live forensic USB boot media where zero residual runtime state is an explicit compliance mandate.
2. **Upstream Bootc Native Ephemeral Support**: The upstream `containers/bootc` project adopts a standardized, fully supported `--stateless` flag with native systemd generator integration.
3. **RAM Densities Render Footprint Irrelevant**: Ultra-high memory baseline environments where allocating 4–8 GB of RAM strictly to volatile root overlay is standard practice.
