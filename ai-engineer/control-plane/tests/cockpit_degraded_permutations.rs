//! Extended Degraded-System Permutation Tests for CockpitState & Health Model (Phase 3 / Hardening).
//!
//! Evaluates:
//! 1. Multiple simultaneous subsystem failures across services.
//! 2. Missing telemetry sources (absent SQLite DB, missing audit log, missing lock file).
//! 3. Malformed and truncated audit log recovery.
//! 4. Flapping services with multi-signal Incident Response posture integration.
//! 5. Exact JSON schema stability and roundtrip fidelity.

use capability_layer::killswitch::KillSwitch;
use control_plane::cockpit::{CockpitRenderer, CockpitSnapshot, CockpitState};
use control_plane::health_model::{HealthModel, HealthModelConfig, HealthStatus};
use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_cockpit_multiple_simultaneous_subsystem_failures() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("worldmodel.db");
    let audit_path = temp.path().join("audit.jsonl");
    let lock_path = temp.path().join("kill.lock");

    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE packages (id INTEGER PRIMARY KEY, name TEXT, epoch INTEGER, version TEXT, release TEXT, arch TEXT, summary TEXT, updated_at TEXT);
        CREATE TABLE units (id INTEGER PRIMARY KEY, name TEXT UNIQUE, load_state TEXT, active_state TEXT, sub_state TEXT, unit_file_state TEXT, description TEXT, updated_at TEXT);
        INSERT INTO packages (name, epoch, version, release, arch, summary, updated_at) VALUES ('systemd', 0, '256', '1.fc41', 'x86_64', 'Init', '2026-08-31T00:00:00Z');
        INSERT INTO units (name, load_state, active_state, sub_state, unit_file_state, description, updated_at) VALUES
            ('firewalld.service', 'loaded', 'failed', 'failed', 'enabled', 'Firewall Daemon', '2026-08-31T00:00:00Z'),
            ('postgresql.service', 'loaded', 'failed', 'failed', 'enabled', 'PostgreSQL DB', '2026-08-31T00:00:00Z'),
            ('sssd.service', 'loaded', 'failed', 'failed', 'enabled', 'Auth Daemon', '2026-08-31T00:00:00Z'),
            ('iscsid.service', 'loaded', 'failed', 'failed', 'enabled', 'Storage Daemon', '2026-08-31T00:00:00Z'),
            ('nginx.service', 'loaded', 'failed', 'failed', 'enabled', 'Reverse Proxy', '2026-08-31T00:00:00Z'),
            ('systemd-journald.service', 'loaded', 'active', 'running', 'enabled', 'Journal Service', '2026-08-31T00:00:00Z');
        "#,
    ).unwrap();

    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(None);

    assert_eq!(snapshot.health.total_packages, 1);
    assert_eq!(snapshot.health.total_units, 6);
    assert_eq!(snapshot.health.failed_unit_count, 5);
    assert!(
        snapshot
            .health
            .failed_units
            .contains(&"firewalld.service".to_string())
    );
    assert!(
        snapshot
            .health
            .failed_units
            .contains(&"postgresql.service".to_string())
    );
    assert!(
        snapshot
            .health
            .failed_units
            .contains(&"sssd.service".to_string())
    );
    assert!(
        snapshot
            .health
            .failed_units
            .contains(&"iscsid.service".to_string())
    );
    assert!(
        snapshot
            .health
            .failed_units
            .contains(&"nginx.service".to_string())
    );

    let rendered = CockpitRenderer::render_terminal(&snapshot);
    assert!(rendered.contains("Degraded Units:   ✘ 5 FAILED"));
}

#[test]
fn test_cockpit_missing_telemetry_sources_graceful_handling() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("nonexistent_db.sqlite");
    let audit_path = temp.path().join("nonexistent_audit.jsonl");
    let lock_path = temp.path().join("nonexistent_kill.lock");

    // Zero telemetry files present on disk
    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(None);

    // Fail closed with safe zeroes, NO panic
    assert!(!snapshot.killswitch_engaged);
    assert_eq!(snapshot.health.total_packages, 0);
    assert_eq!(snapshot.health.total_units, 0);
    assert_eq!(snapshot.health.failed_unit_count, 0);
    assert!(snapshot.health.failed_units.is_empty());
    assert!(snapshot.recent_audit.is_empty());
    assert!(snapshot.active_plan.is_none());

    let rendered = CockpitRenderer::render_terminal(&snapshot);
    assert!(rendered.contains("DIRECTIONER-OS — OPERATIONAL COCKPIT"));
}

