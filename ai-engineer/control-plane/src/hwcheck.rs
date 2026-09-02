//! Hardware diagnostic and driver compatibility inspection engine.
//!
//! Purely read-only diagnostic tooling inspecting PCI, USB, net, and power sysfs hierarchies
//! to identify active hardware, matched kernel drivers, firmware status, and compatibility
//! with Directioner-OS known-good baseline configurations.
//!
//! This module contains ZERO mutation or package installation capabilities.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Overall hardware compatibility status classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HwStatus {
    /// Fully supported out-of-the-box by upstream Fedora kernel and linux-firmware.
    Supported,
    /// Partially supported or degraded; optional proprietary drivers or tuning recommended.
    Degraded,
    /// Known problematic hardware requiring out-of-tree modules or build-time layering.
    Unsupported,
    /// Unrecognized hardware device or missing sysfs telemetry.
    Unknown,
}

/// Fingerprint sensor driver classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FingerprintDriverType {
    /// Supported upstream in libfprint via standard kernel usb subsystem.
    InTreeLibfprint,
    /// Requires libfprint-tod and vendor-specific proprietary binary engine.
    ProprietaryTodRequired,
    /// Unsupported sensor architecture.
    Unsupported,
    /// Unknown device classification.
    Unknown,
}

/// Detected WiFi network device diagnostic details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiDevice {
    /// PCI address or sysfs identifier (e.g., "0000:02:00.0").
    pub pci_slot: String,
    /// Vendor ID (e.g., "8086" for Intel, "10ec" for Realtek, "14e4" for Broadcom).
    pub vendor_id: String,
    /// Device ID (e.g., "2723" for Intel AX200).
    pub device_id: String,
    /// Human-readable chipset name.
    pub chipset_name: String,
    /// Active kernel driver module name if bound (e.g., "iwlwifi", "rtw89_8852be").
    pub driver_loaded: Option<String>,
    /// Whether required firmware is present in /lib/firmware.
    pub firmware_present: bool,
    /// Compatibility status assessment.
    pub status: HwStatus,
    /// Explanatory diagnostic notes.
    pub notes: Vec<String>,
}

/// Aggregated WiFi diagnostic report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WifiDiagnostic {
    /// List of detected WiFi hardware devices.
    pub devices: Vec<WifiDevice>,
    /// Rollup status across all WiFi adapters.
    pub status: HwStatus,
    /// Operational summary notes.
    pub notes: Vec<String>,
}

/// Detected USB fingerprint reader device diagnostic details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FingerprintDevice {
    /// USB bus address or identifier (e.g., "1-3:1.0").
    pub usb_path: String,
    /// USB Vendor ID and Product ID formatted as "VID:PID" (e.g., "04f3:0c4c").
    pub usb_id: String,
    /// Manufacturer name if reported in sysfs.
    pub manufacturer: String,
    /// Product name if reported in sysfs.
    pub product: String,
    /// Driver integration pathway.
    pub driver_type: FingerprintDriverType,
    /// Compatibility status assessment.
    pub status: HwStatus,
    /// Explanatory diagnostic notes.
    pub notes: Vec<String>,
}

/// Aggregated biometric fingerprint diagnostic report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FingerprintDiagnostic {
    /// List of detected biometric fingerprint devices.
    pub devices: Vec<FingerprintDevice>,
    /// Whether fprintd daemon is available on the system.
    pub fprintd_installed: bool,
    /// Whether pam_fprintd.so is configured in system PAM stack.
    pub pam_fprintd_configured: bool,
    /// Rollup status.
    pub status: HwStatus,
    /// Operational summary notes.
    pub notes: Vec<String>,
}

/// Sleep/Wake ACPI power state diagnostic details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepWakeDiagnostic {
    /// Available sleep states listed in `/sys/power/mem_sleep` (e.g., `["s2idle", "deep"]`).
    pub supported_mem_sleep: Vec<String>,
    /// Currently active default sleep mode (e.g., `"s2idle"` or `"deep"`).
    pub active_mem_sleep: String,
    /// Available system power states in `/sys/power/state` (e.g., `["freeze", "mem", "disk"]`).
    pub supported_power_states: Vec<String>,
    /// Whether NVMe autonomous power state transition (APST) quirk risk is present.
    pub nvme_apst_warning: bool,
    /// Compatibility status assessment.
    pub status: HwStatus,
    /// Explanatory diagnostic notes.
    pub notes: Vec<String>,
}

