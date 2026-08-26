//! Benchmark Scenario 6: Kill-Switch Latency and Hard Halt.

use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::Server;
use capability_layer::{CapabilityKind, CapabilityResponse};
use control_plane::client::CapabilityClient;
use parking_lot::Mutex;

use crate::BenchmarkScenario;

/// Scenario 6: Verify kill-switch immediately denies all actions across independent processes.
pub struct KillswitchScenario;

impl BenchmarkScenario for KillswitchScenario {
    fn name(&self) -> &'static str {
        "killswitch_hard_halt"
    }

    fn description(&self) -> &'static str {
        "Engage kill-switch mid-flight and assert all AI capability calls are denied < 1s"
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

        let client = CapabilityClient::new(&sock, "benchmark-killswitch");

        // First call when killswitch is disengaged -> should succeed
        if !client.ping().unwrap_or(false) {
            return (false, "Initial ping probe failed".into());
        }

        // Engage kill-switch externally
        let mut external_ks = KillSwitch::new(&lock_file);
        let start = Instant::now();
        if let Err(e) = external_ks.engage() {
            return (false, format!("Failed to engage killswitch: {e}"));
        }

        // Send ping probe immediately
        let resp = client.send(CapabilityKind::Ping, "probe-after-kill", None);
        let elapsed = start.elapsed();

        match resp {
            Ok(CapabilityResponse::Denied { reason, .. }) => {
                if !reason.contains("kill-switch") {
                    return (
                        false,
                        format!("Denied response did not mention kill-switch: '{reason}'"),
                    );
                }
            }
            Ok(other) => {
                return (
                    false,
                    format!(
                        "Expected Denied response when kill-switch engaged, got: {:?}",
                        other
                    ),
                );
            }
            Err(e) => return (false, format!("Client IPC error: {e}")),
        }

        if elapsed > Duration::from_secs(1) {
            return (
                false,
                format!("Killswitch latency exceeded 1 second: {:?}", elapsed),
            );
        }

        // Release kill-switch and confirm recovery
        if let Err(e) = external_ks.release() {
            return (false, format!("Failed to release killswitch: {e}"));
        }

        if !client.ping().unwrap_or(false) {
            return (
                false,
                "Ping did not recover after killswitch release".into(),
            );
        }

        (
            true,
            format!(
                "Killswitch engaged, halted IPC in {} µs, and recovered cleanly",
                elapsed.as_micros()
            ),
        )
    }
}
