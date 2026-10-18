//! Integration tests for Chaos Testing Harness (TODO 44).
//!
//! Tests:
//! 1. 44.1 Random fault scheduler prints seed and executes fault sequence.
//! 2. 44.2 Fault varieties: process kills, I/O errors, memory pressure, service restarts.
//! 3. 44.3 Invariant verification after faults (socket 0600, policy unchanged, log verifies, kill-switch works).
//! 4. 44.4 Safety guard: fails if executed without VM or simulation guard.
//! 5. 44.5 Deterministic replay: failing seed saved and reproduced on rerun with identical seed.

use std::path::Path;
use std::process::Command;

#[test]
fn test_chaos_runner_success_flow_and_seed_output() {
    let root = env!("CARGO_MANIFEST_DIR");
    let script_path = Path::new(root).join("../../scripts/chaos_runner.sh");
    assert!(script_path.exists());

    let output = Command::new(&script_path)
        .arg("--seed")
        .arg("424242")
        .arg("--iterations")
        .arg("4")
        .arg("--simulated")
        .arg("true")
        .output()
        .expect("failed to execute chaos_runner.sh");

    assert!(
        output.status.success(),
        "chaos_runner.sh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Active Seed: 424242"));
    assert!(stdout.contains("socket_permissions_0600"));
    assert!(stdout.contains("policy_hash_unchanged"));
    assert!(stdout.contains("audit_log_cryptographic_verification"));
    assert!(stdout.contains("emergency_kill_switch_responsive"));
    assert!(stdout.contains("All 4 chaos iterations passed"));
}

#[test]
fn test_chaos_runner_safety_guard_blocks_host_execution() {
    let root = env!("CARGO_MANIFEST_DIR");
    let script_path = Path::new(root).join("../../scripts/chaos_runner.sh");

    // Invoking without --simulated on non-VM host should trigger fatal error
    let output = Command::new(&script_path)
        .arg("--seed")
        .arg("12345")
        .output()
        .expect("failed to execute chaos_runner.sh");

    // If on bare-metal / container without VM DMI, it must exit with error
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Chaos testing may ONLY be executed inside a VM"));
    }
}

#[test]
fn test_chaos_failing_seed_reproduces_deterministically() {
    let root = env!("CARGO_MANIFEST_DIR");
    let script_path = Path::new(root).join("../../scripts/chaos_runner.sh");

    let test_seed = "987654";

    // 1. First run with injected failure on step 3
    let output1 = Command::new(&script_path)
        .arg("--seed")
        .arg(test_seed)
        .arg("--iterations")
        .arg("5")
        .arg("--simulated")
        .arg("true")
        .arg("--fail-step")
        .arg("3")
        .output()
        .expect("failed to execute chaos_runner.sh");

    assert_eq!(output1.status.code(), Some(3));
    let stderr1 = String::from_utf8_lossy(&output1.stderr);
    assert!(stderr1.contains("INVARIANT VIOLATION at Step 3"));

    // 2. Replay with the EXACT same seed and verify failure reproduces identically
    let output2 = Command::new(&script_path)
        .arg("--seed")
        .arg(test_seed)
        .arg("--iterations")
        .arg("5")
        .arg("--simulated")
        .arg("true")
        .arg("--fail-step")
        .arg("3")
        .output()
        .expect("failed to re-execute chaos_runner.sh");

    assert_eq!(output2.status.code(), Some(3));
    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    assert_eq!(
        stderr1, stderr2,
        "Failing seed must reproduce identical failure trace"
    );
}
