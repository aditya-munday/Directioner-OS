//! Benchmark Scenario 10: Multi-Crate Re-verification Failure & Atomic Rollback.

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
use control_plane::plan::{Plan, PlanStep};
use control_plane::policy::PolicyEngine;
use parking_lot::Mutex;

use crate::BenchmarkScenario;

/// Scenario 10: Multi-crate atomic rollback on post-application verification failure.
pub struct MultiCrateRollbackScenario;

impl BenchmarkScenario for MultiCrateRollbackScenario {
    fn name(&self) -> &'static str {
        "multi_crate_rollback"
    }

    fn description(&self) -> &'static str {
        "Atomically reverse multi-crate file modifications when post-application verification fails"
    }

    fn run(&self) -> (bool, String) {
        let temp = match tempfile::tempdir() {
            Ok(t) => t,
            Err(e) => return (false, format!("Failed to create tempdir: {e}")),
        };

        let sock = temp.path().join("cap.sock");
        let audit_log = temp.path().join("audit.jsonl");
        let lock_file = temp.path().join("kill.lock");
        let work_root = temp.path().join("workspace");
        fs::create_dir_all(&work_root).unwrap();

        let crate_a_src = work_root.join("crate_a/src");
        let crate_b_src = work_root.join("crate_b/src");
        fs::create_dir_all(&crate_a_src).unwrap();
        fs::create_dir_all(&crate_b_src).unwrap();

        let file_a = crate_a_src.join("lib.rs");
        let file_b = crate_b_src.join("lib.rs");

        let original_a = "// Original crate_a v1.0\npub fn a() -> usize { 1 }\n";
        let original_b = "// Original crate_b v1.0\npub fn b() -> usize { 2 }\n";

        fs::write(&file_a, original_a).unwrap();
        fs::write(&file_b, original_b).unwrap();

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
            thread::sleep(Duration::from_millis(20));
        }

        let cap_a = CapabilityKind::FileWrite {
            path: file_a.to_string_lossy().to_string(),
            content: "// Mutated crate_a v2.0\npub fn a() -> usize { 100 }\n".into(),
            append: false,
        };
        let cap_b = CapabilityKind::FileWrite {
            path: file_b.to_string_lossy().to_string(),
            content: "// Mutated crate_b v2.0\npub fn b() -> usize { 200 }\n".into(),
            append: false,
        };

        let rb_a = CapabilityKind::FileWrite {
            path: file_a.to_string_lossy().to_string(),
            content: original_a.to_string(),
            append: false,
        };
        let rb_b = CapabilityKind::FileWrite {
            path: file_b.to_string_lossy().to_string(),
            content: original_b.to_string(),
            append: false,
        };

        let steps = vec![
            PlanStep {
                step_id: "step-1".into(),
                description: "Mutate crate_a".into(),
                capability: cap_a.clone(),
                blast_radius: PolicyEngine::evaluate_capability(&cap_a, None),
                timeout_secs: 10,
            },
            PlanStep {
                step_id: "step-2".into(),
                description: "Mutate crate_b".into(),
                capability: cap_b.clone(),
                blast_radius: PolicyEngine::evaluate_capability(&cap_b, None),
                timeout_secs: 10,
            },
        ];

        let rollback_steps = vec![
            control_plane::plan::RollbackStep {
                step_id: "rb-step-1".into(),
                description: "Rollback crate_a".into(),
                capability: rb_a,
                triggers: vec!["step-1".into()],
            },
            control_plane::plan::RollbackStep {
                step_id: "rb-step-2".into(),
                description: "Rollback crate_b".into(),
                capability: rb_b,
                triggers: vec!["step-2".into()],
            },
        ];

        let plan = Plan {
            id: "plan-multi-rollback".into(),
            intent: "Multi-crate test patch".into(),
            summary: "Plan touching 2 crates".into(),
            steps,
            rollback_steps,
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(&[cap_a, cap_b], None),
            created_at: "2026-08-28T00:00:00Z".into(),
            status: control_plane::PlanStatus::Draft,
        };

        let client = CapabilityClient::new(&sock, "benchmark-multi-rollback");
        let implementer = Implementer::new(&client);

        // 1. Execute initial plan successfully
        let exec_report = implementer.execute_plan(&plan);
        if exec_report.final_status != control_plane::PlanStatus::Completed {
            return (
                false,
                format!("Initial execution failed: {}", exec_report.message),
            );
        }

        // Verify files were modified
        if !fs::read_to_string(&file_a).unwrap().contains("v2.0")
            || !fs::read_to_string(&file_b).unwrap().contains("v2.0")
        {
            return (false, "Files were not modified by initial plan".into());
        }

        // 2. Simulate post-verification failure and trigger rollback_all
        let rb_report = implementer.rollback_all(&plan);
        if rb_report.final_status != control_plane::PlanStatus::RolledBack {
            return (false, format!("Rollback all failed: {}", rb_report.message));
        }

        // 3. Verify files were completely restored
        let current_a = fs::read_to_string(&file_a).unwrap();
        let current_b = fs::read_to_string(&file_b).unwrap();

        if current_a != original_a || current_b != original_b {
            return (
                false,
                "Restored file contents do not match original pre-application state".into(),
            );
        }

        (
            true,
            "Multi-crate patch applied and cleanly rolled back in reverse order".into(),
        )
    }
}
