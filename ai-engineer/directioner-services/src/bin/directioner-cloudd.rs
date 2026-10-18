use directioner_services::platform_client::{CircuitBreaker, CloudSyncEngine, SyncStatus};
use serde::{Deserialize, Serialize};
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
enum CloudCommand {
    Status,
    Ping,
    RegisterFile {
        path: String,
        hash: String,
        size: u64,
        modified: i64,
    },
    Reconcile {
        path: String,
        remote_hash: String,
        remote_modified: i64,
    },
    ResolveConflict {
        path: String,
        keep_local: bool,
    },
    DrainQueue,
}

#[derive(Debug, Serialize)]
struct CloudResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pending_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync_status: Option<SyncStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forked_path: Option<String>,
}

struct DaemonState {
    engine: CloudSyncEngine,
    breaker: CircuitBreaker,
}

fn determine_socket_path() -> PathBuf {
    if let Ok(p) = env::var("DIRECTIONER_CLOUDD_SOCKET") {
        return PathBuf::from(p);
    }
    let args: Vec<String> = env::args().collect();
    for i in 1..args.len() {
        if args[i] == "--socket" && i + 1 < args.len() {
            return PathBuf::from(&args[i + 1]);
        }
    }
    let default_path = Path::new("/run/directioner/cloudd.sock");
    if let Some(parent) = default_path.parent()
        && fs::create_dir_all(parent).is_ok()
    {
        return default_path.to_path_buf();
    }
    PathBuf::from("/tmp/directioner-cloudd.sock")
}

fn handle_connection(stream: UnixStream, state: &Arc<Mutex<DaemonState>>) {
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

        let cmd: Result<CloudCommand, _> = serde_json::from_str(trimmed);
        let resp = match cmd {
            Ok(CloudCommand::Status) => {
                let guard = state.lock().unwrap();
                CloudResponse {
                    ok: true,
                    message: Some("Daemon running".to_string()),
                    pending_count: Some(guard.engine.pending_count()),
                    sync_status: None,
                    forked_path: None,
                }
            }
            Ok(CloudCommand::Ping) => CloudResponse {
                ok: true,
                message: Some("pong".to_string()),
                pending_count: None,
                sync_status: None,
                forked_path: None,
            },
            Ok(CloudCommand::RegisterFile {
                path,
                hash,
                size,
                modified,
            }) => {
                let mut guard = state.lock().unwrap();
                guard
                    .engine
                    .register_local_file(&path, &hash, size, modified);
                CloudResponse {
                    ok: true,
                    message: Some("Registered".to_string()),
                    pending_count: Some(guard.engine.pending_count()),
                    sync_status: Some(SyncStatus::PendingUpload),
                    forked_path: None,
                }
            }
            Ok(CloudCommand::Reconcile {
                path,
                remote_hash,
                remote_modified,
            }) => {
                let mut guard = state.lock().unwrap();
                let status = guard
                    .engine
                    .reconcile_remote(&path, &remote_hash, remote_modified);
                CloudResponse {
                    ok: true,
                    message: None,
                    pending_count: Some(guard.engine.pending_count()),
                    sync_status: Some(status),
                    forked_path: None,
                }
            }
            Ok(CloudCommand::ResolveConflict { path, keep_local }) => {
                let mut guard = state.lock().unwrap();
                let forked = guard.engine.resolve_conflict(&path, keep_local);
                CloudResponse {
                    ok: true,
                    message: Some("Conflict resolved".to_string()),
                    pending_count: Some(guard.engine.pending_count()),
                    sync_status: None,
                    forked_path: forked,
                }
            }
            Ok(CloudCommand::DrainQueue) => {
                let mut guard = state.lock().unwrap();
                guard.breaker.record_success();
                CloudResponse {
                    ok: true,
                    message: Some("Queue drained".to_string()),
                    pending_count: Some(guard.engine.pending_count()),
                    sync_status: None,
                    forked_path: None,
                }
            }
            Err(e) => CloudResponse {
                ok: false,
                message: Some(format!("Invalid command: {}", e)),
                pending_count: None,
                sync_status: None,
                forked_path: None,
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

    let state = Arc::new(Mutex::new(DaemonState {
        engine: CloudSyncEngine::new(),
        breaker: CircuitBreaker::new(5, 60),
    }));

    let running = Arc::new(AtomicBool::new(true));
    let r_clone = Arc::clone(&running);
    let s_path_clone = socket_path.clone();

    let bg_state = Arc::clone(&state);
    let bg_running = Arc::clone(&running);
    thread::spawn(move || {
        while bg_running.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(30));
            let mut guard = bg_state.lock().unwrap();
            let now = chrono::Utc::now().timestamp();
            if guard.breaker.allow_request(now) {
                guard.breaker.record_success();
            }
        }
    });

    println!("directioner-cloudd: listening on {}", socket_path.display());

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
