//! Security Audit Hardening Regression Test Suite (AUDIT-CAP-01 through AUDIT-CAP-04).

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tempfile::tempdir;

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::Server;
use parking_lot::Mutex;

#[test]
fn test_audit_cap_01_socket_creation_permissions() {
    // 1. Temporarily set process umask to 0000 to demonstrate default unconstrained socket creation
    // In POSIX systems, standard bind() inherits process umask.
    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("audit_test.sock");

    let audit = AuditLog::open(dir.path().join("audit.jsonl")).unwrap();
    let killswitch = KillSwitch::load(dir.path().join("killswitch.lock")).unwrap();
    let scope = ScopedPath::new(dir.path().join("scope")).unwrap();
    let handler = Arc::new(Mutex::new(Handler::new(
        scope,
        audit,
        killswitch,
        HandlerConfig::default(),
    )));

    // Bind server socket
    let _server = Server::bind(&sock_path, handler).expect("Failed to bind test socket");

    // Inspect actual file mode on disk
    let meta = std::fs::metadata(&sock_path).expect("Failed to read socket metadata");
    let mode = meta.permissions().mode() & 0o777;

    // Must be strictly 0600 (owner read/write only, zero group/other permissions)
    assert_eq!(
        mode, 0o600,
        "AUDIT-CAP-01 VULNERABILITY: Socket was created with mode {:o}, expected strict 0600",
        mode
    );
}

