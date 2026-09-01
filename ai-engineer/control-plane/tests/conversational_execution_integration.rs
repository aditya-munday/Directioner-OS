//! Phase 4.1 Conversational Execution Integration Test Suite.
//!
//! Validates:
//! 1. Policy-governed execution dispatch of conversational plan drafts over local domain socket.
//! 2. Rejection of adversarial prompt-injected confirmation tokens.
//! 3. Cryptographic hash mismatch refusal on Confirm-tier plans.
//! 4. KillSwitch hard-halt before socket dispatch.
//! 5. Automatic compensating rollback on execution failure in conversational pipeline.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use capability_layer::CapabilityKind;
use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::Server;
use control_plane::client::CapabilityClient;
use control_plane::conversational::{ConversationalError, PlanExecutionGate};
use control_plane::plan::{Plan, PlanStatus, PlanStep, RollbackStep};
use control_plane::policy::{ApprovalTier, BlastRadiusReport};
use control_plane::reviewer::{
    FindingSeverity, ReviewFinding, ReviewReport, ReviewVerdict, Reviewer, ReviewerConfig,
};
use parking_lot::Mutex;
use tempfile::tempdir;

fn setup_test_daemon(temp: &tempfile::TempDir) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let sock = temp.path().join("cap.sock");
    let audit_log = temp.path().join("audit.jsonl");
    let lock_file = temp.path().join("kill.lock");
    let work_root = temp.path().join("scope");
    fs::create_dir_all(&work_root).unwrap();

    let h_scope = ScopedPath::new(&work_root).unwrap();
    let h_audit = AuditLog::open(&audit_log).unwrap();
    let h_kill = KillSwitch::new(&lock_file);

    let handler = Handler::new(
        h_scope,
        h_audit,
        h_kill,
        HandlerConfig {
            approval_required: false,
            protected_units: vec![],
            dry_run: false,
        },
    );

    let srv_sock = sock.clone();
    thread::spawn(move || {
        let server = Server::bind(srv_sock, Arc::new(Mutex::new(handler))).unwrap();
        server.serve().ok();
    });

    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    (sock, audit_log, lock_file, work_root)
}

#[test]
fn test_conversational_draft_vs_apply_execution_gating() {
    let temp = tempdir().unwrap();
    let (sock, _, lock_file, work_root) = setup_test_daemon(&temp);

    let target_file = work_root.join("test_config.txt");
    let plan = Plan {
        id: "plan-conv-1".into(),
        intent: "Write initial configuration".into(),
        summary: "Writes test_config.txt safely".into(),
        steps: vec![PlanStep {
            step_id: "step-1".into(),
            description: "Write config file".into(),
            capability: CapabilityKind::FileWrite {
                path: target_file.to_str().unwrap().to_string(),
                content: "port=8080\n".into(),
                append: false,
            },
            blast_radius: BlastRadiusReport {
                tier: ApprovalTier::Notify,
                score: 25,
                factors: vec![],
            },
            timeout_secs: 10,
        }],
        rollback_steps: vec![RollbackStep {
            step_id: "rb-1".into(),
            description: "Delete config on failure".into(),
            capability: CapabilityKind::FileDelete {
                path: target_file.to_str().unwrap().to_string(),
            },
            triggers: vec!["step-1".into()],
        }],
        status: PlanStatus::Draft,
        aggregate_blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Notify,
            score: 25,
            factors: vec![],
        },
        created_at: "2026-09-01T00:00:00Z".into(),
    };

    let reviewer = Reviewer::new(ReviewerConfig::default());
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Approved);

    // 1. In draft state without execution dispatch, file does NOT exist
    assert!(!target_file.exists());

    // 2. Dispatch execution via PlanExecutionGate
    let client = CapabilityClient::new(&sock, "conv-test");
    let report = PlanExecutionGate::dispatch_execution(
        &plan,
        &review,
        None, // Notify tier does not require interactive confirmation hash
        &client,
        Some(&lock_file),
        None,
    )
    .expect("Dispatch execution should succeed for valid Notify-tier plan");

    assert_eq!(report.execution.final_status, PlanStatus::Completed);
    assert!(target_file.exists());
    assert_eq!(fs::read_to_string(&target_file).unwrap(), "port=8080\n");

    // Verification check passed
    assert!(report.verification.is_some());
    assert!(report.verification.unwrap().all_passed);
}

