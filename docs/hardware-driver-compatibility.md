# Directioner-OS — Real Hardware & Driver Compatibility Research

**Status**: Research Document (Phase 4 Scope Exploration — Pure Technical Survey, Zero Capability Code)  
**Target Environment**: Fedora bootc (`fedora-bootc:42` / `x86_64` & `aarch64`)  
**Scope**: Wireless (WiFi/BT), Biometric Fingerprint Readers, and Laptop Sleep/Wake (ACPI S3 / Modern Standby S0ix).

---

## 1. Executive Summary & Problem Statement

Directioner-OS is built upon an immutable, container-derived operating system base (`fedora-bootc`). Because the `/usr` filesystem tree is read-only at runtime and root filesystem mutations are deployed transactionally via OSTree/bootc images, traditional Linux driver management paradigms—such as ad-hoc DKMS kernel module compilation upon package install or manual drops into `/usr/lib/modules`—cannot function as they do on traditional mutable distributions.

This research document surveys:
1. What hardware is natively supported out-of-the-box by upstream Fedora kernel and `linux-firmware`.
2. Which hardware families commonly fail or require out-of-tree / proprietary firmware.
3. How Directioner-OS can passively diagnose hardware states and guide operators without violating the immutable base or capability security model.

---

## 2. Wireless (WiFi / Bluetooth) Chipset Matrix

### 2.1 Upstream Supported (Out-of-the-Box)
The standard `fedora-bootc:42` base image packages the upstream Linux kernel ($\ge 6.11$) alongside `linux-firmware`. The following chipsets operate with zero manual driver installation:

| Chipset Family | Kernel Driver | Firmware Files in `/lib/firmware` | Notes |
| :--- | :--- | :--- | :--- |
| **Intel Wireless** (AX200, AX201, AX210, AX211, BE200) | `iwlwifi` | `iwlwifi-*.ucode` | Native 802.11ax / 802.11be, Bluetooth via `btintel`. Robust power saving support. |
| **MediaTek** (MT7921, MT7922, MT7925) | `mt7921e` / `mt7925e` | `mediatek/BT_RAM_CODE_MT*.bin`, `mediatek/WIFI_RAM_CODE_MT*.bin` | Common in modern AMD laptops (Framework, Lenovo ThinkPad). Native in-tree. |
| **Qualcomm Atheros** (QCA6174, QCNFA765) | `ath10k_pci` / `ath11k_pci` | `ath10k/*`, `ath11k/*` | Fully supported upstream. |
| **Realtek Modern** (RTL8852AE, RTL8852BE, RTL8852CE) | `rtw89_8852ae`, `rtw89_8852be`, `rtw89_8852ce` | `rtw89/*` | Upstreamed in Linux $\ge 6.2$. Operates natively. |

### 2.2 Out-of-Tree / Problematic Wireless Hardware
The following hardware families frequently encounter issues on immutable Fedora systems:

1. **Broadcom BCM43xx Series (BCM43142, BCM4352, BCM4360)**:
   - *Problem*: Requires proprietary `broadcom-wl` driver distributed via RPM Fusion non-free.
   - *Bootc Constraint*: Cannot compile via DKMS on an immutable root at runtime.
   - *Solution Approach*: Must be baked into the base Containerfile at build time (`dnf install broadcom-wl kmod-wl`) using Fedora RPM Fusion repositories before `bootc-image-builder` generates the qcow2/disk image.
2. **Older Realtek Chipsets (RTL8821CE, RTL8812AU, RTL8814AU)**:
   - *Problem*: `rtl8821ce` is not in the mainline Linux kernel.
   - *Bootc Constraint*: Traditional DKMS repos fail on read-only `/usr`.
   - *Solution Approach*: Package as pre-compiled `akmod` in a layered container build stage or replace mini-PCIe/M.2 module with an Intel AX210 card.

---

## 3. Biometric Fingerprint Reader Integration

### 3.1 Architecture Overview
Linux biometric authentication relies on two core user-space layers:
1. `libfprint`: Hardware abstraction library exposing fingerprint enrollment, verification, and image capture over `D-Bus`.
2. `fprintd`: System daemon managing user biometric credentials, integrated with PAM via `pam_fprintd.so`.

```
┌─────────────────────────────────────────────────────────┐
│                    PAM (pam_fprintd.so)                 │
│              GDM / Polkit / sudo Authentication         │
└────────────────────────────┬────────────────────────────┘
                             │ D-Bus (net.reactivated.Fprint)
┌────────────────────────────▼────────────────────────────┐
│                         fprintd                         │
│       /var/lib/fprint/<user> Storage (Mutable /var)     │
└────────────────────────────┬────────────────────────────┘
                             │ libfprint C API
┌────────────────────────────▼────────────────────────────┐
│                        libfprint                        │
│   In-tree Drivers | libfprint-tod (Touch OEM Drivers)   │
└────────────────────────────┬────────────────────────────┘
                             │ libusb (kernel USB subsystem)
┌────────────────────────────▼────────────────────────────┐
│               Hardware USB Fingerprint Sensor           │
│           (Goodix, Elan, Synaptics, AuthenTec)          │
└─────────────────────────────────────────────────────────┘
```

