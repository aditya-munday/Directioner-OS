//! Integration tests for Update Verification and Automatic Rollback Engine (TODO 50).
//!
//! Tests:
//! 1. 50.1 Signature verification: valid signature proceeds; invalid signature triggers immediate rollback.
//! 2. 50.2 Greenboot boot counter decrement and rollback decision when counter reaches 0.
//! 3. 50.3 Good image scenario: all 4 gating health checks pass -> deployment committed.
//! 4. 50.4 Bad image scenario: failing health check decrements counter and triggers fallback rollback.
//! 5. 50.5 End-to-end shell script execution testing good, bad, and hung boot simulation.

use std::path::Path;
use std::process::Command;

use control_plane::update_verifier::{
    GatingHealthCheck, GreenbootController, RollbackDecision, StagedDeployment, UpdateSignature,
    UpdateVerificationReport,
};

fn mock_deployment(valid_sig: bool) -> (StagedDeployment, bool) {
    let signature = UpdateSignature {
        digest: "sha256:d17ec7100e42f00d89ab1234".into(),
        key_id: "directioner-release-key-2026".into(),
        signature: if valid_sig {
            "sig:valid:ed25519:abcdef".into()
        } else {
            "".into()
        },
    };

    let deployment = StagedDeployment {
        deployment_id: "directioner-os-42.0.1".into(),
        version: "42.0.1".into(),
        signature,
        staged_at: "2026-10-18T10:00:00Z".into(),
    };

    (deployment, valid_sig)
}

fn mock_health_checks(all_pass: bool) -> Vec<GatingHealthCheck> {
    vec![
        GatingHealthCheck {
            name: "01_systemd_posture.sh".into(),
            passed: true,
            details: "systemd state: running".into(),
        },
        GatingHealthCheck {
            name: "02_ai_capabilityd.sh".into(),
            passed: all_pass,
            details: if all_pass {
                "socket ping successful".into()
            } else {
                "daemon crashed with SIGSEGV".into()
            },
        },
        GatingHealthCheck {
            name: "03_world_model.sh".into(),
            passed: true,
            details: "sqlite DB integrity OK".into(),
        },
        GatingHealthCheck {
            name: "04_audit_worklog.sh".into(),
            passed: true,
            details: "audit chain verified".into(),
        },
    ]
}

#[test]
fn test_update_verification_good_image_commits() {
    let (deployment, sig_valid) = mock_deployment(true);
    let checks = mock_health_checks(true);
    let report = UpdateVerificationReport { deployment, checks };

    let mut controller = GreenbootController::default();
    assert_eq!(controller.boot_counter, 3);
    assert!(!controller.is_green);

    let decision = controller.evaluate(sig_valid, &report);
    assert_eq!(decision, RollbackDecision::Commit);
    assert!(controller.is_green);
    assert_eq!(controller.boot_counter, 0);
}

#[test]
fn test_update_verification_bad_image_counter_and_rollback() {
    let (deployment, sig_valid) = mock_deployment(true);
    let checks = mock_health_checks(false); // 02_ai_capabilityd.sh fails
    let report = UpdateVerificationReport { deployment, checks };

    // Initial counter = 3
    let mut controller = GreenbootController::default();

    // 1. First failure decrements counter: 3 -> 2
    let dec1 = controller.evaluate(sig_valid, &report);
    assert_eq!(dec1, RollbackDecision::RetryRemaining { attempts_left: 2 });
    assert_eq!(controller.boot_counter, 2);

    // 2. Second failure decrements counter: 2 -> 1
    let dec2 = controller.evaluate(sig_valid, &report);
    assert_eq!(dec2, RollbackDecision::RetryRemaining { attempts_left: 1 });
    assert_eq!(controller.boot_counter, 1);

    // 3. Third failure exhausts counter: 1 -> 0 and triggers automatic rollback!
    let dec3 = controller.evaluate(sig_valid, &report);
    match dec3 {
        RollbackDecision::TriggerRollback { reason } => {
            assert!(reason.contains("Boot counter exhausted"));
            assert!(reason.contains("02_ai_capabilityd.sh"));
        }
        other => panic!("Expected TriggerRollback, got: {:?}", other),
    }
    assert_eq!(controller.boot_counter, 0);
    assert!(!controller.is_green);
}

#[test]
fn test_update_verification_invalid_signature_immediate_rollback() {
    let (deployment, sig_valid) = mock_deployment(false);
    let checks = mock_health_checks(true);
    let report = UpdateVerificationReport { deployment, checks };

    let mut controller = GreenbootController::default();
    let decision = controller.evaluate(sig_valid, &report);

    match decision {
        RollbackDecision::TriggerRollback { reason } => {
            assert!(reason.contains("signature validation failed"));
        }
        other => panic!(
            "Expected TriggerRollback for invalid signature, got {:?}",
            other
        ),
    }
}

#[test]
fn test_update_verify_rollback_script_scenarios() {
    let root = env!("CARGO_MANIFEST_DIR");
    let script_path = Path::new(root).join("../../scripts/update_verify_rollback.sh");
    assert!(script_path.exists());

    // 1. Good image execution
    let good_out = Command::new(&script_path)
        .arg("--simulate-good")
        .output()
        .expect("simulate good");
    assert!(good_out.status.success());
    let stdout_good = String::from_utf8_lossy(&good_out.stdout);
    assert!(stdout_good.contains("All gating health checks PASSED"));
    assert!(stdout_good.contains("Deployment committed successfully"));

    // 2. Bad image execution with counter=1 (triggers rollback)
    let bad_out = Command::new(&script_path)
        .arg("--simulate-bad")
        .arg("--counter")
        .arg("1")
        .output()
        .expect("simulate bad");
    assert!(bad_out.status.success());
    let stdout_bad = String::from_utf8_lossy(&bad_out.stdout);
    assert!(stdout_bad.contains("Gating health checks FAILED"));
    assert!(stdout_bad.contains("[ROLLBACK TRIGGERED]: Boot counter exhausted"));
    assert!(stdout_bad.contains("Rolling back to previous deployment slot 0"));

    // 3. Hung boot execution (watchdog timeout)
    let hung_out = Command::new(&script_path)
        .arg("--simulate-hung")
        .output()
        .expect("simulate hung");
    assert!(hung_out.status.success());
    let stdout_hung = String::from_utf8_lossy(&hung_out.stdout);
    assert!(stdout_hung.contains("watchdog timeout encountered"));
    assert!(
        stdout_hung.contains(
            "[ROLLBACK TRIGGERED]: GRUB automatically boots previous known-good deployment"
        )
    );
}
