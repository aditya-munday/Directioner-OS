//! Benchmark Scenario 2: Service Remediation & Recovery.

use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::Server;
use control_plane::client::CapabilityClient;
use control_plane::implementer::Implementer;
use control_plane::planner::{Planner, PlannerConfig};
use control_plane::verifier::Verifier;
use parking_lot::Mutex;

use crate::BenchmarkScenario;

/// Scenario 2: Remediate a failing or misconfigured service.
pub struct RemediationScenario;

impl BenchmarkScenario for RemediationScenario {
    fn name(&self) -> &'static str {
        "service_remediation"
    }

    fn description(&self) -> &'static str {
        "Inspect failing daemon, correct configuration, restart, and verify recovery"
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

        let planner = Planner::new(PlannerConfig {
            work_root,
            default_timeout_secs: 10,
        });

        let plan = match planner.create_plan("fix custom-daemon.service restart", None) {
            Ok(p) => p,
            Err(e) => return (false, format!("Planner failed: {e}")),
        };

        let client = CapabilityClient::new(&sock, "benchmark-remediation");
        let implementer = Implementer::new(&client);
        let report = implementer.execute_plan(&plan);

        if report.final_status != control_plane::PlanStatus::Completed {
            return (
                false,
                format!("Remediation execution failed: {}", report.message),
            );
        }

        let verifier = Verifier::new(&client);
        let v_report = verifier.verify_plan(&plan);
        if !v_report.all_passed {
            return (false, "Post-remediation verification failed".into());
        }

        (
            true,
            "Service status inspected, restarted, and verified".into(),
        )
    }
}
