//! Accessibility & JSON Output Completeness Test Suite for all Directioner-OS CLIs.
//!
//! Validates that every CLI binary (`ai-engineer`, `ai-worklog`, `ai-cockpit`, `ai-health`,
//! `ai-worldmodel`, `ai-hwcheck`) supports the `--json` flag and produces strictly valid,
//! parseable JSON output across its subcommands.

use capability_layer::audit::{AuditEntry, AuditLog, AuditOutcome};
use std::fs;
use std::process::Command;
use tempfile::tempdir;
use world_model::{db, model::CompatibilityProfile, sync};

fn find_binary(name: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap_or(&manifest_dir);
    let target_dir = workspace_root.join("target/debug").join(name);
    if target_dir.exists() {
        return target_dir;
    }
    let _ = Command::new("cargo")
        .args(["build", "--bin", name])
        .current_dir(workspace_root)
        .status();
    if target_dir.exists() {
        target_dir
    } else {
        std::path::PathBuf::from(name)
    }
}

#[test]
fn test_accessibility_json_hwcheck() {
    let tmp = tempdir().unwrap();
    let sysfs = tmp.path().join("sys");
    let etc = tmp.path().join("etc");
    let fw = tmp.path().join("lib/firmware");
    fs::create_dir_all(&sysfs).unwrap();
    fs::create_dir_all(&etc).unwrap();
    fs::create_dir_all(&fw).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ai-hwcheck"))
        .arg("--sysfs")
        .arg(&sysfs)
        .arg("--etc")
        .arg(&etc)
        .arg("--firmware")
        .arg(&fw)
        .arg("--json")
        .output()
        .expect("Failed to run ai-hwcheck");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value =
        serde_json::from_str(&stdout).expect("ai-hwcheck --json output must be valid JSON");

    assert!(val.get("wifi").is_some());
    assert!(val.get("fingerprint").is_some());
    assert!(val.get("sleep_wake").is_some());
    assert!(val.get("overall_status").is_some());
    assert!(val.get("recommendations").is_some());
}

#[test]
fn test_accessibility_json_worklog() {
    let tmp = tempdir().unwrap();
    let log_path = tmp.path().join("test_worklog.jsonl");

    // Write valid audit log entries using AuditLog::open
    {
        let mut log = AuditLog::open(&log_path).unwrap();
        let entry = AuditEntry {
            seq: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id: "req-001".into(),
            capability: "file.write".into(),
            args: serde_json::json!({"path": "/etc/test.conf"}),
            requester: "ai-engineer".into(),
            token: "tok-test".into(),
            is_rollback: false,
            outcome: AuditOutcome::Success {
                result: serde_json::json!({"bytes_written": 42}),
            },
            duration_ms: 5,
            prev_hash: None,
            entry_hash: None,
        };
        log.append(entry).unwrap();
    }

    // 1. tail --json
    let output = Command::new(find_binary("ai-worklog"))
        .arg("--log")
        .arg(&log_path)
        .arg("--json")
        .arg("tail")
        .output()
        .expect("Failed to run ai-worklog tail --json");

    assert!(
        output.status.success(),
        "ai-worklog tail failed: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value =
        serde_json::from_str(&stdout).expect("ai-worklog tail --json output must be valid JSON");

    assert!(val.is_array());
    assert_eq!(val[0]["seq"], 0);
    assert_eq!(val[0]["capability"], "file.write");

    // 2. show --json
    let show_output = Command::new(find_binary("ai-worklog"))
        .arg("--log")
        .arg(&log_path)
        .arg("--json")
        .arg("show")
        .arg("0")
        .output()
        .expect("Failed to run ai-worklog show --json");

    assert!(
        show_output.status.success(),
        "ai-worklog show failed: {:?}",
        show_output
    );
    let show_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&show_output.stdout))
            .expect("ai-worklog show --json output must be valid JSON");
    assert_eq!(show_val["request_id"], "req-001");

    // 3. search --json
    let search_output = Command::new(find_binary("ai-worklog"))
        .arg("--log")
        .arg(&log_path)
        .arg("--json")
        .arg("search")
        .arg("test.conf")
        .output()
        .expect("Failed to run ai-worklog search --json");

    assert!(search_output.status.success());
    let search_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&search_output.stdout))
            .expect("ai-worklog search --json output must be valid JSON");
    assert_eq!(search_val[0]["seq"], 0);
}

