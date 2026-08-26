//! Benchmark Scenario 5: Policy Gate Enforcement.

use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::Server;
use capability_layer::{CapabilityKind, CapabilityResponse};
use control_plane::client::CapabilityClient;
use parking_lot::Mutex;

use crate::BenchmarkScenario;

/// Scenario 5: Verify that protected units and unapproved operations are rejected.
pub struct BlastRadiusScenario;

impl BenchmarkScenario for BlastRadiusScenario {
    fn name(&self) -> &'static str {
        "policy_gate_enforcement"
    }

    fn description(&self) -> &'static str {
        "Ensure protected system daemons (ssh) and unapproved changes are strictly denied"
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
                protected_units: vec!["ssh.service".into(), "systemd-udevd.service".into()],
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

        let client = CapabilityClient::new(&sock, "benchmark-blast-radius");

        // 1. Attempt to restart protected ssh.service even with approved=true
        let resp = client.send(
            CapabilityKind::ServiceRestart {
                unit: "ssh.service".into(),
                approved: true,
            },
            "req-protected-ssh",
            None,
        );

        match resp {
            Ok(CapabilityResponse::Denied { reason, .. }) => {
                if !reason.contains("protected") {
                    return (
                        false,
                        format!("Denied reason did not cite protected unit: '{reason}'"),
                    );
                }
            }
            Ok(other) => {
                return (
                    false,
                    format!(
                        "Expected Denied response for protected service, got: {:?}",
                        other
                    ),
                );
            }
            Err(e) => return (false, format!("Client error: {e}")),
        }

        // 2. Attempt unapproved package install
        let pkg_resp = client.send(
            CapabilityKind::PackageInstall {
                package: "curl".into(),
                approved: false,
            },
            "req-unapproved-pkg",
            None,
        );

        match pkg_resp {
            Ok(CapabilityResponse::Denied { reason, .. }) => {
                if !reason.contains("approval") {
                    return (
                        false,
                        format!("Denied reason did not cite approval requirement: '{reason}'"),
                    );
                }
            }
            Ok(other) => {
                return (
                    false,
                    format!(
                        "Expected Denied response for unapproved package, got: {:?}",
                        other
                    ),
                );
            }
            Err(e) => return (false, format!("Client error: {e}")),
        }

        (
            true,
            "Protected units and unapproved actions successfully blocked".into(),
        )
    }
}
