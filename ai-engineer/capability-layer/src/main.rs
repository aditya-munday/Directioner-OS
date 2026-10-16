use std::path::PathBuf;
use std::sync::Arc;

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::Server;

use parking_lot::Mutex;

struct Args {
    socket: PathBuf,
    audit_log: PathBuf,
    lock_file: PathBuf,
    work_root: PathBuf,
    dry_run: bool,
    approval_required: bool,
    allowed_uids: Vec<u32>,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = Args {
            socket: PathBuf::from("/run/ai-capabilityd/ai-capabilityd.sock"),
            audit_log: PathBuf::from("/var/lib/ai-engineer/worklog.jsonl"),
            lock_file: PathBuf::from("/run/ai-capabilityd/ai-capabilityd.lock"),
            work_root: PathBuf::from("/home"),
            dry_run: true,
            approval_required: true,
            allowed_uids: Vec::new(),
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--socket" => {
                    args.socket = it.next().ok_or("--socket needs a value")?.into();
                }
                "--audit-log" => {
                    args.audit_log = it.next().ok_or("--audit-log needs a value")?.into();
                }
                "--lock-file" => {
                    args.lock_file = it.next().ok_or("--lock-file needs a value")?.into();
                }
                "--work-root" => {
                    args.work_root = it.next().ok_or("--work-root needs a value")?.into();
                }
                "--allowed-uid" | "--allowed-uids" => {
                    let val = it.next().ok_or("--allowed-uid needs a value")?;
                    capability_layer::server::ServerConfig::parse_uids_into(
                        &val,
                        &mut args.allowed_uids,
                    );
                }
                "--exec" => args.dry_run = false,
                "--auto-approve" => args.approval_required = false,
                "--help" | "-h" => {
                    println!(
                        "ai-capabilityd --socket PATH --audit-log PATH --lock-file PATH \
                         --work-root PATH [--exec] [--auto-approve] [--allowed-uids UID,UID...]"
                    );
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(args)
    }
}

fn main() {
    let args = match Args::parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    let scope = match ScopedPath::new(&args.work_root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "error: cannot use work-root {}: {e}",
                args.work_root.display()
            );
            std::process::exit(2);
        }
    };
    let audit = match AuditLog::open(&args.audit_log) {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "error: cannot open audit log {}: {e}",
                args.audit_log.display()
            );
            std::process::exit(2);
        }
    };
    let killswitch = KillSwitch::load(&args.lock_file).unwrap_or_else(|e| {
        eprintln!(
            "error: cannot read kill-switch lock {}: {e}",
            args.lock_file.display()
        );
        std::process::exit(2);
    });

    if killswitch.is_engaged() {
        eprintln!(
            "kill-switch is ENGAGED ({}): refusing to serve capability requests \
             until `ai-killswitch release` is run",
            args.lock_file.display()
        );
        std::process::exit(3);
    }

    let config = HandlerConfig {
        approval_required: args.approval_required,
        dry_run: args.dry_run,
        ..HandlerConfig::default()
    };
    let handler = Arc::new(Mutex::new(Handler::new(scope, audit, killswitch, config)));

    let mut server_config = capability_layer::server::ServerConfig::default();
    for u in args.allowed_uids {
        if !server_config.allowed_uids.contains(&u) {
            server_config.allowed_uids.push(u);
        }
    }

    let server = match Server::bind_with_config(&args.socket, handler.clone(), server_config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot bind {}: {e}", args.socket.display());
            std::process::exit(2);
        }
    };

    capability_layer::notify::notify_ready();
    let _watchdog_handle = capability_layer::notify::spawn_watchdog_thread();

    eprintln!(
        "ai-capabilityd listening on {} (audit: {}, dry-run: {}, approval-required: {})",
        args.socket.display(),
        args.audit_log.display(),
        args.dry_run,
        args.approval_required
    );

    let res = server.serve();
    capability_layer::notify::notify_stopping();

    if let Err(e) = res {
        eprintln!("server error: {e}");
        std::process::exit(1);
    }
}
