//! Integration Tests for Phase 4.1: Policy-Governed Execution Wiring & Confirmation Gate.
//!
//! Tests the end-to-end transition from validated `PlanStatus::Draft` to approved
//! execution through `CapabilityClient` and `Implementer`, including killswitch blocks,
//! reviewer rejection refusals, cryptographic hash validation, and compensating rollbacks.

use std::fs;
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
use control_plane::conversational::{
    ConversationSession, GateVerdict, IntentParser, MockBackend, PlanExecutionGate,
};
use control_plane::implementer::Implementer;
use control_plane::plan::{Plan, PlanStatus, PlanStep, RollbackStep};
use control_plane::policy::{ApprovalTier, BlastRadiusReport};
use control_plane::reviewer::{ReviewVerdict, Reviewer, ReviewerConfig};
use parking_lot::Mutex;
use tempfile::tempdir;

fn setup_test_server() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let sock_path = root.join("test-cap.sock");
    let audit_path = root.join("test-audit.jsonl");
    let lock_path = root.join("test-killswitch.lock");
    let scope_path = root.join("scope");
    fs::create_dir_all(&scope_path).unwrap();

    let scope = ScopedPath::new(&scope_path).unwrap();
    let audit = AuditLog::open(&audit_path).unwrap();
    let killswitch = KillSwitch::new(&lock_path);
    let config = HandlerConfig {
        approval_required: false,
        protected_units: vec![],
        dry_run: false,
    };

    let handler = Handler::new(scope, audit, killswitch, config);
    let srv_sock = sock_path.clone();
    thread::spawn(move || {
        let server = Server::bind(srv_sock, Arc::new(Mutex::new(handler))).unwrap();
        let _ = server.serve();
    });

    // Wait for socket to become ready
    for _ in 0..50 {
        if sock_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    (dir, sock_path, audit_path, lock_path, scope_path)
}

fn make_test_plan(intent: &str, summary: &str) -> Plan {
    Plan {
        id: "plan-test-01".into(),
        intent: intent.into(),
        summary: summary.into(),
        steps: vec![],
        rollback_steps: vec![],
        status: PlanStatus::Draft,
        aggregate_blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Notify,
            score: 10,
            factors: vec![],
        },
        created_at: "2026-08-31T00:00:00Z".into(),
    }
}