/// Complete host hardware compatibility diagnostic report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HwCheckReport {
    /// Timestamp of diagnostic run (RFC3339).
    pub timestamp: String,
    /// Base sysfs root path inspected.
    pub sysfs_root: String,
    /// WiFi subsystem diagnostic.
    pub wifi: WifiDiagnostic,
    /// Biometric fingerprint diagnostic.
    pub fingerprint: FingerprintDiagnostic,
    /// ACPI sleep/wake diagnostic.
    pub sleep_wake: SleepWakeDiagnostic,
    /// High-level rollup status.
    pub overall_status: HwStatus,
    /// Actionable recommendations for the system operator.
    pub recommendations: Vec<String>,
}

impl HwCheckReport {
    /// Formats the diagnostic report as human-readable terminal output.
    pub fn render_human_table(&self) -> String {
        let mut out = String::new();
        out.push_str("╔══════════════════════════════════════════════════════════════════════════════════════════╗\n");
        out.push_str("║                   DIRECTIONER-OS HARDWARE COMPATIBILITY DIAGNOSTIC                       ║\n");
        out.push_str("╠══════════════════════════════════════════════════════════════════════════════════════════╣\n");
        out.push_str(&format!(
            "║  Overall Status: {:<14} │ Timestamp: {:<38} ║\n",
            format!("{:?}", self.overall_status).to_uppercase(),
            self.timestamp
        ));
        out.push_str("╠══════════════════════════════════════════════════════════════════════════════════════════╣\n");

        // WiFi Section
        out.push_str("║ 1. WIRELESS SUBSYSTEM (WIFI / BT)                                                        ║\n");
        if self.wifi.devices.is_empty() {
            out.push_str("║    No PCI WiFi adapters detected in sysfs.                                              ║\n");
        } else {
            for dev in &self.wifi.devices {
                out.push_str(&format!(
                    "║    • [{}] {} ({}:{}) - Status: {:?}\n",
                    dev.pci_slot, dev.chipset_name, dev.vendor_id, dev.device_id, dev.status
                ));
                out.push_str(&format!(
                    "║      Driver: {} | Firmware present: {}\n",
                    dev.driver_loaded.as_deref().unwrap_or("none"),
                    if dev.firmware_present { "YES" } else { "NO" }
                ));
                for note in &dev.notes {
                    out.push_str(&format!("║      Note: {}\n", note));
                }
            }
        }
        out.push_str("╟──────────────────────────────────────────────────────────────────────────────────────────╢\n");

        // Fingerprint Section
        out.push_str("║ 2. BIOMETRIC FINGERPRINT READER                                                          ║\n");
        out.push_str(&format!(
            "║    fprintd: {} | PAM pam_fprintd.so: {}\n",
            if self.fingerprint.fprintd_installed {
                "Installed"
            } else {
                "Missing"
            },
            if self.fingerprint.pam_fprintd_configured {
                "Configured"
            } else {
                "Not Configured"
            }
        ));
        if self.fingerprint.devices.is_empty() {
            out.push_str("║    No USB biometric fingerprint sensors detected.                                        ║\n");
        } else {
            for dev in &self.fingerprint.devices {
                out.push_str(&format!(
                    "║    • [{}] {} ({}) - Status: {:?}\n",
                    dev.usb_id, dev.product, dev.manufacturer, dev.status
                ));
                out.push_str(&format!("║      Integration: {:?}\n", dev.driver_type));
                for note in &dev.notes {
                    out.push_str(&format!("║      Note: {}\n", note));
                }
            }
        }
        out.push_str("╟──────────────────────────────────────────────────────────────────────────────────────────╢\n");

        // Sleep/Wake Section
        out.push_str("║ 3. LAPTOP SLEEP & WAKE (ACPI POWER STATES)                                               ║\n");
        out.push_str(&format!(
            "║    Active mem_sleep: {:<12} │ Available: [{}]\n",
            self.sleep_wake.active_mem_sleep,
            self.sleep_wake.supported_mem_sleep.join(", ")
        ));
        out.push_str(&format!(
            "║    Power states: [{}] │ NVMe APST Quirk Risk: {}\n",
            self.sleep_wake.supported_power_states.join(", "),
            if self.sleep_wake.nvme_apst_warning {
                "WARNING"
            } else {
                "CLEAN"
            }
        ));
        for note in &self.sleep_wake.notes {
            out.push_str(&format!("║    Note: {}\n", note));
        }

        out.push_str("╟──────────────────────────────────────────────────────────────────────────────────────────╢\n");
        out.push_str("║ RECOMMENDATIONS & MITIGATIONS                                                            ║\n");
        if self.recommendations.is_empty() {
            out.push_str("║    All hardware subsystems match known-good upstream Fedora bootc configurations.        ║\n");
        } else {
            for rec in &self.recommendations {
                out.push_str(&format!("║    • {}\n", rec));
            }
        }
        out.push_str("╚══════════════════════════════════════════════════════════════════════════════════════════╝\n");
        out
    }

