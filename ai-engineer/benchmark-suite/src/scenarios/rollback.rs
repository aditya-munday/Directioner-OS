//! Benchmark Scenario 4: Automated Compensating Rollback.

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
use control_plane::plan::{Plan, PlanStatus, PlanStep, RollbackStep};
use control_plane::policy::PolicyEngine;
use parking_lot::Mutex;

use crate::BenchmarkScenario;

/// Scenario 4: Automatic execution of compensating file.write rollback upon mid-flight step failure.
pub struct RollbackScenario;

impl BenchmarkScenario for RollbackScenario {
    fn name(&self) -> &'static str {
        "file_write_compensating_rollback"
    }

    fn description(&self) -> &'static str {
        "Abort forward execution upon fault injection and execute compensating file.write rollback"
    }

    fn run(&self) -> (bool, String) {
        let temp = match tempfile::tempdir() {
            Ok(t) => t,
            Err(e) => return (false, format!("Failed to create tempdir: {e}")),
        };

        let sock = temp.path().join("cap.sock");
        let audit_log = temp.path().join("audit.jsonl");
        let lock_file = temp.path().join("kill.lock");
        let work_root = temp.path().join("scope");
        fs::create_dir_all(&work_root).unwrap();

        let target_file = work_root.join("canary.txt");
        fs::write(&target_file, "original_clean_state\n").unwrap();

        let h_scope = ScopedPath::new(&work_root).unwrap();
        let h_audit = AuditLog::open(&audit_log).unwrap();
        let h_kill = KillSwitch::new(&lock_file);
        let handler = Handler::new(
            h_scope,
            h_audit,
            h_kill,
            HandlerConfig {
                approval_required: true,
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

        let client = CapabilityClient::new(&sock, "benchmark-rollback");

        // Plan:
        // Step 1: Overwrite canary.txt with modified state (approved)
        // Step 2: Request unapproved package install (will fail approval gate)
        // Rollback: Restore canary.txt to original_clean_state
        let plan = Plan {
            id: "plan-benchmark-rollback".into(),
            intent: "Demonstrate automated rollback on fault injection".into(),
            summary: "Modifies canary then fails at step 2".into(),
            steps: vec![
                PlanStep {
                    step_id: "step-1".into(),
                    description: "Modify canary file".into(),
                    capability: CapabilityKind::FileWrite {
                        path: target_file.to_string_lossy().to_string(),
                        content: "dirty_state_needs_rollback\n".into(),
                        append: false,
                    },
                    blast_radius: PolicyEngine::evaluate_capability(
                        &CapabilityKind::FileWrite {
                            path: target_file.to_string_lossy().to_string(),
                            content: "dirty_state_needs_rollback\n".into(),
                            append: false,
                        },
                        None,
                    ),
                    timeout_secs: 5,
                },
                PlanStep {
                    step_id: "step-2".into(),
                    description: "Unapproved action (injected fault)".into(),
                    capability: CapabilityKind::PackageInstall {
                        package: "non-existent-pkg".into(),
                        approved: false, // gate failure
                    },
                    blast_radius: PolicyEngine::evaluate_capability(
                        &CapabilityKind::PackageInstall {
                            package: "non-existent-pkg".into(),
                            approved: false,
                        },
                        None,
                    ),
                    timeout_secs: 5,
                },
            ],
            rollback_steps: vec![RollbackStep {
                step_id: "rb-1".into(),
                description: "Restore original canary content".into(),
                capability: CapabilityKind::FileWrite {
                    path: target_file.to_string_lossy().to_string(),
                    content: "original_clean_state\n".into(),
                    append: false,
                },
                triggers: vec!["step-2".into()],
            }],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(
                &[
                    CapabilityKind::FileWrite {
                        path: target_file.to_string_lossy().to_string(),
                        content: "dirty_state_needs_rollback\n".into(),
                        append: false,
                    },
                    CapabilityKind::PackageInstall {
                        package: "non-existent-pkg".into(),
                        approved: false,
                    },
                ],
                None,
            ),
            created_at: "2026-08-26T00:00:00Z".into(),
            status: PlanStatus::Approved,
        };

        let implementer = Implementer::new(&client);
        let report = implementer.execute_plan(&plan);

        if report.final_status != PlanStatus::RolledBack {
            return (
                false,
                format!(
                    "Expected final status RolledBack, got {:?}",
                    report.final_status
                ),
            );
        }

        // Verify disk content was restored
        let current_content = fs::read_to_string(&target_file).unwrap_or_default();
        if current_content != "original_clean_state\n" {
            return (
                false,
                format!("Canary file not restored by rollback: '{current_content}'"),
            );
        }

        (
            true,
            "Rollback executed cleanly and restored baseline state".into(),
        )
    }
}
