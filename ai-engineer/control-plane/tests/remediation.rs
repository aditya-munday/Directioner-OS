//! Integration tests for Tier 2 Self-Healing & Remediation (Phase 3.0.4).

use tempfile::tempdir;

use capability_layer::CapabilityKind;
use control_plane::policy::ApprovalTier;
use control_plane::remediation::{RemediationAgent, RemediationConfig, RemediationError};

#[test]
fn test_tier2_kernel_param_tuning_lifecycle() {
    let dir = tempdir().unwrap();
    let proc_net_ipv4 = dir.path().join("net/ipv4");
    std::fs::create_dir_all(&proc_net_ipv4).unwrap();
    std::fs::write(proc_net_ipv4.join("ip_forward"), "0\n").unwrap();

    let agent = RemediationAgent::new(RemediationConfig {
        proc_root: dir.path().to_path_buf(),
        max_blast_radius: 80,
    });

    let plan = agent
        .plan_kernel_param_tuning("net.ipv4.ip_forward", "1", true, None)
        .expect("synthesis should succeed");

    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.rollback_steps.len(), 1);

    // Conservative floor check: score >= 20, tier >= Notify
    assert!(plan.aggregate_blast_radius.tier >= ApprovalTier::Notify);
    assert!(plan.aggregate_blast_radius.score >= 20);

    // Forward step has unapproved status initially
    match &plan.steps[0].capability {
        CapabilityKind::KernelParamSet {
            key,
            value,
            persistent,
            approved,
        } => {
            assert_eq!(key, "net.ipv4.ip_forward");
            assert_eq!(value, "1");
            assert!(persistent);
            assert!(!approved);
        }
        _ => panic!("unexpected forward capability"),
    }

    // Rollback step restores "0"
    match &plan.rollback_steps[0].capability {
        CapabilityKind::KernelParamSet {
            key,
            value,
            persistent,
            approved,
        } => {
            assert_eq!(key, "net.ipv4.ip_forward");
            assert_eq!(value, "0");
            assert!(persistent);
            assert!(approved);
        }
        _ => panic!("unexpected rollback capability"),
    }
}

#[test]
fn test_tier2_kernel_module_remediation_lifecycle() {
    let agent = RemediationAgent::new(RemediationConfig::default());

    let plan = agent
        .plan_module_load("dummy", vec!["numdummies=2".into()], None)
        .expect("synthesis for allowed module should succeed");

    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.rollback_steps.len(), 1);

    // Conservative floor check
    assert!(plan.aggregate_blast_radius.tier >= ApprovalTier::Notify);
    assert!(plan.aggregate_blast_radius.score >= 20);

    match &plan.steps[0].capability {
        CapabilityKind::KernelModuleLoad {
            module,
            args,
            approved,
        } => {
            assert_eq!(module, "dummy");
            assert_eq!(args, &["numdummies=2"]);
            assert!(!approved);
        }
        _ => panic!("unexpected forward capability"),
    }

    match &plan.rollback_steps[0].capability {
        CapabilityKind::KernelModuleUnload { module, approved } => {
            assert_eq!(module, "dummy");
            assert!(approved);
        }
        _ => panic!("unexpected rollback capability"),
    }
}

#[test]
fn test_tier2_prohibited_security_param_hard_fatal_refusal() {
    let agent = RemediationAgent::new(RemediationConfig::default());

    let err = agent
        .plan_kernel_param_tuning("kernel.core_pattern", "|/tmp/bad", false, None)
        .expect_err("prohibited param must be rejected");

    match err {
        RemediationError::ReviewRejected(msg) => {
            assert!(
                msg.contains("prohibited security-critical denylist") || msg.contains("forbidden")
            );
        }
        _ => panic!("expected ReviewRejected error"),
    }
}

#[test]
fn test_tier2_unauthorized_module_hard_fatal_refusal() {
    let agent = RemediationAgent::new(RemediationConfig::default());

    let err = agent
        .plan_module_load("unknown_untrusted_module", vec![], None)
        .expect_err("unauthorized module must be rejected");

    match err {
        RemediationError::ReviewRejected(msg) => {
            assert!(msg.contains("not in the approved kernel module allowlist"));
        }
        _ => panic!("expected ReviewRejected error"),
    }
}
