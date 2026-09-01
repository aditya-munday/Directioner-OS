//! Comprehensive defensive Reviewer test suite covering all 14 capability primitives.
//!
//! Asserts that the independent Reviewer agent correctly audits, flags, and rejects
//! hazardous or malformed execution plans across every capability type without modifying
//! reviewer.rs.

use capability_layer::CapabilityKind;
use control_plane::plan::{Plan, PlanStatus, PlanStep, RollbackStep};
use control_plane::policy::{ApprovalTier, BlastRadiusReport};
use control_plane::reviewer::{FindingSeverity, ReviewVerdict, Reviewer, ReviewerConfig};

fn make_plan(step_id: &str, capability: CapabilityKind, rollbacks: Vec<RollbackStep>) -> Plan {
    Plan {
        id: format!("test-plan-{step_id}"),
        intent: "Defensive Reviewer Invariant Test".into(),
        summary: "Testing Reviewer defensive completeness".into(),
        steps: vec![PlanStep {
            step_id: step_id.into(),
            description: "Test step execution".into(),
            capability,
            blast_radius: BlastRadiusReport {
                tier: ApprovalTier::Notify,
                score: 20,
                factors: vec![],
            },
            timeout_secs: 10,
        }],
        rollback_steps: rollbacks,
        status: PlanStatus::Draft,
        aggregate_blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Notify,
            score: 20,
            factors: vec![],
        },
        created_at: "2026-09-01T00:00:00Z".into(),
    }
}

#[test]
fn test_reviewer_defense_file_write_path_traversal() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::FileWrite {
            path: "/tmp/work/../../etc/shadow".into(),
            content: "root::0:0:::\n".into(),
            append: false,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Rollback".into(),
            capability: CapabilityKind::FileDelete {
                path: "/tmp/work/../../etc/shadow".into(),
            },
            triggers: vec!["s1".into()],
        }],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal && f.category == "path_security"),
        "Reviewer must issue Fatal finding for path traversal in FileWrite"
    );
}

#[test]
fn test_reviewer_defense_file_delete_critical_system_path() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::FileDelete {
            path: "/usr/lib/systemd/systemd/../../bin".into(),
        },
        vec![],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal && f.category == "path_security"),
        "Reviewer must reject path traversal on FileDelete"
    );
}

#[test]
fn test_reviewer_defense_file_write_critical_path_policy_mismatch() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::FileWrite {
            path: "/etc/sudoers.d/99-test".into(),
            content: "operator ALL=(ALL) ALL\n".into(),
            append: false,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Rollback".into(),
            capability: CapabilityKind::FileDelete {
                path: "/etc/sudoers.d/99-test".into(),
            },
            triggers: vec!["s1".into()],
        }],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal && f.category == "policy_mismatch"),
        "Reviewer must issue Fatal policy_mismatch when writing critical paths below ExpertReview"
    );
}

#[test]
fn test_reviewer_defense_package_remove_vital_system_package() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::PackageRemove {
            package: "systemd".into(),
            approved: false,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Reinstall".into(),
            capability: CapabilityKind::PackageInstall {
                package: "systemd".into(),
                approved: false,
            },
            triggers: vec!["s1".into()],
        }],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal && f.category == "destructive_action"),
        "Reviewer must issue Fatal finding when removing vital package 'systemd'"
    );
}

#[test]
fn test_reviewer_defense_service_stop_forbidden_udevd() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::ServiceStop {
            unit: "systemd-udevd.service".into(),
            approved: false,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Restart udevd".into(),
            capability: CapabilityKind::ServiceRestart {
                unit: "systemd-udevd.service".into(),
                approved: false,
            },
            triggers: vec!["s1".into()],
        }],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal && f.category == "critical_unit"),
        "Reviewer must issue Fatal finding for stopping systemd-udevd.service"
    );
}

#[test]
fn test_reviewer_defense_service_restart_forbidden_udevd() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::ServiceRestart {
            unit: "systemd-udevd.service".into(),
            approved: false,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Restart".into(),
            capability: CapabilityKind::ServiceRestart {
                unit: "systemd-udevd.service".into(),
                approved: false,
            },
            triggers: vec!["s1".into()],
        }],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal && f.category == "critical_unit"),
        "Reviewer must reject restart of forbidden systemd-udevd"
    );
}

#[test]
fn test_reviewer_defense_kernel_param_prohibited_security_override() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::KernelParamSet {
            key: "kernel.dmesg_restrict".into(),
            value: "0".into(),
            persistent: true,
            approved: false,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Restore param".into(),
            capability: CapabilityKind::KernelParamSet {
                key: "kernel.dmesg_restrict".into(),
                value: "1".into(),
                persistent: true,
                approved: false,
            },
            triggers: vec!["s1".into()],
        }],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal
                && f.category == "prohibited_security_parameter"),
        "Reviewer must reject prohibited kernel security param relaxation"
    );
}

