use directioner_services::intent_broker::{IntentBroker, IntentKind, IntentRequest, IntentResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum IntentCommand {
    Status,
    Ping,
    Dispatch {
        utterance: String,
        kind: String,
        slots: HashMap<String, String>,
        confidence: f32,
        authorized: bool,
    },
    AuditTrail,
}

#[derive(Debug, Serialize)]
struct IntentDaemonResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<IntentResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audit: Option<Vec<String>>,
}

struct IntentDaemonState {
    broker: IntentBroker,
    audit_log: Vec<String>,
}

fn determine_socket_path() -> PathBuf {
    if let Ok(p) = env::var("DIRECTIONER_INTENTD_SOCKET") {
        return PathBuf::from(p);
    }
    let args: Vec<String> = env::args().collect();
    for i in 1..args.len() {
        if args[i] == "--socket" && i + 1 < args.len() {
            return PathBuf::from(&args[i + 1]);
        }
    }
    let default_path = Path::new("/run/directioner/intentd.sock");
    if let Some(parent) = default_path.parent()
        && fs::create_dir_all(parent).is_ok()
    {
        return default_path.to_path_buf();
    }
    PathBuf::from("/tmp/directioner-intentd.sock")
}

fn parse_kind(s: &str) -> IntentKind {
    match s {
        "launch_application" => IntentKind::LaunchApplication,
        "set_system_setting" => IntentKind::SetSystemSetting,
        "query_system_status" => IntentKind::QuerySystemStatus,
        "schedule_alarm" => IntentKind::ScheduleAlarm,
        _ => IntentKind::ExecuteAction,
    }
}

fn handle_connection(stream: UnixStream, state: &Arc<Mutex<IntentDaemonState>>) {
    let reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    for line_res in reader.lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cmd: Result<IntentCommand, _> = serde_json::from_str(trimmed);
        let resp = match cmd {
            Ok(IntentCommand::Status) => IntentDaemonResponse {
                ok: true,
                message: Some("Intent daemon running".to_string()),
                result: None,
                audit: None,
            },
            Ok(IntentCommand::Ping) => IntentDaemonResponse {
                ok: true,
                message: Some("pong".to_string()),
                result: None,
                audit: None,
            },
            Ok(IntentCommand::Dispatch {
                utterance,
                kind,
                slots,
                confidence,
                authorized,
            }) => {
                let parsed_kind = parse_kind(&kind);
                let req = IntentRequest {
                    raw_utterance: utterance.clone(),
                    kind: parsed_kind,
                    slots,
                    confidence,
                    execution_authorized: authorized,
                };
                let mut guard = state.lock().unwrap();
                let res = guard.broker.dispatch(req);
                guard.audit_log.push(format!(
                    "{} | {} | authorized={}",
                    chrono::Utc::now().to_rfc3339(),
                    utterance,
                    authorized
                ));
                IntentDaemonResponse {
                    ok: true,
                    message: None,
                    result: Some(res),
                    audit: None,
                }
            }
            Ok(IntentCommand::AuditTrail) => {
                let guard = state.lock().unwrap();
                IntentDaemonResponse {
                    ok: true,
                    message: None,
                    result: None,
                    audit: Some(guard.audit_log.clone()),
                }
            }
            Err(e) => IntentDaemonResponse {
                ok: false,
                message: Some(format!("Invalid command: {}", e)),
                result: None,
                audit: None,
            },
        };

        let mut out = serde_json::to_vec(&resp).unwrap();
        out.push(b'\n');
        if writer.write_all(&out).is_err() {
            break;
        }
    }
}

fn main() {
    let socket_path = determine_socket_path();
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }
    if let Some(parent) = socket_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let listener = UnixListener::bind(&socket_path).unwrap_or_else(|e| {
        eprintln!("Failed to bind {}: {}", socket_path.display(), e);
        std::process::exit(1);
    });

    let _ = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o660));

    let state = Arc::new(Mutex::new(IntentDaemonState {
        broker: IntentBroker::new(0.6),
        audit_log: Vec::new(),
    }));

    let running = Arc::new(AtomicBool::new(true));
    let r_clone = Arc::clone(&running);
    let s_path_clone = socket_path.clone();

    println!(
        "directioner-intentd: listening on {}",
        socket_path.display()
    );

    listener.set_nonblocking(true).unwrap();

    while r_clone.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let st_ref = Arc::clone(&state);
                thread::spawn(move || {
                    handle_connection(stream, &st_ref);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
                break;
            }
        }
    }

    let _ = fs::remove_file(s_path_clone);
}