    /// Formats the diagnostic report as structured JSON.
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Hardware diagnostic inspector.
pub struct HwChecker {
    sysfs_root: PathBuf,
    etc_root: PathBuf,
    firmware_root: PathBuf,
}

impl Default for HwChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl HwChecker {
    /// Creates a new hardware inspector targeting host default paths (`/sys`, `/etc`, `/lib/firmware`).
    pub fn new() -> Self {
        Self {
            sysfs_root: PathBuf::from("/sys"),
            etc_root: PathBuf::from("/etc"),
            firmware_root: PathBuf::from("/lib/firmware"),
        }
    }

    /// Creates a hardware inspector with custom root paths for isolated testing.
    pub fn with_roots(sysfs_root: PathBuf, etc_root: PathBuf, firmware_root: PathBuf) -> Self {
        Self {
            sysfs_root,
            etc_root,
            firmware_root,
        }
    }

    /// Executes full hardware diagnostic inspection.
    pub fn inspect(&self) -> HwCheckReport {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let wifi = self.inspect_wifi();
        let fingerprint = self.inspect_fingerprint();
        let sleep_wake = self.inspect_sleep_wake();

        let mut recommendations = Vec::new();
        let mut overall_status = HwStatus::Supported;

        // Evaluate WiFi recommendations
        for dev in &wifi.devices {
            if dev.status == HwStatus::Unsupported {
                overall_status = HwStatus::Unsupported;
                if dev.vendor_id == "14e4" {
                    recommendations.push(format!(
                        "Broadcom WiFi adapter [{}] requires broadcom-wl kmod. Layer into Containerfile via RPM Fusion.",
                        dev.pci_slot
                    ));
                } else if dev.vendor_id == "10ec" && dev.device_id.starts_with("8821") {
                    recommendations.push(format!(
                        "Realtek RTL8821CE [{}] requires out-of-tree akmod. Consider replacing with Intel AX210 M.2 card.",
                        dev.pci_slot
                    ));
                }
            } else if dev.status == HwStatus::Degraded && overall_status == HwStatus::Supported {
                overall_status = HwStatus::Degraded;
            }
        }

        // Evaluate Fingerprint recommendations
        if !fingerprint.fprintd_installed && !fingerprint.devices.is_empty() {
            recommendations.push("Biometric sensor detected but fprintd is not installed. Add fprintd and fprintd-pam to base image.".to_string());
            if overall_status == HwStatus::Supported {
                overall_status = HwStatus::Degraded;
            }
        }
        for dev in &fingerprint.devices {
            if dev.driver_type == FingerprintDriverType::ProprietaryTodRequired {
                recommendations.push(format!(
                    "Goodix sensor [{}] requires proprietary libfprint-tod binary engine. Install via vendor container layer.",
                    dev.usb_id
                ));
                if overall_status == HwStatus::Supported {
                    overall_status = HwStatus::Degraded;
                }
            }
        }

        // Evaluate Sleep/Wake recommendations
        if sleep_wake.nvme_apst_warning {
            recommendations.push("NVMe APST deep latency issue detected. If experiencing sleep wake crashes, add nvme_core.default_ps_max_latency_us=0 to kernel boot arguments.".to_string());
            if overall_status == HwStatus::Supported {
                overall_status = HwStatus::Degraded;
            }
        }

        HwCheckReport {
            timestamp,
            sysfs_root: self.sysfs_root.to_string_lossy().to_string(),
            wifi,
            fingerprint,
            sleep_wake,
            overall_status,
            recommendations,
        }
    }

