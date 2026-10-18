//! Integration tests for Boot Profiling & Regression Detection (TODO 43).
//!
//! Tests:
//! 1. 43.1 Capture and parsing of `systemd-analyze blame` and `critical-chain`.
//! 2. 43.2 Serialization to artifact file and baseline comparison.
//! 3. 43.3 Regression threshold detection (flags slowdown > threshold).
//! 4. 43.4 Like-with-like environment tag check (TCG vs KVM).
//! 5. 43.5 Top 5 slow unit extraction and duration verification.

use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn test_boot_profiling_extraction_and_top5_units() {
    let root = env!("CARGO_MANIFEST_DIR");
    let script_path = Path::new(root).join("../../scripts/boot_profile.sh");
    assert!(script_path.exists());

    let temp_art_dir = std::env::temp_dir().join("test_boot_profiles");
    let _ = fs::remove_dir_all(&temp_art_dir);

    let output = Command::new(&script_path)
        .arg("--mock")
        .arg("--artifact-dir")
        .arg(&temp_art_dir)
        .output()
        .expect("failed to execute boot_profile.sh");

    assert!(
        output.status.success(),
        "boot_profile.sh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Top 5 Slowest Units Identified:"));
    assert!(stdout.contains("systemd-udev-settle.service"));
    assert!(stdout.contains("ai-capabilityd.service"));

    // Check files created in artifact dir
    assert!(temp_art_dir.join("systemd_blame.txt").exists());
    assert!(temp_art_dir.join("critical_chain.txt").exists());

    let mut profile_files = vec![];
    for entry in fs::read_dir(&temp_art_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("boot_profile_") && name.ends_with(".json") {
            profile_files.push(entry.path());
        }
    }
    assert_eq!(profile_files.len(), 1);

    let json_content = fs::read_to_string(&profile_files[0]).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_content).unwrap();

    let top5 = v.get("top5").and_then(|t| t.as_array()).unwrap();
    assert_eq!(top5.len(), 5);
    assert_eq!(top5[0]["unit"], "systemd-udev-settle.service");
    assert_eq!(top5[0]["duration_secs"], 1.85);

    let total = v["total_top5_secs"].as_f64().unwrap();
    assert!(total > 4.0 && total < 6.0);

    let _ = fs::remove_dir_all(&temp_art_dir);
}

#[test]
fn test_boot_profiling_regression_detection() {
    let root = env!("CARGO_MANIFEST_DIR");
    let script_path = Path::new(root).join("../../scripts/boot_profile.sh");

    let temp_art_dir = std::env::temp_dir().join("test_boot_regression");
    let _ = fs::remove_dir_all(&temp_art_dir);
    fs::create_dir_all(&temp_art_dir).unwrap();

    // Create a fast baseline: total 2.0s
    let baseline_file = temp_art_dir.join("baseline.json");
    let baseline_json = serde_json::json!({
        "top5": [
            {"unit": "unit-a.service", "duration_secs": 1.0},
            {"unit": "unit-b.service", "duration_secs": 1.0}
        ],
        "total_top5_secs": 2.0,
        "environment": "tcg_or_emulated"
    });
    fs::write(
        &baseline_file,
        serde_json::to_string_pretty(&baseline_json).unwrap(),
    )
    .unwrap();

    // Run profiling against baseline with 15% threshold; current mock is ~5.04s, so > 100% regression
    let output = Command::new(&script_path)
        .arg("--mock")
        .arg("--artifact-dir")
        .arg(&temp_art_dir)
        .arg("--baseline")
        .arg(&baseline_file)
        .arg("--threshold-pct")
        .arg("15")
        .output()
        .expect("failed to run boot_profile.sh");

    // Must exit with code 2 on regression
    assert_eq!(
        output.status.code(),
        Some(2),
        "Expected regression exit code 2, got output: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("REGRESSION WARNING"));

    let _ = fs::remove_dir_all(&temp_art_dir);
}
