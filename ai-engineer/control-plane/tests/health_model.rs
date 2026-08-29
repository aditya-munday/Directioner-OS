//! Integration tests for Health Model v1 (Phase 3.0.2).

use std::io::Write;
use std::process::Command;

use control_plane::health_model::{
    HealthModel, HealthModelConfig, HealthStatus, SystemHealthReport,
};
use rusqlite::Connection;
use tempfile::NamedTempFile;

fn setup_test_sqlite_db() -> NamedTempFile {
    let file = NamedTempFile::new().unwrap();
    let conn = Connection::open(file.path()).unwrap();

    conn.execute_batch(
        r#"
        CREATE TABLE packages (
            name TEXT PRIMARY KEY,
            version TEXT NOT NULL,
            arch TEXT NOT NULL,
            summary TEXT NOT NULL,
            install_time INTEGER NOT NULL
        );
        CREATE TABLE pkg_requires (
            pkg TEXT NOT NULL,
            dep TEXT NOT NULL,
            flags TEXT NOT NULL,
            version TEXT NOT NULL
        );
        CREATE TABLE pkg_provides (
            pkg TEXT NOT NULL,
            provide TEXT NOT NULL
        );
        CREATE TABLE units (
            name TEXT PRIMARY KEY,
            load_state TEXT NOT NULL,
            active_state TEXT NOT NULL,
            sub_state TEXT NOT NULL,
            enabled TEXT NOT NULL,
            since_ts TEXT NOT NULL,
            updated_ts TEXT NOT NULL
        );
        CREATE TABLE unit_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL,
            unit TEXT NOT NULL,
            kind TEXT NOT NULL,
            old_value TEXT NOT NULL,
            new_value TEXT NOT NULL
        );
        "#,
    )
    .unwrap();

    // Populate packages
    conn.execute(
        "INSERT INTO packages (name, version, arch, summary, install_time) VALUES ('systemd', '257.13', 'x86_64', 'init system', 1700000000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO pkg_requires (pkg, dep, flags, version) VALUES ('systemd', 'glibc', '>=', '2.34')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO pkg_provides (pkg, provide) VALUES ('systemd', 'systemd')",
        [],
    )
    .unwrap();

    // Populate healthy units
    conn.execute(
        "INSERT INTO units (name, load_state, active_state, sub_state, enabled, since_ts, updated_ts) \
         VALUES ('sshd.service', 'loaded', 'active', 'running', 'enabled', '2026-08-29T10:00:00Z', '2026-08-29T10:00:00Z')",
        [],
    )
    .unwrap();

    file
}

#[test]
fn test_health_model_healthy_state_and_cli() {
    let db_file = setup_test_sqlite_db();
    let mut worklog_file = NamedTempFile::new().unwrap();

    // Write a pinned slot record to work log
    writeln!(
        worklog_file,
        r#"{{"seq":1,"timestamp":"2026-08-29T10:00:00Z","capability":"boot_slot.pin","args":{{"slot":0,"approved":true}},"outcome":{{"status":"success"}}}}"#
    )
    .unwrap();

    let config = HealthModelConfig {
        db_path: db_file.path().to_path_buf(),
        worklog_path: Some(worklog_file.path().to_path_buf()),
    };

    let model = HealthModel::new(config);
    let report = model.evaluate();

    assert_eq!(report.overall_status, HealthStatus::Healthy);
    assert_eq!(report.composite_score, Some(100));

    let boot = report.subsystems.iter().find(|s| s.name == "boot").unwrap();
    assert_eq!(boot.status, HealthStatus::Healthy);
    assert_eq!(boot.score, Some(100));

    let services = report.subsystems.iter().find(|s| s.name == "services").unwrap();
    assert_eq!(services.status, HealthStatus::Healthy);
    assert_eq!(services.score, Some(100));

    let packages = report.subsystems.iter().find(|s| s.name == "packages").unwrap();
    assert_eq!(packages.status, HealthStatus::Healthy);
    assert_eq!(packages.score, Some(100));

    // Test ai-health CLI binary execution with --json
    let output = Command::new(env!("CARGO_BIN_EXE_ai-health"))
        .arg("--db")
        .arg(db_file.path())
        .arg("--worklog")
        .arg(worklog_file.path())
        .arg("--json")
        .arg("status")
        .output()
        .expect("Failed to execute ai-health binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: SystemHealthReport = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.overall_status, HealthStatus::Healthy);
    assert_eq!(parsed.composite_score, Some(100));
}

#[test]
fn test_health_model_degraded_crashloop_and_rollback() {
    let db_file = setup_test_sqlite_db();
    let conn = Connection::open(db_file.path()).unwrap();

    // Insert failing unit
    conn.execute(
        "INSERT INTO units (name, load_state, active_state, sub_state, enabled, since_ts, updated_ts) \
         VALUES ('faulty-daemon.service', 'loaded', 'failed', 'failed', 'enabled', '2026-08-29T11:00:00Z', '2026-08-29T11:00:00Z')",
        [],
    )
    .unwrap();

    // Insert multiple transition events to simulate crash-looping
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO unit_events (ts, unit, kind, old_value, new_value) VALUES (?1, 'faulty-daemon.service', 'active', 'activating', 'failed')",
        [&now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO unit_events (ts, unit, kind, old_value, new_value) VALUES (?1, 'faulty-daemon.service', 'active', 'activating', 'failed')",
        [&now],
    )
    .unwrap();

    // Create work log with Tier 1 rollback observation
    let mut worklog_file = NamedTempFile::new().unwrap();
    writeln!(
        worklog_file,
        r#"{{"seq":99,"timestamp":"2026-08-29T11:05:00Z","capability":"boot_watchdog.tier1_rollback_observed","args":{{"trigger":"watchdog: required milestone health-check failed 3 consecutive attempts","action":"automatic rollback to pinned deployment 0"}},"outcome":{{"status":"success"}}}}"#
    )
    .unwrap();

    let config = HealthModelConfig {
        db_path: db_file.path().to_path_buf(),
        worklog_path: Some(worklog_file.path().to_path_buf()),
    };

    let model = HealthModel::new(config);
    let report = model.evaluate();

    assert_eq!(report.overall_status, HealthStatus::Degraded);
    assert!(report.composite_score.unwrap() < 80);

    let boot = report.subsystems.iter().find(|s| s.name == "boot").unwrap();
    assert_eq!(boot.status, HealthStatus::Degraded);
    assert_eq!(boot.score, Some(65));
    assert!(boot.evidence[0].description.contains("Tier 1 autonomous rollback"));

    let services = report.subsystems.iter().find(|s| s.name == "services").unwrap();
    assert_eq!(services.status, HealthStatus::Critical);
    assert_eq!(services.score, Some(60));
    assert_eq!(services.evidence[0].entity, "faulty-daemon.service");
    assert_eq!(services.evidence[0].severity, "critical");

    // Test terminal text rendering via ai-health CLI
    let output = Command::new(env!("CARGO_BIN_EXE_ai-health"))
        .arg("--db")
        .arg(db_file.path())
        .arg("--worklog")
        .arg(worklog_file.path())
        .arg("status")
        .output()
        .expect("Failed to execute ai-health binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("faulty-daemon.service"));
    assert!(stdout.contains("Tier 1 autonomous rollback"));
    assert!(stdout.contains("DEGRADED"));
}
