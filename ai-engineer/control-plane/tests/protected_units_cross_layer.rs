//! Table-driven cross-layer verification for protected units.
//!
//! Asserts that across all three defense-in-depth layers (Handler, PolicyEngine, Reviewer),
//! every protected unit name is consistently recognized and protected without drift.

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::{CapabilityKind, CapabilityRequest, CapabilityResponse};
use control_plane::plan::{Plan, PlanStatus, PlanStep};
use control_plane::policy::{ApprovalTier, PolicyEngine};
use control_plane::reviewer::{ReviewVerdict, Reviewer, ReviewerConfig};

#[test]
fn test_cross_layer_protected_units_consistency() {
    let temp = tempfile::tempdir().unwrap();
    let scope = ScopedPath::new(temp.path()).unwrap();
    let killswitch = KillSwitch::load(temp.path().join("kill.lock")).unwrap();

    // Shared list of protected units and their normalized forms
    let shared_protected_units = [
        ("ssh.service", "SSH daemon Debian unit"),
        ("ssh", "SSH daemon bare name"),
        ("sshd.service", "SSH daemon Fedora unit"),
        ("sshd", "SSH daemon Fedora bare name"),
        ("systemd-udevd.service", "udev daemon with suffix"),
        ("systemd-udevd", "udev daemon bare name"),
        ("dbus.service", "D-Bus daemon with suffix"),
        ("dbus", "D-Bus daemon bare name"),
        ("firewalld.service", "Firewalld daemon with suffix"),
        ("firewalld", "Firewalld daemon bare name"),
    ];

    let reviewer = Reviewer::new(ReviewerConfig::default());

    for (unit, label) in shared_protected_units {
        // -------------------------------------------------------------
        // Layer 1: Handler (ai-capabilityd socket dispatcher)
        // Must deny restart and stop in both dry-run and live modes
        // -------------------------------------------------------------
        for dry_run in [true, false] {
            let audit =
                AuditLog::open(temp.path().join(format!("audit-{dry_run}-{unit}.jsonl"))).unwrap();
            let mut h = Handler::new(
                scope.clone(),
                audit,
                killswitch.clone(),
                HandlerConfig {
                    dry_run,
                    ..HandlerConfig::default()
                },
            );

            // Restart test
            let req_restart = CapabilityRequest {
                api_version: 1,
                id: format!("cross-restart-{unit}"),
                requester: "test".into(),
                token: "tok".into(),
                is_rollback: false,
                capability: CapabilityKind::ServiceRestart {
                    unit: unit.into(),
                    approved: true,
                },
            };
            let (_, resp_restart) = h.execute(&req_restart).unwrap();
            assert!(
                matches!(resp_restart, CapabilityResponse::Denied { .. }),
                "Handler (dry_run={dry_run}) MUST deny restart of {label} ('{unit}')"
            );

            // Stop test
            let req_stop = CapabilityRequest {
                api_version: 1,
                id: format!("cross-stop-{unit}"),
                requester: "test".into(),
                token: "tok".into(),
                is_rollback: false,
                capability: CapabilityKind::ServiceStop {
                    unit: unit.into(),
                    approved: true,
                },
            };
            let (_, resp_stop) = h.execute(&req_stop).unwrap();
            assert!(
                matches!(resp_stop, CapabilityResponse::Denied { .. }),
                "Handler (dry_run={dry_run}) MUST deny stop of {label} ('{unit}')"
            );
        }

        // -------------------------------------------------------------
        // Layer 2: PolicyEngine
        // Must score restart >= 90 and stop >= 95 with ExpertReview tier
        // -------------------------------------------------------------
        let pol_restart = PolicyEngine::evaluate_capability(
            &CapabilityKind::ServiceRestart {
                unit: unit.into(),
                approved: true,
            },
            None,
        );
        assert!(
            pol_restart.score >= 90,
            "PolicyEngine score for restart of {label} ('{unit}') must be >= 90 (got {})",
            pol_restart.score
        );
        assert_eq!(
            pol_restart.tier,
            ApprovalTier::ExpertReview,
            "PolicyEngine tier for restart of {label} ('{unit}') must be ExpertReview"
        );

        let pol_stop = PolicyEngine::evaluate_capability(
            &CapabilityKind::ServiceStop {
                unit: unit.into(),
                approved: true,
            },
            None,
        );
        assert!(
            pol_stop.score >= 95,
            "PolicyEngine score for stop of {label} ('{unit}') must be >= 95 (got {})",
            pol_stop.score
        );
        assert_eq!(
            pol_stop.tier,
            ApprovalTier::ExpertReview,
            "PolicyEngine tier for stop of {label} ('{unit}') must be ExpertReview"
        );

        // -------------------------------------------------------------
        // Layer 3: Reviewer
        // Must reject a plan attempting to restart/stop this unit if tier < ExpertReview
        // -------------------------------------------------------------
        let mut non_expert_blast = pol_restart.clone();
        non_expert_blast.tier = ApprovalTier::Notify;

        let plan = Plan {
            id: format!("cross-plan-{unit}"),
            intent: format!("restart {unit}"),
            summary: format!("restart {unit}"),
            steps: vec![PlanStep {
                step_id: "step-1".into(),
                description: format!("restart {unit}"),
                capability: CapabilityKind::ServiceRestart {
                    unit: unit.into(),
                    approved: true,
                },
                blast_radius: non_expert_blast,
                timeout_secs: 5,
            }],
            rollback_steps: vec![],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(
                &[CapabilityKind::ServiceRestart {
                    unit: unit.into(),
                    approved: true,
                }],
                None,
            ),
            created_at: "2026-08-28T00:00:00Z".into(),
            status: PlanStatus::Draft,
        };

        let report = reviewer.review_plan(&plan);
        assert_eq!(
            report.verdict,
            ReviewVerdict::Rejected,
            "Reviewer MUST reject plan attempting restart of {label} ('{unit}') with tier < ExpertReview"
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == "policy_mismatch" || f.category == "critical_unit"),
            "Reviewer MUST emit policy_mismatch or critical_unit finding for {label} ('{unit}')"
        );
    }
}
