//! Integration and Unit Test Suite for Hardware Compatibility Diagnostic (ai-hwcheck).

use std::fs;
use std::process::Command;
use tempfile::tempdir;

use control_plane::hwcheck::{FingerprintDriverType, HwChecker, HwStatus};

#[test]
fn test_hwcheck_intel_elan_s2idle_supported() {
    let tmp = tempdir().unwrap();
    let sysfs = tmp.path().join("sys");
    let etc = tmp.path().join("etc");
    let fw = tmp.path().join("lib/firmware");

    // Create Intel AX200 WiFi mock PCI node (0000:02:00.0)
    let pci_dev = sysfs.join("bus/pci/devices/0000:02:00.0");
    fs::create_dir_all(&pci_dev).unwrap();
    fs::write(pci_dev.join("class"), "0x028000\n").unwrap();
    fs::write(pci_dev.join("vendor"), "0x8086\n").unwrap();
    fs::write(pci_dev.join("device"), "0x2723\n").unwrap();

    // Create Elan Microelectronics USB Fingerprint mock node (1-3:1.0)
    let usb_dev = sysfs.join("bus/usb/devices/1-3:1.0");
    fs::create_dir_all(&usb_dev).unwrap();
    fs::write(usb_dev.join("idVendor"), "04f3\n").unwrap();
    fs::write(usb_dev.join("idProduct"), "0c4c\n").unwrap();
    fs::write(usb_dev.join("manufacturer"), "ELAN Microelectronics\n").unwrap();
    fs::write(usb_dev.join("product"), "ELAN Touch Fingerprint Sensor\n").unwrap();

    // Create PAM config
    let pam_dir = etc.join("pam.d");
    fs::create_dir_all(&pam_dir).unwrap();
    fs::write(
        pam_dir.join("system-auth"),
        "auth sufficient pam_fprintd.so\nauth required pam_unix.so\n",
    )
    .unwrap();

    // Create Sleep/Wake power nodes
    let power_dir = sysfs.join("power");
    fs::create_dir_all(&power_dir).unwrap();
    fs::write(power_dir.join("mem_sleep"), "[s2idle] deep\n").unwrap();
    fs::write(power_dir.join("state"), "freeze mem disk\n").unwrap();

    fs::create_dir_all(&fw).unwrap();

    let checker = HwChecker::with_roots(sysfs, etc, fw);
    let report = checker.inspect();

    assert_eq!(report.overall_status, HwStatus::Supported);
    assert_eq!(report.wifi.status, HwStatus::Supported);
    assert_eq!(report.wifi.devices.len(), 1);
    assert_eq!(report.wifi.devices[0].chipset_name, "Intel Wi-Fi 6 AX200");
    assert_eq!(report.wifi.devices[0].vendor_id, "8086");
    assert_eq!(report.wifi.devices[0].device_id, "2723");

    assert_eq!(report.fingerprint.status, HwStatus::Supported);
    assert_eq!(report.fingerprint.devices.len(), 1);
    assert_eq!(
        report.fingerprint.devices[0].driver_type,
        FingerprintDriverType::InTreeLibfprint
    );
    assert!(report.fingerprint.pam_fprintd_configured);

    assert_eq!(report.sleep_wake.status, HwStatus::Supported);
    assert_eq!(report.sleep_wake.active_mem_sleep, "s2idle");
    assert_eq!(
        report.sleep_wake.supported_mem_sleep,
        vec!["s2idle", "deep"]
    );

    let json = report.render_json();
    assert!(json.contains("\"chipset_name\": \"Intel Wi-Fi 6 AX200\""));
    assert!(json.contains("\"overall_status\": \"supported\""));

    let table = report.render_human_table();
    assert!(table.contains("Intel Wi-Fi 6 AX200"));
    assert!(table.contains("ELAN Touch Fingerprint Sensor"));
}

