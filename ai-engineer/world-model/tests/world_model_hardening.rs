//! Hardening Test Suite for World Model (Phase 1.1 / Maintenance).
//!
//! Tests:
//! 1. Concurrent read access across multiple threads during active syncing.
//! 2. Malformed, truncated, and partial dnf5 repoquery line protocol handling.
//! 3. Malformed and corrupted systemctl JSON and text output handling.
//! 4. Database corruption detection and clean recovery.
//! 5. SQLite lock contention and busy timeout behavior.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use world_model::model::*;
use world_model::sources;
use world_model::{db, query, sync};

fn write_script(path: &Path, body: &str) {
    {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f.sync_all().unwrap();
    }
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

#[test]
fn test_concurrent_read_access_during_active_sync() {
    let dir = tempfile::tempdir().unwrap();
    let dbp = dir.path().join("concurrent_wm.sqlite");

    // Initialize schema
    {
        let conn = db::open(&dbp).unwrap();
        let init_data = PackageData {
            packages: vec![PackageRow {
                name: "base-system".into(),
                evr: "1.0.0-1".into(),
                arch: "x86_64".into(),
                summary: "Base system".into(),
            }],
            requires: vec![RequireEdge {
                pkg: "base-system".into(),
                dep: "glibc".into(),
                flags: "GE".into(),
                version: "2.38".into(),
            }],
            provides: vec![ProvideEdge {
                pkg: "base-system".into(),
                provide: "system-release".into(),
                version: "1.0".into(),
            }],
        };
        sync::sync_packages(&conn, &init_data).unwrap();
    }

    let running = Arc::new(AtomicBool::new(true));
    let mut reader_handles = Vec::new();

    // Spawn 4 concurrent reader threads
    for thread_idx in 0..4 {
        let dbp_clone = dbp.clone();
        let running_clone = running.clone();

        reader_handles.push(thread::spawn(move || {
            let conn = db::open(&dbp_clone).unwrap();
            let mut reads_performed = 0;

            loop {
                // Reader queries
                let _ = query::reverse_deps(&conn, "glibc");
                let _ = query::graph_stats(&conn);
                let _ = query::requires(&conn, "base-system");
                let _ = query::unit_info(&conn, "nginx.service");
                reads_performed += 1;
                if !running_clone.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(Duration::from_millis(1));
            }

            println!(
                "Reader thread {} performed {} concurrent queries",
                thread_idx, reads_performed
            );
            reads_performed
        }));
    }

    // Writer thread performing 15 sync iterations
    let dbp_writer = dbp.clone();
    let writer_handle = thread::spawn(move || {
        let conn = db::open(&dbp_writer).unwrap();
        for iter in 0..15 {
            let pkg_name = format!("pkg-{iter}");
            let data = PackageData {
                packages: vec![PackageRow {
                    name: pkg_name.clone(),
                    evr: format!("{iter}.0.0-1"),
                    arch: "x86_64".into(),
                    summary: "Test package".into(),
                }],
                requires: vec![RequireEdge {
                    pkg: pkg_name.clone(),
                    dep: "glibc".into(),
                    flags: "GE".into(),
                    version: "2.38".into(),
                }],
                provides: vec![],
            };
            sync::sync_packages(&conn, &data).unwrap();

            let unit_snap = vec![UnitSnapshot {
                name: format!("service-{iter}.service"),
                load_state: "loaded".into(),
                active_state: "active".into(),
                sub_state: "running".into(),
                enabled: "enabled".into(),
            }];
            sync::apply_unit_snapshot(&conn, &unit_snap).unwrap();

            thread::sleep(Duration::from_millis(10));
        }
    });

    writer_handle.join().unwrap();
    running.store(false, Ordering::SeqCst);

    for handle in reader_handles {
        let reads = handle.join().unwrap();
        assert!(reads > 0, "Reader thread must have completed queries");
    }

    // Verify final database state is consistent
    let final_conn = db::open(&dbp).unwrap();
    let stats = query::graph_stats(&final_conn).unwrap();
    assert!(stats.packages > 0);
    assert!(stats.units > 0);
}

#[test]
fn test_malformed_and_partial_dnf5_line_protocol_handling() {
    // 1. Truncated fields in PKG line -> returns structured Err
    let truncated_pkg = "PKG|missing_fields_only_one_item\nEND\n";
    let res = sources::parse_line_protocol(truncated_pkg);
    assert!(
        res.is_err(),
        "Truncated PKG line must fail gracefully with Err"
    );

    // 2. Missing END delimiter with partial output -> succeeds with parsed package
    let missing_end = "PKG|incomplete-pkg|1.0|x86_64|desc\nREQ|incomplete-pkg|dep|EQ|1\n";
    let res2 = sources::parse_line_protocol(missing_end);
    assert!(res2.is_ok());
    assert_eq!(res2.unwrap().packages.len(), 1);

    // 3. Completely corrupt garbage text (lines with unknown headers are ignored safely)
    let garbage = "RANDOM_CORRUPT_BYTES\x00\x7F\x01\nANOTHER_LINE|FOO\n";
    let res3 = sources::parse_line_protocol(garbage);
    assert!(res3.is_ok());
    let data = res3.unwrap();
    assert!(data.packages.is_empty());

    // 4. Empty input
    let empty = "";
    let res4 = sources::parse_line_protocol(empty).unwrap();
    assert!(res4.packages.is_empty());
}

#[test]
fn test_malformed_and_partial_systemctl_output_handling() {
    let dir = tempfile::tempdir().unwrap();
    let bin_dir = dir.path().join("bin-corrupt-sysctl");
    fs::create_dir_all(&bin_dir).unwrap();

    // Script emitting malformed JSON
    write_script(
        &bin_dir.join("systemctl"),
        r#"#!/bin/sh
if [ "$1" = "list-unit-files" ]; then
    echo '[{"unit_file": "broken.service", "state": "enabled"}' # Truncated JSON
    exit 0
fi
echo 'INVALID NON-JSON OUTPUT HERE'
"#,
    );

    let units_spec = sources::systemctl_list_units_spec(Some(&bin_dir));
    let files_spec = sources::systemctl_unit_files_spec(Some(&bin_dir));

    let snap_res = sources::load_unit_snapshot(&units_spec, &files_spec);
    // Should fail gracefully with error, NOT panic
    assert!(snap_res.is_err());
}

#[test]
fn test_database_corruption_detection_and_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let dbp = dir.path().join("corrupt.sqlite");

    // 1. Write corrupted header into sqlite file
    fs::write(&dbp, b"CORRUPTED SQLITE HEADER GARBAGE DATA 1234567890").unwrap();

    // 2. Attempting to open / query corrupted database returns an error
    let conn_res = db::open(&dbp);
    if let Ok(conn) = conn_res {
        // If open succeeded, executing query must return error
        let query_res = query::graph_stats(&conn);
        assert!(query_res.is_err());
    }

    // 3. Recovery: remove corrupt file and recreate clean database
    fs::remove_file(&dbp).unwrap();
    let clean_conn = db::open(&dbp).expect("Recreated database must open cleanly");
    let stats = query::graph_stats(&clean_conn).unwrap();
    assert_eq!(stats.packages, 0);
    assert_eq!(stats.units, 0);
}

