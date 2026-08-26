use std::io::Write;
use std::path::{Path, PathBuf};

use std::time::Duration;
use world_model::model::*;
use world_model::sources::{self, CmdSpec};
use world_model::{cutoff_rfc3339, db, query, sync};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn write_script(path: &Path, body: &str) {
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

fn make_pkg_bin(dir: &Path) -> PathBuf {
    let bin = dir.join("bin-pkg");
    std::fs::create_dir_all(&bin).unwrap();
    let fix = fixtures_dir().display().to_string();
    write_script(
        &bin.join("dnf5"),
        &format!("#!/bin/sh\ncat {fix}/repoquery_protocol.txt\n"),
    );
    write_script(
        &bin.join("rpm"),
        "#!/bin/sh\necho 'PKG|fake-fallback|1.0-1|noarch|fallback adapter'\necho END\n",
    );
    bin
}

fn make_units_bin(dir: &Path, fixture_file: &str) -> PathBuf {
    let bin = dir.join(format!("bin-units-{fixture_file}"));
    std::fs::create_dir_all(&bin).unwrap();
    let fix = fixtures_dir().display().to_string();
    write_script(
        &bin.join("systemctl"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"list-unit-files\" ]; then cat {fix}/unit_files.json; exit 0; fi\ncat {fix}/{fixture_file}\n"
        ),
    );
    bin
}

fn pkg_spec(bin: &Path) -> CmdSpec {
    sources::dnf5_repoquery_spec(Some(bin))
}

fn units_specs(bin: &Path) -> (CmdSpec, CmdSpec) {
    (
        sources::systemctl_list_units_spec(Some(bin)),
        sources::systemctl_unit_files_spec(Some(bin)),
    )
}

fn load_and_apply(db_path: &Path, units_bin: &Path) -> (usize, Vec<UnitChange>) {
    let conn = db::open(db_path).unwrap();
    let snap =
        sources::load_unit_snapshot(&units_specs(units_bin).0, &units_specs(units_bin).1).unwrap();
    sync::apply_unit_snapshot(&conn, &snap).unwrap()
}

#[test]
fn packages_sync_reverse_deps_and_requires() {
    let dir = tempfile::tempdir().unwrap();
    let dbp = dir.path().join("wm.sqlite");
    let conn = db::open(&dbp).unwrap();

    let out = pkg_spec(&make_pkg_bin(dir.path())).output().unwrap();
    assert!(out.status.success());
    let data = sources::parse_line_protocol(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(data.packages.len(), 17);

    let n = sync::sync_packages(&conn, &data).unwrap();
    assert_eq!(n, 17);

    let dependents_of_openssl = query::reverse_deps(&conn, "openssl").unwrap();
    assert_eq!(dependents_of_openssl, vec!["nginx", "stunnel"]);

    let dependents_of_openssl_libs = query::reverse_deps(&conn, "openssl-libs").unwrap();
    for expected in [
        "libcurl",
        "mod_ssl",
        "openssh-clients",
        "openssh-server",
        "openssl",
    ] {
        assert!(
            dependents_of_openssl_libs.contains(&expected.to_string()),
            "expected {expected} in {:?}",
            dependents_of_openssl_libs
        );
    }
    assert!(!dependents_of_openssl_libs.contains(&"bash".to_string()));

    let reqs = query::requires(&conn, "nginx").unwrap();
    let openssl_req = reqs
        .iter()
        .find(|r| r.dep == "openssl")
        .expect("nginx requires openssl");
    assert_eq!(openssl_req.flags, ">=");
    assert_eq!(openssl_req.version, "1:1.1.1");
}

#[test]
fn unit_snapshot_diff_events_and_failed_since() {
    let dir = tempfile::tempdir().unwrap();
    let dbp = dir.path().join("wm.sqlite");

    let bin_a = make_units_bin(dir.path(), "units_a.json");
    let (units, changes) = load_and_apply(&dbp, &bin_a);
    assert_eq!(units, 4);
    assert_eq!(changes.iter().filter(|c| c.kind == "added").count(), 4);
    let docker_active = changes
        .iter()
        .find(|c| c.unit == "docker.service" && c.kind == "active")
        .expect("observed-failed unit must emit an active event");
    assert_eq!(docker_active.new_value, "failed");
    assert_eq!(changes.len(), 5);

    let bin_b = make_units_bin(dir.path(), "units_b.json");
    let (_, changes_b) = load_and_apply(&dbp, &bin_b);
    let kinds: Vec<(&str, &str)> = changes_b
        .iter()
        .map(|c| (c.unit.as_str(), c.kind.as_str()))
        .collect();
    assert!(kinds.contains(&("ssh.service", "active")), "{kinds:?}");
    assert!(
        kinds.contains(&("ssh.service", "sub")),
        "sub-state must flip alongside active: {kinds:?}"
    );
    assert!(kinds.contains(&("nginx.service", "added")), "{kinds:?}");
    assert!(
        kinds.contains(&("web-legacy.service", "removed")),
        "{kinds:?}"
    );
    assert_eq!(changes_b.len(), 4);

    let (_, idempotent) = load_and_apply(&dbp, &bin_b);
    assert!(
        idempotent.is_empty(),
        "re-applying identical snapshot must emit no events: {idempotent:?}"
    );

    let conn = db::open(&dbp).unwrap();
    let two_hours_ago = cutoff_rfc3339(Duration::from_secs(2 * 3600));
    conn.execute(
        "UPDATE unit_events SET ts = ?1 WHERE unit = 'ssh.service' AND kind = 'active' AND new_value = 'failed'",
        [two_hours_ago.as_str()],
    ).unwrap();

    let recent = query::failed_since(&conn, &cutoff_rfc3339(Duration::from_secs(3600))).unwrap();
    let names: Vec<&str> = recent.iter().map(|e| e.unit.as_str()).collect();
    assert!(
        !names.contains(&"ssh.service"),
        "backdated failure must be excluded from last-1h"
    );
    assert!(
        names.contains(&"docker.service"),
        "observed-failed docker must be in last-1h"
    );

    let wider = query::failed_since(&conn, &cutoff_rfc3339(Duration::from_secs(3 * 3600))).unwrap();
    let wide_names: Vec<&str> = wider.iter().map(|e| e.unit.as_str()).collect();
    assert!(wide_names.contains(&"ssh.service"));
    assert!(wide_names.contains(&"docker.service"));
}

#[test]
fn reverse_deps_matches_soname_basename_with_version_suffix() {
    // Reproduces the CI failure: `depends-on glibc` returned 0 because packages
    // depend on versioned sonames (libc.so.6(GLIBC_2.2.5)(64bit)) which never
    // exact-match the unversioned provide (libc.so.6()(64bit)).
    let dir = tempfile::tempdir().unwrap();
    let dbp = dir.path().join("wm.sqlite");
    let conn = db::open(&dbp).unwrap();

    fn row(name: &str) -> PackageRow {
        PackageRow {
            name: name.into(),
            evr: "1.0-1".into(),
            arch: "x86_64".into(),
            summary: "pkg".into(),
        }
    }

    let data = PackageData {
        packages: vec![
            row("glibc"),
            row("coreutils"),
            row("bash"),
            row("openssl-libs"),
        ],
        requires: vec![
            RequireEdge {
                pkg: "bash".into(),
                dep: "libc.so.6()(64bit)".into(),
                flags: String::new(),
                version: String::new(),
            },
            RequireEdge {
                pkg: "coreutils".into(),
                dep: "libc.so.6(GLIBC_2.2.5)(64bit)".into(),
                flags: String::new(),
                version: String::new(),
            },
            // A versioned pkg-name require, e.g. "glibc >= 2.34"
            RequireEdge {
                pkg: "openssl-libs".into(),
                dep: "glibc >= 2.34".into(),
                flags: ">=".into(),
                version: "2.34".into(),
            },
        ],
        provides: vec![
            ProvideEdge {
                pkg: "glibc".into(),
                provide: "libc.so.6()(64bit)".into(),
                version: String::new(),
            },
            ProvideEdge {
                pkg: "glibc".into(),
                provide: "ld-linux-x86-64.so.2()(64bit)".into(),
                version: String::new(),
            },
        ],
    };
    sync::sync_packages(&conn, &data).unwrap();

    let names = query::reverse_deps(&conn, "glibc").unwrap();
    assert!(
        names.contains(&"bash".to_string()),
        "bash via unversioned soname: {names:?}"
    );
    assert!(
        names.contains(&"coreutils".to_string()),
        "coreutils via versioned soname: {names:?}"
    );
    assert!(
        names.contains(&"openssl-libs".to_string()),
        "openssl-libs via name+version: {names:?}"
    );
    assert!(!names.contains(&"glibc".to_string()));
}

#[test]
fn worklog_staleness_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let dbp = dir.path().join("wm.sqlite");
    let wl = dir.path().join("worklog.jsonl");
    std::fs::write(&wl, b"").unwrap();

    let conn = db::open(&dbp).unwrap();
    sync::mark_worklog(&conn, &wl);
    assert!(!sync::worklog_stale(&conn, &wl));

    let mut f = std::fs::OpenOptions::new().append(true).open(&wl).unwrap();
    f.write_all(b"{\"seq\":0}\n").unwrap();
    f.sync_all().unwrap();
    drop(f);
    assert!(sync::worklog_stale(&conn, &wl));

    sync::mark_worklog(&conn, &wl);
    assert!(!sync::worklog_stale(&conn, &wl));
}