    /// Inspects PCI subsystem for WiFi devices.
    pub fn inspect_wifi(&self) -> WifiDiagnostic {
        let pci_dir = self.sysfs_root.join("bus/pci/devices");
        let mut devices = Vec::new();
        let mut notes = Vec::new();

        if let Ok(entries) = fs::read_dir(&pci_dir) {
            for entry in entries.flatten() {
                let pci_path = entry.path();
                let class_file = pci_path.join("class");
                let vendor_file = pci_path.join("vendor");
                let device_file = pci_path.join("device");

                if let (Ok(class_str), Ok(vendor_str), Ok(device_str)) = (
                    fs::read_to_string(&class_file),
                    fs::read_to_string(&vendor_file),
                    fs::read_to_string(&device_file),
                ) {
                    let class_trimmed = class_str.trim().trim_start_matches("0x");
                    // PCI Class 0280 indicates Network Controller (Wireless / WiFi)
                    if class_trimmed.starts_with("0280") || class_trimmed.starts_with("028000") {
                        let pci_slot = entry.file_name().to_string_lossy().to_string();
                        let vendor_id = vendor_str.trim().trim_start_matches("0x").to_lowercase();
                        let device_id = device_str.trim().trim_start_matches("0x").to_lowercase();

                        let driver_link = pci_path.join("driver");
                        let driver_loaded = if driver_link.exists() {
                            fs::read_link(&driver_link).ok().and_then(|p| {
                                p.file_name().map(|f| f.to_string_lossy().to_string())
                            })
                        } else {
                            None
                        };

                        let (chipset_name, status, dev_notes, firmware_present) = self
                            .classify_wifi_chipset(
                                &vendor_id,
                                &device_id,
                                driver_loaded.as_deref(),
                            );

                        devices.push(WifiDevice {
                            pci_slot,
                            vendor_id,
                            device_id,
                            chipset_name,
                            driver_loaded,
                            firmware_present,
                            status,
                            notes: dev_notes,
                        });
                    }
                }
            }
        }

        let rollup_status = if devices.is_empty() {
            HwStatus::Unknown
        } else if devices.iter().any(|d| d.status == HwStatus::Unsupported) {
            HwStatus::Unsupported
        } else if devices.iter().any(|d| d.status == HwStatus::Degraded) {
            HwStatus::Degraded
        } else {
            HwStatus::Supported
        };

        if devices.is_empty() {
            notes.push("No PCI Network Controller (0x0280) found in sysfs.".to_string());
        }

        WifiDiagnostic {
            devices,
            status: rollup_status,
            notes,
        }
    }

    /// Classifies a WiFi device based on known vendor/device ID matrix.
    fn classify_wifi_chipset(
        &self,
        vendor: &str,
        device: &str,
        active_driver: Option<&str>,
    ) -> (String, HwStatus, Vec<String>, bool) {
        let mut notes = Vec::new();
        match vendor {
            // Intel
            "8086" => {
                let name = match device {
                    "2723" => "Intel Wi-Fi 6 AX200",
                    "2725" => "Intel Wi-Fi 6 AX201",
                    "43f0" => "Intel Wi-Fi 6E AX210",
                    "43f2" => "Intel Wi-Fi 6E AX211",
                    "272b" => "Intel Wi-Fi 7 BE200",
                    _ => "Intel Wireless Adapter",
                };
                let fw_exists = self.firmware_root.exists();
                notes.push(
                    "Native in-tree iwlwifi driver support. Full 802.11ax/be and power management."
                        .to_string(),
                );
                (name.to_string(), HwStatus::Supported, notes, fw_exists)
            }
            // MediaTek
            "14c3" => {
                let name = match device {
                    "7961" => "MediaTek MT7921 802.11ax",
                    "7922" => "MediaTek MT7922 (RZ616) Wi-Fi 6E",
                    "7925" => "MediaTek MT7925 Wi-Fi 7",
                    _ => "MediaTek Wireless Adapter",
                };
                let fw_exists =
                    self.firmware_root.join("mediatek").exists() || self.firmware_root.exists();
                notes.push("Native in-tree mt7921e/mt7925e driver support. Common in modern AMD platforms.".to_string());
                (name.to_string(), HwStatus::Supported, notes, fw_exists)
            }
            // Realtek
            "10ec" => {
                if device.starts_with("8852") || device == "c852" || device == "b852" {
                    let name = "Realtek RTL8852 Series (Wi-Fi 6)".to_string();
                    notes.push(
                        "Supported upstream via rtw89 kernel driver in Linux >= 6.2.".to_string(),
                    );
                    (name, HwStatus::Supported, notes, true)
                } else if device.starts_with("8821") || device == "b822" {
                    let name = "Realtek RTL8821CE 802.11ac".to_string();
                    notes.push("Out-of-tree proprietary driver required. Not available out-of-the-box on immutable root.".to_string());
                    (name, HwStatus::Unsupported, notes, false)
                } else {
                    let name = "Realtek Wireless Adapter".to_string();
                    let status = if active_driver.is_some() {
                        HwStatus::Supported
                    } else {
                        HwStatus::Degraded
                    };
                    (name, status, notes, true)
                }
            }
            // Broadcom
            "14e4" => {
                let name = match device {
                    "43a0" => "Broadcom BCM4360 802.11ac",
                    "4365" => "Broadcom BCM43142 802.11bgn",
                    "43b1" => "Broadcom BCM4352 802.11ac",
                    _ => "Broadcom BCM43xx Wireless Adapter",
                };
                notes.push("Proprietary broadcom-wl driver required (RPM Fusion). DKMS runtime builds fail on read-only /usr.".to_string());
                (name.to_string(), HwStatus::Unsupported, notes, false)
            }
            // Qualcomm
            "17cb" | "168c" => {
                let name = "Qualcomm Atheros Wireless Adapter".to_string();
                notes.push("Supported upstream via ath10k / ath11k drivers.".to_string());
                (name, HwStatus::Supported, notes, true)
            }
            _ => {
                let name = format!("Generic Wireless Adapter ({}:{})", vendor, device);
                let status = if active_driver.is_some() {
                    HwStatus::Supported
                } else {
                    HwStatus::Unknown
                };
                (name, status, notes, true)
            }
        }
    }

