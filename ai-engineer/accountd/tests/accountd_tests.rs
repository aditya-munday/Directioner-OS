use accountd::{
    AccountEngine, AccountRequest, AccountResponse, AccountState, Audience, MockBackend,
    PlatformConfig, RateLimiter, ResponseData, StateStore,
};
use tempfile::tempdir;

#[test]
fn test_state_machine_transitions() {
    assert!(AccountState::SignedOut.can_transition_to(AccountState::SigningIn));
    assert!(AccountState::SigningIn.can_transition_to(AccountState::SignedIn));
    assert!(AccountState::SignedIn.can_transition_to(AccountState::Expiring));
    assert!(AccountState::Expiring.can_transition_to(AccountState::Expired));
    assert!(AccountState::SignedIn.can_transition_to(AccountState::OfflineGrace));
    assert!(AccountState::OfflineGrace.can_transition_to(AccountState::SignedIn));
    assert!(AccountState::SignedIn.can_transition_to(AccountState::Revoked));
    assert!(AccountState::Revoked.can_transition_to(AccountState::SignedOut));

    assert!(!AccountState::SignedOut.can_transition_to(AccountState::Expiring));
    assert!(!AccountState::Revoked.can_transition_to(AccountState::SignedIn));
}

#[test]
fn test_platform_config_validation() {
    let valid_toml = r#"
base_url = "https://auth.directioner.internal"
region = "us-east"
channel = "stable"
require_https = true
poll_interval_secs = 5
clock_skew_tolerance_secs = 60
"#;
    let cfg = PlatformConfig::from_str(valid_toml).expect("must parse valid toml");
    assert_eq!(cfg.region, "us-east");
    assert_eq!(cfg.channel, "stable");

    let invalid_http = r#"
base_url = "http://insecure.internal"
region = "global"
channel = "stable"
require_https = true
poll_interval_secs = 5
clock_skew_tolerance_secs = 60
"#;
    assert!(PlatformConfig::from_str(invalid_http).is_err());
}

#[test]
fn test_device_code_login_flow() {
    let dir = tempdir().unwrap();
    let cfg = PlatformConfig::default();
    let backend = MockBackend::new();
    let mut engine = AccountEngine::new(dir.path(), cfg, backend).unwrap();

    let start_resp = engine.handle_request(AccountRequest::LoginStartDevice, 1000, 10000);
    let dev_code = match start_resp {
        AccountResponse::Ok(ResponseData::DeviceCodeInitiated {
            device_code,
            user_code,
            ..
        }) => {
            assert!(user_code.starts_with("DIR-"));
            device_code
        }
        _ => panic!("Expected DeviceCodeInitiated"),
    };

    let poll_pending = engine.handle_request(
        AccountRequest::LoginPollDevice {
            device_code: dev_code.clone(),
        },
        1000,
        10001,
    );
    match poll_pending {
        AccountResponse::Error { code, .. } => assert_eq!(code, "authorization_pending"),
        _ => panic!("Expected authorization_pending error"),
    }

    engine.backend.approve_device_code(&dev_code);

    let poll_success = engine.handle_request(
        AccountRequest::LoginPollDevice {
            device_code: dev_code,
        },
        1000,
        10002,
    );
    match poll_success {
        AccountResponse::Ok(ResponseData::TokenIssued(token)) => {
            assert!(!token.access_token.is_empty());
            assert!(!token.refresh_token.is_empty());
        }
        _ => panic!("Expected TokenIssued"),
    }

    let status = engine.handle_request(AccountRequest::Status, 1000, 10003);
    match status {
        AccountResponse::Ok(ResponseData::StatusInfo {
            state, user, plan, ..
        }) => {
            assert_eq!(state, AccountState::SignedIn);
            assert!(user.is_some());
            assert_eq!(plan, Some("pro".to_string()));
        }
        _ => panic!("Expected StatusInfo signed in"),
    }
}

#[test]
fn test_pkce_login_flow() {
    let dir = tempdir().unwrap();
    let cfg = PlatformConfig::default();
    let backend = MockBackend::new();
    let mut engine = AccountEngine::new(dir.path(), cfg, backend).unwrap();

    let verifier = "secret_verifier_string_12345";
    let challenge = "ba8995a8ec55fdc714cc5d40a94f33c2daf0af48465ecb40f244d1697f972b57";

    let resp = engine.handle_request(
        AccountRequest::LoginPkce {
            verifier: verifier.to_string(),
            challenge: challenge.to_string(),
        },
        1000,
        20000,
    );

    match resp {
        AccountResponse::Ok(ResponseData::TokenIssued(t)) => {
            assert!(t.access_token.starts_with("at-pkce"));
        }
        _ => panic!("Expected TokenIssued for PKCE"),
    }
}

