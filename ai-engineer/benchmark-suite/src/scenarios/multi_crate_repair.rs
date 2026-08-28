//! Benchmark Scenario 9: Cross-Crate Compilation Repair (Phase 2).

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
use control_plane::code_engine::{CodeEngine, Diagnostic, DiagnosticSpan};
use control_plane::implementer::Implementer;
use control_plane::verifier::Verifier;
use parking_lot::Mutex;

use crate::BenchmarkScenario;

/// Scenario 9: Multi-crate compilation error localization and automated repair.
pub struct MultiCrateRepairScenario;

impl BenchmarkScenario for MultiCrateRepairScenario {
    fn name(&self) -> &'static str {
        "multi_crate_repair"
    }

    fn description(&self) -> &'static str {
        "Localize diagnostic across crate boundary and apply machine suggestion patch"
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

        let crate_a_lib = crate_a_src.join("lib.rs");
        let crate_b_main = crate_b_src.join("main.rs");

        fs::write(
            &crate_a_lib,
            "pub fn compute(val: i32) -> i32 {\n    val * 2\n}\n",
        )
        .unwrap();
        fs::write(
            &crate_b_main,
            "fn main() {\n    let _ = crate_a::compute(10);\n}\n",
        )
        .unwrap();

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

        let engine = CodeEngine::new(&work_root);

        let diag = Diagnostic {
            level: "error".into(),
            code: Some("E0061".into()),
            message: "this function takes 1 argument but 0 were supplied".into(),
            file_path: Some("crate_b/src/main.rs".into()),
            line_number: Some(2),
            column_number: Some(13),
            suggested_replacement: None,
            spans: vec![
                DiagnosticSpan {
                    file_path: "crate_b/src/main.rs".into(),
                    line_start: 2,
                    line_end: 2,
                    column_start: 13,
                    column_end: 32,
                    is_primary: true,
                    label: Some("call takes 1 arg".into()),
                    suggested_replacement: Some("crate_a::compute(42)".into()),
                },
                DiagnosticSpan {
                    file_path: "crate_a/src/lib.rs".into(),
                    line_start: 1,
                    line_end: 1,
                    column_start: 1,
                    column_end: 33,
                    is_primary: false,
                    label: Some("defined here".into()),
                    suggested_replacement: None,
                },
            ],
            notes: vec![],
        };

        let plan = match engine.synthesize_cross_crate_plan(&[diag], None) {
            Some(p) => p,
            None => {
                return (
                    false,
                    "Failed to synthesize cross-crate remediation plan".into(),
                );
            }
        };

        let client = CapabilityClient::new(&sock, "benchmark-cross-crate");
        let implementer = Implementer::new(&client);
        let exec_report = implementer.execute_plan(&plan);

        if exec_report.final_status != control_plane::PlanStatus::Completed {
            return (false, format!("Execution failed: {}", exec_report.message));
        }

        let verifier = Verifier::new(&client);
        let v_report = verifier.verify_plan(&plan);
        if !v_report.all_passed {
            return (false, "Post-remediation verification failed".into());
        }

        let updated_main = fs::read_to_string(&crate_b_main).unwrap();
        if !updated_main.contains("crate_a::compute(42)") {
            return (
                false,
                "File was not correctly patched with suggested replacement".into(),
            );
        }

        (
            true,
            "Cross-crate diagnostic localized, patched, and verified".into(),
        )
    }
}
