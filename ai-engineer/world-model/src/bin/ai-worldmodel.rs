use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use world_model::model::UnitChange;
use world_model::{cutoff_rfc3339, db, query, sources, sync};

fn usage() -> ! {
    eprintln!(
        "usage: ai-worldmodel [--db PATH] [--worklog PATH] [--json] \
         <sync | depends-on PKG | requires PKG | unit NAME | failed-since DUR | watch --interval SECS>"
    );
    exit(2);
}

fn parse_duration(s: &str) -> Option<Duration> {
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = num.parse().ok()?;
    match unit {
        "s" => Some(Duration::from_secs(n)),
        "m" => Some(Duration::from_secs(n * 60)),
        "h" => Some(Duration::from_secs(n * 3600)),
        "d" => Some(Duration::from_secs(n * 86400)),
        _ => None,
    }
}

struct Ctx {
    db_path: PathBuf,
    worklog: Option<PathBuf>,
}

impl Ctx {
    fn open_db(&self) -> Result<rusqlite::Connection, String> {
        db::open(&self.db_path).map_err(|e| format!("open db {}: {e}", self.db_path.display()))
    }

    fn run_pkg_source(spec: &sources::CmdSpec) -> Result<world_model::model::PackageData, String> {
        let out = spec.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "{} exited {}: {}",
                spec.program.display(),
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        sources::parse_line_protocol(&text)
    }

    fn sync_packages_auto(&self) -> Result<(usize, &'static str), String> {
        let attempted = Self::run_pkg_source(&sources::dnf5_repoquery_spec(None));
        let (data, adapter) = match attempted {
            Ok(data) if !data.packages.is_empty() => (data, "dnf5 repoquery"),
            first => {
                let err1 = first
                    .err()
                    .unwrap_or_else(|| "dnf5 repoquery returned no packages".into());
                let data = Self::run_pkg_source(&sources::rpm_qa_spec(None))
                    .map_err(|err2| format!("dnf5: {err1}; rpm fallback: {err2}"))?;
                (data, "rpm -qa")
            }
        };
        let conn = self.open_db()?;
        let n = sync::sync_packages(&conn, &data)?;
        Ok((n, adapter))
    }

    fn sync_units_once(&self) -> Result<(usize, Vec<UnitChange>), String> {
        let snap = sources::load_unit_snapshot(
            &sources::systemctl_list_units_spec(None),
            &sources::systemctl_unit_files_spec(None),
        )?;
        let conn = self.open_db()?;
        sync::apply_unit_snapshot(&conn, &snap)
    }

    fn full_sync(&self) -> Result<(), String> {
        let (n, adapter) = self.sync_packages_auto()?;
        let (units, changes) = self.sync_units_once()?;
        println!(
            "synced {n} packages via {adapter}; {units} units; {} unit event(s)",
            changes.len()
        );
        for c in &changes {
            let arrow = if c.old_value.is_empty() {
                String::new()
            } else {
                format!(": {} -> {}", c.old_value, c.new_value)
            };
            println!("  [{}] {}{}", c.kind, c.unit, arrow);
        }
        if let Some(wl) = &self.worklog {
            let conn = self.open_db()?;
            sync::mark_worklog(&conn, wl);
        }
        Ok(())
    }

    fn maybe_resync(&self) -> Result<(), String> {
        if let Some(wl) = &self.worklog {
            let conn = self.open_db()?;
            if sync::worklog_stale(&conn, wl) {
                eprintln!("note: work log changed since last sync; resyncing...");
                drop(conn);
                self.full_sync()?;
            }
        }
        Ok(())
    }
}

