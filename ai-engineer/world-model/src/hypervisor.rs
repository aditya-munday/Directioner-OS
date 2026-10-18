//! Hypervisor and Virtualization Platform Detection (Phase 1.1 / TODO 34).
//!
//! Integrates multi-layer virtualization detection combining:
//! 1. `systemd-detect-virt` / `hostnamectl` virtualization field.
//! 2. DMI table sysfs entries (`/sys/class/dmi/id/sys_vendor`, `/sys/class/dmi/id/product_name`).
//! 3. CPUID hypervisor flag (`/proc/cpuinfo` `flags: ... hypervisor`).
//!
//! Provides deterministic fixture-driven parsers and live host detection.

use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

/// Supported hypervisor and virtualization platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HypervisorPlatform {
    /// Kernel-based Virtual Machine (native KVM hypervisor).
    Kvm,
    /// Quick Emulator (QEMU emulated machine).
    Qemu,
    /// VMware ESXi / Workstation / Fusion.
    Vmware,
    /// Microsoft Hyper-V / Azure VM.
    Hyperv,
    /// Oracle VirtualBox.
    Virtualbox,
    /// Bare-metal system (no hypervisor detected).
    None,
    /// Virtualized platform of unidentified type.
    Unknown,
}

impl std::fmt::Display for HypervisorPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kvm => write!(f, "kvm"),
            Self::Qemu => write!(f, "qemu"),
            Self::Vmware => write!(f, "vmware"),
            Self::Hyperv => write!(f, "hyperv"),
            Self::Virtualbox => write!(f, "virtualbox"),
            Self::None => write!(f, "none"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Structured detection report indicating detected platform and telemetry sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypervisorDetectionReport {
    /// Categorical detected hypervisor platform.
    pub platform: HypervisorPlatform,
    /// Primary signal source that confirmed detection.
    pub primary_source: String,
    /// Raw output or matched identifiers from hardware/subsystem queries.
    pub details: String,
    /// True if running inside a virtualized environment.
    pub is_virtualized: bool,
}

/// Parses output from `systemd-detect-virt` into [`HypervisorPlatform`].
pub fn parse_detect_virt_output(raw: &str) -> HypervisorPlatform {
    match raw.trim().to_ascii_lowercase().as_str() {
        "kvm" => HypervisorPlatform::Kvm,
        "qemu" => HypervisorPlatform::Qemu,
        "vmware" => HypervisorPlatform::Vmware,
        "microsoft" | "hyper-v" | "hyperv" => HypervisorPlatform::Hyperv,
        "oracle" | "virtualbox" => HypervisorPlatform::Virtualbox,
        "none" => HypervisorPlatform::None,
        "" => HypervisorPlatform::Unknown,
        _ => HypervisorPlatform::Unknown,
    }
}

/// Parses DMI vendor and product name strings to infer hypervisor.
pub fn parse_dmi_strings(sys_vendor: &str, product_name: &str) -> Option<HypervisorPlatform> {
    let vendor = sys_vendor.trim().to_ascii_lowercase();
    let product = product_name.trim().to_ascii_lowercase();

    if vendor.contains("vmware") || product.contains("vmware") {
        return Some(HypervisorPlatform::Vmware);
    }
    if vendor.contains("qemu") || product.contains("qemu") || product.contains("standard pc") {
        return Some(HypervisorPlatform::Qemu);
    }
    if vendor.contains("microsoft") || product.contains("virtual machine") {
        return Some(HypervisorPlatform::Hyperv);
    }
    if vendor.contains("innotek") || product.contains("virtualbox") {
        return Some(HypervisorPlatform::Virtualbox);
    }
    if vendor.contains("kvm") || product.contains("kvm") {
        return Some(HypervisorPlatform::Kvm);
    }

    None
}

/// Checks `/proc/cpuinfo` content for the presence of the `hypervisor` CPU flag.
pub fn parse_cpuinfo_hypervisor_flag(cpuinfo: &str) -> bool {
    for line in cpuinfo.lines() {
        if line.starts_with("flags") && line.contains(':') {
            let flags = line.split(':').nth(1).unwrap_or("");
            return flags.split_whitespace().any(|f| f == "hypervisor");
        }
    }
    false
}

/// Parses `hostnamectl` output (such as recorded in forensic bundles) to extract hypervisor.
pub fn parse_hostnamectl_output(out: &str) -> HypervisorPlatform {
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Virtualization:") {
            if let Some(val) = trimmed.split(':').nth(1) {
                return parse_detect_virt_output(val);
            }
        }
    }

    // Fall back to hardware vendor / model parsing in hostnamectl
    let mut vendor = String::new();
    let mut model = String::new();
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Hardware Vendor:") {
            if let Some(v) = trimmed.split(':').nth(1) {
                vendor = v.to_string();
            }
        } else if trimmed.starts_with("Hardware Model:") {
            if let Some(m) = trimmed.split(':').nth(1) {
                model = m.to_string();
            }
        }
    }

    if let Some(p) = parse_dmi_strings(&vendor, &model) {
        return p;
    }

    HypervisorPlatform::Unknown
}