#[test]
fn test_refresh_token_rotation_and_reuse_detection() {
    let dir = tempdir().unwrap();
    let cfg = PlatformConfig::default();
    let backend = MockBackend::new();
    let mut engine = AccountEngine::new(dir.path(), cfg, backend).unwrap();

    let verifier = "secret_verifier_string_12345";
    let challenge = "ba8995a8ec55fdc714cc5d40a94f33c2daf0af48465ecb40f244d1697f972b57";
    let _ = engine.handle_request(
        AccountRequest::LoginPkce {
            verifier: verifier.to_string(),
            challenge: challenge.to_string(),
        },
        1000,
        30000,
    );

    let refresh_1 = engine.handle_request(AccountRequest::RefreshToken, 1000, 30001);
    let old_rt = match refresh_1 {
        AccountResponse::Ok(ResponseData::TokenIssued(t)) => t.refresh_token.clone(),
        _ => panic!("Expected refreshed token"),
    };

    let refresh_2 = engine.handle_request(AccountRequest::RefreshToken, 1000, 30002);
    assert!(matches!(
        refresh_2,
        AccountResponse::Ok(ResponseData::TokenIssued(_))
    ));

    let reuse_attempt = engine.backend.refresh_token(&old_rt, 30003);
    assert!(reuse_attempt.is_err());
    assert!(reuse_attempt.unwrap_err().contains("token_compromised"));
}

#[test]
fn test_scoped_audience_token_exchange() {
    let dir = tempdir().unwrap();
    let cfg = PlatformConfig::default();
    let backend = MockBackend::new();
    let mut engine = AccountEngine::new(dir.path(), cfg, backend).unwrap();

    let verifier = "secret_verifier_string_12345";
    let challenge = "ba8995a8ec55fdc714cc5d40a94f33c2daf0af48465ecb40f244d1697f972b57";
    let _ = engine.handle_request(
        AccountRequest::LoginPkce {
            verifier: verifier.to_string(),
            challenge: challenge.to_string(),
        },
        1000,
        40000,
    );

    for aud in &[
        Audience::Cli,
        Audience::Aria,
        Audience::Cloud,
        Audience::Update,
    ] {
        let resp = engine.handle_request(
            AccountRequest::TokenGet {
                audience: aud.as_str().to_string(),
            },
            1000,
            40001,
        );
        match resp {
            AccountResponse::Ok(ResponseData::TokenIssued(t)) => {
                assert_eq!(t.audience, aud.as_str());
                assert_eq!(t.scopes, aud.allowed_scopes());
            }
            _ => panic!("Expected scoped token for {}", aud.as_str()),
        }
    }
}

#[test]
fn test_local_rate_limiter() {
    let mut limiter = RateLimiter::new(3, 1);
    let now = 1000;

    assert!(limiter.check_and_consume(1000, now));
    assert!(limiter.check_and_consume(1000, now));
    assert!(limiter.check_and_consume(1000, now));
    assert!(!limiter.check_and_consume(1000, now));

    assert!(limiter.check_and_consume(1001, now));

    assert!(limiter.check_and_consume(1000, now + 1));
}

#[test]
fn test_offline_grace_and_clock_rollback() {
    let entitlements = accountd::Entitlements {
        plan: "pro".to_string(),
        features: vec!["aria_assist".to_string()],
        quota_remaining: 100,
        reset_epoch: 200000,
        grace_expiry: 250000,
        signature: "sig-valid".to_string(),
        last_verified_clock: 100000,
    };

    assert!(entitlements.is_valid_offline(150000));
    assert!(entitlements.is_valid_offline(250000));
    assert!(!entitlements.is_valid_offline(250001));
    assert!(!entitlements.is_valid_offline(90000));
}

#[test]
fn test_devices_list_and_revocation() {
    let dir = tempdir().unwrap();
    let cfg = PlatformConfig::default();
    let mut backend = MockBackend::new();

    backend.register_device(accountd::DeviceRegistration {
        device_id: "dev-laptop-02".to_string(),
        device_name: "directioner-laptop".to_string(),
        os_version: "42.0.0".to_string(),
        variant_id: "desktop".to_string(),
        registered_at: 10000,
        is_current: false,
        revoked: false,
    });

    let mut engine = AccountEngine::new(dir.path(), cfg, backend).unwrap();

    let list_resp = engine.handle_request(AccountRequest::DeviceList, 1000, 50000);
    match list_resp {
        AccountResponse::Ok(ResponseData::DeviceCollection(devs)) => {
            assert!(devs.len() >= 2);
        }
        _ => panic!("Expected DeviceCollection"),
    }

    let revoke_resp = engine.handle_request(
        AccountRequest::DeviceRevoke {
            device_id: "dev-laptop-02".to_string(),
        },
        1000,
        50001,
    );
    match revoke_resp {
        AccountResponse::Ok(ResponseData::DeviceRevoked { device_id }) => {
            assert_eq!(device_id, "dev-laptop-02");
        }
        _ => panic!("Expected DeviceRevoked"),
    }
}

