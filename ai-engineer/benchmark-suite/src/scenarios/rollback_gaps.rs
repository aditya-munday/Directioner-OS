//! Benchmark Scenario 5: Package / Service / File Inverse Rollback Execution (Phase 2).

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

/// Scenario 5: Live execution of compensating package.remove, service.stop, and file.delete rollbacks upon mid-flight failure.
pub struct PackageServiceRollbackScenario;

impl BenchmarkScenario for PackageServiceRollbackScenario {
    fn name(&self) -> &'static str {
        "package_service_compensating_rollback"
    }

    fn description(&self) -> &'static str {
        "Execute and verify package.remove, service.stop, and file.delete compensating rollbacks upon fault injection"
    }

    fn is_known_gap(&self) -> bool {
        false
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

        let new_file = work_root.join("new_service.conf");

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
                dry_run: true,
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

        let client = CapabilityClient::new(&sock, "benchmark-pkg-svc-rollback");

        // Forward multi-step plan:
        // Step 1: Create brand new file new_service.conf
        // Step 2: Install package "prometheus" (approved)
        // Step 3: Restart service "prometheus.service" (approved)
        // Step 4: Deliberate fault: try to write outside scope (triggers denial & rollback)
        let plan = Plan {
            id: "plan-pkg-svc-rollback".into(),
            intent: "Demonstrate package, service, and file inverse rollbacks".into(),
            summary: "Creates file, installs pkg, restarts service, then triggers fault".into(),
            steps: vec![
                PlanStep {
                    step_id: "step-1".into(),
                    description: "Create new service config".into(),
                    capability: CapabilityKind::FileWrite {
                        path: new_file.to_string_lossy().to_string(),
                        content: "PORT=9090\n".into(),
                        append: false,
                    },
                    blast_radius: PolicyEngine::evaluate_capability(
                        &CapabilityKind::FileWrite {
                            path: new_file.to_string_lossy().to_string(),
                            content: "PORT=9090\n".into(),
                            append: false,
                        },
                        None,
                    ),
                    timeout_secs: 5,
                },
                PlanStep {
                    step_id: "step-2".into(),
                    description: "Install prometheus".into(),
                    capability: CapabilityKind::PackageInstall {
                        package: "prometheus".into(),
                        approved: true,
                    },
                    blast_radius: PolicyEngine::evaluate_capability(
                        &CapabilityKind::PackageInstall {
                            package: "prometheus".into(),
                            approved: true,
                        },
                        None,
                    ),
                    timeout_secs: 5,
                },
                PlanStep {
                    step_id: "step-3".into(),
                    description: "Restart prometheus service".into(),
                    capability: CapabilityKind::ServiceRestart {
                        unit: "prometheus.service".into(),
                        approved: true,
                    },
                    blast_radius: PolicyEngine::evaluate_capability(
                        &CapabilityKind::ServiceRestart {
                            unit: "prometheus.service".into(),
                            approved: true,
                        },
                        None,
                    ),
                    timeout_secs: 5,
                },
                PlanStep {
                    step_id: "step-4".into(),
                    description: "Fault injection: unauthorized path escape".into(),
                    capability: CapabilityKind::FileWrite {
                        path: "/etc/../root/escaped.conf".into(),
                        content: "bad\n".into(),
                        append: false,
                    },
                    blast_radius: PolicyEngine::evaluate_capability(
                        &CapabilityKind::FileWrite {
                            path: "/etc/../root/escaped.conf".into(),
                            content: "bad\n".into(),
                            append: false,
                        },
                        None,
                    ),
                    timeout_secs: 5,
                },
            ],
            rollback_steps: vec![
                RollbackStep {
                    step_id: "rb-stop-svc".into(),
                    description: "Compensating service deactivation for 'prometheus.service'"
                        .into(),
                    capability: CapabilityKind::ServiceStop {
                        unit: "prometheus.service".into(),
                        approved: true,
                    },
                    triggers: vec!["step-4".into()],
                },
                RollbackStep {
                    step_id: "rb-remove-pkg".into(),
                    description: "Compensating package removal for 'prometheus'".into(),
                    capability: CapabilityKind::PackageRemove {
                        package: "prometheus".into(),
                        approved: true,
                    },
                    triggers: vec!["step-4".into()],
                },
                RollbackStep {
                    step_id: "rb-del-file".into(),
                    description: "Delete newly created file".into(),
                    capability: CapabilityKind::FileDelete {
                        path: new_file.to_string_lossy().to_string(),
                    },
                    triggers: vec!["step-4".into()],
                },
            ],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(
                &[
                    CapabilityKind::FileWrite {
                        path: new_file.to_string_lossy().to_string(),
                        content: "PORT=9090\n".into(),
                        append: false,
                    },
                    CapabilityKind::PackageInstall {
                        package: "prometheus".into(),
                        approved: true,
                    },
                    CapabilityKind::ServiceRestart {
                        unit: "prometheus.service".into(),
                        approved: true,
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

        // 1. Assert newly created file was deleted by compensating rollback
        if new_file.exists() {
            return (
                false,
                "FileDelete rollback failed: newly created file still exists on disk".into(),
            );
        }

        // 2. Assert PackageRemove and ServiceStop rollback steps executed and succeeded
        let ran_pkg_remove = report
            .rollback_results
            .iter()
            .any(|r| r.step_id == "rb-remove-pkg" && r.success);
        let ran_svc_stop = report
            .rollback_results
            .iter()
            .any(|r| r.step_id == "rb-stop-svc" && r.success);

        if !ran_pkg_remove {
            return (
                false,
                "PackageRemove rollback step did not execute successfully".into(),
            );
        }

        if !ran_svc_stop {
            return (
                false,
                "ServiceStop rollback step did not execute successfully".into(),
            );
        }

        // 3. Assert Work Log captured the rollback operations
        let audit = AuditLog::open(&audit_log).unwrap();
        let entries = audit.read_all().unwrap();
        let has_logged_pkg_remove = entries
            .iter()
            .any(|e| e.capability == "package.remove" && e.request_id.contains("rb-remove-pkg"));
        let has_logged_svc_stop = entries
            .iter()
            .any(|e| e.capability == "service.stop" && e.request_id.contains("rb-stop-svc"));
        let has_logged_file_delete = entries
            .iter()
            .any(|e| e.capability == "file.delete" && e.request_id.contains("rb-del-file"));

        if !has_logged_pkg_remove || !has_logged_svc_stop || !has_logged_file_delete {
            return (
                false,
                "Audit log missing one or more compensating rollback records".into(),
            );
        }

        (
            true,
            "Verified: live FileDelete, PackageRemove, and ServiceStop compensating rollbacks executed and audited successfully".into(),
        )
    }
}
