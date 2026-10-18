use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;

#[test]
fn test_bootc_verify_staging_detects_staged_deployment() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/bootc_verify_staging.sh");

    assert!(script.exists(), "Verification script must exist");

    let mock_json = r#"{
        "apiVersion": "org.containers.bootc/v1",
        "kind": "BootcHost",
        "status": {
            "staged": {
                "image": {
                    "image": "quay.io/fedora/fedora-bootc:42",
                    "digest": "sha256:1111222233334444"
                }
            },
            "booted": {
                "image": {
                    "image": "quay.io/fedora/fedora-bootc:42",
                    "digest": "sha256:5555666677778888"
                }
            }
        }
    }"#;

    let tmp = NamedTempFile::new().unwrap();
    fs::write(tmp.path(), mock_json).unwrap();

    let output = Command::new(&script)
        .arg("--mock-status-file")
        .arg(tmp.path())
        .output()
        .expect("Failed to execute verification script");

    assert!(
        output.status.success(),
        "Staged deployment must pass verification"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Verified: Staged deployment exists in status"));
    assert!(stdout.contains("3 failed attempts before automatic grub2/greenboot rollback"));
}

#[test]
fn test_bootc_verify_staging_fails_when_no_staged_deployment() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/bootc_verify_staging.sh");

    let mock_json = r#"{
        "apiVersion": "org.containers.bootc/v1",
        "kind": "BootcHost",
        "status": {
            "booted": {
                "image": {
                    "image": "quay.io/fedora/fedora-bootc:42",
                    "digest": "sha256:5555666677778888"
                }
            }
        }
    }"#;

    let tmp = NamedTempFile::new().unwrap();
    fs::write(tmp.path(), mock_json).unwrap();

    let output = Command::new(&script)
        .arg("--mock-status-file")
        .arg(tmp.path())
        .output()
        .expect("Failed to execute verification script");

    assert!(
        !output.status.success(),
        "Missing staged deployment must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No staged deployment found"));
}
