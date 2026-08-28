//! Benchmark Scenario 12: Real-Time Operational Cockpit Telemetry & Degraded Units.

use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::Server;
use control_plane::cockpit::{CockpitRenderer, CockpitState};
use parking_lot::Mutex;
use world_model::db;

use crate::BenchmarkScenario;

/// Scenario 12: Operational Cockpit health monitoring, alerts, and real-time dashboard rendering.
pub struct CockpitTelemetryScenario;

impl BenchmarkScenario for CockpitTelemetryScenario {
    fn name(&self) -> &'static str {
        "cockpit_telemetry"
    }

    fn description(&self) -> &'static str {
        "Aggregate system health, detect degraded units, inspect killswitch, and render telemetry"
    }

    fn run(&self) -> (bool, String) {
        let temp = match tempfile::tempdir() {
            Ok(t) => t,
            Err(e) => return (false, format!("Failed to create tempdir: {e}")),
        };

        let db_path = temp.path().join("world_model.db");
        let sock = temp.path().join("cap.sock");
        let audit_log = temp.path().join("audit.jsonl");
        let lock_file = temp.path().join("kill.lock");
        let work_root = temp.path().join("scope");
        fs::create_dir_all(&work_root).unwrap();

        // 1. Initialize DB with a degraded/failed systemd unit
        let conn = db::open(&db_path).unwrap();

        conn.execute(
            "INSERT INTO units (name, load_state, active_state, sub_state, enabled, since_ts)
             VALUES ('network-fault.service', 'loaded', 'failed', 'failed', 'enabled', '2026-08-28T00:00:00Z')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO unit_events (unit, kind, old_value, new_value, ts)
             VALUES ('network-fault.service', 'sub', 'running', 'failed', '2026-08-28T00:00:00Z')",
            [],
        )
        .unwrap();

        // 2. Set up capability daemon and engage killswitch
        let h_scope = ScopedPath::new(&work_root).unwrap();
        let h_audit = AuditLog::open(&audit_log).unwrap();
        let mut h_kill = KillSwitch::new(&lock_file);
        h_kill.engage().unwrap();

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

        // 3. Initialize CockpitState and poll telemetry
        let cockpit = CockpitState::new(db_path, audit_log, lock_file);

        let snapshot = cockpit.poll(None);

        if snapshot.health.failed_unit_count != 1
            || snapshot.health.failed_units.is_empty()
            || snapshot.health.failed_units[0] != "network-fault.service"
        {
            return (
                false,
                format!(
                    "Failed to record degraded unit in health summary (got count {}, units {:?})",
                    snapshot.health.failed_unit_count, snapshot.health.failed_units
                ),
            );
        }

        if !snapshot.killswitch_engaged {
            return (false, "Cockpit failed to detect engaged killswitch".into());
        }

        // 4. Test terminal and JSON rendering
        let rendered_term = CockpitRenderer::render_terminal(&snapshot);
        let rendered_json = serde_json::to_string_pretty(&snapshot).unwrap();

        if !rendered_term.contains("DEGRADED") && !rendered_term.contains("network-fault.service") {
            return (
                false,
                "Rendered terminal output missing degraded status indicator".into(),
            );
        }

        if !rendered_json.contains("\"failed_unit_count\": 1") {
            return (false, "Rendered JSON output missing degraded status".into());
        }

        (
            true,
            "Cockpit telemetry captured degraded units, killswitch state, and rendered dashboards"
                .into(),
        )
    }
}
