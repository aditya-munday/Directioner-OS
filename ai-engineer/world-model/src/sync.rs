//! State synchronization engines for the SQLite World Model.
//!
//! Provides transactional synchronization routines for installed packages, unit states,
//! storage filesystems, thermal sensor readings, and network interface operational states.

use crate::db;
use crate::model::{DiskSnapshot, NetIfaceSnapshot, ThermalSnapshot, UnitChange, UnitSnapshot};
use crate::now_rfc3339;
use rusqlite::{Connection, params};
use std::path::Path;

/// Synchronizes package inventory and dependency edges transactionally.
pub fn sync_packages(conn: &Connection, data: &crate::model::PackageData) -> Result<usize, String> {
    let ts = now_rfc3339();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM packages", [])
        .map_err(|e| e.to_string())?;

    {
        let mut ins_pkg = tx
            .prepare("INSERT OR REPLACE INTO packages(name, evr, arch, summary, synced_at) VALUES (?1, ?2, ?3, ?4, ?5)")
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

/// Applies a systemd unit snapshot, diffing transitions and recording unit events.
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
                    new_value: String::new(),
                });
                if s.active_state == "failed" {
                    changes.push(UnitChange {
                        unit: s.name.clone(),
                        kind: "active".into(),
                        old_value: "unknown".into(),
                        new_value: "failed".into(),
                    });
                }
            }
            Some((ol, oa, os, oe)) => {
                let mut push_if = |kind: &str, old: &str, new: &str| {
                    if let Some(mut c) = field_changed(kind, old, new) {
                        c.unit = s.name.clone();
                        changes.push(c);
                    }
                };
                push_if("load", ol, &s.load_state);
                push_if("active", oa, &s.active_state);
                push_if("sub", os, &s.sub_state);
                push_if("enabled", oe, &s.enabled);
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

/// Synchronizes read-only storage and filesystem capacity snapshots.
pub fn sync_disks(conn: &Connection, disks: &[DiskSnapshot]) -> Result<usize, String> {
    let ts = now_rfc3339();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM system_disks", [])
        .map_err(|e| e.to_string())?;

    {
        let mut ins = tx
            .prepare(
                "INSERT INTO system_disks(mount_point, device, fs_type, total_bytes, used_bytes, avail_bytes, use_percent, synced_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .map_err(|e| e.to_string())?;
        for d in disks {
            ins.execute(params![
                d.mount_point,
                d.device,
                d.fs_type,
                d.total_bytes,
                d.used_bytes,
                d.avail_bytes,
                d.use_percent,
                ts
            ])
            .map_err(|e| format!("insert disk {}: {e}", d.mount_point))?;
        }
    }

    db::set_meta(&tx, "disks_synced_at", &ts).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(disks.len())
}

/// Synchronizes read-only hardware thermal zone sensor readings.
pub fn sync_thermals(conn: &Connection, thermals: &[ThermalSnapshot]) -> Result<usize, String> {
    let ts = now_rfc3339();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM system_thermals", [])
        .map_err(|e| e.to_string())?;

    {
        let mut ins = tx
            .prepare(
                "INSERT INTO system_thermals(zone, temp_celsius, trip_type, synced_at) \
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(|e| e.to_string())?;
        for t in thermals {
            ins.execute(params![t.zone, t.temp_celsius, t.trip_type, ts])
                .map_err(|e| format!("insert thermal {}: {e}", t.zone))?;
        }
    }

    db::set_meta(&tx, "thermals_synced_at", &ts).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(thermals.len())
}

/// Synchronizes read-only network interface operational states.
pub fn sync_net_ifaces(conn: &Connection, ifaces: &[NetIfaceSnapshot]) -> Result<usize, String> {
    let ts = now_rfc3339();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM system_net_ifaces", [])
        .map_err(|e| e.to_string())?;

    {
        let mut ins = tx
            .prepare(
                "INSERT INTO system_net_ifaces(name, oper_state, mac_address, carrier, synced_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|e| e.to_string())?;
        for iface in ifaces {
            let carrier_int = if iface.carrier { 1 } else { 0 };
            ins.execute(params![
                iface.name,
                iface.oper_state,
                iface.mac_address,
                carrier_int,
                ts
            ])
            .map_err(|e| format!("insert net iface {}: {e}", iface.name))?;
        }
    }

    db::set_meta(&tx, "net_ifaces_synced_at", &ts).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ifaces.len())
}

/// Returns the file size in bytes of the worklog path if it exists.
pub fn worklog_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// Records the last observed worklog file size in World Model metadata.
pub fn mark_worklog(conn: &Connection, path: &Path) {
    if let Some(size) = worklog_size(path) {
        let _ = db::set_meta(conn, "worklog_size_last_seen", &size.to_string());
    }
}

/// Checks whether the worklog file has changed in size since the last sync.
pub fn worklog_stale(conn: &Connection, path: &Path) -> bool {
    let stored = db::get_meta(conn, "worklog_size_last_seen").and_then(|v| v.parse::<u64>().ok());
    match (stored, worklog_size(path)) {
        (Some(s), Some(cur)) => s != cur,
        (_, Some(_)) => true,
        _ => false,
    }
}
