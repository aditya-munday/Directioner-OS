//! Directioner-OS System Cockpit CLI Binary.
//!
//! Real-time interactive operational monitor and incident dashboard.

use std::env;
use std::path::PathBuf;
use std::process::exit;
use std::thread;
use std::time::Duration;

use control_plane::cockpit::{CockpitRenderer, CockpitState};

fn print_usage() {
    eprintln!(
        r#"Directioner-OS Operational Cockpit (Phase 3 Dashboard)

USAGE:
    ai-cockpit [OPTIONS]

OPTIONS:
    --db <PATH>          Path to world-model SQLite DB (default: /tmp/ai-engineer/worldmodel.db)
    --audit <PATH>       Path to capability-layer audit.jsonl (default: /tmp/ai-engineer/audit.jsonl)
    --lock <PATH>        Path to killswitch lockfile (default: /tmp/ai-engineer/killswitch.lock)
    --watch              Continuously monitor and refresh dashboard in real time
    --interval <SECS>    Refresh interval in seconds when using --watch [default: 1]
    --json               Output point-in-time telemetry snapshot in JSON format
    -h, --help           Print help information
"#
    );
}

struct CliArgs {
    db_path: PathBuf,
    audit_path: PathBuf,
    lock_path: PathBuf,
    watch: bool,
    interval_secs: u64,
    json: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = env::args().collect();
    let mut db_path = PathBuf::from("/tmp/ai-engineer/worldmodel.db");
    let mut audit_path = PathBuf::from("/tmp/ai-engineer/audit.jsonl");
    let mut lock_path = PathBuf::from("/tmp/ai-engineer/killswitch.lock");
    let mut watch = false;
    let mut interval_secs = 1;
    let mut json = false;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                exit(0);
            }
            "--db" => {
                i += 1;
                if i >= args.len() {
                    return Err("--db requires an argument".into());
                }
                db_path = PathBuf::from(&args[i]);
            }
            "--audit" => {
                i += 1;
                if i >= args.len() {
                    return Err("--audit requires an argument".into());
                }
                audit_path = PathBuf::from(&args[i]);
            }
            "--lock" => {
                i += 1;
                if i >= args.len() {
                    return Err("--lock requires an argument".into());
                }
                lock_path = PathBuf::from(&args[i]);
            }
            "--watch" => {
                watch = true;
            }
            "--interval" => {
                i += 1;
                if i >= args.len() {
                    return Err("--interval requires an integer argument".into());
                }
                interval_secs = args[i].parse::<u64>().unwrap_or(1);
            }
            "--json" => {
                json = true;
            }
            _ => {
                return Err(format!("Unknown option: '{arg}'"));
            }
        }
        i += 1;
    }

    Ok(CliArgs {
        db_path,
        audit_path,
        lock_path,
        watch,
        interval_secs,
        json,
    })
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {e}");
            print_usage();
            exit(1);
        }
    };

    let state = CockpitState::new(&args.db_path, &args.audit_path, &args.lock_path);

    if args.json {
        let snapshot = state.poll(None);
        println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
        return;
    }

    if !args.watch {
        let snapshot = state.poll(None);
        print!("{}", CockpitRenderer::render_terminal(&snapshot));
        return;
    }

    // Continuous watch mode
    loop {
        let snapshot = state.poll(None);
        // Clear screen (ANSI code \x1B[2J\x1B[1;1H)
        print!(
            "\x1B[2J\x1B[1;1H{}",
            CockpitRenderer::render_terminal(&snapshot)
        );
        thread::sleep(Duration::from_secs(args.interval_secs));
    }
}
