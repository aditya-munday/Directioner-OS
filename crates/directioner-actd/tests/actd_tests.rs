use chrono::Duration;
use directioner_actd::{
    ActDaemon, ActRequest, ActResponse, ActionError, ActionRisk, PolkitMap, PreviewEngine,
    UndoJournal, UserExecutor,
};
use std::path::PathBuf;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[test]
fn test_preview_generation() {
    let params = serde_json::json!({"volume_percent": 80});
    let preview = PreviewEngine::generate_preview("audio.volume.set", &params, None);
    assert_eq!(preview.capability_id, "audio.volume.set");
    assert!(preview.reversible);
    assert!(!preview.requires_polkit);
    assert_eq!(preview.risk, ActionRisk::Low);

    let reboot_preview =
        PreviewEngine::generate_preview("power.system.reboot", &serde_json::json!({}), None);
    assert!(reboot_preview.requires_polkit);
    assert!(!reboot_preview.reversible);
    assert_eq!(reboot_preview.risk, ActionRisk::High);
}

#[test]
fn test_polkit_mapping() {
    assert_eq!(
        PolkitMap::action_for_capability("power.system.reboot"),
        Some("org.freedesktop.login1.reboot")
    );
    assert_eq!(
        PolkitMap::action_for_capability("network.wifi.connect"),
        Some("org.freedesktop.NetworkManager.network-control")
    );
    assert_eq!(PolkitMap::action_for_capability("audio.volume.set"), None);
}

#[test]
fn test_user_executor_root_write_denied() {
    let temp = tempdir().unwrap();
    let executor = UserExecutor::with_home(temp.path());

    // 1. Safe write inside home directory succeeds
    let user_file = temp.path().join("config.txt");
    let res = executor.safe_write_user_file(&user_file, b"test content");
    assert!(res.is_ok(), "Writing inside user home should succeed");

    // 2. Writing to system root directory strictly fails (U072)
    let root_file = PathBuf::from("/etc/forbidden.conf");
    let res_root = executor.safe_write_user_file(&root_file, b"malicious");
    assert!(res_root.is_err(), "Writing to root path must fail");
    match res_root {
        Err(ActionError::SecurityViolation(msg)) => {
            assert!(msg.contains("outside user home directory"));
        }
        other => panic!("Expected SecurityViolation, got {other:?}"),
    }
}

#[test]
fn test_undo_journal_lifecycle() {
    let temp = tempdir().unwrap();
    let mut journal = UndoJournal::new(temp.path());

    let entry = journal
        .record(
            "act-123",
            "audio.volume.set",
            "audio.volume.set",
            serde_json::json!({"volume_percent": 40}),
            "Reset volume to 40%",
            Duration::minutes(30),
        )
        .unwrap();

    assert_eq!(entry.action_id, "act-123");
    assert_eq!(journal.active_entries().len(), 1);

    // Consume entry
    let consumed = journal.consume("act-123").expect("Should consume entry");
    assert_eq!(consumed.action_id, "act-123");
    assert_eq!(journal.active_entries().len(), 0);
}

#[tokio::test]
async fn test_actd_server_preview_execute_undo_pipeline() {
    let temp = tempdir().unwrap();
    let socket_path = temp.path().join("actd.sock");
    let undo_dir = temp.path().join("undo");

    let daemon = ActDaemon::new(socket_path.clone(), undo_dir);
    tokio::spawn(async move {
        let _ = daemon.run().await;
    });

    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // 1. Preview request
    let preview_req = serde_json::to_string(&ActRequest::Preview {
        capability_id: "audio.volume.set".into(),
        params: serde_json::json!({"volume_percent": 90}),
    })
    .unwrap()
        + "\n";
    writer.write_all(preview_req.as_bytes()).await.unwrap();

    let resp_line = lines.next_line().await.unwrap().unwrap();
    let resp: ActResponse = serde_json::from_str(&resp_line).unwrap();
    match resp {
        ActResponse::Success { data } => {
            assert_eq!(data["capability_id"], "audio.volume.set");
            assert_eq!(data["reversible"], true);
        }
        other => panic!("Expected Success, got {other:?}"),
    }

    // 2. Execute request
    let exec_req = serde_json::to_string(&ActRequest::Execute {
        capability_id: "audio.volume.set".into(),
        params: serde_json::json!({"volume_percent": 90}),
    })
    .unwrap()
        + "\n";
    writer.write_all(exec_req.as_bytes()).await.unwrap();

    let resp_line2 = lines.next_line().await.unwrap().unwrap();
    let resp2: ActResponse = serde_json::from_str(&resp_line2).unwrap();
    let action_id = match resp2 {
        ActResponse::Success { data } => data["action_id"].as_str().unwrap().to_string(),
        other => panic!("Expected Success, got {other:?}"),
    };

    // 3. List undo entries
    let list_req = serde_json::to_string(&ActRequest::ListUndo).unwrap() + "\n";
    writer.write_all(list_req.as_bytes()).await.unwrap();

    let resp_line3 = lines.next_line().await.unwrap().unwrap();
    let resp3: ActResponse = serde_json::from_str(&resp_line3).unwrap();
    match resp3 {
        ActResponse::Success { data } => {
            assert!(!data.as_array().unwrap().is_empty());
        }
        other => panic!("Expected Success, got {other:?}"),
    }

    // 4. Trigger Undo
    let undo_req = serde_json::to_string(&ActRequest::Undo { action_id }).unwrap() + "\n";
    writer.write_all(undo_req.as_bytes()).await.unwrap();

    let resp_line4 = lines.next_line().await.unwrap().unwrap();
    let resp4: ActResponse = serde_json::from_str(&resp_line4).unwrap();
    match resp4 {
        ActResponse::Success { data } => {
            assert!(data["restored_result"]["applied"].as_bool().unwrap());
        }
        other => panic!("Expected Success, got {other:?}"),
    }
}