#[test]
fn test_database_lock_contention_and_busy_handling() {
    let dir = tempfile::tempdir().unwrap();
    let dbp = dir.path().join("locked.sqlite");

    // Initialize database
    let conn1 = db::open(&dbp).unwrap();
    conn1.pragma_update(None, "busy_timeout", 100).unwrap();

    let conn2 = db::open(&dbp).unwrap();
    conn2.pragma_update(None, "busy_timeout", 50).unwrap();

    // Hold exclusive transaction in conn1
    conn1.execute("BEGIN EXCLUSIVE;", []).unwrap();

    // Second connection attempting write should fail with database locked / busy timeout
    let write_res = conn2.execute(
        "INSERT INTO packages (name, evr, arch, summary, synced_at) VALUES ('p', '1', 'x', 's', '2026-08-31T00:00:00Z');",
        [],
    );
    assert!(
        write_res.is_err(),
        "Concurrent write while exclusive lock is held must fail closed"
    );

    // Rollback conn1
    conn1.execute("ROLLBACK;", []).unwrap();

    // Now write on conn2 should succeed
    let write_res2 = conn2.execute(
        "INSERT INTO packages (name, evr, arch, summary, synced_at) VALUES ('p2', '1', 'x', 's', '2026-08-31T00:00:00Z');",
        [],
    );
    assert!(write_res2.is_ok(), "Write after lock release must succeed");
}
