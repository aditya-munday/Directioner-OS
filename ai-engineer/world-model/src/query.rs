use rusqlite::{Connection, params};

#[derive(Debug, Clone, serde::Serialize)]
pub struct RequireInfo {
    pub dep: String,
    pub flags: String,
    pub version: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UnitEventRow {
    pub id: i64,
    pub ts: String,
    pub unit: String,
    pub kind: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UnitInfo {
    pub name: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub enabled: String,
    pub since_ts: String,
    pub recent_events: Vec<UnitEventRow>,
}

pub fn reverse_deps(conn: &Connection, name: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT dependent FROM (
            SELECT pkg AS dependent FROM pkg_requires
             WHERE dep = ?1 OR dep LIKE ?1 || ' %' OR dep LIKE ?1 || '(%'
            UNION
            SELECT pr.pkg AS dependent FROM pkg_requires pr
              JOIN pkg_provides pp ON (
                pr.dep = pp.provide
                OR pr.dep LIKE pp.provide || '(%'
                OR pp.provide LIKE pr.dep || '(%'
                OR pr.dep LIKE pp.provide || ' %'
                OR pp.provide LIKE pr.dep || ' =%'
              )
             WHERE pp.pkg = ?1
         ) ORDER BY dependent",
    )?;
    let rows = stmt
        .query_map(params![name], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

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
                 WHERE unit = ?1 ORDER BY id DESC LIMIT 10",
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

pub fn counts(conn: &Connection) -> rusqlite::Result<(i64, i64)> {
    let pkgs: i64 = conn.query_row("SELECT COUNT(*) FROM packages", [], |r| r.get(0))?;
    let units: i64 = conn.query_row("SELECT COUNT(*) FROM units", [], |r| r.get(0))?;
    Ok((pkgs, units))
}