### 3.2 Sensor Compatibility Matrix

| Vendor | USB ID Example | Driver Status | Compatibility Notes |
| :--- | :--- | :--- | :--- |
| **Elan Microelectronics** | `04f3:0c4c`, `04f3:0c00` | **Supported Upstream** | Native in `libfprint`. Works out-of-the-box on Fedora bootc. |
| **Synaptics Prometheus** | `06cb:00bd`, `06cb:00df` | **Supported Upstream** | Firmware loaded via `fwupd`. Full match-on-chip support. |
| **Goodix MOC (Match-on-Chip)** | `27c6:5301`, `27c6:5385` | **Supported Upstream** | Recent Linux libfprint ($\ge 1.94$) supports most Dell/Lenovo models. |
| **Goodix Proprietary SPI/USB** | `27c6:5110`, `27c6:63ac` | **Proprietary TOD** | Requires `libfprint-tod` and proprietary vendor `.so` engine. |

### 3.3 Directioner-OS Biometric Integration Flow
Because `/var` is mutable on Fedora bootc, user biometric profiles stored in `/var/lib/fprint` persist across OSTree upgrades. Hardware enablement requires:
1. Ensuring `fprintd` and `fprintd-pam` packages are included in the base image `Containerfile`.
2. Integrating PAM configuration in `/etc/pam.d/system-auth` and `/etc/pam.d/sudo` with `pam_fprintd.so`.
3. Updating firmware cryptographically via `fwupd` daemon (`fwupdmgr refresh && fwupdmgr update`).

---

## 4. Laptop Sleep & Wake (ACPI S3 vs S0ix / Modern Standby)

### 4.1 ACPI Power State Paradigms
Modern laptops use one of two primary sleep states:
1. **S3 (Suspend-to-RAM)**: Legacy power state. Cuts power to CPU cores, PCI root complexes, and peripherals; RAM remains refreshed in self-refresh mode. Power draw: 0.5W–1.5W.
2. **S0ix (Modern Standby / Low Power S0 Idle - `s2idle`)**: Hardware remains in ACPI S0 state while CPU packages and PCIe devices enter deep platform power states (C10, D3cold). Power draw: 0.8W–2.5W.

### 4.2 Common Sleep/Wake Failure Modes on Linux
1. **NVMe APST (Autonomous Power State Transition) Lockups**:
   - *Symptom*: System wakes to a frozen screen or I/O errors because NVMe controller does not exit D3cold within PCI latency tolerance.
   - *Remediation*: Kernel parameter `nvme_core.default_ps_max_latency_us=0` or `5500`.
2. **Modern Standby Battery Drain (Failing to Reach Deep C-States)**:
   - *Symptom*: High battery drain while suspended (10–20% overnight).
   - *Cause*: Uncooperative USB devices or PCIe bridges failing runtime PM (`power/control` set to `on` instead of `auto`).
   - *Remediation*: Power management tuning via `power-profiles-daemon` or `tlp`, and ACPI sleep mode verification:
     ```sh
     cat /sys/power/mem_sleep  # Should output: [s2idle] deep
     ```
3. **Audio / Bluetooth Wake Flapping**:
   - *Symptom*: System immediately wakes up 2 seconds after sleep.
   - *Cause*: USB wakeup flags enabled on internal Bluetooth HCI (`/proc/acpi/wakeup`).

---

## 5. Proposed Directioner-OS Diagnostic & Guidance Concept

Without modifying the immutable base or requiring runtime driver synthesis, Directioner-OS can implement **purely read-only hardware introspection**:

1. **Passive Sysfs / Udev Introspection**:
   - Query `/sys/class/net/*` to detect missing wireless interfaces.
   - Query `/sys/bus/usb/devices/*` and `/sys/bus/pci/devices/*` to identify sensor Vendor/Product IDs.
   - Query `/sys/power/mem_sleep` and `/sys/kernel/debug/pmc_core/substate_residencies` (if available) to evaluate sleep efficiency.
2. **Deterministic Operator Guidance**:
   - If an unsupported Broadcom or proprietary Goodix sensor is detected, output exact Containerfile layering directives (e.g. `RUN dnf install -y kmod-wl`) for inclusion in the user's container build pipeline.
   - Never attempt runtime ad-hoc compilation in `/usr`.

---

## 6. Conclusion

Fedora bootc 42 provides an exceptionally solid base for 90%+ of modern laptop hardware (Intel/AMD/MediaTek). By understanding the immutable boundaries of OSTree and leveraging `/var` for biometric credential persistence and Containerfile layering for out-of-tree drivers, Directioner-OS can provide a seamless hardware experience while strictly preserving system integrity.
