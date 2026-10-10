//! Comprehensive integration and unit tests for socket protocol wire limits (TODO 001).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::{
    DEFAULT_MAX_CONCURRENT_CONNECTIONS, DEFAULT_MAX_REQUEST_BYTES, DEFAULT_READ_TIMEOUT, Server,
    ServerConfig, SharedHandler,
};
use capability_layer::{API_VERSION, CapabilityKind, CapabilityRequest, CapabilityResponse};
use parking_lot::Mutex;

fn spawn_test_server(
    dir: &std::path::Path,
    config: ServerConfig,
) -> (SharedHandler, std::path::PathBuf) {
    let socket = dir.join(format!("wire_limits_{}.sock", fastrand_u32()));
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
    let server = Server::bind_with_config(&socket, handler.clone(), config).unwrap();
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

fn fastrand_u32() -> u32 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos()
}

#[test]
fn test_wire_limit_config_defaults() {
    let config = ServerConfig::default();
    assert_eq!(config.max_request_bytes, DEFAULT_MAX_REQUEST_BYTES);
    assert_eq!(config.max_request_bytes, 64 * 1024);
    assert_eq!(config.read_timeout, DEFAULT_READ_TIMEOUT);
    assert_eq!(config.read_timeout, Duration::from_secs(5));
    assert_eq!(
        config.max_concurrent_connections,
        DEFAULT_MAX_CONCURRENT_CONNECTIONS
    );
    assert_eq!(config.max_concurrent_connections, 64);
}

#[test]
fn test_wire_limit_custom_config() {
    let config = ServerConfig {
        max_request_bytes: 4096,
        read_timeout: Duration::from_millis(500),
        max_concurrent_connections: 10,
        ..ServerConfig::default()
    };

    assert_eq!(config.max_request_bytes, 4096);
    assert_eq!(config.read_timeout, Duration::from_millis(500));
    assert_eq!(config.max_concurrent_connections, 10);
}

#[test]
fn test_wire_limit_oversized_payload_rejected_with_structured_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = ServerConfig {
        max_request_bytes: 1024, // 1 KiB test limit
        ..ServerConfig::default()
    };
    let (_handler, socket) = spawn_test_server(dir.path(), config);

    let mut stream = UnixStream::connect(&socket).unwrap();

    // Send a 2048-byte line which exceeds the 1024 limit
    let large_garbage = vec![b'a'; 2048];
    stream.write_all(&large_garbage).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut resp_line = String::new();
    let n = reader.read_line(&mut resp_line).unwrap();
    assert!(n > 0, "Server must respond before closing");

    let resp: CapabilityResponse = serde_json::from_str(&resp_line).expect("Valid JSON response");
    match resp {
        CapabilityResponse::Error { message, .. } => {
            assert!(
                message.contains("request payload exceeds maximum allowed size"),
                "Expected payload size rejection, got: {message}"
            );
            assert!(
                message.contains("1024"),
                "Message must mention the max limit: {message}"
            );
        }
        other => panic!("Expected CapabilityResponse::Error, got: {other:?}"),
    }

    // Connection should be closed by server immediately after sending error
    resp_line.clear();
    let n_after = reader.read_line(&mut resp_line).unwrap();
    assert_eq!(n_after, 0, "Connection should be closed on EOF");
}

#[test]
fn test_wire_limit_read_timeout_slow_client() {
    let dir = tempfile::tempdir().unwrap();
    let config = ServerConfig {
        read_timeout: Duration::from_millis(200), // Short timeout for test
        ..ServerConfig::default()
    };
    let (_handler, socket) = spawn_test_server(dir.path(), config);

    let mut stream = UnixStream::connect(&socket).unwrap();

    // Send incomplete partial line and do not complete it
    stream.write_all(b"{\"partial\":").unwrap();
    stream.flush().unwrap();

    // Sleep longer than the 200ms timeout
    sleep(Duration::from_millis(350));

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut resp_line = String::new();
    let n = reader.read_line(&mut resp_line).unwrap();
    assert!(n > 0, "Server must return timeout error response");

    let resp: CapabilityResponse = serde_json::from_str(&resp_line).expect("Valid JSON response");
    match resp {
        CapabilityResponse::Error { message, .. } => {
            assert!(
                message.contains("connection read timed out"),
                "Expected timeout error, got: {message}"
            );
        }
        other => panic!("Expected CapabilityResponse::Error on timeout, got: {other:?}"),
    }
}

#[test]
fn test_wire_limit_max_concurrent_connections_cap() {
    let dir = tempfile::tempdir().unwrap();
    let config = ServerConfig {
        max_concurrent_connections: 2, // Only 2 connections allowed
        read_timeout: Duration::from_secs(5),
        ..ServerConfig::default()
    };
    let (_handler, socket) = spawn_test_server(dir.path(), config);

    // Open connection 1 and 2
    let mut conn1 = UnixStream::connect(&socket).unwrap();
    let mut conn2 = UnixStream::connect(&socket).unwrap();

    // Connection 3 should exceed limit and receive structured rejection
    let conn3 = UnixStream::connect(&socket).unwrap();
    let mut reader3 = BufReader::new(conn3);
    let mut resp3 = String::new();
    let n = reader3.read_line(&mut resp3).unwrap();
    assert!(n > 0, "Server must return error for connection 3");

    let resp: CapabilityResponse = serde_json::from_str(&resp3).expect("Valid JSON response");
    match resp {
        CapabilityResponse::Error { message, .. } => {
            assert!(
                message.contains("server connection limit of 2 exceeded"),
                "Expected limit exceeded, got: {message}"
            );
        }
        other => panic!("Expected CapabilityResponse::Error, got: {other:?}"),
    }

    // Clean up conn1 and conn2
    let _ = conn1.write_all(b"\n");
    let _ = conn2.write_all(b"\n");
}

#[test]
fn test_wire_limit_valid_request_under_limit_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let config = ServerConfig::default();
    let (_handler, socket) = spawn_test_server(dir.path(), config);

    let mut stream = UnixStream::connect(&socket).unwrap();
    let req = CapabilityRequest {
        api_version: API_VERSION,
        id: "ping-ok".into(),
        requester: "itest".into(),
        token: "tok-1".into(),
        is_rollback: false,
        capability: CapabilityKind::Ping,
    };
    let mut bytes = serde_json::to_vec(&req).unwrap();
    bytes.push(b'\n');
    stream.write_all(&bytes).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp: CapabilityResponse = serde_json::from_str(&line).unwrap();
    assert!(matches!(resp, CapabilityResponse::Success { .. }));
}
