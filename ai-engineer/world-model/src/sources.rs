//! Data extraction adapters and source readers for the World Model.
//!
//! Provides parsers for `dnf5`/`rpm` package line protocols, `systemctl` JSON snapshots,
//! `/proc/mounts` storage utilization, `/sys/class/thermal` sensors, and network interface telemetry.

use crate::model::{
    DiskSnapshot, NetIfaceSnapshot, PackageData, PackageRow, ProvideEdge, RequireEdge,
    ThermalSnapshot, UnitSnapshot,
};
use std::path::Path;
use std::process::Command;

/// Command specification wrapper allowing custom execution paths and PATH overrides.
#[derive(Debug, Clone)]
pub struct CmdSpec {
    /// Executable program path or name.
    pub program: std::path::PathBuf,
    /// Command line arguments.
    pub args: Vec<String>,
    /// Optional directory to prepend to `PATH` environment variable.
    pub path_override: Option<std::path::PathBuf>,
}

impl CmdSpec {
    /// Creates a new command specification for the given program.
    pub fn new(program: impl Into<std::path::PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            path_override: None,
        }
    }

    /// Appends an argument to the command specification.
    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    /// Configures an optional directory override prepended to the system PATH.
    pub fn with_path_override(mut self, dir: Option<&Path>) -> Self {
        self.path_override = dir.map(|p| p.to_path_buf());
        self
    }

    /// Executes the configured command and returns the standard output.
    pub fn output(&self) -> std::io::Result<std::process::Output> {
        let mut c = Command::new(&self.program);
        c.args(&self.args);
        if let Some(dir) = &self.path_override {
            let mut paths = vec![dir.clone()];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
            if let Ok(joined) = std::env::join_paths(paths) {
                c.env("PATH", joined);
            }
        }
        c.output()
    }
}

/// Queryformat string for DNF5 / RPM repoquery line protocol extraction.
pub const REPOQUERY_QF: &str = "PKG|%{NAME}|%{EVR}|%{ARCH}|%{SUMMARY}\n\
[REQ|%{REQUIRENAME}|%{REQUIREFLAGS:depflags}|%{REQUIREVERSION}\n]\
[PROV|%{PROVIDENAME}|%{PROVIDEVERSION}\n]\
END\n";

/// Returns a [`CmdSpec`] targeting `dnf5 repoquery --installed`.
pub fn dnf5_repoquery_spec(path_override: Option<&Path>) -> CmdSpec {
    CmdSpec::new("dnf5")
        .arg("repoquery")
        .arg("--installed")
        .arg("--queryformat")
        .arg(REPOQUERY_QF)
        .with_path_override(path_override)
}

/// Returns a [`CmdSpec`] targeting `rpm -qa`.
pub fn rpm_qa_spec(path_override: Option<&Path>) -> CmdSpec {
    CmdSpec::new("rpm")
        .arg("-qa")
        .arg("--qf")
        .arg(REPOQUERY_QF)
        .with_path_override(path_override)
}

/// Returns a [`CmdSpec`] targeting `systemctl list-units`.
pub fn systemctl_list_units_spec(path_override: Option<&Path>) -> CmdSpec {
    CmdSpec::new("systemctl")
        .arg("list-units")
        .arg("--type=service")
        .arg("--all")
        .arg("--output=json")
        .with_path_override(path_override)
}

/// Returns a [`CmdSpec`] targeting `systemctl list-unit-files`.
pub fn systemctl_unit_files_spec(path_override: Option<&Path>) -> CmdSpec {
    CmdSpec::new("systemctl")
        .arg("list-unit-files")
        .arg("--type=service")
        .arg("--output=json")
        .with_path_override(path_override)
}

fn sanitize(field: &str) -> String {
    field.replace(['\t', '\r', '\n'], " ").trim().to_string()
}

/// Parses the line protocol text emitted by `dnf5 repoquery` or `rpm -qa` into [`PackageData`].
pub fn parse_line_protocol(text: &str) -> Result<PackageData, String> {
    let mut data = PackageData::default();
    let mut current: Option<String> = None;

    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('|').collect();
        match cols[0] {
            "PKG" => {
                if cols.len() < 5 {
                    return Err(format!(
                        "line {}: PKG needs at least 5 columns, got {}",
                        idx + 1,
                        cols.len()
                    ));
                }
                let name = cols[1].to_string();
                let evr = cols[2].to_string();
                let arch = cols[3].to_string();
                let summary = sanitize(&cols[4..].join("|"));
                current = Some(name.clone());
                if let Some(existing) = data.packages.iter_mut().find(|p| p.name == name) {
                    *existing = PackageRow {
                        name,
                        evr,
                        arch,
                        summary,
                    };
                } else {
                    data.packages.push(PackageRow {
                        name,
                        evr,
                        arch,
                        summary,
                    });
                }
            }
            "REQ" => {
                let pkg = current
                    .as_ref()
                    .ok_or_else(|| format!("line {}: REQ before any PKG", idx + 1))?;
                if cols.len() < 4 {
                    return Err(format!(
                        "line {}: REQ needs at least 4 columns, got {}",
                        idx + 1,
                        cols.len()
                    ));
                }
                let dep = cols[1].trim();
                if dep.is_empty() || dep == "(none)" || dep.starts_with('(') {
                    continue;
                }
                data.requires.push(RequireEdge {
                    pkg: pkg.clone(),
                    dep: dep.to_string(),
                    flags: cols[2].to_string(),
                    version: cols[3..].join("|"),
                });
            }
            "PROV" => {
                let pkg = current
                    .as_ref()
                    .ok_or_else(|| format!("line {}: PROV before any PKG", idx + 1))?;
                if cols.len() < 3 {
                    return Err(format!(
                        "line {}: PROV needs at least 3 columns, got {}",
                        idx + 1,
                        cols.len()
                    ));
                }
                let prov = cols[1].trim();
                if prov.is_empty() || prov == "(none)" {
                    continue;
                }
                data.provides.push(ProvideEdge {
                    pkg: pkg.clone(),
                    provide: prov.to_string(),
                    version: cols[2..].join("|"),
                });
            }
            "END" => {
                current = None;
            }
            _ => continue,
        }
    }

    Ok(data)
}

