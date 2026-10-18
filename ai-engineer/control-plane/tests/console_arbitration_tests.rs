use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn test_serial_probe_rejects_non_existent_device() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/directioner-serial-probe.sh");

    assert!(script.exists(), "Probe script must exist at {}", script.display());

    let status = Command::new(&script)
        .arg("/dev/non_existent_tty_device_999")
        .status()
        .expect("Failed to execute probe script");

    assert!(!status.success(), "Non-existent device must exit non-zero");
}

#[test]
fn test_serial_getty_dropin_configuration() {
    let dropin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("image/configs/systemd/serial-getty@ttyS0.service.d/directioner-console.conf");

    assert!(dropin_path.exists(), "Dropin file must exist");
    let content = fs::read_to_string(dropin_path).unwrap();
    assert!(content.contains("[Unit]"));
    assert!(content.contains("ExecCondition="));
    assert!(content.contains("directioner-serial-probe"));
}