fn main() {
    let mut raw: Vec<String> = std::env::args().skip(1).collect();
    let mut ctx = Ctx {
        db_path: std::env::var_os("AI_WORLD_MODEL_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("world-model.sqlite")),
        worklog: None,
    };
    let mut json_out = false;
    let mut rest: Vec<String> = Vec::new();

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--db" => {
                i += 1;
                if i >= raw.len() {
                    usage();
                }
                ctx.db_path = PathBuf::from(&raw[i]);
            }
            "--worklog" => {
                i += 1;
                if i >= raw.len() {
                    usage();
                }
                ctx.worklog = Some(PathBuf::from(&raw[i]));
            }
            "--json" => json_out = true,
            other => rest.push(other.to_string()),
        }
        i += 1;
    }
    raw = rest;

    let command = match raw.first() {
        Some(c) => c.clone(),
        None => usage(),
    };
    let cmd_args = raw[1..].to_vec();

    let result: Result<(), String> = (|| match command.as_str() {
        "sync" => ctx.full_sync(),

        "depends-on" | "reverse-deps" => {
            let pkg = cmd_args.first().ok_or("depends-on needs a package name")?;
            ctx.maybe_resync()?;
            let conn = ctx.open_db()?;
            let rows = query::reverse_deps(&conn, pkg).map_err(|e| e.to_string())?;
            if json_out {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                println!("{} package(s) depend on {pkg}:", rows.len());
                for r in rows {
                    println!("  {r}");
                }
            }
            Ok(())
        }

        "requires" => {
            let pkg = cmd_args.first().ok_or("requires needs a package name")?;
            ctx.maybe_resync()?;
            let conn = ctx.open_db()?;
            let rows = query::requires(&conn, pkg).map_err(|e| e.to_string())?;
            if json_out {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                println!("{pkg} requires {} thing(s):", rows.len());
                for r in rows {
                    if r.flags.is_empty() && r.version.is_empty() {
                        println!("  {}", r.dep);
                    } else {
                        println!("  {} {} {}", r.dep, r.flags, r.version);
                    }
                }
            }
            Ok(())
        }

        "unit" => {
            let name = cmd_args.first().ok_or("unit needs a unit name")?;
            ctx.maybe_resync()?;
            let conn = ctx.open_db()?;
            let info = query::unit_info(&conn, name)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("unknown unit: {name}"))?;
            if json_out {
                println!("{}", serde_json::to_string_pretty(&info).unwrap());
            } else {
                println!(
                    "{}: load={} active={} sub={} enabled={} since={}",
                    info.name,
                    info.load_state,
                    info.active_state,
                    info.sub_state,
                    info.enabled,
                    info.since_ts
                );
                for e in &info.recent_events {
                    let arrow = if e.old_value.is_empty() {
                        e.new_value.clone()
                    } else {
                        format!("{} -> {}", e.old_value, e.new_value)
                    };
                    println!("  #{} [{}] {}: {}", e.id, e.ts, e.kind, arrow);
                }
            }
            Ok(())
        }

        "failed-since" => {
            let dur = cmd_args
                .first()
                .ok_or("failed-since needs a duration like 1h")?;
            let d = parse_duration(dur)
                .ok_or_else(|| format!("bad duration '{dur}' (use Ns|Nm|Nh|Nd)"))?;
            ctx.maybe_resync()?;
            let conn = ctx.open_db()?;
            let cutoff = cutoff_rfc3339(d);
            let rows = query::failed_since(&conn, &cutoff).map_err(|e| e.to_string())?;
            if json_out {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                println!("{} unit failure(s) within {dur}:", rows.len());
                for e in rows {
                    println!(
                        "  [{}] {} ({} -> {})",
                        e.ts, e.unit, e.old_value, e.new_value
                    );
                }
            }
            Ok(())
        }

        "watch" => {
            let mut interval = 30u64;
            let mut it = cmd_args.iter();
            while let Some(a) = it.next() {
                if a == "--interval" {
                    interval = it.next().and_then(|v| v.parse().ok()).unwrap_or(30);
                }
            }
            loop {
                match ctx.sync_units_once() {
                    Ok((_, changes)) => {
                        for c in &changes {
                            let arrow = if c.old_value.is_empty() {
                                c.new_value.clone()
                            } else {
                                format!("{} -> {}", c.old_value, c.new_value)
                            };
                            println!(
                                "[{}] {}: {}: {}",
                                world_model::now_rfc3339(),
                                c.unit,
                                c.kind,
                                arrow
                            );
                        }
                    }
                    Err(e) => eprintln!("watch error: {e}"),
                }
                std::thread::sleep(Duration::from_secs(interval.max(5)));
            }
        }

        other => {
            eprintln!("unknown command: {other}");
            usage();
        }
    })();

    if let Err(e) = result {
        eprintln!("error: {e}");
        exit(1);
    }
}