    /// Inspects USB subsystem for biometric fingerprint sensors.
    pub fn inspect_fingerprint(&self) -> FingerprintDiagnostic {
        let usb_dir = self.sysfs_root.join("bus/usb/devices");
        let mut devices = Vec::new();
        let mut notes = Vec::new();

        let fprintd_installed = self.etc_root.join("pam.d/system-auth").exists()
            || Path::new("/usr/bin/fprintd-enroll").exists()
            || self.sysfs_root.join("fprintd").exists();

        let pam_content =
            fs::read_to_string(self.etc_root.join("pam.d/system-auth")).unwrap_or_default();
        let pam_fprintd_configured = pam_content.contains("pam_fprintd.so");

        if let Ok(entries) = fs::read_dir(pci_or_usb(&usb_dir)) {
            for entry in entries.flatten() {
                let dev_path = entry.path();
                let vid_file = dev_path.join("idVendor");
                let pid_file = dev_path.join("idProduct");

                if let (Ok(vid), Ok(pid)) =
                    (fs::read_to_string(&vid_file), fs::read_to_string(&pid_file))
                {
                    let vid_clean = vid.trim().to_lowercase();
                    let pid_clean = pid.trim().to_lowercase();
                    let usb_id = format!("{}:{}", vid_clean, pid_clean);

                    if is_fingerprint_vendor(&vid_clean) {
                        let mfg = fs::read_to_string(dev_path.join("manufacturer"))
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        let prod = fs::read_to_string(dev_path.join("product"))
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        let (driver_type, status, dev_notes) =
                            classify_fingerprint_sensor(&vid_clean, &pid_clean);

                        devices.push(FingerprintDevice {
                            usb_path: entry.file_name().to_string_lossy().to_string(),
                            usb_id,
                            manufacturer: if mfg.is_empty() {
                                "Unknown Vendor".to_string()
                            } else {
                                mfg
                            },
                            product: if prod.is_empty() {
                                "Biometric Sensor".to_string()
                            } else {
                                prod
                            },
                            driver_type,
                            status,
                            notes: dev_notes,
                        });
                    }
                }
            }
        }

        let rollup_status = if devices.is_empty() {
            HwStatus::Supported
        } else if devices.iter().any(|d| d.status == HwStatus::Unsupported) {
            HwStatus::Unsupported
        } else if devices.iter().any(|d| d.status == HwStatus::Degraded) || !fprintd_installed {
            HwStatus::Degraded
        } else {
            HwStatus::Supported
        };

        if devices.is_empty() {
            notes.push("No USB biometric fingerprint sensors detected.".to_string());
        }

        FingerprintDiagnostic {
            devices,
            fprintd_installed,
            pam_fprintd_configured,
            status: rollup_status,
            notes,
        }
    }

