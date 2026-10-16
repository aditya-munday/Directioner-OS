use capability_layer::atomic::{atomic_write, fault};
use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::{CapabilityKind, CapabilityRequest, API_VERSION};
use std::fs;
use std::os::unix::fs::symlink;
use tempfile::tempdir;

#[test]
fn test_atomic_write_basic() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("subdir").join("test_file.txt");
    let content = b"hello directioner os atomic file ops";

    atomic_write(&target, content).unwrap();

    let read_back = fs::read(&target).unwrap();
    assert_eq!(read_back, content);
}

#[test]
fn test_atomic_write_overwrite() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("overwrite_test.txt");

    atomic_write(&target, b"version 1").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "version 1");

    atomic_write(&target, b"version 2 updated").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "version 2 updated");
}

#[test]
fn test_atomic_write_rejects_symlink() {
    let dir = tempdir().unwrap();
    let outside_victim = dir.path().join("victim.txt");
    fs::write(&outside_victim, "SAFE_VICTIM_CONTENT").unwrap();

    let link_target = dir.path().join("evil_symlink");
    symlink(&outside_victim, &link_target).unwrap();

    let result = atomic_write(&link_target, b"ATTEMPTED_OVERWRITE");
    assert!(
        result.is_err(),
        "atomic_write must reject existing symlink destinations to prevent symlink traversal"
    );

    // Victim content must remain unchanged
    let victim_content = fs::read_to_string(&outside_victim).unwrap();
    assert_eq!(victim_content, "SAFE_VICTIM_CONTENT");
}

#[test]
fn test_atomic_write_fault_injection_leaves_no_torn_file() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("crash_consistent_target.txt");
    let initial_content = "CLEAN_ORIGINAL_DATA_12345";
    fs::write(&target, initial_content).unwrap();

    // Inject fault right before rename
    fault::set_fault_before_rename(true);

    let torn_attempt = b"CORRUPTED_PARTIAL_WRITE_THAT_CRASHED_HALFWAY";
    let res = atomic_write(&target, torn_attempt);
    assert!(res.is_err(), "Expected injected fault to fail the atomic write");

    // Reset fault flag
    fault::set_fault_before_rename(false);

    // 1. Target file must remain 100% clean original data (NO torn or corrupted write)
    let after_crash = fs::read_to_string(&target).unwrap();
    assert_eq!(
        after_crash, initial_content,
        "Fault before rename must leave original file intact and untorn"
    );

    // 2. No lingering temporary files left behind
    let entries: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();

    assert_eq!(
        entries,
        vec!["crash_consistent_target.txt"],
        "Temporary files must be cleaned up on failure, found: {:?}",
        entries
    );

    // 3. Subsequent write without fault succeeds cleanly
    atomic_write(&target, b"NEW_PERFECT_DATA").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "NEW_PERFECT_DATA");
}

#[test]
fn test_killswitch_engage_uses_atomic_write() {
    let dir = tempdir().unwrap();
    let lock_path = dir.path().join("killswitch.lock");

    let mut ks = KillSwitch::load(&lock_path).unwrap();
    assert!(!ks.is_engaged());

    ks.engage().unwrap();
    assert!(ks.is_engaged());
    assert_eq!(fs::read_to_string(&lock_path).unwrap(), "ENGAGED\n");

    ks.release().unwrap();
    assert!(!ks.is_engaged());
}

#[test]
fn test_cap_file_write_atomic_integration() {
    let dir = tempdir().unwrap();
    let scope_dir = dir.path().join("scope");
    fs::create_dir_all(&scope_dir).unwrap();

    let audit = AuditLog::open(dir.path().join("audit.jsonl")).unwrap();
    let killswitch = KillSwitch::load(dir.path().join("killswitch.lock")).unwrap();
    let scope = ScopedPath::new(&scope_dir).unwrap();
    let mut handler = Handler::new(scope, audit, killswitch, HandlerConfig::default());

    let req = CapabilityRequest {
        api_version: API_VERSION,
        id: "atomic-file-write-1".into(),
        requester: "test-runner".into(),
        token: "tok-1".into(),
        is_rollback: false,
        capability: CapabilityKind::FileWrite {
            path: "config.json".into(),
            content: r#"{"status":"active","workers":4}"#.into(),
            append: false,
        },
    };

    let result = handler.execute(&req);
    assert!(result.is_ok());

    let written = fs::read_to_string(scope_dir.join("config.json")).unwrap();
    assert_eq!(written, r#"{"status":"active","workers":4}"#);
}