fn json_array(bytes: &[u8], what: &str) -> Result<Vec<serde_json::Value>, String> {
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("{what}: invalid JSON from systemctl: {e}"))?;
    Ok(v.as_array().cloned().unwrap_or_default())
}

/// Loads and reconciles unit states from `systemctl list-units` and `list-unit-files`.
pub fn load_unit_snapshot(
    units_spec: &CmdSpec,
    files_spec: &CmdSpec,
) -> Result<Vec<UnitSnapshot>, String> {
    let out_u = units_spec
        .output()
        .map_err(|e| format!("systemctl list-units: {e}"))?;
    if !out_u.status.success() {
        return Err(format!(
            "systemctl list-units failed: {}",
            String::from_utf8_lossy(&out_u.stderr).trim()
        ));
    }
    let out_f = files_spec
        .output()
        .map_err(|e| format!("systemctl list-unit-files: {e}"))?;

    let mut enabled_by_name = std::collections::HashMap::new();
    for item in json_array(&out_f.stdout, "list-unit-files")? {
        if let (Some(name), Some(state)) = (
            item.get("unit_file").and_then(|v| v.as_str()),
            item.get("state").and_then(|v| v.as_str()),
        ) {
            enabled_by_name.insert(name.to_string(), state.to_string());
        }
    }

    let mut snap = Vec::new();
    for item in json_array(&out_u.stdout, "list-units")? {
        let name = item.get("unit").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        snap.push(UnitSnapshot {
            name: name.to_string(),
            load_state: item
                .get("load")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .into(),
            active_state: item
                .get("active")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .into(),
            sub_state: item
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .into(),
            enabled: enabled_by_name
                .get(name)
                .cloned()
                .unwrap_or_else(|| "unknown".into()),
        });
    }
    snap.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(snap)
}

/// Parses raw mount text (e.g. `/proc/mounts`) and synthetic disk capacity records into [`DiskSnapshot`].
pub fn parse_mounts_protocol(text: &str) -> Vec<DiskSnapshot> {
    let mut disks = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 {
            // Expected format: <device> <mount_point> <fs_type> <total_bytes> <used_bytes> <avail_bytes>
            let device = parts[0].to_string();
            let mount_point = parts[1].to_string();
            let fs_type = parts[2].to_string();
            let total_bytes = parts[3].parse::<u64>().unwrap_or(0);
            let used_bytes = parts[4].parse::<u64>().unwrap_or(0);
            let avail_bytes = parts[5].parse::<u64>().unwrap_or(0);
            let use_percent = if total_bytes > 0 {
                (used_bytes as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };

            disks.push(DiskSnapshot {
                mount_point,
                device,
                fs_type,
                total_bytes,
                used_bytes,
                avail_bytes,
                use_percent,
            });
        }
    }
    disks
}

/// Parses thermal sensor reading tuples (`zone`, `temp_milli_celsius`, `trip_type`) into [`ThermalSnapshot`].
pub fn parse_thermals_data(entries: &[(&str, f64, &str)]) -> Vec<ThermalSnapshot> {
    entries
        .iter()
        .map(|(zone, temp_celsius, trip)| ThermalSnapshot {
            zone: zone.to_string(),
            temp_celsius: *temp_celsius,
            trip_type: trip.to_string(),
        })
        .collect()
}

/// Parses network interface telemetry entries (`name`, `oper_state`, `mac_address`, `carrier`) into [`NetIfaceSnapshot`].
pub fn parse_net_ifaces_data(entries: &[(&str, &str, &str, bool)]) -> Vec<NetIfaceSnapshot> {
    entries
        .iter()
        .map(|(name, state, mac, carrier)| NetIfaceSnapshot {
            name: name.to_string(),
            oper_state: state.to_string(),
            mac_address: mac.to_string(),
            carrier: *carrier,
        })
        .collect()
}