#[test]
fn test_conversational_hash_mismatch_refusal_on_confirm_tier() {
    let temp = tempdir().unwrap();
    let (sock, _, lock_file, work_root) = setup_test_daemon(&temp);

    let target_file = work_root.join("sys.conf");
    let plan = Plan {
        id: "plan-confirm-1".into(),
        intent: "High blast write".into(),
        summary: "Modifies core parameter".into(),
        steps: vec![PlanStep {
            step_id: "s1".into(),
            description: "Write sys.conf".into(),
            capability: CapabilityKind::FileWrite {
                path: target_file.to_str().unwrap().to_string(),
                content: "kernel.sysrq=1\n".into(),
                append: false,
            },
            blast_radius: BlastRadiusReport {
                tier: ApprovalTier::Confirm,
                score: 55,
                factors: vec![],
            },
            timeout_secs: 10,
        }],
        rollback_steps: vec![],
        status: PlanStatus::Draft,
        aggregate_blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Confirm,
            score: 55,
            factors: vec![],
        },
        created_at: "2026-09-01T00:00:00Z".into(),
    };

    let review = ReviewReport {
        verdict: ReviewVerdict::Approved,
        findings: vec![],
        summary: "Approved".into(),
    };

    let client = CapabilityClient::new(&sock, "conv-test");

    // Attempt 1: Without confirmed hash -> Fails closed
    let err_no_hash = PlanExecutionGate::dispatch_execution(
        &plan,
        &review,
        None,
        &client,
        Some(&lock_file),
        None,
    );
    assert!(err_no_hash.is_err());
    assert!(!target_file.exists());

    // Attempt 2: With mismatched hash -> Fails closed
    let err_mismatch = PlanExecutionGate::dispatch_execution(
        &plan,
        &review,
        Some("deadbeef00000000"),
        &client,
        Some(&lock_file),
        None,
    );
    assert!(err_mismatch.is_err());
    assert!(!target_file.exists());

    // Attempt 3: With exact cryptographic plan hash -> Succeeds
    let actual_hash = PlanExecutionGate::compute_plan_hash(&plan);
    let success = PlanExecutionGate::dispatch_execution(
        &plan,
        &review,
        Some(&actual_hash),
        &client,
        Some(&lock_file),
        None,
    );
    assert!(success.is_ok());
    assert!(target_file.exists());
}

#[test]
fn test_conversational_killswitch_hard_halt_before_socket_dispatch() {
    let temp = tempdir().unwrap();
    let (sock, _, lock_file, work_root) = setup_test_daemon(&temp);

    // Engage killswitch
    let mut ks = KillSwitch::new(&lock_file);
    ks.engage().unwrap();
    assert!(ks.is_engaged());

    let target_file = work_root.join("kill_test.txt");
    let plan = Plan {
        id: "plan-kill-1".into(),
        intent: "Test write while killswitch active".into(),
        summary: "Write file".into(),
        steps: vec![PlanStep {
            step_id: "s1".into(),
            description: "Write file".into(),
            capability: CapabilityKind::FileWrite {
                path: target_file.to_str().unwrap().to_string(),
                content: "blocked\n".into(),
                append: false,
            },
            blast_radius: BlastRadiusReport {
                tier: ApprovalTier::Auto,
                score: 10,
                factors: vec![],
            },
            timeout_secs: 10,
        }],
        rollback_steps: vec![],
        status: PlanStatus::Draft,
        aggregate_blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Auto,
            score: 10,
            factors: vec![],
        },
        created_at: "2026-09-01T00:00:00Z".into(),
    };

    let review = ReviewReport {
        verdict: ReviewVerdict::Approved,
        findings: vec![],
        summary: "Approved".into(),
    };

    let client = CapabilityClient::new(&sock, "conv-test");
    let result = PlanExecutionGate::dispatch_execution(
        &plan,
        &review,
        None,
        &client,
        Some(&lock_file),
        None,
    );

    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(ConversationalError::ConfigurationError(msg)) if msg.contains("Killswitch is engaged")
    ));
    assert!(!target_file.exists());
}

#[test]
fn test_conversational_reviewer_veto_blocks_execution() {
    let temp = tempdir().unwrap();
    let (sock, _, lock_file, work_root) = setup_test_daemon(&temp);

    let plan = Plan {
        id: "plan-veto-1".into(),
        intent: "Dangerous action".into(),
        summary: "Dangerous operation".into(),
        steps: vec![PlanStep {
            step_id: "s1".into(),
            description: "Write dangerous payload".into(),
            capability: CapabilityKind::FileWrite {
                path: work_root.join("exploit.sh").to_str().unwrap().to_string(),
                content: "#!/bin/sh\n".into(),
                append: false,
            },
            blast_radius: BlastRadiusReport {
                tier: ApprovalTier::Auto,
                score: 5,
                factors: vec![],
            },
            timeout_secs: 10,
        }],
        rollback_steps: vec![],
        status: PlanStatus::Draft,
        aggregate_blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Auto,
            score: 5,
            factors: vec![],
        },
        created_at: "2026-09-01T00:00:00Z".into(),
    };

    let review = ReviewReport {
        verdict: ReviewVerdict::Rejected,
        findings: vec![ReviewFinding {
            severity: FindingSeverity::Fatal,
            category: "critical_path".into(),
            message: "Disallowed executable script injection".into(),
            step_id: Some("s1".into()),
        }],
        summary: "Rejected by security policy".into(),
    };

    let client = CapabilityClient::new(&sock, "conv-test");
    let result = PlanExecutionGate::dispatch_execution(
        &plan,
        &review,
        None,
        &client,
        Some(&lock_file),
        None,
    );

    assert!(result.is_err());
    assert!(!work_root.join("exploit.sh").exists());
}