#[test]
fn test_audit_cap_02_peercred_caller_uid_verification() {
    use capability_layer::server::ServerConfig;
    use capability_layer::{API_VERSION, CapabilityKind, CapabilityRequest, CapabilityResponse};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::thread;

    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("audit_test_cap02.sock");

    let audit = AuditLog::open(dir.path().join("audit.jsonl")).unwrap();
    let killswitch = KillSwitch::load(dir.path().join("killswitch.lock")).unwrap();
    let scope = ScopedPath::new(dir.path().join("scope")).unwrap();
    let handler = Arc::new(Mutex::new(Handler::new(
        scope,
        audit,
        killswitch,
        HandlerConfig::default(),
    )));

    // 1. Start server that ONLY authorizes UID 99999 (simulating an unauthorized caller connecting)
    let current_uid = rustix::process::getuid().as_raw();
    let unauthorized_config = ServerConfig {
        allowed_uids: vec![99999],
    };
    let server = Server::bind_with_config(&sock_path, handler.clone(), unauthorized_config)
        .expect("Failed to bind server socket");

    let handle = thread::spawn(move || {
        let _ = server.serve();
    });

    // Connect from current process (UID = current_uid != 99999)
    let mut client = UnixStream::connect(&sock_path).expect("Failed to connect to socket");
    let ping_req = CapabilityRequest {
        api_version: API_VERSION,
        id: "probe-1".into(),
        requester: "unauthorized-probe".into(),
        token: "tok-test".into(),
        is_rollback: false,
        capability: CapabilityKind::Ping,
    };
    let mut req_bytes = serde_json::to_vec(&ping_req).unwrap();
    req_bytes.push(b'\n');
    let write_result = client.write_all(&req_bytes);
    match write_result {
        Ok(()) => {}
        Err(ref e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(e) => panic!("Unexpected write error: {e}"),
    }
    let _ = client.flush();

    let mut reader = BufReader::new(client);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .expect("Failed to read server response");
    assert!(
        n > 0,
        "Expected server rejection response before connection close"
    );

    let resp: CapabilityResponse =
        serde_json::from_str(&line).expect("Failed to parse response JSON");
    match resp {
        CapabilityResponse::Denied { reason, .. } => {
            assert!(
                reason.contains("not authorized to connect"),
                "Expected rejection message for unauthorized UID, got: {reason}"
            );
        }
        other => panic!(
            "Expected CapabilityResponse::Denied for unauthorized caller UID, got: {other:?}"
        ),
    }

    // Subsequent read should hit EOF because server immediately shuts down connection
    line.clear();
    let n2 = reader.read_line(&mut line).unwrap();
    assert_eq!(
        n2, 0,
        "Connection should be immediately closed after denial"
    );

    // 2. Now verify that an authorized UID (current_uid) succeeds
    let sock_path_auth = dir.path().join("audit_test_cap02_auth.sock");
    let authorized_config = ServerConfig {
        allowed_uids: vec![current_uid],
    };
    let server_auth = Server::bind_with_config(&sock_path_auth, handler, authorized_config)
        .expect("Failed to bind authorized server socket");

    thread::spawn(move || {
        let _ = server_auth.serve();
    });

    let mut client_auth =
        UnixStream::connect(&sock_path_auth).expect("Failed to connect to authorized socket");
    client_auth.write_all(&req_bytes).unwrap();
    client_auth.flush().unwrap();

    let mut reader_auth = BufReader::new(client_auth);
    let mut line_auth = String::new();
    reader_auth
        .read_line(&mut line_auth)
        .expect("Failed to read response");
    let resp_auth: CapabilityResponse =
        serde_json::from_str(&line_auth).expect("Failed to parse response");
    assert!(
        matches!(resp_auth, CapabilityResponse::Success { .. }),
        "Expected Success for authorized caller UID, got: {resp_auth:?}"
    );

    // Clean up
    drop(handle);
}

#[test]
fn test_audit_cap_03_symlink_redirection_defense() {
    use capability_layer::{API_VERSION, CapabilityKind, CapabilityRequest, CapabilityResponse};
    use std::fs;
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let scope_dir = dir.path().join("scope");
    fs::create_dir_all(&scope_dir).unwrap();

    let outside_dir = dir.path().join("outside");
    fs::create_dir_all(&outside_dir).unwrap();
    let victim_file = outside_dir.join("victim.txt");
    fs::write(&victim_file, "ORIGINAL_SAFE_CONTENT").unwrap();

    // Plant a symlink inside the scoped root pointing to outside target
    let symlink_in_scope = scope_dir.join("evil_link");
    symlink(&victim_file, &symlink_in_scope).unwrap();

    let audit = AuditLog::open(dir.path().join("audit.jsonl")).unwrap();
    let killswitch = KillSwitch::load(dir.path().join("killswitch.lock")).unwrap();
    let scope = ScopedPath::new(&scope_dir).unwrap();
    let mut handler = Handler::new(scope, audit, killswitch, HandlerConfig::default());

    let req = CapabilityRequest {
        api_version: API_VERSION,
        id: "cap03-write".into(),
        requester: "audit-test".into(),
        token: "tok-test".into(),
        is_rollback: false,
        capability: CapabilityKind::FileWrite {
            path: "evil_link".into(),
            content: "MALICIOUS_OVERWRITE".into(),
            append: false,
        },
    };

    let result = handler.execute(&req);
    let victim_content = fs::read_to_string(&victim_file).unwrap();

    // The write must be rejected and the target file outside scope must NOT be modified
    assert_eq!(
        victim_content, "ORIGINAL_SAFE_CONTENT",
        "AUDIT-CAP-03 VULNERABILITY: FileWrite followed symlink out of scope and overwritten target file!"
    );

    match result {
        Ok((_seq, CapabilityResponse::Denied { reason, .. })) => {
            assert!(
                reason.to_lowercase().contains("symlink")
                    || reason.to_lowercase().contains("too many levels"),
                "Expected denial for symlink, got: {reason}"
            );
        }
        Ok((_seq, CapabilityResponse::Error { message, .. })) => {
            assert!(
                message.to_lowercase().contains("symlink")
                    || message.to_lowercase().contains("too many levels")
                    || message.to_lowercase().contains("code: 40"), // ELOOP
                "Expected error preventing symlink follow, got: {message}"
            );
        }
        other => {
            panic!("Expected CapabilityResponse::Denied or Error for symlink write, got: {other:?}")
        }
    }
}

#[test]
fn test_audit_cap_04_killswitch_terminates_inflight_subprocess() {
    use capability_layer::killswitch::KillSwitch;
    use capability_layer::{API_VERSION, CapabilityKind, CapabilityRequest, CapabilityResponse};
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    let dir = tempdir().unwrap();
    let lock_path = dir.path().join("killswitch.lock");
    let mut killswitch = KillSwitch::new(&lock_path);
    let audit = AuditLog::open(dir.path().join("audit.jsonl")).unwrap();
    let scope = ScopedPath::new(dir.path().join("scope")).unwrap();

    // Create a mock script in a temp bin directory that records its PID and sleeps for 30s
    let bin_dir = dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let mock_bin = bin_dir.join("systemctl");
    let pid_file = dir.path().join("child.pid");

    let script = format!(
        "#!/bin/sh\necho $$ > {}\nexec sleep 30\n",
        pid_file.display()
    );
    let mut f = fs::File::create(&mock_bin).unwrap();
    f.write_all(script.as_bytes()).unwrap();
    drop(f);
    fs::set_permissions(&mock_bin, fs::Permissions::from_mode(0o755)).unwrap();

    let handler_config = HandlerConfig {
        approval_required: false,
        protected_units: vec![],
        dry_run: false, // Must execute systemctl!
    };
    let mut handler = Handler::new(scope, audit, KillSwitch::new(&lock_path), handler_config)
        .with_custom_bin_dir(&bin_dir);

    // Request a service restart which invokes systemctl
    let req = CapabilityRequest {
        api_version: API_VERSION,
        id: "cap04-test".into(),
        requester: "audit-test".into(),
        token: "tok-test".into(),
        is_rollback: false,
        capability: CapabilityKind::ServiceRestart {
            unit: "worker.service".into(),
            approved: true,
        },
    };

    let (tx, rx) = mpsc::channel();
    let start_time = Instant::now();

    // Spawn execution in background thread
    let worker_thread = thread::spawn(move || {
        let res = handler.execute(&req);
        let _ = tx.send(res);
    });

    // Wait until child script has actually started and written its PID
    let mut child_pid: Option<u32> = None;
    for _ in 0..50 {
        if let Ok(content) = fs::read_to_string(&pid_file)
            && let Ok(pid) = content.trim().parse::<u32>()
        {
            child_pid = Some(pid);
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let pid = child_pid.expect("Child process did not start or write pid file");

    // Verify child process is currently alive
    let rustix_pid = rustix::process::Pid::from_raw(pid as i32).unwrap();
    assert!(
        rustix::process::test_kill_process(rustix_pid).is_ok(),
        "Child process should be running"
    );

    // Engage the killswitch mid-flight!
    killswitch.engage().unwrap();

    // The execution should be terminated promptly, NOT after 30 seconds
    let res = rx.recv_timeout(Duration::from_secs(2));

    // Clean up child process if it's still alive so it doesn't linger after test
    let is_alive_before_assert = rustix::process::test_kill_process(rustix_pid).is_ok();
    if is_alive_before_assert {
        let _ = rustix::process::kill_process(rustix_pid, rustix::process::Signal::KILL);
    }

    let elapsed = start_time.elapsed();
    assert!(
        res.is_ok(),
        "AUDIT-CAP-04 VULNERABILITY: In-flight subprocess was not terminated upon killswitch engagement (timed out waiting for execution to abort)!"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "Execution took too long: {elapsed:?}"
    );

    assert!(
        !is_alive_before_assert,
        "AUDIT-CAP-04 VULNERABILITY: Child subprocess (PID {pid}) was still alive after killswitch engaged!"
    );

    // Verify subsequent request is refused
    let req2 = CapabilityRequest {
        api_version: API_VERSION,
        id: "cap04-subsequent".into(),
        requester: "audit-test".into(),
        token: "tok-test".into(),
        is_rollback: false,
        capability: CapabilityKind::Ping,
    };
    let mut handler2 = Handler::new(
        ScopedPath::new(dir.path().join("scope2")).unwrap(),
        AuditLog::open(dir.path().join("audit2.jsonl")).unwrap(),
        KillSwitch::new(&lock_path),
        HandlerConfig::default(),
    );
    let (_seq, resp2) = handler2.execute(&req2).unwrap();
    assert!(
        matches!(resp2, CapabilityResponse::Denied { .. }),
        "Subsequent requests after killswitch engage must be denied"
    );

    let _ = worker_thread.join();
}