#[test]
fn test_atomic_state_store_persistence() {
    let dir = tempdir().unwrap();
    let store = StateStore::new(dir.path()).unwrap();

    let rec = accountd::store::SessionRecord {
        state: AccountState::SignedIn,
        profile: None,
        token: None,
        entitlements: None,
        current_device: accountd::DeviceRegistration {
            device_id: "dev-01".to_string(),
            device_name: "pc".to_string(),
            os_version: "42".to_string(),
            variant_id: "server".to_string(),
            registered_at: 100,
            is_current: true,
            revoked: false,
        },
        updated_at: 1000,
    };

    store.save_session(&rec).unwrap();
    let loaded = store
        .load_session()
        .unwrap()
        .expect("must load saved session");
    assert_eq!(loaded.state, AccountState::SignedIn);

    store.clear_session().unwrap();
    assert!(store.load_session().unwrap().is_none());
}

#[test]
fn test_protocol_malformed_json_fuzz() {
    let malformed_inputs = [
        "",
        "{",
        "[]",
        "{\"action\": \"UnknownAction\"}",
        "{\"action\": \"LoginPollDevice\", \"params\": {}}",
        "null",
        "12345",
        "{\"action\": \"TokenGet\", \"params\": {\"audience\": 99}}",
    ];

    for input in &malformed_inputs {
        let res = serde_json::from_str::<AccountRequest>(input);
        assert!(res.is_err(), "Malformed input should fail: {}", input);
    }
}

#[test]
fn test_encrypted_token_storage_zero_disk_plaintext_leak() {
    let dir = tempdir().unwrap();
    let store = StateStore::new(dir.path()).unwrap();

    let secret_token = "ultra_secret_access_token_99999";
    let secret_refresh = "ultra_secret_refresh_token_88888";

    let tok = accountd::AuthToken {
        access_token: secret_token.to_string(),
        refresh_token: secret_refresh.to_string(),
        token_type: "Bearer".to_string(),
        expires_in: 3600,
        created_at: 1000,
        audience: "cli".to_string(),
        scopes: vec!["cli:exec".to_string()],
    };

    let rec = accountd::store::SessionRecord {
        state: AccountState::SignedIn,
        profile: None,
        token: Some(tok),
        entitlements: None,
        current_device: accountd::DeviceRegistration {
            device_id: "dev-01".to_string(),
            device_name: "pc".to_string(),
            os_version: "42".to_string(),
            variant_id: "server".to_string(),
            registered_at: 100,
            is_current: true,
            revoked: false,
        },
        updated_at: 1000,
    };

    store.save_session(&rec).unwrap();

    let session_file_raw = std::fs::read_to_string(dir.path().join("session.json")).unwrap();
    assert!(
        !session_file_raw.contains(secret_token),
        "Plaintext access token leaked to disk"
    );
    assert!(
        !session_file_raw.contains(secret_refresh),
        "Plaintext refresh token leaked to disk"
    );

    let loaded = store
        .load_session()
        .unwrap()
        .expect("must decrypt stored token");
    let loaded_tok = loaded.token.expect("must contain token");
    assert_eq!(loaded_tok.access_token, secret_token);
    assert_eq!(loaded_tok.refresh_token, secret_refresh);
}

#[test]
fn test_live_unix_socket_ipc() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::mpsc;
    use std::thread;

    let dir = tempdir().unwrap();
    let sock_path = dir.path().join("test_accountd.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let (tx, rx) = mpsc::channel();
    let data_dir = dir.path().to_path_buf();

    let handle = thread::spawn(move || {
        let cfg = PlatformConfig::default();
        let backend = MockBackend::new();
        let mut engine = AccountEngine::new(&data_dir, cfg, backend).unwrap();

        tx.send(()).unwrap();

        if let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok()
                && let Ok(req) = serde_json::from_str::<AccountRequest>(&line)
            {
                let resp = engine.handle_request(req, 1000, 100000);
                let out = serde_json::to_string(&resp).unwrap();
                let _ = stream.write_all(out.as_bytes());
                let _ = stream.write_all(b"\n");
                let _ = stream.flush();
            }
        }
    });

    rx.recv().unwrap();

    let mut stream = UnixStream::connect(&sock_path).unwrap();
    let req = AccountRequest::Status;
    let serialized = serde_json::to_string(&req).unwrap();
    stream.write_all(serialized.as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).unwrap();

    let resp: AccountResponse = serde_json::from_str(&resp_line).unwrap();
    match resp {
        AccountResponse::Ok(ResponseData::StatusInfo { state, .. }) => {
            assert_eq!(state, AccountState::SignedOut);
        }
        _ => panic!("Expected StatusInfo response"),
    }

    drop(reader);
    let _ = handle.join();
}
