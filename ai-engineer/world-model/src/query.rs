//! Query interfaces and telemetry extractors for the SQLite World Model.
//!
//! Exposes graph queries for reverse dependency resolution, unit transition histories,
//! package requirements, and read-only hardware/filesystem telemetry.

use crate::model::{DiskSnapshot, NetIfaceSnapshot, ThermalSnapshot};
use rusqlite::{Connection, params};

/// Represents a package requirement dependency record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequireInfo {
    /// Required package or capability symbol name.
    pub dep: String,
    /// Comparison operator flags.
    pub flags: String,
    /// Version constraint string.
    pub version: String,
}

/// Recorded unit state transition event row.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnitEventRow {
    /// Auto-incrementing primary key ID.
    pub id: i64,
    /// RFC3339 timestamp when event was recorded.
    pub ts: String,
    /// Unit name.
    pub unit: String,
    /// Transition category (e.g. `active`, `load`, `sub`, `enabled`).
    pub kind: String,
    /// Value prior to transition.
    pub old_value: String,
    /// Value after transition.
    pub new_value: String,
}

/// Detailed state information and recent event history for a unit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnitInfo {
    /// Unit name.
    pub name: String,
    /// Load state (e.g. `loaded`).
    pub load_state: String,
    /// Active state (e.g. `active`, `failed`).
    pub active_state: String,
    /// Sub state (e.g. `running`, `dead`).
    pub sub_state: String,
    /// Unit enablement status.
    pub enabled: String,
    /// Timestamp when this state was entered.
    pub since_ts: String,
    /// Recent event history transitions.
    pub recent_events: Vec<UnitEventRow>,
}

/// Normalizes a package or soname symbol to its base comparison token.
pub fn comparable_base(s: &str) -> String {
    let s = s.trim();
    // Cut at the first '(' (soname version/arch suffix)
    let s = s.split('(').next().unwrap_or(s);
    // Cut at the first space or '=' (comparison operator)
    let cut = s.find([' ', '=']).unwrap_or(s.len());
    s[..cut].to_string()
}