#[test]
fn test_conversational_execution_gate_blocks_on_reviewer_fatal_finding() {
    let dangerous_draft = r#"```json
    {
        "intent": "Remove glibc core libraries",
        "summary": "Removes core glibc package",
        "steps": [
            {
                "step_id": "step-1",
                "description": "Remove glibc",
                "capability": {
                    "kind": "PackageRemove",
                    "package": "glibc",
                    "approved": false
                }
            }
        ]
    }
    ```"#;

    let mock = MockBackend::new(vec![dangerous_draft.to_string()]);
    let mut session = ConversationSession::new(10);
    session.add_user_message("Remove glibc");

    let draft = IntentParser::parse_intent(&session, &mock, None).unwrap();
    assert_eq!(draft.status, PlanStatus::Draft);

    let reviewer = Reviewer::new(ReviewerConfig::default());
    let review = reviewer.review_plan(&draft);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);

    let gate = PlanExecutionGate::evaluate(&draft, &review);
    match gate {
        GateVerdict::Rejected { reason, findings } => {
            assert!(reason.contains("Reviewer rejected plan"));
            assert!(!findings.is_empty());
        }
        other => panic!("Expected GateVerdict::Rejected, got {other:?}"),
    }
}

#[test]
fn test_conversational_execution_gate_approves_and_executes_on_daemon() {
    let (_dir, sock_path, audit_path, _lock_path, scope_path) = setup_test_server();

    let valid_draft = r#"```json
    {
        "intent": "Configure app setting",
        "summary": "Writes app.conf in scope",
        "steps": [
            {
                "step_id": "step-1",
                "description": "Write app config",
                "capability": {
                    "kind": "FileWrite",
                    "path": "app.conf",
                    "content": "port=8080\n",
                    "append": false
                }
            }
        ],
        "rollback_steps": [
            {
                "step_id": "rb-1",
                "description": "Delete app config",
                "capability": {
                    "kind": "FileDelete",
                    "path": "app.conf"
                },
                "triggers": ["step-1"]
            }
        ]
    }
    ```"#;

    let mock = MockBackend::new(vec![valid_draft.to_string()]);
    let mut session = ConversationSession::new(10);
    session.add_user_message("Configure app port to 8080");

    let draft = IntentParser::parse_intent(&session, &mock, None).unwrap();
    assert_eq!(draft.status, PlanStatus::Draft);

    let reviewer = Reviewer::new(ReviewerConfig::default());
    let review = reviewer.review_plan(&draft);
    assert_ne!(review.verdict, ReviewVerdict::Rejected);

    let gate = PlanExecutionGate::evaluate(&draft, &review);
    let plan_hash = match gate {
        GateVerdict::Ready { plan_hash, .. } => plan_hash,
        other => panic!("Expected GateVerdict::Ready, got {other:?}"),
    };

    // Transition from Draft to Approved
    let approved_plan = PlanExecutionGate::approve_plan(draft, &plan_hash).unwrap();
    assert_eq!(approved_plan.status, PlanStatus::Approved);

    // Execute via Implementer
    let client = CapabilityClient::new(&sock_path, "ai-engineer-test");
    let implementer = Implementer::new(&client);
    let report = implementer.execute_plan(&approved_plan);

    assert_eq!(
        report.final_status,
        PlanStatus::Completed,
        "Execution must succeed: {}",
        report.message
    );

    // Verify file written to scope
    let written = fs::read_to_string(scope_path.join("app.conf")).unwrap();
    assert_eq!(written, "port=8080\n");

    // Verify audit log has entries
    let audit_content = fs::read_to_string(&audit_path).unwrap();
    assert!(audit_content.contains("file.write"));
    assert!(audit_content.contains("app.conf"));
}

#[test]
fn test_conversational_execution_killswitch_hard_blocks_execution() {
    let (_dir, sock_path, _audit_path, lock_path, _scope_path) = setup_test_server();

    // Engage killswitch
    let mut killswitch = KillSwitch::new(&lock_path);
    killswitch.engage().unwrap();
    assert!(killswitch.is_engaged());

    let mut plan = make_test_plan("Test Killswitch", "Ping test");
    plan.steps.push(PlanStep {
        step_id: "s1".into(),
        description: "Ping".into(),
        capability: CapabilityKind::Ping,
        blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Auto,
            score: 0,
            factors: vec![],
        },
        timeout_secs: 10,
    });
    plan.status = PlanStatus::Approved;

    let client = CapabilityClient::new(&sock_path, "ai-engineer-test");
    let implementer = Implementer::new(&client);
    let report = implementer.execute_plan(&plan);

    assert_eq!(
        report.final_status,
        PlanStatus::RolledBack,
        "Execution must abort when killswitch is engaged"
    );
    assert!(!report.step_results.is_empty());
    assert!(!report.step_results[0].success);
    let details_str = serde_json::to_string(&report.step_results[0].details).unwrap();
    assert!(
        details_str.to_lowercase().contains("kill-switch")
            || details_str.to_lowercase().contains("killswitch")
    );
}

#[test]
fn test_conversational_cryptographic_hash_mismatch_fails_closed() {
    let mut plan = make_test_plan("Hash Test", "Summary");
    plan.steps.push(PlanStep {
        step_id: "s1".into(),
        description: "Original step".into(),
        capability: CapabilityKind::Ping,
        blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Auto,
            score: 0,
            factors: vec![],
        },
        timeout_secs: 10,
    });

    let original_hash = PlanExecutionGate::compute_plan_hash(&plan);

    // Tamper with plan step description
    plan.steps[0].description = "Tampered malicious step".into();

    let approve_result = PlanExecutionGate::approve_plan(plan, &original_hash);
    assert!(
        approve_result.is_err(),
        "Tampered plan must be rejected by cryptographic hash mismatch"
    );
}

#[test]
fn test_conversational_execution_step_failure_triggers_compensating_rollback() {
    let (_dir, sock_path, _audit_path, _lock_path, scope_path) = setup_test_server();

    // Plan: Step 1 writes a temporary canary file; Step 2 tries to read a non-existent file
    let mut plan = make_test_plan("Rollback Test", "Tests automated rollback on step 2 error");
    plan.steps.push(PlanStep {
        step_id: "step-1".into(),
        description: "Write canary file".into(),
        capability: CapabilityKind::FileWrite {
            path: "canary.txt".into(),
            content: "canary data".into(),
            append: false,
        },
        blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Notify,
            score: 10,
            factors: vec![],
        },
        timeout_secs: 10,
    });
    plan.steps.push(PlanStep {
        step_id: "step-2".into(),
        description: "Read non-existent file (will fail)".into(),
        capability: CapabilityKind::FileRead {
            path: "non_existent_file.txt".into(),
        },
        blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Auto,
            score: 0,
            factors: vec![],
        },
        timeout_secs: 10,
    });

    // Rollback step for step-1: triggers on step-2 failure
    plan.rollback_steps.push(RollbackStep {
        step_id: "rb-1".into(),
        description: "Delete canary file on failure".into(),
        capability: CapabilityKind::FileDelete {
            path: "canary.txt".into(),
        },
        triggers: vec!["step-2".into()],
    });

    let hash = PlanExecutionGate::compute_plan_hash(&plan);
    let approved_plan = PlanExecutionGate::approve_plan(plan, &hash).unwrap();

    let client = CapabilityClient::new(&sock_path, "ai-engineer-test");
    let implementer = Implementer::new(&client);
    let report = implementer.execute_plan(&approved_plan);

    assert_eq!(
        report.final_status,
        PlanStatus::RolledBack,
        "Plan must roll back due to step 2 error"
    );

    // Rollback should have cleaned up canary.txt
    assert!(
        !scope_path.join("canary.txt").exists(),
        "Compensating rollback must delete canary.txt on failure"
    );
}
