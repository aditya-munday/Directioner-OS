//! Tests for Dynamic Live Package Unlayering and Layering.

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::{API_VERSION, CapabilityKind, CapabilityRequest, CapabilityResponse};
use tempfile::TempDir;

fn make_req(id: &str, capability: CapabilityKind) -> CapabilityRequest {
    CapabilityRequest {
        api_version: API_VERSION,
        id: id.into(),
        requester: "test".into(),
        token: "tok-test".into(),
        is_rollback: false,
        capability,
    }
}

#[test]
fn test_package_remove_dry_run_reports_unlayering_intent() {
    let tmp = TempDir::new().unwrap();
    let scope = ScopedPath::new(tmp.path()).unwrap();
    let audit = AuditLog::open(tmp.path().join("audit.jsonl")).unwrap();
    let killswitch = KillSwitch::new(tmp.path().join("killswitch.lock"));
    let config = HandlerConfig {
        approval_required: false,
        protected_units: vec![],
        dry_run: true,
    };

    let mut handler = Handler::new(scope, audit, killswitch, config);
    let req = make_req(
        "req-1",
        CapabilityKind::PackageRemove {
            package: "htop".to_string(),
            approved: true,
        },
    );
    let res = handler.execute(&req);
    assert!(res.is_ok());
    let (_seq, resp) = res.unwrap();
    match resp {
        CapabilityResponse::Success { result, .. } => {
            assert_eq!(result.get("dry_run").and_then(|v| v.as_bool()), Some(true));
            assert_eq!(result.get("package").and_then(|v| v.as_str()), Some("htop"));
        }
        _ => panic!("Expected Success, got {resp:?}"),
    }
}

#[test]
fn test_package_install_dry_run_reports_layering_intent() {
    let tmp = TempDir::new().unwrap();
    let scope = ScopedPath::new(tmp.path()).unwrap();
    let audit = AuditLog::open(tmp.path().join("audit.jsonl")).unwrap();
    let killswitch = KillSwitch::new(tmp.path().join("killswitch.lock"));
    let config = HandlerConfig {
        approval_required: false,
        protected_units: vec![],
        dry_run: true,
    };

    let mut handler = Handler::new(scope, audit, killswitch, config);
    let req = make_req(
        "req-2",
        CapabilityKind::PackageInstall {
            package: "ripgrep".to_string(),
            approved: true,
        },
    );
    let res = handler.execute(&req);
    assert!(res.is_ok());
    let (_seq, resp) = res.unwrap();
    match resp {
        CapabilityResponse::Success { result, .. } => {
            assert_eq!(result.get("dry_run").and_then(|v| v.as_bool()), Some(true));
            assert_eq!(
                result.get("package").and_then(|v| v.as_str()),
                Some("ripgrep")
            );
        }
        _ => panic!("Expected Success, got {resp:?}"),
    }
}

#[test]
fn test_package_remove_approval_gate_enforcement() {
    let tmp = TempDir::new().unwrap();
    let scope = ScopedPath::new(tmp.path()).unwrap();
    let audit = AuditLog::open(tmp.path().join("audit.jsonl")).unwrap();
    let killswitch = KillSwitch::new(tmp.path().join("killswitch.lock"));
    let config = HandlerConfig {
        approval_required: true,
        protected_units: vec![],
        dry_run: false,
    };

    let mut handler = Handler::new(scope, audit, killswitch, config);
    let req = make_req(
        "req-3",
        CapabilityKind::PackageRemove {
            package: "htop".to_string(),
            approved: false,
        },
    );
    let res = handler.execute(&req);
    assert!(res.is_ok());
    let (_seq, resp) = res.unwrap();
    match resp {
        CapabilityResponse::Denied { reason, .. } => {
            assert!(reason.contains("approval gate not cleared"));
        }
        _ => panic!("Expected Denied, got {resp:?}"),
    }
}
