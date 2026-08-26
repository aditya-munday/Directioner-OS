//! Benchmark Scenario 7: 100% Work Log Auditability.

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
use parking_lot::Mutex;

use crate::BenchmarkScenario;

/// Scenario 7: Verify that 100% of capability requests are durably recorded in the append-only Work Log.
pub struct AuditabilityScenario;

impl BenchmarkScenario for AuditabilityScenario {
    fn name(&self) -> &'static str {
        "worklog_auditability"
    }

    fn description(&self) -> &'static str {
        "Verify 100% of capability operations generate durable, sequential Work Log records"
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

        let client = CapabilityClient::new(&sock, "benchmark-auditor");

        let request_ids = [
            "req-audit-1",
            "req-audit-2",
            "req-audit-3",
            "req-audit-4",
            "req-audit-5",
        ];

        let target_file = work_root.join("audit_test.txt");

        client
            .send(CapabilityKind::Ping, request_ids[0], None)
            .unwrap();
        client
            .send(
                CapabilityKind::FileWrite {
                    path: target_file.to_string_lossy().to_string(),
                    content: "line1\n".into(),
                    append: false,
                },
                request_ids[1],
                None,
            )
            .unwrap();
        client
            .send(
                CapabilityKind::FileRead {
                    path: target_file.to_string_lossy().to_string(),
                },
                request_ids[2],
                None,
            )
            .unwrap();
        client
            .send(
                CapabilityKind::ServiceStatus {
                    unit: "dbus.service".into(),
                },
                request_ids[3],
                None,
            )
            .unwrap();
        client
            .send(CapabilityKind::Ping, request_ids[4], None)
            .unwrap();

        // Read audit log file directly
        let log_content = fs::read_to_string(&audit_log).unwrap_or_default();
        let lines: Vec<&str> = log_content.lines().collect();

        if lines.len() != request_ids.len() {
            return (
                false,
                format!(
                    "Expected {} audit log entries, found {}",
                    request_ids.len(),
                    lines.len()
                ),
            );
        }

        for (i, req_id) in request_ids.iter().enumerate() {
            let line = lines[i];
            let entry: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => return (false, format!("Failed to parse audit log line {i}: {e}")),
            };

            let recorded_req_id = entry.get("request_id").and_then(|r| r.as_str());
            if recorded_req_id != Some(*req_id) {
                return (
                    false,
                    format!(
                        "Audit entry {i} correlation id mismatch: expected '{req_id}', found '{:?}'",
                        recorded_req_id
                    ),
                );
            }

            let seq = entry.get("seq").and_then(|s| s.as_u64());
            if seq != Some(i as u64) {
                return (
                    false,
                    format!(
                        "Audit sequence mismatch at line {i}: expected {}, found {:?}",
                        i, seq
                    ),
                );
            }
        }

        (
            true,
            "100% of requests sequentially and durably recorded".into(),
        )
    }
}
