use control_plane::coredump::{
    CoredumpForensicsStore, CoredumpRecord, parse_coredump_from_fixture,
};
use tempfile::NamedTempFile;

#[test]
fn test_parse_coredump_from_fixture_json() {
    let raw = r#"[
        {
            "pid": 1234,
            "uid": 1000,
            "gid": 1000,
            "signal": "SIGSEGV",
            "exe": "/usr/bin/broken-app"
        },
        {
            "pid": 987,
            "uid": 0,
            "gid": 0,
            "signal": "SIGABRT",
            "exe": "/usr/sbin/agetty",
            "unit": "serial-getty@ttyS0.service"
        }
    ]"#;

    let report = parse_coredump_from_fixture(raw);
    assert_eq!(report.total_dumps, 2);
    assert_eq!(report.records.len(), 2);

    let first = &report.records[0];
    assert_eq!(first.pid, 1234);
    assert_eq!(first.signal.as_deref(), Some("SIGSEGV"));
    assert_eq!(first.exe, "/usr/bin/broken-app");

    let second = &report.records[1];
    assert_eq!(second.pid, 987);
    assert_eq!(second.signal.as_deref(), Some("SIGABRT"));
    assert_eq!(second.exe, "/usr/sbin/agetty");
    assert_eq!(second.unit.as_deref(), Some("serial-getty@ttyS0.service"));
}

#[test]
fn test_coredump_forensics_store_and_rotation() {
    let tmp = NamedTempFile::new().unwrap();
    let mut store = CoredumpForensicsStore::new(tmp.path().to_path_buf(), 2);

    let rec1 = CoredumpRecord {
        pid: 101,
        uid: 0,
        gid: 0,
        signal: Some("SIGABRT".to_string()),
        exe: "/usr/sbin/agetty".to_string(),
        unit: Some("serial-getty@ttyS0.service".to_string()),
        boot_id: Some("boot-111".to_string()),
        timestamp: "2026-10-18T00:01:00Z".to_string(),
        backtrace_summary: Some("frame #0 0x7f1a in agetty_main".to_string()),
    };
    let rec2 = CoredumpRecord {
        pid: 102,
        uid: 0,
        gid: 0,
        signal: Some("SIGSEGV".to_string()),
        exe: "/usr/bin/runner".to_string(),
        unit: None,
        boot_id: Some("boot-111".to_string()),
        timestamp: "2026-10-18T00:02:00Z".to_string(),
        backtrace_summary: None,
    };
    let rec3 = CoredumpRecord {
        pid: 103,
        uid: 1000,
        gid: 1000,
        signal: Some("SIGILL".to_string()),
        exe: "/usr/bin/worker".to_string(),
        unit: None,
        boot_id: Some("boot-222".to_string()),
        timestamp: "2026-10-18T00:03:00Z".to_string(),
        backtrace_summary: None,
    };

    store.save_record(&rec1).unwrap();
    store.save_record(&rec2).unwrap();
    store.save_record(&rec3).unwrap(); // max 2, rec1 should rotate out

    let loaded = store.load_all().unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].pid, 102);
    assert_eq!(loaded[1].pid, 103);
}
