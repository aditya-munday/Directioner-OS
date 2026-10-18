# Research Memo: File System Integrity Verification & Detection Tripwires

**Author**: Directioner-OS Security & Observability Team  
**Date**: 2026-10-18  
**Status**: Proposal & Research Evaluation (Gate: RESEARCH-FIRST)  
**Terminology Constraint (Rule E)**: Never use "tamper-proof"; use precise detection semantics.

---

## 1. Executive Summary

This memo analyzes integrity tripwire technologies for Directioner-OS ostree/bootc images: `ostree fsck`, `composefs`, `IMA` (Integrity Measurement Architecture), and `fs-verity`.

Integrity tripwires serve as passive early-warning tripwires. When integrity deviations are detected, they emit structured audit records directly into the hash-chained Work Log (`ai-worklog`), alerting operators without triggering unvetted, destructive automated remediations.

---

## 2. Integrity Technology Comparison Matrix

| Technology | Detection Scope | What it DOES NOT Detect | Boot & Runtime Overhead | Upstream Citation / Reference |
|:---|:---|:---|:---|:---|
| **ostree fsck** | Corrupted or modified object blobs in `/ostree/repo`. | Does not protect runtime memory, volatile mounts, or `/var`. | High: Full object scan adds 4-12s to boot time on cold disk. | `man ostree-fsck(1)` |
| **composefs** | Validates filesystem metadata tree against signed EROFS digest; validates payload files via fs-verity. | Does not prevent runtime memory injection or kernel exploit hijacking. | Low: Merkle tree verification adds <150ms to mount phase; read path cached in pagecache. | `github.com/containers/composefs`, Fedora Bootc Architecture Guide |
| **IMA (Integrity Measurement Architecture)** | Measures file hashes into TPM2 PCR 10 before execution; appraises hashes against signed xattrs (`security.ima`). | Does not detect offline wholesale root filesystem replacement without Secure Boot + TPM2 binding. | Moderate: 2-5% runtime penalty on process `execve` and file open paths. | Linux Kernel Documentation `Documentation/ABI/testing/ima_policy` |
| **fs-verity** | Cryptographic hash tree validation of individual read-only files on block reads. | Does not protect mutable state in `/var`. | Negligible runtime overhead; hardware-accelerated SHA-256 in kernel crypto. | Linux Kernel Documentation `Documentation/filesystems/fsverity.rst` |

---

## 3. Deep-Dive Capabilities & Limitations

### 3.1 `composefs` (Standard for Bootc)
- **How it Works**: The root filesystem is presented as an overlay where lowerdir is a composefs image pointing to CAS (Content Addressable Storage) files validated by kernel fs-verity digests.
- **Detections**: Detects unauthorized offline modifications to `/usr`, `/lib`, and system binaries immediately on read (`EIO` returned by kernel if hash mismatches).
- **Gaps**: Does not protect `/etc` (which is a 3-way merged writable copy in ostree) or `/var`.

### 3.2 `ostree fsck` (Periodic Diagnostic Verification)
- **How it Works**: Traverses the local ostree repository, recomputing commit checksums and object SHA-256 digests.
- **Cost**: Walking thousands of loose objects takes several seconds. Therefore, running `ostree fsck` on every boot is strictly contraindicated.
- **Usage**: Recommended as an on-demand diagnostic probe (`ai-hwcheck` or `ai-health` weekly maintenance timer).

### 3.3 IMA Appraisal
- **Limitations**: Requires signing every RPM package's extended attributes during container build. Adds significant build complexity and fragile keyring synchronization.

---

## 4. Alerting Path: Integration with the Work Log

When an integrity tripwire detects a checksum or hash mismatch:
1. **Passive Logging**: The failure is captured as an audit event:
   ```json
   {
     "subsystem": "integrity_tripwire",
     "event": "hash_mismatch",
     "path": "/usr/bin/bash",
     "expected_digest": "sha256:...",
     "actual_digest": "sha256:...",
     "action_taken": "none_logged_only"
   }
   ```
2. **Work Log Append**: The event is recorded into `ai-worklog` with hash-chaining (TODO 19).
3. **No Blind Auto-Remediation**: The system does NOT delete files or unilaterally reboot; it flags health status as `Critical` in `HealthModelV2` (TODO 30), allowing the operator or self-healing engine to formulate an evidence-gated proposal (TODO 22).

---

## 5. Architectural Recommendation

1. **Primary Layer**: Rely on `composefs` (native to Fedora 42 bootc) with kernel `fs-verity` for real-time read-only root integrity.
2. **Diagnostic Layer**: Run `ostree fsck --quiet` as a weekly low-priority systemd timer (`directioner-integrity-check.timer`), piping results to `ai-worklog`.
3. **Avoid IMA for Phase 4**: IMA appraisal introduces high complexity without clear security gain over composefs + dm-verity/fs-verity in container-based bootc systems.
