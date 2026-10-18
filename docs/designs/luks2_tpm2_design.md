# Design Specification: LUKS2 + TPM2 Disk Encryption for Directioner-OS

**Author**: Directioner-OS Security & Architecture  
**Date**: 2026-10-18  
**Status**: Proposal & Design Specification (Gate: DESIGN-FIRST — Awaiting Aditya Sign-off)  
**Target Subsystems**: `bootc`, `ostree`, `systemd-cryptenroll`, `systemd-pcrlock`, `tpm2-tools`

---

## 1. Objective

Provide hardware-rooted at-rest encryption for Directioner-OS instances without requiring operator passphrase entry during normal boots, while guaranteeing robust fail-safe recovery during bootc ostree updates.

---

## 2. Threat Model & Partition Layout

### 2.1 Threat Vectors Addressed
1. **Physical / Offline Disk Theft**: Attackers extracting NVMe / virtual disk cannot read `/var` (containing secrets, Work Log, system state) or modify immutable root.
2. **Cold Boot / Evil Maid Attacks**: Attackers tampering with UEFI firmware or kernel cmdline will cause TPM2 unsealing to fail.
3. **Storage Separation**:
   - `/boot` & EFI System Partition (ESP): Plaintext FAT32 / ext4, authenticated via Secure Boot (PE-COFF signatures).
   - `root` (`/`): Read-only ostree deployment tree (`composefs` / `erofs` or read-only ext4 over LUKS2).
   - `/var`: Stateful encrypted partition (ext4 on LUKS2).

### 2.2 Partition Architecture
```text
+-----------------------------------------------------------+
| ESP (FAT32, /boot/efi) - Secure Boot Authenticated       |
+-----------------------------------------------------------+
| Boot (/boot, ext4) - ostree kernel, initramfs, bls configs|
+-----------------------------------------------------------+
| LUKS2 Container 1: Root Volume (dm-crypt 'root')          |
| -> OSTree Deployment sysroot (Read-Only)                  |
+-----------------------------------------------------------+
| LUKS2 Container 2: Var Volume (dm-crypt 'var')           |
| -> /var state, logs, containers (Read-Write)              |
+-----------------------------------------------------------+
```

---

## 3. TPM2 PCR Binding Architecture

### 3.1 PCR Selection
Directioner-OS binds the TPM2 sealed key to:
- **PCR 7** (Secure Boot State): Measures PK, KEK, db, dbx, and Secure Boot enforcement mode.
- **PCR 11** (Unified Kernel Image / Bootc OSTree Digest): Measures the ostree deployment commit hash passed by the bootloader.

```bash
systemd-cryptenroll --tpm2-device=auto \
    --tpm2-pcrs=7+11 \
    /dev/disk/by-partlabel/directioner-var
```

### 3.2 Key Enrollment & Dual Slots
LUKS2 provides up to 32 key slots. Directioner-OS configures:
- **Slot 0**: Hardware TPM2 sealed secret (`systemd-cryptenroll`).
- **Slot 1**: High-entropy recovery key (random 128-bit passphrase formatted in RFC1751 mnemonic or 8-group base32).
- **Slot 2**: Staging re-seal slot (used during atomic bootc updates).

---

## 4. Re-Sealing Protocol Across Bootc Updates

### The Challenge
A bootc upgrade downloads a new deployment with a new kernel, initramfs, and ostree commit. If PCR 11 is measured before reboot, the new measurement differs, causing TPM2 unsealing to fail and locking the user out.

### The Directioner-OS Safe Re-Seal Protocol:
1. **Pre-Calculation**: During `bootc upgrade` staging, the system pre-computes the PCR 11 measurement of the newly staged deployment.
2. **Predictive Enrollment (`systemd-pcrlock`)**:
   - Directioner-OS writes a secondary policy prediction into LUKS2 Slot 2:
   ```bash
   systemd-pcrlock predict-deployment --staged
   systemd-pcrlock lock
   ```
3. **Grace Window**: The previous deployment's key remains valid in Slot 0 until greenboot marks the new deployment healthy.
4. **Finalization**: Once greenboot confirms healthy boot of the new deployment, Slot 0 is refreshed to the active PCR values and temporary prediction slots are purged.

---

## 5. Recovery Key Lifecycle (No Hardcoded Secrets / B4 Invariant)

1. **Generation**: Generated on-box during initial provisioning via `/dev/urandom` / `getrandom(2)`.
2. **Display**: Printed to operator console during first boot (`directioner-firstboot`).
3. **Escrow**: Optional QR code generation or export to encrypted USB token.
4. **Zero Repo Secrets**: No default recovery key or salt is ever committed to source control (violates B4).

---

## 6. QEMU Testing Plan with `swtpm`

```bash
# 1. Launch software TPM 2.0 emulator
swtpm socket --tpm2 --tpmstate dir=/tmp/mytpm --ctrl type=unixio,path=/tmp/mytpm/swtpm-sock &

# 2. Launch QEMU with TPM device
qemu-system-x86_64 ... \
    -chardev socket,id=chrtpm,path=/tmp/mytpm/swtpm-sock \
    -tpmdev emulator,id=tpm0,chardev=chrtpm \
    -device tpm-tis,tpmdev=tpm0
```

---

## 7. Failure Modes & Mitigations

| Failure Mode | Trigger | Impact | Mitigation |
|:---|:---|:---|:---|
| Firmware / BIOS update | Motherboard firmware flash changes PCR 0/2 | Unsealing fails | Prompt operator for Recovery Passphrase at plymouth screen; re-enroll TPM after login. |
| Incomplete Bootc Upgrade | Kernel panics before greenboot | Rollback triggered | Previous deployment re-boots; original PCR measurements match Slot 0; auto-unlocks seamlessly. |
| TPM Chip Hardware Fault | Physical TPM communication failure | Boot halts at cryptsetup | Fallback to password prompt; logs hardware failure to serial console. |
