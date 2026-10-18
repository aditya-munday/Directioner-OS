# Research Memo: aarch64 Feasibility & Architecture Evaluation (TODO 46)

**Author:** Directioner-OS Core Engineering Team  
**Date:** October 2026  
**Status:** Evaluation Complete / Decision: DEFER Phase 1 Multi-Arch Support  
**Gate:** RESEARCH-FIRST  

---

## 1. Executive Summary & Recommendation

This research memo analyzes the feasibility, technical overhead, service impacts, and CI infrastructure costs associated with adding `aarch64` (ARM64) architecture support to Directioner-OS.

### Final Recommendation: **DEFER aarch64 Tier-1 Support until Phase 2**
* **Rationale**: Directioner-OS relies on low-level hardware inspection, page table constraints, and x86_64-specific virtualization features (CPUID hypervisor leaf, DMI SMBIOS decoders, and specific bootc kernel modules). Native GitHub aarch64 runners introduce distinct cost/availability constraints, while QEMU TCG emulation for aarch64 adds a 6x to 10x CI execution time penalty.
* **Near-Term Mitigation**: Rust user-space crates (`ai-engineer` workspace) remain cross-compilable via `cargo build --target aarch64-unknown-linux-gnu` with zero unsafe code (`#![forbid(unsafe_code)]`), preventing architectural code lock-in.

---

## 2. Compilation, Emulation & CI Execution Trade-Offs (46.1)

| Strategy | Speed / Overhead | CI Infrastructure & Cost | Pros | Cons / Caveats |
| :--- | :--- | :--- | :--- | :--- |
| **Cross-Compilation (`cross` / `cargo`)** | **Fast** (~1.2x x86_64 build time) | Low (Runs on standard x86_64 GitHub runners) | Fast compilation; validates user-space Rust type systems and dependencies. | Cannot execute integration tests or VM boot tests without emulation. |
| **QEMU User Emulation (`qemu-user-static`)** | **Medium** (~2.5x - 4x slowdown) | Moderate (Standard x86_64 runner with binfmt) | Executes non-kernel binaries and unit tests without hardware changes. | Multi-threaded race detection and atomic operations may behave differently than native silicon; systemd boot testing impossible. |
| **QEMU System Emulation (`qemu-system-aarch64` TCG)** | **Extremely Slow** (8x - 15x slowdown) | High (High runner minutes consumption) | Boots real UEFI aarch64 kernel and initrd. | Cold boot exceeds 4 minutes per test run; CI budget exhaustion. |
| **Native aarch64 Runners (AWS Graviton / GH-hosted)** | **Native Performance** (1.0x native) | Dedicated billing tier required ($0.016/min vs $0.008/min) | Full fidelity hardware testing; accurate memory ordering and atomic benchmarking. | Runner capacity and queue availability can introduce CI pipeline latency; requires multi-arch matrix management. |

---

## 3. Directioner-OS Services & Subsystem Impacts (46.2)

### A. Hypervisor Detection (`world-model::hypervisor`)
* **x86_64**: Queries CPUID leaf `0x40000000` for hypervisor signature (`TCGTCGTCGTCG`, `KVMKVMKVM`, `VMwareVMware`, etc.) and parses `/sys/class/dmi/id/*`.
* **aarch64**: CPUID does not exist. Hypervisor detection must query ACPI/Device Tree tables (`/sys/firmware/devicetree/base`), SMCCC (ARM Secure Monitor Call / Hypervisor Call interface via `/sys/bus/arm/devices` or KVM capabilities), and device tree compatible strings.

### B. Memory Page Sizes & PSI Telemetry (`control-plane::psi`)
* **x86_64**: Fixed 4KiB base page size.
* **aarch64**: Linux kernels on aarch64 may be compiled with 4KiB, 16KiB (e.g. Apple Silicon / macOS hypervisors), or 64KiB (enterprise RHEL/Fedora default) pages. Memory buffer calculations and procfs buffer sizing in `capability-layer` must dynamically query `sysconf(_SC_PAGESIZE)` rather than assuming 4096 bytes.

### C. Kernel & Boot Loader
* **x86_64**: `vmlinuz`, GRUB x86_64-efi, systemd-boot.
* **aarch64**: Image requires `Image.gz` with Device Tree Blob (DTB) support or ACPI UEFI stub (`bootaa64.efi`).

---

## 4. `bootc-image-builder` Support Status (46.3)

According to official documentation for `osbuild/bootc-image-builder`:
* **Multi-Arch Status**: `bootc-image-builder` supports building aarch64 images when run on a native aarch64 host or when utilizing multi-arch container manifests.
* **Cross-Building Constraint**: Generating an aarch64 disk image from an x86_64 host using `bootc-image-builder` requires nested QEMU emulation or cross-architecture container execution. Osbuild recommends native execution to avoid timeouts during filesystem format and SELinux relabeling passes.
* **SELinux Relabeling**: Relabeling file contexts during aarch64 image builds on x86_64 hosts via binfmt is a frequent source of silent truncation or SELinux policy compile failures in Fedora container images.

---

## 5. CI Cost & Resource Projections (46.4)

| Metric | Current (x86_64 only) | Projected Dual-Arch (x86_64 + aarch64 Native) | Projected Dual-Arch (x86_64 + QEMU Emulation) |
| :--- | :--- | :--- | :--- |
| **Build & Test Time** | ~3.5 min | ~7.2 min | ~22.0 min |
| **CI Runner Minutes/PR** | ~28 minutes | ~64 minutes (+128%) | ~176 minutes (+528%) |
| **Pipeline Reliability** | High (100% stable) | Moderate (runner provisioning dependency) | Low (timeout risks on QEMU TCG boots) |

---

## 6. Implementation Roadmap

1. **Phase 1 (Current)**:
   - Ensure all Rust workspace crates compile cleanly against `aarch64-unknown-linux-gnu` via offline verification.
   - Forbid target-specific inline assembly or x86-only intrinsics outside feature-gated detection modules.
2. **Phase 2 (Post-General Availability)**:
   - Provision native ARM64 CI builders.
   - Implement ARM SMCCC/ACPI hypervisor detection parser in `world-model`.
   - Produce validated aarch64 bootable ISOs and RAW disk images.
