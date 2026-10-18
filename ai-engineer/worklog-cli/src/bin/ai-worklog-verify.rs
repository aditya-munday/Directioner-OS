use std::path::PathBuf;
use std::process::exit;

use capability_layer::audit::AuditLog;

fn usage() -> ! {
    eprintln!("usage: ai-worklog-verify [--log PATH] [--json]");
    eprintln!();
    eprintln!("Verifies hash chain integrity of the Work Log.");
    eprintln!("Security limits: Detects modification, deletion, and reordering.");
    eprintln!("Does not detect wholesale file replacement or unanchored tail truncation.");
    exit(2);
}

fn default_log() -> PathBuf {
    PathBuf::from("/var/lib/ai-engineer/worklog.jsonl")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut log_path = default_log();
    let mut json_mode = false;
    let mut idx = 0;

    while idx < args.len() {
        match args[idx].as_str() {
            "--log" => {
                idx += 1;
                if idx >= args.len() {
                    usage();
                }
                log_path = PathBuf::from(&args[idx]);
            }
            "--json" => {
                json_mode = true;
            }
            "--help" | "-h" => {
                usage();
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
        idx += 1;
    }

    if !log_path.exists() {
        if json_mode {
            println!(
                "{}",
                serde_json::json!({
                    "valid": true,
                    "count_verified": 0,
                    "count_unchained": 0,
                    "broken_index": null,
                    "error_message": null,
                    "note": "log file does not exist (empty log)"
                })
            );
        } else {
            println!(
                "Work log does not exist ({}), treating as empty valid log.",
                log_path.display()
            );
        }
        exit(0);
    }

    let log = match AuditLog::open_read_only(&log_path) {
        Ok(l) => l,
        Err(e) => {
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "valid": false,
                        "count_verified": 0,
                        "count_unchained": 0,
                        "broken_index": null,
                        "error_message": format!("cannot open log: {e}")
                    })
                );
            } else {
                eprintln!("error: cannot open work log {}: {e}", log_path.display());
            }
            exit(2);
        }
    };

    let report = match log.verify() {
        Ok(r) => r,
        Err(e) => {
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "valid": false,
                        "count_verified": 0,
                        "count_unchained": 0,
                        "broken_index": null,
                        "error_message": format!("verification read error: {e}")
                    })
                );
            } else {
                eprintln!("error: failed to read work log for verification: {e}");
            }
            exit(2);
        }
    };

    if json_mode {
        println!(
            "{}",
            serde_json::json!({
                "valid": report.is_valid(),
                "count_verified": report.count_verified,
                "count_unchained": report.count_unchained,
                "broken_index": report.broken_index,
                "error_message": report.error_message,
            })
        );
    } else if report.is_valid() {
        println!(
            "Work log verification PASSED: {} verified, {} unchained.",
            report.count_verified, report.count_unchained
        );
    } else {
        eprintln!(
            "Work log verification FAILED at sequence #{}: {}",
            report.broken_index.unwrap_or(0),
            report.error_message.as_deref().unwrap_or("corrupted")
        );
        eprintln!(
            "Verified before failure: {}, unchained: {}",
            report.count_verified, report.count_unchained
        );
    }

    if report.is_valid() {
        exit(0);
    } else {
        exit(1);
    }
}