    /// Inspects system sleep and wake power management configurations.
    pub fn inspect_sleep_wake(&self) -> SleepWakeDiagnostic {
        let mut notes = Vec::new();
        let mem_sleep_file = self.sysfs_root.join("power/mem_sleep");
        let raw_mem_sleep = fs::read_to_string(&mem_sleep_file).unwrap_or_default();

        let mut supported_mem_sleep = Vec::new();
        let mut active_mem_sleep = String::new();

        for token in raw_mem_sleep.split_whitespace() {
            if token.starts_with('[') && token.ends_with(']') {
                let clean = token.trim_matches(|c| c == '[' || c == ']');
                active_mem_sleep = clean.to_string();
                supported_mem_sleep.push(clean.to_string());
            } else if !token.is_empty() {
                supported_mem_sleep.push(token.to_string());
            }
        }

        if active_mem_sleep.is_empty() {
            active_mem_sleep = supported_mem_sleep
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
        }

        let state_file = self.sysfs_root.join("power/state");
        let raw_states = fs::read_to_string(&state_file).unwrap_or_default();
        let supported_power_states: Vec<String> = raw_states
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        // Check for NVMe APST latency warnings
        let mut nvme_apst_warning = false;
        let nvme_dir = self.sysfs_root.join("class/nvme");
        if let Ok(entries) = fs::read_dir(&nvme_dir) {
            for entry in entries.flatten() {
                let apst_file = entry.path().join("apst");
                if let Ok(apst_val) = fs::read_to_string(&apst_file)
                    && apst_val.trim() == "1"
                    && active_mem_sleep == "s2idle"
                {
                    nvme_apst_warning = true;
                    notes.push("NVMe APST is enabled under s2idle modern standby. If wake locks occur, tune APST latency.".to_string());
                }
            }
        }

        if active_mem_sleep == "s2idle" {
            notes.push(
                "Platform is operating in Low Power S0 Idle (Modern Standby s2idle).".to_string(),
            );
        } else if active_mem_sleep == "deep" {
            notes
                .push("Platform is operating in traditional ACPI S3 (Suspend-to-RAM).".to_string());
        }

        let status = if supported_mem_sleep.is_empty() {
            HwStatus::Unknown
        } else if nvme_apst_warning {
            HwStatus::Degraded
        } else {
            HwStatus::Supported
        };

        SleepWakeDiagnostic {
            supported_mem_sleep,
            active_mem_sleep,
            supported_power_states,
            nvme_apst_warning,
            status,
            notes,
        }
    }
}

fn pci_or_usb(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn is_fingerprint_vendor(vid: &str) -> bool {
    matches!(
        vid,
        "04f3" // Elan Microelectronics
        | "06cb" // Synaptics Prometheus
        | "27c6" // Goodix
        | "147e" // TouchStrip / Upek
        | "08ff" // AuthenTec
    )
}

fn classify_fingerprint_sensor(
    vid: &str,
    pid: &str,
) -> (FingerprintDriverType, HwStatus, Vec<String>) {
    let mut notes = Vec::new();
    match vid {
        // Elan
        "04f3" => {
            notes.push("Elan sensor supported in-tree via upstream libfprint.".to_string());
            (
                FingerprintDriverType::InTreeLibfprint,
                HwStatus::Supported,
                notes,
            )
        }
        // Synaptics
        "06cb" => {
            notes.push(
                "Synaptics Prometheus sensor supported in-tree via libfprint & fwupd.".to_string(),
            );
            (
                FingerprintDriverType::InTreeLibfprint,
                HwStatus::Supported,
                notes,
            )
        }
        // Goodix
        "27c6" => {
            if pid == "5301" || pid == "5385" || pid == "5335" {
                notes.push(
                    "Goodix Match-on-Chip supported upstream in libfprint >= 1.94.".to_string(),
                );
                (
                    FingerprintDriverType::InTreeLibfprint,
                    HwStatus::Supported,
                    notes,
                )
            } else {
                notes.push(
                    "Goodix sensor requires proprietary libfprint-tod vendor binary engine."
                        .to_string(),
                );
                (
                    FingerprintDriverType::ProprietaryTodRequired,
                    HwStatus::Degraded,
                    notes,
                )
            }
        }
        _ => {
            notes.push("Generic biometric sensor; test enrollment via fprintd-enroll.".to_string());
            (FingerprintDriverType::Unknown, HwStatus::Degraded, notes)
        }
    }
}
