//! Integration tests for Control Plane (Phase 1.2, 1.3, 1.5).

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
use control_plane::implementer::Implementer;
use control_plane::plan::{PlanStatus, PlanStep, RollbackStep};
use control_plane::planner::{Planner, PlannerConfig};
use control_plane::policy::PolicyEngine;
use control_plane::simulation::Simulator;
use control_plane::verifier::Verifier;
use parking_lot::Mutex;

#[test]
fn test_planner_and_simulation_pipeline() {
    let temp = tempfile::tempdir().unwrap();
    let planner = Planner::new(PlannerConfig {
        work_root: temp.path().to_path_buf(),
        default_timeout_secs: 10,
    });

    let plan = planner
        .create_plan("configure web server on port 8080", None)
        .unwrap();

    assert_eq!(plan.status, PlanStatus::Draft);
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.rollback_steps.len(), 3);

    // Blast radius evaluation
    assert!(plan.aggregate_blast_radius.score >= 45);
    assert!(plan.aggregate_blast_radius.tier.requires_confirmation());

    // Dry-run preview
    let preview = Simulator::simulate_plan(&plan, Some(temp.path()));
    assert_eq!(preview.step_previews.len(), 3);
    let term_output = preview.render_terminal();
    assert!(term_output.contains("DIRECTIONER-OS AI ENGINEER — PLAN PREVIEW"));
    assert!(term_output.contains("listen 8080"));
}

#[test]
fn test_implementer_and_verifier_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let sock = temp.path().join("cap.sock");
    let audit_log = temp.path().join("audit.jsonl");
    let lock_file = temp.path().join("kill.lock");
    let work_root = temp.path().join("scope");
    fs::create_dir_all(&work_root).unwrap();

    // Start capability daemon in background thread
    let h_scope = ScopedPath::new(&work_root).unwrap();
    let h_audit = AuditLog::open(&audit_log).unwrap();
    let h_kill = KillSwitch::new(&lock_file);
    let handler = Handler::new(
        h_scope,
        h_audit,
        h_kill,
        HandlerConfig {
            approval_required: false,
            protected_units: vec!["ssh.service".into()],
            dry_run: false, // test actual file writes in scope
        },
    );

    let srv_sock = sock.clone();
    thread::spawn(move || {
        let server = Server::bind(srv_sock, Arc::new(Mutex::new(handler))).unwrap();
        server.serve().ok();
    });

    // Wait for socket to become available
    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    let client = CapabilityClient::new(&sock, "test-runner");
    assert!(client.ping().unwrap());

    // Construct a plan modifying a file in scope
    let target_file = work_root.join("config.ini");
    let plan = control_plane::Plan {
        id: "plan-test-exec".into(),
        intent: "Setup custom configuration".into(),
        summary: "Writes config.ini and checks status".into(),
        steps: vec![
            PlanStep {
                step_id: "step-1".into(),
                description: "Write config file".into(),
                capability: CapabilityKind::FileWrite {
                    path: target_file.to_string_lossy().to_string(),
                    content: "setting=active\nworkers=8\n".into(),
                    append: false,
                },
                blast_radius: PolicyEngine::evaluate_capability(
                    &CapabilityKind::FileWrite {
                        path: target_file.to_string_lossy().to_string(),
                        content: "setting=active\nworkers=8\n".into(),
                        append: false,
                    },
                    None,
                ),
                timeout_secs: 5,
            },
            PlanStep {
                step_id: "step-2".into(),
                description: "Check liveness".into(),
                capability: CapabilityKind::Ping,
                blast_radius: PolicyEngine::evaluate_capability(&CapabilityKind::Ping, None),
                timeout_secs: 5,
            },
        ],
        rollback_steps: vec![RollbackStep {
            step_id: "rb-1".into(),
            description: "Clear config file".into(),
            capability: CapabilityKind::FileWrite {
                path: target_file.to_string_lossy().to_string(),
                content: String::new(),
                append: false,
            },
            triggers: vec!["step-1".into()],
        }],
        aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(
            &[
                CapabilityKind::FileWrite {
                    path: target_file.to_string_lossy().to_string(),
                    content: "setting=active\nworkers=8\n".into(),
                    append: false,
                },
                CapabilityKind::Ping,
            ],
            None,
        ),
        created_at: "2026-08-26T00:00:00Z".into(),
        status: PlanStatus::Approved,
    };

    let implementer = Implementer::new(&client);
    let report = implementer.execute_plan(&plan);
    assert_eq!(report.final_status, PlanStatus::Completed);
    assert_eq!(report.step_results.len(), 2);
    assert!(report.step_results[0].success);
    assert!(report.step_results[1].success);

    // Verify written file
    let verifier = Verifier::new(&client);
    let v_report = verifier.verify_plan(&plan);
    assert!(v_report.all_passed);

    let disk_content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(disk_content, "setting=active\nworkers=8\n");
}

#[test]
fn test_implementer_automatic_rollback_on_failure() {
    let temp = tempfile::tempdir().unwrap();
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
            approval_required: true, // will reject unapproved steps
            protected_units: vec!["ssh.service".into()],
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
        thread::sleep(Duration::from_millis(20));
    }

    let client = CapabilityClient::new(&sock, "test-runner");
    let target_file = work_root.join("partial.conf");

    let plan = control_plane::Plan {
        id: "plan-fail-test".into(),
        intent: "Faulty plan triggering rollback".into(),
        summary: "Step 1 writes file, Step 2 is unapproved and fails".into(),
        steps: vec![
            PlanStep {
                step_id: "step-1".into(),
                description: "Write initial file".into(),
                capability: CapabilityKind::FileWrite {
                    path: target_file.to_string_lossy().to_string(),
                    content: "temporary_data\n".into(),
                    append: false,
                },
                blast_radius: PolicyEngine::evaluate_capability(
                    &CapabilityKind::FileWrite {
                        path: target_file.to_string_lossy().to_string(),
                        content: "temporary_data\n".into(),
                        append: false,
                    },
                    None,
                ),
                timeout_secs: 5,
            },
            PlanStep {
                step_id: "step-2".into(),
                description: "Unapproved package install (will be denied)".into(),
                capability: CapabilityKind::PackageInstall {
                    package: "unapproved-pkg".into(),
                    approved: false, // denied by approval gate
                },
                blast_radius: PolicyEngine::evaluate_capability(
                    &CapabilityKind::PackageInstall {
                        package: "unapproved-pkg".into(),
                        approved: false,
                    },
                    None,
                ),
                timeout_secs: 5,
            },
        ],
        rollback_steps: vec![RollbackStep {
            step_id: "rb-1".into(),
            description: "Clean up partial file".into(),
            capability: CapabilityKind::FileWrite {
                path: target_file.to_string_lossy().to_string(),
                content: String::new(),
                append: false,
            },
            triggers: vec!["step-2".into()],
        }],
        aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(
            &[CapabilityKind::PackageInstall {
                package: "unapproved-pkg".into(),
                approved: false,
            }],
            None,
        ),
        created_at: "2026-08-26T00:00:00Z".into(),
        status: PlanStatus::Approved,
    };

    let implementer = Implementer::new(&client);
    let report = implementer.execute_plan(&plan);

    assert_eq!(report.final_status, PlanStatus::RolledBack);
    assert_eq!(report.step_results.len(), 2);
    assert!(report.step_results[0].success);
    assert!(!report.step_results[1].success); // Step 2 was denied
    assert_eq!(report.rollback_results.len(), 1);
    assert!(report.rollback_results[0].success); // Rollback step 1 cleaned up file

    let disk_content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(disk_content, ""); // File was truncated by rollback
}
