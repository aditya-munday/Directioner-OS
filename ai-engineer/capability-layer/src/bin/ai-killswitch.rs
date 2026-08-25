use std::path::PathBuf;
use std::process::exit;

use capability_layer::killswitch::{KillSwitch, KillSwitchState};

fn default_lock() -> PathBuf {
    PathBuf::from("/run/ai-capabilityd/ai-capabilityd.lock")
}

fn usage() -> ! {
    eprintln!("usage: ai-killswitch <engage [--stop-service]|release|status> [--lock-file PATH]");
    exit(2);
}

fn stop_ai_services() {
    let sysctl = find_systemctl();
    match sysctl {
        Some(bin) => {
            let out = std::process::Command::new(&bin)
                .args([
                    "list-units",
                    "--type=service",
                    "--all",
                    "--plain",
                    "--no-legend",
                ])
                .output();
            match out {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    for line in stdout.lines() {
                        let unit = line.split_whitespace().next().unwrap_or("");
                        if unit.starts_with("ai-") {
                            let _ = std::process::Command::new(&bin)
                                .args(["stop", unit])
                                .status();
                            println!("stopped {unit}");
                        }
                    }
                }
                Err(e) => eprintln!("warning: could not list units: {e}"),
            }
        }
        None => {
            println!("systemctl not available; no ai-* services stopped (dev mode)");
        }
    }
}

fn find_systemctl() -> Option<std::path::PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join("systemctl");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut lock = default_lock();

    let mut command: Option<String> = None;
    let mut stop_service = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--lock-file" => {
                i += 1;
                if i >= args.len() {
                    usage();
                }
                lock = PathBuf::from(&args[i]);
            }
            "--stop-service" => stop_service = true,
            "engage" | "release" | "status" => {
                if command.is_some() {
                    usage();
                }
                command = Some(args[i].clone());
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
        i += 1;
    }

    let command = match command {
        Some(c) => c,
        None => usage(),
    };
    let mut ks = match KillSwitch::load(&lock) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("error: cannot read lock file {}: {e}", lock.display());
            exit(2);
        }
    };

    match command.as_str() {
        "engage" => {
            if let Err(e) = ks.engage() {
                eprintln!("error: cannot engage kill-switch: {e}");
                exit(1);
            }
            println!(
                "kill-switch ENGAGED at {} — ai-capabilityd refuses all requests until released",
                lock.display()
            );
            if stop_service {
                stop_ai_services();
            }
        }
        "release" => match ks.release() {
            Ok(()) => println!("kill-switch released — ai-capabilityd may serve requests again"),
            Err(e) => {
                eprintln!("error: cannot release kill-switch: {e}");
                exit(1);
            }
        },
        "status" => {
            println!(
                "kill-switch: {}",
                match ks.state() {
                    KillSwitchState::Engaged => "ENGAGED",
                    KillSwitchState::Disengaged => "disengaged",
                }
            );
        }
        _ => usage(),
    }
}