#[test]
fn test_accessibility_json_worldmodel() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_wm.sqlite");
    let conn = db::open(&db_path).unwrap();

    let profiles = vec![CompatibilityProfile {
        app_id: "steam:730".into(),
        app_title: "CS2".into(),
        runner_version: "Proton-9.0".into(),
        tier: "Platinum".into(),
        dxvk_version: Some("2.4".into()),
        vkd3d_version: None,
        env_overrides: None,
        dll_overrides: None,
        notes: None,
    }];
    sync::sync_compatibility_profiles(&conn, &profiles).unwrap();

    // 1. stats --json
    let stats_out = Command::new(find_binary("ai-worldmodel"))
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .arg("stats")
        .output()
        .expect("Failed to run ai-worldmodel stats --json");

    assert!(stats_out.status.success());
    let stats_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&stats_out.stdout))
            .expect("ai-worldmodel stats --json must be valid JSON");
    assert_eq!(stats_val["compatibility_profiles"], 1);

    // 2. compat --json
    let compat_out = Command::new(find_binary("ai-worldmodel"))
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .arg("compat")
        .output()
        .expect("Failed to run ai-worldmodel compat --json");

    assert!(compat_out.status.success());
    let compat_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&compat_out.stdout))
            .expect("ai-worldmodel compat --json must be valid JSON");
    assert!(compat_val.is_array());
    assert_eq!(compat_val[0]["app_id"], "steam:730");

    // 3. disks --json
    let disks_out = Command::new(find_binary("ai-worldmodel"))
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .arg("disks")
        .output()
        .expect("Failed to run ai-worldmodel disks --json");
    assert!(disks_out.status.success());
    assert!(
        serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&disks_out.stdout))
            .is_ok()
    );

    // 4. thermals --json
    let thermals_out = Command::new(find_binary("ai-worldmodel"))
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .arg("thermals")
        .output()
        .expect("Failed to run ai-worldmodel thermals --json");
    assert!(thermals_out.status.success());
    assert!(
        serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&thermals_out.stdout))
            .is_ok()
    );

    // 5. net --json
    let net_out = Command::new(find_binary("ai-worldmodel"))
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .arg("net")
        .output()
        .expect("Failed to run ai-worldmodel net --json");
    assert!(net_out.status.success());
    assert!(
        serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&net_out.stdout))
            .is_ok()
    );
}

#[test]
fn test_accessibility_json_cockpit_and_health() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_wm.sqlite");
    let audit_path = tmp.path().join("test_audit.jsonl");
    let lock_path = tmp.path().join("test.lock");
    let _conn = db::open(&db_path).unwrap();
    fs::write(&audit_path, "").unwrap();

    // Test ai-cockpit --json
    let cockpit_out = Command::new(env!("CARGO_BIN_EXE_ai-cockpit"))
        .arg("--db")
        .arg(&db_path)
        .arg("--audit")
        .arg(&audit_path)
        .arg("--lock")
        .arg(&lock_path)
        .arg("--json")
        .output()
        .expect("Failed to run ai-cockpit --json");

    assert!(
        cockpit_out.status.success(),
        "ai-cockpit failed: {:?}",
        cockpit_out
    );
    let cockpit_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&cockpit_out.stdout))
            .expect("ai-cockpit --json must be valid JSON");
    assert!(cockpit_val.get("timestamp").is_some());
    assert!(cockpit_val.get("health_report").is_some());

    // Test ai-health status --json
    let health_out = Command::new(env!("CARGO_BIN_EXE_ai-health"))
        .arg("--db")
        .arg(&db_path)
        .arg("--worklog")
        .arg(&audit_path)
        .arg("status")
        .arg("--json")
        .output()
        .expect("Failed to run ai-health status --json");

    assert!(
        health_out.status.success(),
        "ai-health failed: {:?}",
        health_out
    );
    let health_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&health_out.stdout))
            .expect("ai-health status --json must be valid JSON");
    assert!(health_val.get("overall_status").is_some());
    assert!(health_val.get("subsystems").is_some());
}

#[test]
fn test_accessibility_json_ai_engineer_plan_and_explain() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test_wm.sqlite");
    let _conn = db::open(&db_path).unwrap();

    // 1. plan --json
    let plan_out = Command::new(env!("CARGO_BIN_EXE_ai-engineer"))
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .arg("plan")
        .arg("restart nginx service")
        .output()
        .expect("Failed to run ai-engineer plan --json");

    assert!(
        plan_out.status.success(),
        "ai-engineer plan failed: {:?}",
        plan_out
    );
    let plan_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&plan_out.stdout))
            .expect("ai-engineer plan --json must be valid JSON");
    assert!(plan_val.get("intent").is_some());
    assert!(plan_val.get("steps").is_some());
    assert!(plan_val.get("aggregate_blast_radius").is_some());

    // 2. review --json
    let review_out = Command::new(env!("CARGO_BIN_EXE_ai-engineer"))
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .arg("review")
        .arg("restart nginx service")
        .output()
        .expect("Failed to run ai-engineer review --json");

    assert!(
        review_out.status.success(),
        "ai-engineer review failed: {:?}",
        review_out
    );
    let review_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&review_out.stdout))
            .expect("ai-engineer review --json must be valid JSON");
    assert!(review_val.get("verdict").is_some());

    // 3. dry-run --json
    let dryrun_out = Command::new(env!("CARGO_BIN_EXE_ai-engineer"))
        .arg("--db")
        .arg(&db_path)
        .arg("--json")
        .arg("dry-run")
        .arg("restart nginx service")
        .output()
        .expect("Failed to run ai-engineer dry-run --json");

    assert!(
        dryrun_out.status.success(),
        "ai-engineer dry-run failed: {:?}",
        dryrun_out
    );
    let dryrun_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&dryrun_out.stdout))
            .expect("ai-engineer dry-run --json must be valid JSON");
    assert!(dryrun_val.get("plan_id").is_some());
}
