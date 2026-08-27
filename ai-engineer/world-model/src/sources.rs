use crate::model::{PackageData, PackageRow, ProvideEdge, RequireEdge, UnitSnapshot};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct CmdSpec {
    pub program: std::path::PathBuf,
    pub args: Vec<String>,
    pub path_override: Option<std::path::PathBuf>,
}

impl CmdSpec {
    pub fn new(program: impl Into<std::path::PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            path_override: None,
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn with_path_override(mut self, dir: Option<&Path>) -> Self {
        self.path_override = dir.map(|p| p.to_path_buf());
        self
    }

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

pub const REPOQUERY_QF: &str = "PKG|%{NAME}|%{EVR}|%{ARCH}|%{SUMMARY}\n\
[REQ|%{REQUIRESNAME}|%{REQUIRESFLAGS}|%{REQUIRESVERSION}\n]\
[PROV|%{PROVIDESNAME}|%{PROVIDESVERSION}\n]\
END\n";

pub fn dnf5_repoquery_spec(path_override: Option<&Path>) -> CmdSpec {
    CmdSpec::new("dnf5")
        .arg("repoquery")
        .arg("--installed")
        .arg("--queryformat")
        .arg(REPOQUERY_QF)
        .with_path_override(path_override)
}

pub fn rpm_qa_spec(path_override: Option<&Path>) -> CmdSpec {
    CmdSpec::new("rpm")
        .arg("-qa")
        .arg("--qf")
        .arg(REPOQUERY_QF)
        .with_path_override(path_override)
}

pub fn systemctl_list_units_spec(path_override: Option<&Path>) -> CmdSpec {
    CmdSpec::new("systemctl")
        .arg("list-units")
        .arg("--type=service")
        .arg("--all")
        .arg("--output=json")
        .with_path_override(path_override)
}

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