#[test]
fn test_reviewer_defense_kernel_module_unauthorized_load() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::KernelModuleLoad {
            module: "untrusted_rootkit".into(),
            args: vec![],
            approved: false,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Unload".into(),
            capability: CapabilityKind::KernelModuleUnload {
                module: "untrusted_rootkit".into(),
                approved: false,
            },
            triggers: vec!["s1".into()],
        }],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal
                && f.category == "unauthorized_kernel_module"),
        "Reviewer must reject unauthorized kernel module insertion"
    );
}

#[test]
fn test_reviewer_defense_kernel_module_unauthorized_unload() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::KernelModuleUnload {
            module: "unknown_custom_mod".into(),
            approved: false,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Reload".into(),
            capability: CapabilityKind::KernelModuleLoad {
                module: "unknown_custom_mod".into(),
                args: vec![],
                approved: false,
            },
            triggers: vec!["s1".into()],
        }],
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal
                && f.category == "unauthorized_kernel_module"),
        "Reviewer must reject unloading unallowlisted kernel module"
    );
}

#[test]
fn test_reviewer_defense_missing_rollback_coverage() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = make_plan(
        "s1",
        CapabilityKind::FileWrite {
            path: "/tmp/work/nginx.conf".into(),
            content: "server { listen 80; }\n".into(),
            append: false,
        },
        vec![], // Missing rollback step for mutating FileWrite
    );
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::ApprovedWithWarnings);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Warning && f.category == "rollback_coverage"),
        "Reviewer must issue Warning finding for missing rollback coverage on mutating step"
    );
}

#[test]
fn test_reviewer_defense_step_ordering_anomaly_fatal() {
    let reviewer = Reviewer::new(ReviewerConfig::default());
    let plan = Plan {
        id: "plan-order-anomaly".into(),
        intent: "Restart before install".into(),
        summary: "Step ordering anomaly".into(),
        steps: vec![
            PlanStep {
                step_id: "s1".into(),
                description: "Restart service before package install".into(),
                capability: CapabilityKind::ServiceRestart {
                    unit: "nginx.service".into(),
                    approved: false,
                },
                blast_radius: BlastRadiusReport {
                    tier: ApprovalTier::Notify,
                    score: 20,
                    factors: vec![],
                },
                timeout_secs: 10,
            },
            PlanStep {
                step_id: "s2".into(),
                description: "Install nginx package after restart".into(),
                capability: CapabilityKind::PackageInstall {
                    package: "nginx".into(),
                    approved: false,
                },
                blast_radius: BlastRadiusReport {
                    tier: ApprovalTier::Notify,
                    score: 20,
                    factors: vec![],
                },
                timeout_secs: 10,
            },
        ],
        rollback_steps: vec![],
        status: PlanStatus::Draft,
        aggregate_blast_radius: BlastRadiusReport {
            tier: ApprovalTier::Notify,
            score: 20,
            factors: vec![],
        },
        created_at: "2026-09-01T00:00:00Z".into(),
    };
    let review = reviewer.review_plan(&plan);
    assert_eq!(review.verdict, ReviewVerdict::Rejected);
    assert!(
        review
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal && f.category == "ordering_anomaly"),
        "Reviewer must issue Fatal finding for ordering anomaly (package installed after restart)"
    );
}

#[test]
fn test_reviewer_defense_read_only_capabilities_permitted() {
    let reviewer = Reviewer::new(ReviewerConfig::default());

    // Ping (non-mutating, no rollback required)
    let plan_ping = make_plan("p1", CapabilityKind::Ping, vec![]);
    assert_eq!(
        reviewer.review_plan(&plan_ping).verdict,
        ReviewVerdict::Approved
    );

    // FileRead (non-mutating, no rollback required)
    let plan_read = make_plan(
        "r1",
        CapabilityKind::FileRead {
            path: "/etc/os-release".into(),
        },
        vec![],
    );
    assert_eq!(
        reviewer.review_plan(&plan_read).verdict,
        ReviewVerdict::Approved
    );

    // ServiceStatus (non-mutating, no rollback required)
    let plan_status = make_plan(
        "st1",
        CapabilityKind::ServiceStatus {
            unit: "chronyd.service".into(),
        },
        vec![],
    );
    assert_eq!(
        reviewer.review_plan(&plan_status).verdict,
        ReviewVerdict::Approved
    );
}

#[test]
fn test_reviewer_defense_boot_slot_and_bootc_rollback_review() {
    let reviewer = Reviewer::new(ReviewerConfig::default());

    // BootSlotPin requires rollback coverage
    let plan_boot = make_plan(
        "b1",
        CapabilityKind::BootSlotPin {
            slot: 0,
            approved: true,
        },
        vec![RollbackStep {
            step_id: "rb1".into(),
            description: "Restore previous slot".into(),
            capability: CapabilityKind::BootSlotPin {
                slot: 1,
                approved: true,
            },
            triggers: vec!["b1".into()],
        }],
    );
    let review_boot = reviewer.review_plan(&plan_boot);
    assert_eq!(review_boot.verdict, ReviewVerdict::Approved);

    // BootcRollback is non-mutating with respect to rollback coverage requirement
    let plan_rb = make_plan("rb1", CapabilityKind::BootcRollback, vec![]);
    let review_rb = reviewer.review_plan(&plan_rb);
    assert_eq!(review_rb.verdict, ReviewVerdict::Approved);
}
