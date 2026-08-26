use std::path::PathBuf;
use std::process::exit;

use capability_layer::audit::{AuditLog, AuditOutcome};

fn usage() -> ! {
    eprintln!(
        "usage: ai-worklog [--log PATH] <tail [--lines N] | show <id> | today | search <term>>"
    );
    exit(2);
}

fn default_log() -> PathBuf {
    PathBuf::from("/var/lib/ai-engineer/worklog.jsonl")
}

fn scan_and_print_alerts(entries: &[capability_layer::audit::AuditEntry]) {
    let failed_rollbacks: Vec<_> = entries
        .iter()
        .filter(|e| {
            let is_rollback = e.request_id.contains("rb-") || e.request_id.contains("rollback");
            let is_failure = matches!(
                e.outcome,
                AuditOutcome::Denied { .. } | AuditOutcome::Error { .. }
            );
            is_rollback && is_failure
        })
        .collect();

    if !failed_rollbacks.is_empty() {
        eprintln!(
            "╔══════════════════════════════════════════════════════════════════════════════════════╗"
        );
        eprintln!(
            "║  🚨 CRITICAL SYSTEM ALERT: COMPENSATING ROLLBACK FAILURE DETECTED                    ║"
        );
        for fr in &failed_rollbacks {
            let err_msg = match &fr.outcome {
                AuditOutcome::Denied { reason } => format!("DENIED: {reason}"),
                AuditOutcome::Error { message } => format!("ERROR: {message}"),
                _ => "FAILED".into(),
            };
            eprintln!(
                "║  • Seq #{}: '{}' on {} -> {} ║",
                fr.seq, fr.capability, fr.request_id, err_msg
            );
        }
        eprintln!(
            "║  ACTION REQUIRED: System state may be dirty or inconsistent. Inspect immediately.   ║"
        );
        eprintln!(
            "╚══════════════════════════════════════════════════════════════════════════════════════╝\n"
        );
    }
}

fn print_entry(idx: usize, e: &capability_layer::audit::AuditEntry) {
    println!(
        "#{} [{}] {} by {} ({} ms)",
        e.seq, e.timestamp, e.capability, e.requester, e.duration_ms
    );
    if idx != usize::MAX {
        println!("   seq={} id={}", e.seq, e.request_id);
    }
    println!(
        "   args: {}",
        serde_json::to_string(&e.args).unwrap_or_default()
    );
    match &e.outcome {
        AuditOutcome::Success { result } => {
            println!(
                "   result: OK {}",
                serde_json::to_string(result).unwrap_or_default()
            );
        }
        AuditOutcome::Denied { reason } => println!("   result: DENIED ({reason})"),
        AuditOutcome::Error { message } => println!("   result: ERROR ({message})"),
    }
    println!();
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut log = default_log();
    let mut idx = 0usize;
    let mut command: Option<String> = None;
    let mut n = 20usize;
    let mut term: Option<String> = None;

    while idx < raw.len() {
        match raw[idx].as_str() {
            "--log" => {
                idx += 1;
                if idx >= raw.len() {
                    usage();
                }
                log = PathBuf::from(&raw[idx]);
            }
            "--lines" => {
                idx += 1;
                if idx >= raw.len() {
                    usage();
                }
                n = raw[idx].parse().unwrap_or(20);
            }
            "tail" | "today" | "search" => {
                if command.is_some() {
                    usage();
                }
                command = Some(raw[idx].clone());
                if raw[idx] == "search" {
                    idx += 1;
                    if idx >= raw.len() {
                        usage();
                    }
                    term = Some(raw[idx].clone());
                }
            }
            "show" => {
                if command.is_some() {
                    usage();
                }
                command = Some("show".into());
                idx += 1;
                if idx >= raw.len() {
                    usage();
                }
                term = Some(raw[idx].clone());
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
        idx += 1;
    }

    let open = match AuditLog::open(&log) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: cannot open work log {}: {e}", log.display());
            exit(1);
        }
    };

    match command.as_deref() {
        Some("tail") => {
            let entries = match open.tail(n) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {e}");
                    exit(1);
                }
            };
            scan_and_print_alerts(&entries);
            for e in &entries {
                print_entry(usize::MAX, e);
            }
        }
        Some("today") => {
            let entries = match open.read_all() {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {e}");
                    exit(1);
                }
            };
            scan_and_print_alerts(&entries);
            let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
            for e in entries.iter().filter(|e| e.timestamp.starts_with(&today)) {
                print_entry(usize::MAX, e);
            }
        }
        Some("show") => {
            let want: u64 = match term.as_deref().unwrap_or("").parse() {
                Ok(v) => v,
                Err(_) => {
                    eprintln!("error: show needs a numeric seq");
                    exit(2);
                }
            };
            let entries = match open.read_all() {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {e}");
                    exit(1);
                }
            };
            match entries.iter().find(|e| e.seq == want) {
                Some(e) => print_entry(e.seq as usize, e),
                None => {
                    eprintln!("no entry with seq {want}");
                    exit(1);
                }
            }
        }
        Some("search") => {
            let needle = term.unwrap_or_default();
            let entries = match open.read_all() {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {e}");
                    exit(1);
                }
            };
            let mut found = 0usize;
            for e in entries {
                let hay = format!(
                    "{}{}{}",
                    e.capability,
                    e.requester,
                    serde_json::to_string(&e.args).unwrap_or_default()
                );
                if hay.contains(&needle) {
                    print_entry(e.seq as usize, &e);
                    found += 1;
                }
            }
            if found == 0 {
                println!("no entries matched '{needle}'");
                exit(1);
            }
        }
        _ => usage(),
    }
}
