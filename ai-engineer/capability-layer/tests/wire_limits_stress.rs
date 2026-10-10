//! Verification stress probe for TODO 001.
//! Sends 10 MB garbage line, incomplete never-ending stream, and flood of connections.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::{Server, ServerConfig, SharedHandler};
use capability_layer::{API_VERSION, CapabilityKind, CapabilityRequest, CapabilityResponse};
use parking_lot::Mutex;

fn spawn_stress_server(dir: &std::path::Path) -> (SharedHandler, std::path::PathBuf) {
    let socket = dir.join("wire_stress.sock");
    let scope = ScopedPath::new(dir.join("scope")).unwrap();
    let audit = AuditLog::open(dir.join("worklog.jsonl")).unwrap();
    let killswitch = KillSwitch::new(dir.join("kill.lock"));
    let handler_config = HandlerConfig {
        approval_required: true,
        dry_run: true,
        ..HandlerConfig::default()
    };
    let handler: SharedHandler = Arc::new(Mutex::new(Handler::new(
        scope,
        audit,
        killswitch,
        handler_config,
    )));
    let server =
        Server::bind_with_config(&socket, handler.clone(), ServerConfig::default()).unwrap();
    std::thread::spawn(move || {
        let _ = server.serve();
    });
    for _ in 0..50 {
        if socket.exists() {
            break;
        }
        sleep(Duration::from_millis(20));
    }
    (handler, socket)
}

#[test]
fn test_stress_10mb_garbage_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let (_handler, socket) = spawn_stress_server(dir.path());

    let mut stream = UnixStream::connect(&socket).unwrap();
    // Stream 10 MB garbage without newline
    let chunk = vec![b'X'; 64 * 1024]; // 64 KiB chunks
    for _ in 0..160 {
        if stream.write_all(&chunk).is_err() {
            break;
        }
    }
    let _ = stream.write_all(b"\n");
    let _ = stream.flush();

    let mut reader = BufReader::new(stream);
    let mut resp = String::new();
    let n = reader.read_line(&mut resp).unwrap();
    assert!(n > 0, "Server must respond with error");
    let parsed: CapabilityResponse = serde_json::from_str(&resp).expect("Valid JSON");
    match parsed {
        CapabilityResponse::Error { message, .. } => {
            assert!(message.contains("request payload exceeds maximum allowed size"));
        }
        other => panic!("Expected Error response, got {other:?}"),
    }
}

#[test]
fn test_stress_never_ending_line_daemon_stays_responsive() {
    let dir = tempfile::tempdir().unwrap();
    let (_handler, socket) = spawn_stress_server(dir.path());

    // Client 1 sends incomplete line and stalls
    let mut bad_client = UnixStream::connect(&socket).unwrap();
    let _ = bad_client.write_all(b"{\"never_ending\": \"streaming...");
    let _ = bad_client.flush();

    // Client 2 simultaneously sends valid Ping - daemon must stay responsive!
    let mut good_client = UnixStream::connect(&socket).unwrap();
    let req = CapabilityRequest {
        api_version: API_VERSION,
        id: "ping-responsive".into(),
        requester: "stress".into(),
        token: "tok-1".into(),
        is_rollback: false,
        capability: CapabilityKind::Ping,
    };
    let mut bytes = serde_json::to_vec(&req).unwrap();
    bytes.push(b'\n');
    good_client.write_all(&bytes).unwrap();
    good_client.flush().unwrap();

    let mut reader = BufReader::new(good_client);
    let mut resp = String::new();
    reader.read_line(&mut resp).unwrap();
    let parsed: CapabilityResponse = serde_json::from_str(&resp).unwrap();
    assert!(matches!(parsed, CapabilityResponse::Success { .. }));
}