#[test]
fn test_cockpit_corrupt_and_truncated_audit_log_recovery() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("worldmodel.db");
    let audit_path = temp.path().join("corrupt_audit.jsonl");
    let lock_path = temp.path().join("kill.lock");

    let valid_entry = serde_json::json!({
        "seq": 500,
        "timestamp": "2026-08-31T12:00:00Z",
        "caller": "ai-engineer",
        "capability": { "kind": "ServiceRestart" },
        "response": { "type": "Success" },
        "correlation_id": "corr-500"
    });

    let raw_audit_content = format!(
        "\n\n{}\n{{ \"truncated\": true \nNOT_JSON_AT_ALL\n{}\n",
        serde_json::to_string(&valid_entry).unwrap(),
        serde_json::to_string(&serde_json::json!({
            "seq": 501,
            "timestamp": "2026-08-31T12:01:00Z",
            "caller": "ai-engineer",
            "capability": { "kind": "KernelParamSet" },
            "response": { "type": "Denied" },
            "correlation_id": "corr-501"
        }))
        .unwrap()
    );

    fs::write(&audit_path, raw_audit_content).unwrap();

    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(None);

    assert_eq!(snapshot.recent_audit.len(), 2);
    assert_eq!(snapshot.recent_audit[0].seq, 501); // newest first
    assert_eq!(snapshot.recent_audit[1].seq, 500);
}

#[test]
fn test_cockpit_flapping_services_multi_signal_health_scoring() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("worldmodel.db");
    let audit_path = temp.path().join("audit.jsonl");
    let lock_path = temp.path().join("kill.lock");

    let now_ts = chrono::Utc::now().to_rfc3339();

    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        &format!(r#"
        CREATE TABLE packages (name TEXT PRIMARY KEY, evr TEXT, arch TEXT, summary TEXT, synced_at TEXT);
        CREATE TABLE units (name TEXT PRIMARY KEY, load_state TEXT, active_state TEXT, sub_state TEXT, enabled TEXT, since_ts TEXT);
        CREATE TABLE unit_events (id INTEGER PRIMARY KEY, ts TEXT, unit TEXT, kind TEXT, old_value TEXT, new_value TEXT);

        INSERT INTO units (name, load_state, active_state, sub_state, enabled, since_ts)
        VALUES ('flapping-agent.service', 'loaded', 'failed', 'failed', 'enabled', '{now_ts}');

        -- Multiple restart flapping failure events within past hour
        INSERT INTO unit_events (ts, unit, kind, old_value, new_value) VALUES
            ('{now_ts}', 'flapping-agent.service', 'active', 'active', 'failed'),
            ('{now_ts}', 'flapping-agent.service', 'active', 'active', 'failed'),
            ('{now_ts}', 'flapping-agent.service', 'active', 'active', 'failed');
        "#),
    ).unwrap();

    let health_evaluator = HealthModel::new(HealthModelConfig {
        db_path: db_path.clone(),
        worklog_path: None,
    });
    let report = health_evaluator.evaluate();

    assert!(matches!(
        report.overall_status,
        HealthStatus::Degraded | HealthStatus::Critical
    ));
    let services_subsystem = report
        .subsystems
        .iter()
        .find(|s| s.name == "services")
        .expect("services subsystem must be present");
    assert!(matches!(
        services_subsystem.status,
        HealthStatus::Degraded | HealthStatus::Critical
    ));
    assert!(
        services_subsystem
            .evidence
            .iter()
            .any(|e| e.entity == "flapping-agent.service")
    );

    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(None);
    assert_eq!(snapshot.health.failed_unit_count, 1);
    assert_eq!(snapshot.health.failed_units, vec!["flapping-agent.service"]);
}

#[test]
fn test_cockpit_json_schema_stability_and_roundtrip() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("worldmodel.db");
    let audit_path = temp.path().join("audit.jsonl");
    let lock_path = temp.path().join("kill.lock");

    let mut killswitch = KillSwitch::new(&lock_path);
    killswitch.engage().unwrap();

    let entry = serde_json::json!({
        "seq": 42,
        "timestamp": "2026-08-31T12:30:00Z",
        "caller": "ai-engineer",
        "capability": { "kind": "FileWrite" },
        "response": { "type": "Success" },
        "correlation_id": "corr-42"
    });
    fs::write(
        &audit_path,
        format!("{}\n", serde_json::to_string(&entry).unwrap()),
    )
    .unwrap();

    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(None);

    let serialized_json = serde_json::to_string_pretty(&snapshot).unwrap();

    // Verify presence of canonical top-level fields
    let parsed: serde_json::Value = serde_json::from_str(&serialized_json).unwrap();
    assert!(parsed.get("timestamp").is_some());
    assert_eq!(parsed.get("killswitch_engaged").unwrap(), true);
    assert!(parsed.get("health").is_some());
    assert!(parsed.get("recent_audit").is_some());

    // Roundtrip back into typed CockpitSnapshot struct
    let deserialized: CockpitSnapshot = serde_json::from_str(&serialized_json).unwrap();
    assert!(deserialized.killswitch_engaged);
    assert_eq!(deserialized.recent_audit.len(), 1);
    assert_eq!(deserialized.recent_audit[0].seq, 42);
    assert_eq!(deserialized.recent_audit[0].status, "Success");
}
