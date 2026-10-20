use directioner_statd::{
    ConsentLedger, RedactionEngine, SensitivityLevel, StatDaemon, StatRequest, StatResponse,
    StateTree,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[test]
fn test_state_tree_insert_and_version() {
    let mut tree = StateTree::new();
    assert_eq!(tree.version, 1);

    tree.insert(
        "system.os.info",
        serde_json::json!({"id": "directioner"}),
        SensitivityLevel::Public,
        "test",
    );
    assert_eq!(tree.version, 2);

    let node = tree.get("system.os.info").expect("Node should exist");
    assert_eq!(node.path, "system.os.info");
    assert_eq!(node.sensitivity, SensitivityLevel::Public);
}

#[test]
fn test_redaction_engine_golden() {
    let mut tree = StateTree::new();
    tree.insert(
        "system.os.info",
        serde_json::json!({"id": "directioner"}),
        SensitivityLevel::Public,
        "test",
    );
    tree.insert(
        "network.wifi.connection",
        serde_json::json!({
            "ssid": "MySecretHomeWiFi",
            "ip_address": "192.168.1.50",
            "mac_address": "AA:BB:CC:DD:EE:FF",
            "user_path": "/home/aditya/documents",
        }),
        SensitivityLevel::Personal,
        "test",
    );
    tree.insert(
        "security.crypto.key",
        serde_json::json!({"private_key": "supersecret"}),
        SensitivityLevel::Secret,
        "test",
    );

    // 1. Level 0: Public only. Personal and Secret are omitted.
    let l0 = RedactionEngine::filter_tree(&tree, 0);
    assert!(l0.get("system.os.info").is_some());
    assert!(l0.get("network.wifi.connection").is_none());
    assert!(l0.get("security.crypto.key").is_none());

    // 2. Level 2: Personal included, but sanitized if fields are checked or full if authorized
    let l2 = RedactionEngine::filter_tree(&tree, 2);
    assert!(l2.get("system.os.info").is_some());
    assert!(l2.get("network.wifi.connection").is_some());
    // Secret (Level 3) is NEVER exposed under any circumstances
    assert!(l2.get("security.crypto.key").is_none());
}

#[test]
fn test_consent_ledger_default_zero() {
    let mut ledger = ConsentLedger::new();
    assert_eq!(ledger.resolve_level(1000), 0); // Unknown UID gets Level 0

    ledger.grant(1000, 2);
    assert_eq!(ledger.resolve_level(1000), 2);

    ledger.revoke(1000);
    assert_eq!(ledger.resolve_level(1000), 0);
}

#[tokio::test]
async fn test_statd_server_socket_read_only_and_permissions() {
    let temp = tempdir().unwrap();
    let socket_path = temp.path().join("statd.sock");

    let daemon = StatDaemon::new(socket_path.clone());
    let tree = daemon.tree();
    {
        let mut t = tree.lock().unwrap();
        t.insert(
            "system.test.node",
            serde_json::json!({"status": "healthy"}),
            SensitivityLevel::Public,
            "test",
        );
    }

    // Spawn daemon server in background task
    tokio::spawn(async move {
        let _ = daemon.run().await;
    });

    // Wait for socket to bind
    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }
    assert!(socket_path.exists(), "Socket file should exist");

    // Verify 0600 permissions per U066
    let perms = fs::metadata(&socket_path).unwrap().permissions();
    let mode = perms.mode() & 0o777;
    assert_eq!(mode, 0o600, "Socket must have 0600 permissions");

    // Connect to server
    let stream = UnixStream::connect(&socket_path)
        .await
        .expect("Should connect to statd socket");
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // 1. Query state tree
    let req = serde_json::to_string(&StatRequest::GetTree).unwrap() + "\n";
    writer.write_all(req.as_bytes()).await.unwrap();

    let resp_line = lines.next_line().await.unwrap().unwrap();
    let resp: StatResponse = serde_json::from_str(&resp_line).unwrap();
    match resp {
        StatResponse::Success { data } => {
            let tree_obj: StateTree = serde_json::from_value(data).unwrap();
            assert!(tree_obj.get("system.test.node").is_some());
        }
        other => panic!("Expected Success, got {other:?}"),
    }

    // 2. Query specific node
    let req_node = serde_json::to_string(&StatRequest::GetNode {
        path: "system.test.node".into(),
    })
    .unwrap()
        + "\n";
    writer.write_all(req_node.as_bytes()).await.unwrap();

    let resp_line2 = lines.next_line().await.unwrap().unwrap();
    let resp2: StatResponse = serde_json::from_str(&resp_line2).unwrap();
    match resp2 {
        StatResponse::Success { data } => {
            assert_eq!(data["value"]["status"], "healthy");
        }
        other => panic!("Expected Success, got {other:?}"),
    }

    // 3. ATTEMPT MUTATION / WRITE: Must be rejected with WRITE_NOT_ALLOWED (U066)
    let bad_req = "{\"action\": \"set_volume\", \"value\": 100}\n";
    writer.write_all(bad_req.as_bytes()).await.unwrap();

    let resp_line3 = lines.next_line().await.unwrap().unwrap();
    let resp3: StatResponse = serde_json::from_str(&resp_line3).unwrap();
    match resp3 {
        StatResponse::Error { code, message } => {
            assert_eq!(code, "WRITE_NOT_ALLOWED");
            assert!(message.contains("strictly read-only by construction"));
        }
        other => panic!("Expected WRITE_NOT_ALLOWED error, got {other:?}"),
    }
}
