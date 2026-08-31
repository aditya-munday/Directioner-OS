//! Comprehensive tests for LocalSocketBackend hardening:
//! retries, backoff, chunked reads, oversized response protection, and disconnect recovery.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use control_plane::conversational::{ConversationalError, LlmBackend, LocalSocketBackend};
use tempfile::tempdir;

#[test]
fn test_local_socket_successful_chunked_response_parsing() {
    let tmp = tempdir().unwrap();
    let socket_path = tmp.path().join("model.sock");

    let listener = UnixListener::bind(&socket_path).unwrap();
    let sock_clone = socket_path.clone();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();

        // Send multi-chunk valid JSON response
        let chunk1 = b"{\"response\": \"{\\\"intent\\\": \\\"configure auditd\\\", ";
        let chunk2 = b"\\\"summary\\\": \\\"auditing\\\", \\\"steps\\\": []}\"}\n";
        stream.write_all(chunk1).unwrap();
        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(10));
        stream.write_all(chunk2).unwrap();
        stream.flush().unwrap();
    });

    let backend = LocalSocketBackend::with_options(
        sock_clone,
        Duration::from_secs(5),
        2,
        Duration::from_millis(10),
        512 * 1024,
    );

    let res = backend
        .complete("diagnostic query")
        .expect("Chunked response should succeed");
    assert!(res.contains("configure auditd"));

    server_handle.join().unwrap();
}

#[test]
fn test_local_socket_retry_on_transient_failure() {
    let tmp = tempdir().unwrap();
    let socket_path = tmp.path().join("retry_model.sock");

    let listener = UnixListener::bind(&socket_path).unwrap();
    let attempts_count = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts_count.clone();

    let server_handle = thread::spawn(move || {
        // First connection: drop immediately after connecting
        if let Ok((stream, _)) = listener.accept() {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            drop(stream); // Drop connection to cause transient EOF/error
        }

        // Second connection: send valid response
        if let Ok((mut stream, _)) = listener.accept() {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);

            let resp = b"{\"response\": \"{\\\"intent\\\": \\\"recovered plan\\\", \\\"summary\\\": \\\"ok\\\", \\\"steps\\\": []}\"}\n";
            let _ = stream.write_all(resp);
            let _ = stream.flush();
        }
    });

    let backend = LocalSocketBackend::with_options(
        socket_path,
        Duration::from_secs(5),
        3,
        Duration::from_millis(20),
        512 * 1024,
    );

    let res = backend
        .complete("retry prompt")
        .expect("Retry should succeed on second attempt");
    assert!(res.contains("recovered plan"));
    assert_eq!(attempts_count.load(Ordering::SeqCst), 2);

    server_handle.join().unwrap();
}

#[test]
fn test_local_socket_oversized_response_rejected() {
    let tmp = tempdir().unwrap();
    let socket_path = tmp.path().join("oversized.sock");

    let listener = UnixListener::bind(&socket_path).unwrap();
    let sock_clone = socket_path.clone();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        // Send large data exceeding the 1024 byte limit set for this test
        let large_payload = vec![b'A'; 2048];
        let _ = stream.write_all(&large_payload);
        let _ = stream.flush();
    });

    let backend = LocalSocketBackend::with_options(
        sock_clone,
        Duration::from_secs(2),
        0, // No retry for single connection test
        Duration::from_millis(10),
        1024, // 1KB limit
    );

    let res = backend.complete("prompt");
    assert!(
        matches!(res, Err(ConversationalError::BackendError(ref msg)) if msg.contains("exceeded maximum allowed limit")),
        "Oversized response must be rejected with error: {res:?}"
    );

    server_handle.join().unwrap();
}

#[test]
fn test_local_socket_empty_response_fails_closed() {
    let tmp = tempdir().unwrap();
    let socket_path = tmp.path().join("empty.sock");

    let listener = UnixListener::bind(&socket_path).unwrap();
    let sock_clone = socket_path.clone();

    let server_handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        drop(stream); // Immediate EOF
    });

    let backend = LocalSocketBackend::with_options(
        sock_clone,
        Duration::from_secs(2),
        0, // No retry for single connection test
        Duration::from_millis(10),
        512 * 1024,
    );

    let res = backend.complete("prompt");
    assert!(
        matches!(res, Err(ConversationalError::BackendError(_))),
        "Empty response must fail closed with BackendError"
    );

    server_handle.join().unwrap();
}
