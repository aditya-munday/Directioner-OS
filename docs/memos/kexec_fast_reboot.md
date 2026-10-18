# Research Memo: Evaluation of Kexec Fast Reboot for Bootc Deployments

**Author**: Directioner-OS Platform Engineering  
**Date**: 2026-10-18  
**Status**: Proposal & Research Evaluation (Gate: RESEARCH-FIRST / TODO 39.4)  
**Target Mechanism**: `kexec -l` / `systemctl kexec` across staged OSTree deployments

---

## 1. Executive Summary

When updating an immutable OS via `bootc upgrade`, a reboot is required to pivot into the newly staged OSTree deployment. A full ACPI reboot incurs substantial hardware initialization overhead (motherboard POST, memory training, PCIe link training, UEFI firmware load), which can take 15–90 seconds on server-grade bare metal.

`kexec` (Kernel Execution) bypasses firmware and bootloader initialization by loading the new kernel and initramfs directly into memory from the running kernel and executing a direct processor reset into the new kernel entry point.

This memo evaluates whether Directioner-OS should enable kexec-based fast reboot by default.

---

## 2. Technical Benefits of Kexec

1. **Reboot Time Reduction**:
   - Reduces reboot downtime from ~60s down to ~4–8 seconds on server hardware.
   - Ideal for autonomous zero-downtime maintenance windows.
2. **Deterministic Handoff**:
   - Kernel command-line parameters and initramfs images can be passed directly from the staged deployment path in `/boot/ostree/...`.

---

## 3. Critical Limitations & Failure Modes

### 3.1 TPM2 PCR Measurement Breakage
- **The Issue**: On cold boot or ACPI warm reboot, the hardware processor and CRTM (Core Root of Trust for Measurement) reset TPM2 PCR banks (PCR 0 through PCR 7).
- **The Kexec Gap**: Standard `kexec` does NOT re-initialize hardware PCR 0–7 measurements. While newer kernels support `kexec_file_load` measuring the new kernel into PCR 10, PCR 7 (Secure Boot state) and firmware PCRs retain stale values or enter an unsealable state unless specifically configured with TPM2 kexec support.
- **Impact on LUKS2 (TODO 36)**: If LUKS2 relies on PCR 7/11, kexec into an unsealed state can trigger a cryptsetup recovery passphrase prompt during headless boot.

### 3.2 Hardware State Residuals & PCIe Device Reset
- Some hardware devices (NVMe controllers, high-speed NICs, GPU accelerators) require full PCIe bus resets to clear dirty DMA queues.
- `kexec` relies on driver `.shutdown()` hooks. Incomplete or buggy driver shutdown routines can cause kernel panic (`DMAR: Faulting address...`) on the newly booted kernel.

### 3.3 Kernel Lockdown Conflict
- Under `lockdown=integrity` or `lockdown=confidentiality` (TODO 32), legacy `kexec_load(2)` is blocked entirely.
- Only `kexec_file_load(2)` with a kernel image signed by a trusted platform key in the system keyring is permitted.

---

## 4. Boot Counter & Automated Rollback Behavior (TODO 39.5)

Directioner-OS relies on `greenboot` and `grub2` boot counting:
1. `grub2` maintains a boot counter (`boot_counter`) in `/boot/grub2/grubenv`.
2. On every boot into a staged deployment, the counter decrements from `3`.
3. If `greenboot-healthcheck.service` succeeds, the counter is reset to `0` and marked healthy.
4. If the boot fails 3 consecutive times (kernel panic, systemd hangs, milestone check failure), grub2 automatically triggers an ostree rollback to the previous working deployment.
5. **Kexec Interaction**: Bypassing grub2 via kexec skips the grub boot counter decrement unless explicitly decremented by userspace prior to invoking `kexec`.

---

## 5. Architectural Recommendation

1. **Default Mode**: Use standard ACPI reboot (`systemctl reboot`) for standard unattended updates to ensure full hardware clean-state, TPM2 measurement re-seeding, and grub2 counter safety.
2. **Fast-Path Opt-In**: Provide `directioner-upgrade --fast-kexec` as an opt-in CLI flag for development VMs and non-TPM-sealed virtual environments.
