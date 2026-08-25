use crate::db;
use crate::model::{UnitChange, UnitSnapshot};
use crate::now_rfc3339;
use rusqlite::{Connection, params};
use std::path::Path;

pub fn sync_packages(conn: &Connection, data: &crate::model::PackageData) -> Result<usize, String> {
    let ts = now_rfc3339();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM packages", [])
        .map_err(|e| e.to_string())?;

    {
        let mut ins_pkg = tx
            .prepare("INSERT INTO packages(name, evr, arch, summary, synced_at) VALUES (?1, ?2, ?3, ?4, ?5)")
            .map_err(|e| e.to_string())?;
        for p in &data.packages {
            ins_pkg
                .execute(params![p.name, p.evr, p.arch, p.summary, ts])
                .map_err(|e| format!("insert package {}: {e}", p.name))?;
        }
    }
    {
        let mut ins_req = tx
            .prepare("INSERT OR IGNORE INTO pkg_requires(pkg, dep, flags, version) VALUES (?1, ?2, ?3, ?4)")
            .map_err(|e| e.to_string())?;
        for r in &data.requires {
            ins_req
                .execute(params![r.pkg, r.dep, r.flags, r.version])
                .map_err(|e| e.to_string())?;
        }
    }
    {
        let mut ins_prov = tx
            .prepare(
                "INSERT OR IGNORE INTO pkg_provides(pkg, provide, version) VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| e.to_string())?;
        for p in &data.provides {
            ins_prov
                .execute(params![p.pkg, p.provide, p.version])
                .map_err(|e| e.to_string())?;
        }
    }

    db::set_meta(&tx, "packages_synced_at", &ts).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(data.packages.len())
}

fn field_changed(kind: &str, old: &str, new: &str) -> Option<UnitChange> {
    if old != new {
        Some(UnitChange {
            unit: String::new(),
            kind: kind.into(),
            old_value: old.into(),
            new_value: new.into(),
        })
    } else {
        None
    }
}

pub fn apply_unit_snapshot(
    conn: &Connection,
    snap: &[UnitSnapshot],
) -> Result<(usize, Vec<UnitChange>), String> {
    let ts = now_rfc3339();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;

    let mut stmt = tx
        .prepare("SELECT name, load_state, active_state, sub_state, enabled FROM units")
        .map_err(|e| e.to_string())?;
    let existing: std::collections::HashMap<String, (String, String, String, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|t| (t.0.clone(), (t.1, t.2, t.3, t.4)))
        .collect();
    drop(stmt);

    let mut changes: Vec<UnitChange> = Vec::new();
    let snap_names: std::collections::HashSet<&str> =
        snap.iter().map(|s| s.name.as_str()).collect();

    for name in existing.keys() {
        if !snap_names.contains(name.as_str()) {
            changes.push(UnitChange {
                unit: name.clone(),
                kind: "removed".into(),
                old_value: String::new(),
                new_value: String::new(),
            });
        }
    }

    for s in snap {
        match existing.get(&s.name) {
            None => {
                changes.push(UnitChange {
                    unit: s.name.clone(),
                    kind: "added".into(),
                    old_value: String::new(),
                    new_value: s.active_state.clone(),
                });
                if s.active_state == "failed" {
                    changes.push(UnitChange {
                        unit: s.name.clone(),
                        kind: "active".into(),
                        old_value: String::new(),
                        new_value: "failed".into(),
                    });
                }
            }
            Some((old_load, old_active, old_sub, old_enabled)) => {
                for mut c in [
                    field_changed("load", old_load, &s.load_state),
                    field_changed("active", old_active, &s.active_state),
                    field_changed("sub", old_sub, &s.sub_state),
                    field_changed("enabled", old_enabled, &s.enabled),
                ]
                .into_iter()
                .flatten()
                {
                    c.unit = s.name.clone();
                    changes.push(c);
                }
            }
        }
    }

    tx.execute("DELETE FROM units", [])
        .map_err(|e| e.to_string())?;
    {
        let mut ins_unit = tx
            .prepare(
                "INSERT INTO units(name, load_state, active_state, sub_state, enabled, since_ts) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|e| e.to_string())?;
        for s in snap {
            let since = match existing.get(&s.name) {
                Some((ol, oa, os, oe))
                    if (ol.as_str(), oa.as_str(), os.as_str(), oe.as_str()) == s.tuple() =>
                {
                    db::get_meta(&tx, &format!("unit_since:{}", s.name)).unwrap_or_default()
                }
                _ => ts.clone(),
            };
            db::set_meta(&tx, &format!("unit_since:{}", s.name), &since)
                .map_err(|e| e.to_string())?;
            ins_unit
                .execute(params![
                    s.name,
                    s.load_state,
                    s.active_state,
                    s.sub_state,
                    s.enabled,
                    since
                ])
                .map_err(|e| e.to_string())?;
        }
    }
    {
        let mut ins_ev = tx
            .prepare(
                "INSERT INTO unit_events(ts, unit, kind, old_value, new_value) VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|e| e.to_string())?;
        for c in &changes {
            ins_ev
                .execute(params![ts, c.unit, c.kind, c.old_value, c.new_value])
                .map_err(|e| e.to_string())?;
        }
    }

    db::set_meta(&tx, "units_synced_at", &ts).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok((snap.len(), changes))
}

pub fn worklog_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

pub fn mark_worklog(conn: &Connection, path: &Path) {
    if let Some(size) = worklog_size(path) {
        let _ = db::set_meta(conn, "worklog_size_last_seen", &size.to_string());
    }
}

pub fn worklog_stale(conn: &Connection, path: &Path) -> bool {
    let stored = db::get_meta(conn, "worklog_size_last_seen").and_then(|v| v.parse::<u64>().ok());
    match (stored, worklog_size(path)) {
        (Some(s), Some(cur)) => s != cur,
        (_, Some(_)) => true,
        _ => false,
    }
}