/// Pure detection function combining detect-virt, DMI, and CPU info without side effects.
pub fn detect_hypervisor_pure(
    virt_out: Option<&str>,
    dmi_vendor: Option<&str>,
    dmi_product: Option<&str>,
    cpuinfo: Option<&str>,
) -> HypervisorDetectionReport {
    // 1. Check systemd-detect-virt if provided
    if let Some(virt) = virt_out {
        let platform = parse_detect_virt_output(virt);
        if platform != HypervisorPlatform::Unknown {
            return HypervisorDetectionReport {
                platform,
                primary_source: "systemd-detect-virt".to_string(),
                details: format!("virt={virt}"),
                is_virtualized: platform != HypervisorPlatform::None,
            };
        }
    }

    // 2. Check DMI vendor / product strings
    if let (Some(v), Some(p)) = (dmi_vendor, dmi_product) {
        if let Some(platform) = parse_dmi_strings(v, p) {
            return HypervisorDetectionReport {
                platform,
                primary_source: "dmi_sysfs".to_string(),
                details: format!("vendor='{v}', product='{p}'"),
                is_virtualized: true,
            };
        }
    }

    // 3. Check CPUID hypervisor flag
    let has_hypervisor_flag = cpuinfo.map(parse_cpuinfo_hypervisor_flag).unwrap_or(false);

    if has_hypervisor_flag {
        HypervisorDetectionReport {
            platform: HypervisorPlatform::Unknown,
            primary_source: "cpuinfo_flag".to_string(),
            details: "CPUID hypervisor flag present but vendor unidentified".to_string(),
            is_virtualized: true,
        }
    } else {
        HypervisorDetectionReport {
            platform: HypervisorPlatform::None,
            primary_source: "bare_metal_heuristics".to_string(),
            details: "No virtualization signatures found in DMI, CPUID, or kernel".to_string(),
            is_virtualized: false,
        }
    }
}

/// Live detection querying the running host environment.
pub fn detect_hypervisor() -> HypervisorDetectionReport {
    // Try systemd-detect-virt
    let virt_output = Command::new("systemd-detect-virt")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    // Read DMI sysfs
    let dmi_vendor = fs::read_to_string("/sys/class/dmi/id/sys_vendor")
        .ok()
        .map(|s| s.trim().to_string());
    let dmi_product = fs::read_to_string("/sys/class/dmi/id/product_name")
        .ok()
        .map(|s| s.trim().to_string());

    // Read /proc/cpuinfo
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").ok();

    detect_hypervisor_pure(
        virt_output.as_deref(),
        dmi_vendor.as_deref(),
        dmi_product.as_deref(),
        cpuinfo.as_deref(),
    )
}
