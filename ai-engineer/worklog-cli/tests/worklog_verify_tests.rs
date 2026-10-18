use std::fs::File;
use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

use capability_layer::audit::{AuditEntry, AuditLog, AuditOutcome};

fn make_entry(seq: u64, cap: &str) -> AuditEntry {
    AuditEntry {
        seq,
        timestamp: "2026-10-18T00:00:00Z".into(),
        request_id: format!("req-{seq}"),
        capability: cap.into(),
        args: serde_json::json!({"test": seq}),
        requester: "test-runner".into(),
        token: "tok".into(),
        is_rollback: false,
        outcome: AuditOutcome::Success {
            result: serde_json::json!({}),
        },
        duration_ms: 1,
        prev_hash: None,
        entry_hash: None,
    }
}

#[test]
fn test_worklog_verify_valid_chain() {
    let tmp = tempdir().unwrap();
    let log_path = tmp.path().join("worklog.jsonl");

    {
        let mut log = AuditLog::open(&log_path).unwrap();
        log.append(make_entry(0, "cap_1")).unwrap();
        log.append(make_entry(1, "cap_2")).unwrap();
        log.append(make_entry(2, "cap_3")).unwrap();
    }

    let bin = env!("CARGO_BIN_EXE_ai-worklog-verify");
    let output = Command::new(bin)
        .arg("--log")
        .arg(&log_path)
        .output()
        .expect("failed to execute ai-worklog-verify");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PASSED"));
    assert!(stdout.contains("3 verified"));
}

#[test]
fn test_worklog_verify_json_mode() {
    let tmp = tempdir().unwrap();
    let log_path = tmp.path().join("worklog.jsonl");

    {
        let mut log = AuditLog::open(&log_path).unwrap();
        log.append(make_entry(0, "cap_1")).unwrap();
    }

    let bin = env!("CARGO_BIN_EXE_ai-worklog-verify");
    let output = Command::new(bin)
        .arg("--log")
        .arg(&log_path)
        .arg("--json")
        .output()
        .expect("failed to execute ai-worklog-verify");

    assert!(output.status.success());
    let val: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(val["valid"], true);
    assert_eq!(val["count_verified"], 1);
    assert_eq!(val["count_unchained"], 0);
}

#[test]
fn test_worklog_verify_tamper_detected() {
    let tmp = tempdir().unwrap();
    let log_path = tmp.path().join("worklog.jsonl");

    {
        let mut log = AuditLog::open(&log_path).unwrap();
        log.append(make_entry(0, "cap_1")).unwrap();
        log.append(make_entry(1, "cap_2")).unwrap();
        log.append(make_entry(2, "cap_3")).unwrap();
    }

    let raw = std::fs::read_to_string(&log_path).unwrap();
    let tampered = raw.replace("cap_2", "cap_tampered");
    std::fs::write(&log_path, tampered).unwrap();

    let bin = env!("CARGO_BIN_EXE_ai-worklog-verify");
    let output = Command::new(bin)
        .arg("--log")
        .arg(&log_path)
        .output()
        .expect("failed to execute ai-worklog-verify");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("FAILED at sequence #1"));
}

#[test]
fn test_worklog_verify_legacy_unchained_migration() {
    let tmp = tempdir().unwrap();
    let log_path = tmp.path().join("worklog.jsonl");

    {
        let mut f = File::create(&log_path).unwrap();
        let unchained_entry = serde_json::json!({
            "seq": 0,
            "timestamp": "2026-09-01T00:00:00Z",
            "request_id": "req-0",
            "capability": "old_cap",
            "args": {},
            "requester": "user",
            "token": "tok",
            "is_rollback": false,
            "outcome": {"status": "success", "result": {}},
            "duration_ms": 1
        });
        writeln!(f, "{}", unchained_entry).unwrap();
    }

    {
        let mut log = AuditLog::open(&log_path).unwrap();
        log.append(make_entry(1, "new_cap")).unwrap();
    }

    let bin = env!("CARGO_BIN_EXE_ai-worklog-verify");
    let output = Command::new(bin)
        .arg("--log")
        .arg(&log_path)
        .arg("--json")
        .output()
        .expect("failed to execute ai-worklog-verify");

    assert!(output.status.success());
    let val: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(val["valid"], true);
    assert_eq!(val["count_verified"], 1);
    assert_eq!(val["count_unchained"], 1);
}