#[test]
fn test_hwcheck_broadcom_goodix_tod_unsupported_warning() {
    let tmp = tempdir().unwrap();
    let sysfs = tmp.path().join("sys");
    let etc = tmp.path().join("etc");
    let fw = tmp.path().join("lib/firmware");

    // Create Broadcom BCM4360 WiFi mock PCI node (0000:03:00.0)
    let pci_dev = sysfs.join("bus/pci/devices/0000:03:00.0");
    fs::create_dir_all(&pci_dev).unwrap();
    fs::write(pci_dev.join("class"), "0x028000\n").unwrap();
    fs::write(pci_dev.join("vendor"), "0x14e4\n").unwrap();
    fs::write(pci_dev.join("device"), "0x43a0\n").unwrap();

    // Create Goodix proprietary TOD Fingerprint mock node (1-4:1.0)
    let usb_dev = sysfs.join("bus/usb/devices/1-4:1.0");
    fs::create_dir_all(&usb_dev).unwrap();
    fs::write(usb_dev.join("idVendor"), "27c6\n").unwrap();
    fs::write(usb_dev.join("idProduct"), "5110\n").unwrap();
    fs::write(usb_dev.join("manufacturer"), "Goodix Technology\n").unwrap();
    fs::write(usb_dev.join("product"), "Goodix Fingerprint Sensor\n").unwrap();

    // Create NVMe with APST=1 under s2idle
    let nvme_dev = sysfs.join("class/nvme/nvme0");
    fs::create_dir_all(&nvme_dev).unwrap();
    fs::write(nvme_dev.join("apst"), "1\n").unwrap();

    let power_dir = sysfs.join("power");
    fs::create_dir_all(&power_dir).unwrap();
    fs::write(power_dir.join("mem_sleep"), "[s2idle]\n").unwrap();
    fs::write(power_dir.join("state"), "freeze\n").unwrap();

    let checker = HwChecker::with_roots(sysfs, etc, fw);
    let report = checker.inspect();

    assert_eq!(report.overall_status, HwStatus::Unsupported);
    assert_eq!(report.wifi.status, HwStatus::Unsupported);
    assert_eq!(
        report.wifi.devices[0].chipset_name,
        "Broadcom BCM4360 802.11ac"
    );

    assert_eq!(report.fingerprint.status, HwStatus::Degraded);
    assert_eq!(
        report.fingerprint.devices[0].driver_type,
        FingerprintDriverType::ProprietaryTodRequired
    );

    assert_eq!(report.sleep_wake.status, HwStatus::Degraded);
    assert!(report.sleep_wake.nvme_apst_warning);

    // Verify actionable recommendations are populated
    assert!(!report.recommendations.is_empty());
    assert!(
        report
            .recommendations
            .iter()
            .any(|r| r.contains("Broadcom WiFi adapter") && r.contains("broadcom-wl"))
    );
    assert!(
        report
            .recommendations
            .iter()
            .any(|r| r.contains("Goodix sensor") && r.contains("libfprint-tod"))
    );
    assert!(
        report
            .recommendations
            .iter()
            .any(|r| r.contains("NVMe APST deep latency"))
    );
}

#[test]
fn test_hwcheck_cli_binary_execution() {
    let tmp = tempdir().unwrap();
    let sysfs = tmp.path().join("sys");
    let etc = tmp.path().join("etc");
    let fw = tmp.path().join("lib/firmware");

    let pci_dev = sysfs.join("bus/pci/devices/0000:01:00.0");
    fs::create_dir_all(&pci_dev).unwrap();
    fs::write(pci_dev.join("class"), "0x028000\n").unwrap();
    fs::write(pci_dev.join("vendor"), "0x14c3\n").unwrap();
    fs::write(pci_dev.join("device"), "0x7961\n").unwrap();

    let power_dir = sysfs.join("power");
    fs::create_dir_all(&power_dir).unwrap();
    fs::write(power_dir.join("mem_sleep"), "s2idle [deep]\n").unwrap();
    fs::write(power_dir.join("state"), "freeze mem disk\n").unwrap();

    fs::create_dir_all(&fw).unwrap();
    fs::create_dir_all(&etc).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ai-hwcheck"))
        .arg("--sysfs")
        .arg(&sysfs)
        .arg("--etc")
        .arg(&etc)
        .arg("--firmware")
        .arg(&fw)
        .arg("--json")
        .output()
        .expect("Failed to execute ai-hwcheck binary");

    assert!(
        output.status.success(),
        "ai-hwcheck exited with non-zero: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_val: serde_json::Value =
        serde_json::from_str(&stdout).expect("ai-hwcheck --json must emit valid json");

    assert_eq!(
        json_val["wifi"]["devices"][0]["chipset_name"],
        "MediaTek MT7921 802.11ax"
    );
    assert_eq!(json_val["sleep_wake"]["active_mem_sleep"], "deep");
}
