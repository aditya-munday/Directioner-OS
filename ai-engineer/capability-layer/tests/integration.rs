use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use capability_layer::audit::{AuditLog, AuditOutcome};
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::{Server, SharedHandler};
use capability_layer::{API_VERSION, CapabilityKind, CapabilityRequest, CapabilityResponse};

use parking_lot::Mutex;
use serde_json::json;

fn write_req(stream: &mut UnixStream, req: &CapabilityRequest) {
    let mut line = serde_json::to_vec(req).unwrap();
    line.push(b'\n');
    stream.write_all(&line).unwrap();
    stream.flush().unwrap();
}

fn read_resp(stream: &mut UnixStream) -> CapabilityResponse {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn req(id: &str, capability: CapabilityKind) -> CapabilityRequest {
    CapabilityRequest {
        api_version: API_VERSION,
        id: id.into(),
        requester: "itest".into(),
        token: "tok-1".into(),
        is_rollback: false,
        capability,
    }
}

fn spawn_server(
    dir: &std::path::Path,
) -> (
    SharedHandler,
    std::thread::JoinHandle<()>,
    std::path::PathBuf,
) {
    let socket = dir.join("cap.sock");
    let scope = ScopedPath::new(dir.join("scope")).unwrap();
    let audit = AuditLog::open(dir.join("worklog.jsonl")).unwrap();
    let killswitch = KillSwitch::new(dir.join("kill.lock"));
    let config = HandlerConfig {
        approval_required: true,
        dry_run: true,
        ..HandlerConfig::default()
    };
    let handler: SharedHandler =
        Arc::new(Mutex::new(Handler::new(scope, audit, killswitch, config)));
    let server = Server::bind(&socket, handler.clone()).unwrap();
    let thread = std::thread::spawn(move || {
        let _ = server.serve();
    });
    for _ in 0..50 {
        if socket.exists() {
            break;
        }
        sleep(Duration::from_millis(20));
    }
    (handler, thread, socket)
}

#[test]
fn end_to_end_over_unix_socket() {
    let dir = tempfile::tempdir().unwrap();
    let (handler, _thread, socket) = spawn_server(dir.path());

    let mut stream = UnixStream::connect(&socket).unwrap();

    write_req(&mut stream, &req("it-1", CapabilityKind::Ping));
    assert!(matches!(
        read_resp(&mut stream),
        CapabilityResponse::Success { .. }
    ));

    write_req(
        &mut stream,
        &req(
            "it-2",
            CapabilityKind::PackageInstall {
                package: "nginx".into(),
                approved: false,
            },
        ),
    );
    assert!(matches!(
        read_resp(&mut stream),
        CapabilityResponse::Denied { .. }
    ));

    write_req(
        &mut stream,
        &req(
            "it-3",
            CapabilityKind::PackageInstall {
                package: "nginx".into(),
                approved: true,
            },
        ),
    );
    match read_resp(&mut stream) {
        CapabilityResponse::Success { result, .. } => {
            assert_eq!(result["dry_run"], true);
            let would_run = result["would_run"].as_str().unwrap();
            assert!(would_run.contains("rpm-ostree") || would_run.contains("dnf5"));
            assert!(would_run.contains("nginx"));
        }
        other => panic!("unexpected: {other:?}"),
    }

    write_req(
        &mut stream,
        &req(
            "it-4",
            CapabilityKind::FileWrite {
                path: "conf/app.conf".into(),
                content: "enabled=true\n".into(),
                append: false,
            },
        ),
    );
    assert!(matches!(
        read_resp(&mut stream),
        CapabilityResponse::Success { .. }
    ));

    write_req(
        &mut stream,
        &req(
            "it-5",
            CapabilityKind::FileRead {
                path: "conf/app.conf".into(),
            },
        ),
    );
    match read_resp(&mut stream) {
        CapabilityResponse::Success { result, .. } => {
            assert_eq!(result["content"], "enabled=true\n");
        }
        other => panic!("unexpected: {other:?}"),
    }

    write_req(
        &mut stream,
        &req(
            "it-6",
            CapabilityKind::FileWrite {
                path: "../evil.txt".into(),
                content: "x".into(),
                append: false,
            },
        ),
    );
    assert!(matches!(
        read_resp(&mut stream),
        CapabilityResponse::Denied { .. }
    ));

    drop(stream);
    sleep(Duration::from_millis(100));

    {
        let mut h = handler.lock();
        h.killswitch.engage().unwrap();
    }
    let mut stream = UnixStream::connect(&socket).unwrap();
    write_req(&mut stream, &req("it-7", CapabilityKind::Ping));
    assert!(matches!(
        read_resp(&mut stream),
        CapabilityResponse::Denied { .. }
    ));
    drop(stream);
    sleep(Duration::from_millis(100));

    let audit = handler.lock().audit.reopen().unwrap();
    let all = audit.read_all().unwrap();
    let caps: Vec<&str> = all.iter().map(|e| e.capability.as_str()).collect();
    assert_eq!(
        caps,
        vec![
            "ping",
            "package.install",
            "package.install",
            "file.write",
            "file.read",
            "file.write",
            "ping"
        ]
    );
    assert!(matches!(all[1].outcome, AuditOutcome::Denied { .. }));
    assert!(matches!(all[6].outcome, AuditOutcome::Denied { .. }));
    assert_eq!(all[6].request_id, "it-7");
}

#[test]
fn malformed_line_gets_error_response() {
    let dir = tempfile::tempdir().unwrap();
    let (_handler, _thread, socket) = spawn_server(dir.path());
    let mut stream = UnixStream::connect(&socket).unwrap();
    stream.write_all(b"{ not json }\n").unwrap();
    stream.flush().unwrap();
    let resp = read_resp(&mut stream);
    assert!(matches!(resp, CapabilityResponse::Error { .. }));
}

#[test]
fn wrong_api_version_is_denied() {
    let dir = tempfile::tempdir().unwrap();
    let (_handler, _thread, socket) = spawn_server(dir.path());
    let mut stream = UnixStream::connect(&socket).unwrap();
    let bad = json!({
        "api_version": 99,
        "id": "bad-1",
        "requester": "itest",
        "token": "tok",
        "capability": { "capability": "ping" },
    });
    let mut line = serde_json::to_vec(&bad).unwrap();
    line.push(b'\n');
    stream.write_all(&line).unwrap();
    stream.flush().unwrap();
    let resp = read_resp(&mut stream);
    assert!(matches!(resp, CapabilityResponse::Denied { .. }));
}

#[test]
fn json_shape_matches_documented_contract() {
    let req = serde_json::to_value(CapabilityRequest {
        api_version: API_VERSION,
        id: "r1".into(),
        requester: "test".into(),
        token: "tok".into(),
        is_rollback: false,
        capability: CapabilityKind::PackageInstall {
            package: "nginx".into(),
            approved: true,
        },
    })
    .unwrap();
    assert_eq!(req["capability"]["package"], json!("nginx"));
    assert_eq!(req["capability"]["approved"], json!(true));
}

#[test]
fn test_file_read_write_append_and_scope_confinement() {
    let dir = tempfile::tempdir().unwrap();
    let (_handler, _thread, socket) = spawn_server(dir.path());
    let mut stream = UnixStream::connect(&socket).unwrap();

    // 1. Write file in scope
    let write_req = req(
        "file-w1",
        CapabilityKind::FileWrite {
            path: "test.txt".into(),
            content: "line 1\n".into(),
            append: false,
        },
    );
    write_req_stream(&mut stream, &write_req);
    let resp = read_resp(&mut stream);
    assert!(matches!(resp, CapabilityResponse::Success { .. }));

    // 2. Read file back
    let read_req = req(
        "file-r1",
        CapabilityKind::FileRead {
            path: "test.txt".into(),
        },
    );
    write_req_stream(&mut stream, &read_req);
    let resp_r = read_resp(&mut stream);
    assert!(matches!(resp_r, CapabilityResponse::Success { .. }));

    // 3. Append to file
    let append_req = req(
        "file-w2",
        CapabilityKind::FileWrite {
            path: "test.txt".into(),
            content: "line 2\n".into(),
            append: true,
        },
    );
    write_req_stream(&mut stream, &append_req);
    let resp_a = read_resp(&mut stream);
    assert!(matches!(resp_a, CapabilityResponse::Success { .. }));

    // 4. Path traversal outside scope is rejected
    let traversal_req = req(
        "file-bad",
        CapabilityKind::FileRead {
            path: "../../../etc/shadow".into(),
        },
    );
    write_req_stream(&mut stream, &traversal_req);
    let resp_bad = read_resp(&mut stream);
    assert!(matches!(resp_bad, CapabilityResponse::Denied { .. }));
}

fn write_req_stream(stream: &mut UnixStream, req: &CapabilityRequest) {
    let mut line = serde_json::to_vec(req).unwrap();
    line.push(b'\n');
    stream.write_all(&line).unwrap();
    stream.flush().unwrap();
}
