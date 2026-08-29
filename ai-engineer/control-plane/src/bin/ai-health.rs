//! Directioner-OS Health Model CLI (`ai-health`).
//!
//! Query surface for multi-signal system health evaluation (Phase 3.0.2).
//! Evaluates subsystem scores strictly from World Model and Work Log event streams.
//!
//! USAGE:
//!     ai-health [OPTIONS] <COMMAND>
//!
//! COMMANDS:
//!     status       Evaluate and display current system health scores and evidence
//!
//! OPTIONS:
//!     --db <PATH>         Path to world-model SQLite DB [default: /tmp/wm.sqlite]
//!     --worklog <PATH>    Path to capability-layer audit log [default: /var/log/live-worklog.jsonl]
//!     --json              Output evaluation in structured JSON format
//!     -h, --help          Print help information

use std::env;
use std::path::PathBuf;
use std::process::exit;

use control_plane::health_model::{
    HealthModel, HealthModelConfig, HealthStatus, SystemHealthReport,
};

fn print_usage() {
    eprintln!(
        r#"Directioner-OS Health Model Evaluator (Phase 3.0.2)

USAGE:
    ai-health [OPTIONS] status

OPTIONS:
    --db <PATH>         Path to world-model SQLite DB (default: /tmp/wm.sqlite)
    --worklog <PATH>    Path to capability-layer audit log (default: /var/log/live-worklog.jsonl)
    --json              Output health report in structured JSON format
    -h, --help          Print help information
"#
    );
}

struct CliArgs {
    db_path: PathBuf,
    worklog_path: Option<PathBuf>,
    json: bool,
    command: String,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = env::args().collect();
    let mut db_path = PathBuf::from("/tmp/wm.sqlite");
    let mut worklog_path = Some(PathBuf::from("/var/log/live-worklog.jsonl"));
    let mut json = false;
    let mut command = String::new();

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
                    return Err("--db requires a path argument".into());
                }
                db_path = PathBuf::from(&args[i]);
            }
            "--worklog" => {
                i += 1;
                if i >= args.len() {
                    return Err("--worklog requires a path argument".into());
                }
                worklog_path = Some(PathBuf::from(&args[i]));
            }
            "--json" => {
                json = true;
            }
            "status" => {
                command = "status".into();
            }
            other if !other.starts_with('-') => {
                command = other.into();
            }
            _ => {
                return Err(format!("Unknown option: '{arg}'"));
            }
        }
        i += 1;
    }

    if command.is_empty() {
        command = "status".into();
    }

    Ok(CliArgs {
        db_path,
        worklog_path,
        json,
        command,
    })
}

fn render_terminal(report: &SystemHealthReport) {
    let status_color = match report.overall_status {
        HealthStatus::Healthy => "\x1b[1;32m",  // Bright Green
        HealthStatus::Degraded => "\x1b[1;33m", // Bright Yellow
        HealthStatus::Critical => "\x1b[1;31m", // Bright Red
        HealthStatus::Unknown => "\x1b[1;37m",  // Bright White
    };
    let reset = "\x1b[0m";

    let score_str = match report.composite_score {
        Some(s) => format!("{s}/100"),
        None => "N/A".to_string(),
    };

    println!("╔══════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                    DIRECTIONER-OS HEALTH MODEL EVALUATOR (PHASE 3.0.2)                   ║");
    println!("╠══════════════════════════════════════════════════════════════════════════════════════════╣");
    println!(
        "║  Overall Status: {status_color}{:<10}{reset} │  Composite Score: {:<7} │  Timestamp: {:<20} ║",
        format!("{}", report.overall_status),
        score_str,
        &report.evaluated_at[..19]
    );
    println!("╠══════════════════════════════════════════════════════════════════════════════════════════╣");
    println!("║  Summary: {:<78} ║", truncate_str(&report.summary, 78));
    println!("╠══════════════════════════════════════════════════════════════════════════════════════════╣");
    println!("║  #  │ Subsystem   │ Status     │ Score │ Summary                                        ║");
    println!("╟─────┼─────────────┼────────────┼───────┼────────────────────────────────────────────────╢");

    for (idx, sub) in report.subsystems.iter().enumerate() {
        let sub_color = match sub.status {
            HealthStatus::Healthy => "\x1b[32m",
            HealthStatus::Degraded => "\x1b[33m",
            HealthStatus::Critical => "\x1b[31m",
            HealthStatus::Unknown => "\x1b[37m",
        };
        let sub_score_str = match sub.score {
            Some(s) => format!("{s:>3}/100"),
            None => "  N/A  ".to_string(),
        };

        println!(
            "║ {:>2}  │ {:<11} │ {}{:<10}{} │ {} │ {:<46} ║",
            idx + 1,
            sub.name,
            sub_color,
            format!("{}", sub.status),
            reset,
            sub_score_str,
            truncate_str(&sub.summary, 46)
        );
    }

    println!("╠══════════════════════════════════════════════════════════════════════════════════════════╣");
    println!("║ Traceable Underlying Evidence:                                                           ║");

    let mut evidence_count = 0;
    for sub in &report.subsystems {
        for ev in &sub.evidence {
            evidence_count += 1;
            let sev_color = match ev.severity.as_str() {
                "critical" => "\x1b[31m[CRIT]\x1b[0m",
                "warning" => "\x1b[33m[WARN]\x1b[0m",
                _ => "\x1b[36m[INFO]\x1b[0m",
            };
            let desc = format!("{} {}: {}", ev.source, ev.entity, ev.description);
            println!(
                "║  • {} {:<77} ║",
                sev_color,
                truncate_str(&desc, 77)
            );
        }
    }

    if evidence_count == 0 {
        println!("║  (No specific evidence items recorded for current state)                                ║");
    }

    println!("╚══════════════════════════════════════════════════════════════════════════════════════════╝");
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        format!("{:<width$}", s, width = max)
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n");
            print_usage();
            exit(1);
        }
    };

    if args.command != "status" {
        eprintln!("error: unknown command '{}'", args.command);
        print_usage();
        exit(1);
    }

    let config = HealthModelConfig {
        db_path: args.db_path,
        worklog_path: args.worklog_path,
    };

    let model = HealthModel::new(config);
    let report = model.evaluate();

    if args.json {
        match serde_json::to_string_pretty(&report) {
            Ok(j) => println!("{j}"),
            Err(e) => {
                eprintln!("error serializing health report to JSON: {e}");
                exit(1);
            }
        }
    } else {
        render_terminal(&report);
    }
}