/// Resolves all installed packages that depend on the given package or capability.
pub fn reverse_deps(conn: &Connection, name: &str) -> rusqlite::Result<Vec<String>> {
    use std::collections::{BTreeSet, HashSet};

    let mut bases: HashSet<String> = HashSet::new();
    bases.insert(comparable_base(name));

    let mut stmt = conn.prepare("SELECT provide FROM pkg_provides WHERE pkg = ?1")?;
    let provides = stmt
        .query_map(params![name], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect::<Vec<String>>();
    drop(stmt);
    for p in provides {
        bases.insert(comparable_base(&p));
    }

    let mut dependents: BTreeSet<String> = BTreeSet::new();
    let mut stmt =
        conn.prepare("SELECT pkg, dep FROM pkg_requires WHERE dep >= ?1 AND dep <= ?2")?;
    for base in &bases {
        let low = base.clone();
        let high = format!("{}\u{10FFFF}", base);
        for (pkg, dep) in stmt
            .query_map(params![low, high], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .flatten()
        {
            if bases.contains(&comparable_base(&dep)) {
                dependents.insert(pkg);
            }
        }
    }
    dependents.remove(name);
    Ok(dependents.into_iter().collect())
}

/// Returns all requirements declared by the specified package.
pub fn requires(conn: &Connection, pkg: &str) -> rusqlite::Result<Vec<RequireInfo>> {
    let mut stmt =
        conn.prepare("SELECT dep, flags, version FROM pkg_requires WHERE pkg = ?1 ORDER BY dep")?;
    let rows = stmt
        .query_map(params![pkg], |r| {
            Ok(RequireInfo {
                dep: r.get(0)?,
                flags: r.get(1)?,
                version: r.get(2)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// Returns full state and event history for a given systemd unit.
pub fn unit_info(conn: &Connection, name: &str) -> rusqlite::Result<Option<UnitInfo>> {
    let base = conn
        .query_row(
            "SELECT load_state, active_state, sub_state, enabled, since_ts \
             FROM units WHERE name = ?1",
            params![name],
            |r| {
                Ok(UnitInfo {
                    name: name.to_string(),
                    load_state: r.get(0)?,
                    active_state: r.get(1)?,
                    sub_state: r.get(2)?,
                    enabled: r.get(3)?,
                    since_ts: r.get(4)?,
                    recent_events: Vec::new(),
                })
            },
        )
        .ok();

    match base {
        Some(mut info) => {
            let mut stmt = conn.prepare(
                "SELECT id, ts, unit, kind, old_value, new_value FROM unit_events \
                 WHERE unit = ?1 ORDER BY ts DESC LIMIT 50",
            )?;
            info.recent_events = stmt
                .query_map(params![name], |r| {
                    Ok(UnitEventRow {
                        id: r.get(0)?,
                        ts: r.get(1)?,
                        unit: r.get(2)?,
                        kind: r.get(3)?,
                        old_value: r.get(4)?,
                        new_value: r.get(5)?,
                    })
                })?
                .filter_map(Result::ok)
                .collect();
            Ok(Some(info))
        }
        None => Ok(None),
    }
}

/// Returns all unit failure events recorded since the given cutoff timestamp.
pub fn failed_since(conn: &Connection, cutoff_ts: &str) -> rusqlite::Result<Vec<UnitEventRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, unit, kind, old_value, new_value FROM unit_events \
         WHERE kind = 'active' AND new_value = 'failed' AND ts >= ?1 \
         ORDER BY ts DESC LIMIT 200",
    )?;
    let rows = stmt
        .query_map(params![cutoff_ts], |r| {
            Ok(UnitEventRow {
                id: r.get(0)?,
                ts: r.get(1)?,
                unit: r.get(2)?,
                kind: r.get(3)?,
                old_value: r.get(4)?,
                new_value: r.get(5)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// Returns the count of installed packages and tracked units.
pub fn counts(conn: &Connection) -> rusqlite::Result<(i64, i64)> {
    let pkgs: i64 = conn.query_row("SELECT COUNT(*) FROM packages", [], |r| r.get(0))?;
    let units: i64 = conn.query_row("SELECT COUNT(*) FROM units", [], |r| r.get(0))?;
    Ok((pkgs, units))
}

/// Summary statistics of the indexed entities and edges in the World Model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphStats {
    /// Number of distinct installed packages.
    pub packages: i64,
    /// Total require dependency edges.
    pub requires: i64,
    /// Total provide capability edges.
    pub provides: i64,
    /// Number of tracked systemd units.
    pub units: i64,
    /// Number of tracked disk mounts.
    pub disks: i64,
    /// Number of tracked thermal sensors.
    pub thermals: i64,
    /// Number of tracked network interfaces.
    pub net_ifaces: i64,
}

/// Computes summary statistics across all entities in the World Model.
pub fn graph_stats(conn: &Connection) -> rusqlite::Result<GraphStats> {
    let packages: i64 = conn.query_row("SELECT COUNT(*) FROM packages", [], |r| r.get(0))?;
    let requires: i64 = conn.query_row("SELECT COUNT(*) FROM pkg_requires", [], |r| r.get(0))?;
    let provides: i64 = conn.query_row("SELECT COUNT(*) FROM pkg_provides", [], |r| r.get(0))?;
    let units: i64 = conn.query_row("SELECT COUNT(*) FROM units", [], |r| r.get(0))?;
    let disks: i64 = conn
        .query_row("SELECT COUNT(*) FROM system_disks", [], |r| r.get(0))
        .unwrap_or(0);
    let thermals: i64 = conn
        .query_row("SELECT COUNT(*) FROM system_thermals", [], |r| r.get(0))
        .unwrap_or(0);
    let net_ifaces: i64 = conn
        .query_row("SELECT COUNT(*) FROM system_net_ifaces", [], |r| r.get(0))
        .unwrap_or(0);

    Ok(GraphStats {
        packages,
        requires,
        provides,
        units,
        disks,
        thermals,
        net_ifaces,
    })
}

/// Returns all currently tracked filesystem disk snapshots.
pub fn system_disks(conn: &Connection) -> rusqlite::Result<Vec<DiskSnapshot>> {
    let mut stmt = conn.prepare(
        "SELECT mount_point, device, fs_type, total_bytes, used_bytes, avail_bytes, use_percent \
         FROM system_disks ORDER BY mount_point",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DiskSnapshot {
                mount_point: r.get(0)?,
                device: r.get(1)?,
                fs_type: r.get(2)?,
                total_bytes: r.get(3)?,
                used_bytes: r.get(4)?,
                avail_bytes: r.get(5)?,
                use_percent: r.get(6)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// Returns all currently tracked hardware thermal sensor readings.
pub fn system_thermals(conn: &Connection) -> rusqlite::Result<Vec<ThermalSnapshot>> {
    let mut stmt =
        conn.prepare("SELECT zone, temp_celsius, trip_type FROM system_thermals ORDER BY zone")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ThermalSnapshot {
                zone: r.get(0)?,
                temp_celsius: r.get(1)?,
                trip_type: r.get(2)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// Returns all currently tracked network interface operational states.
pub fn system_net_ifaces(conn: &Connection) -> rusqlite::Result<Vec<NetIfaceSnapshot>> {
    let mut stmt = conn.prepare(
        "SELECT name, oper_state, mac_address, carrier FROM system_net_ifaces ORDER BY name",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let carrier_int: i64 = r.get(3)?;
            Ok(NetIfaceSnapshot {
                name: r.get(0)?,
                oper_state: r.get(1)?,
                mac_address: r.get(2)?,
                carrier: carrier_int != 0,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}
