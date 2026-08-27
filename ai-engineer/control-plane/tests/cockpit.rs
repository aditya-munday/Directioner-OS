//! Integration tests for Directioner-OS Cockpit & Dashboard (Phase 3).

use capability_layer::killswitch::KillSwitch;
use control_plane::cockpit::{CockpitRenderer, CockpitState};
use control_plane::planner::{Planner, PlannerConfig};
use rusqlite::Connection;
use std::fs;

#[test]
fn test_cockpit_full_health_aggregation() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("worldmodel.db");
    let audit_path = temp.path().join("audit.jsonl");
    let lock_path = temp.path().join("kill.lock");

    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE packages (id INTEGER PRIMARY KEY, name TEXT, epoch INTEGER, version TEXT, release TEXT, arch TEXT, summary TEXT, updated_at TEXT);
        CREATE TABLE units (id INTEGER PRIMARY KEY, name TEXT UNIQUE, load_state TEXT, active_state TEXT, sub_state TEXT, unit_file_state TEXT, description TEXT, updated_at TEXT);
        INSERT INTO packages (name, epoch, version, release, arch, summary, updated_at) VALUES ('systemd', 0, '256', '1.fc41', 'x86_64', 'System and Service Manager', '2026-08-27T00:00:00Z');
        INSERT INTO packages (name, epoch, version, release, arch, summary, updated_at) VALUES ('cockpit', 0, '310', '1.fc41', 'x86_64', 'Web Console', '2026-08-27T00:00:00Z');
        INSERT INTO units (name, load_state, active_state, sub_state, unit_file_state, description, updated_at) VALUES ('cockpit.service', 'loaded', 'active', 'running', 'enabled', 'Cockpit Web Service', '2026-08-27T00:00:00Z');
        INSERT INTO units (name, load_state, active_state, sub_state, unit_file_state, description, updated_at) VALUES ('auditd.service', 'loaded', 'failed', 'failed', 'enabled', 'Security Audit Daemon', '2026-08-27T00:00:00Z');
        "#,
    ).unwrap();

    let entry1 = serde_json::json!({
        "seq": 101,
        "timestamp": "2026-08-27T10:00:00Z",
        "caller": "ai-engineer",
        "capability": { "kind": "FileWrite" },
        "response": { "type": "Success" },
        "correlation_id": "plan-1:step-1"
    });
    let entry2 = serde_json::json!({
        "seq": 102,
        "timestamp": "2026-08-27T10:01:00Z",
        "caller": "ai-engineer",
        "capability": { "kind": "PackageInstall" },
        "response": { "type": "Denied" },
        "correlation_id": "plan-1:step-2"
    });

    fs::write(
        &audit_path,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&entry1).unwrap(),
            serde_json::to_string(&entry2).unwrap()
        ),
    )
    .unwrap();

    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(None);

    assert!(!snapshot.killswitch_engaged);
    assert_eq!(snapshot.health.total_packages, 2);
    assert_eq!(snapshot.health.total_units, 2);
    assert_eq!(snapshot.health.failed_unit_count, 1);
    assert_eq!(snapshot.health.failed_units, vec!["auditd.service"]);
    assert_eq!(snapshot.recent_audit.len(), 2);
    assert_eq!(snapshot.recent_audit[0].seq, 102); // Newest first
    assert_eq!(snapshot.recent_audit[1].seq, 101);

    let rendered = CockpitRenderer::render_terminal(&snapshot);
    assert!(rendered.contains("Packages Tracked: 2"));
    assert!(rendered.contains("Systemd Units:    2 total"));
    assert!(rendered.contains("Degraded Units:   ✘ 1 FAILED (auditd.service)"));
    assert!(rendered.contains("#102  ✘ [DENIED]"));
    assert!(rendered.contains("#101  ✔"));
}

#[test]
fn test_cockpit_killswitch_engaged_rendering() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("worldmodel.db");
    let audit_path = temp.path().join("audit.jsonl");
    let lock_path = temp.path().join("kill.lock");

    let mut killswitch = KillSwitch::new(&lock_path);
    killswitch.engage().unwrap();

    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(None);

    assert!(snapshot.killswitch_engaged);
    let rendered = CockpitRenderer::render_terminal(&snapshot);
    assert!(rendered.contains("KILLSWITCH ENGAGED - ALL DISPATCH BLOCKED"));
}

#[test]
fn test_cockpit_active_plan_rendering() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("worldmodel.db");
    let audit_path = temp.path().join("audit.jsonl");
    let lock_path = temp.path().join("kill.lock");

    let planner = Planner::new(PlannerConfig {
        work_root: temp.path().to_path_buf(),
        default_timeout_secs: 10,
    });
    let plan = planner.create_plan("restart auditd service", None).unwrap();

    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(Some(plan));

    assert!(snapshot.active_plan.is_some());
    let rendered = CockpitRenderer::render_terminal(&snapshot);
    assert!(rendered.contains("ACTIVE REMEDIATION PLAN:"));
    assert!(rendered.contains("restart auditd service"));
}
