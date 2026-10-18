use capability_layer::CapabilityKind;
use control_plane::policy::{ApprovalTier, PolicyEngine};

#[test]
fn test_network_manager_restart_demands_confirmation() {
    let req = CapabilityKind::ServiceRestart {
        unit: "NetworkManager.service".into(),
        approved: false,
    };
    let report = PolicyEngine::evaluate_capability(&req, None);
    assert!(report.tier.requires_confirmation());
    assert!(report.tier >= ApprovalTier::Confirm);
}

#[test]
fn test_firewalld_restart_demands_confirmation() {
    let req = CapabilityKind::ServiceRestart {
        unit: "firewalld.service".into(),
        approved: false,
    };
    let report = PolicyEngine::evaluate_capability(&req, None);
    assert!(report.tier.requires_confirmation());
    assert!(report.tier >= ApprovalTier::Confirm);
}

#[test]
fn test_network_sysctl_modification_demands_confirmation() {
    let req = CapabilityKind::KernelParamSet {
        key: "net.ipv4.ip_forward".into(),
        value: "1".into(),
        persistent: true,
        approved: false,
    };
    let report = PolicyEngine::evaluate_capability(&req, None);
    assert!(report.tier >= ApprovalTier::Notify);
    assert!(report.score >= 20);
}
