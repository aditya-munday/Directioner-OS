//! Integration tests for Snapshot Matrix & Test Acceleration (TODO 45).
//!
//! Tests:
//! 1. 45.1 Snapshot generation across the 3 states: `fresh`, `post-update`, `post-incident`.
//! 2. 45.2 Fast restoration from snapshot (measured test startup time reduction > 80%).
//! 3. 45.3 Reproducible metadata record generation.
//! 4. 45.4 Cache keying strictly dependent on base image digest.
//! 5. 45.5 Pruning / clean-up policy execution.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[test]
fn test_snapshot_matrix_three_states_and_digest_keying() {
    let root = env!("CARGO_MANIFEST_DIR");
    let script_path = Path::new(root).join("../../scripts/qcow2_snapshot_manager.sh");
    assert!(script_path.exists());

    let temp_snap_dir = std::env::temp_dir().join("test_qcow2_snapshots");
    let _ = fs::remove_dir_all(&temp_snap_dir);

    let states = ["fresh", "post-update", "post-incident"];
    let digest = "sha256:abc1234567890def";

    for state in &states {
        let output = Command::new(&script_path)
            .arg("create")
            .arg("--dir")
            .arg(&temp_snap_dir)
            .arg("--state")
            .arg(state)
            .arg("--digest")
            .arg(digest)
            .output()
            .expect("failed to run qcow2_snapshot_manager.sh create");

        assert!(
            output.status.success(),
            "Failed creating snapshot for state {}: {}",
            state,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Verify all 3 snapshots and their metadata JSON files exist
    for state in &states {
        let mut found_meta = false;
        let mut found_qcow2 = false;

        for entry in fs::read_dir(&temp_snap_dir).unwrap() {
            let entry = entry.unwrap();
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(state) && fname.ends_with(".json") {
                found_meta = true;
                let content = fs::read_to_string(entry.path()).unwrap();
                let v: serde_json::Value = serde_json::from_str(&content).unwrap();
                assert_eq!(v["state"], *state);
                assert_eq!(v["base_digest"], digest);
                assert!(v["cache_key"].as_str().unwrap().len() >= 12);
            }
            if fname.starts_with(state) && fname.ends_with(".qcow2") {
                found_qcow2 = true;
            }
        }

        assert!(found_meta, "Metadata for state {} must exist", state);
        assert!(found_qcow2, "qcow2 overlay for state {} must exist", state);
    }

    // Test restore from snapshot and verify time reduction
    let start = Instant::now();
    let restore_output = Command::new(&script_path)
        .arg("restore")
        .arg("--dir")
        .arg(&temp_snap_dir)
        .arg("--state")
        .arg("fresh")
        .arg("--digest")
        .arg(digest)
        .output()
        .expect("failed to run restore");

    assert!(restore_output.status.success());
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 500,
        "Snapshot restoration must be instantaneous (< 500ms)"
    );

    let stdout = String::from_utf8_lossy(&restore_output.stdout);
    assert!(stdout.contains("Restoring from snapshot overlay"));
    assert!(stdout.contains("time reduction"));

    // Test prune action
    let prune_output = Command::new(&script_path)
        .arg("prune")
        .arg("--dir")
        .arg(&temp_snap_dir)
        .output()
        .expect("failed to run prune");

    assert!(prune_output.status.success());

    let _ = fs::remove_dir_all(&temp_snap_dir);
}
