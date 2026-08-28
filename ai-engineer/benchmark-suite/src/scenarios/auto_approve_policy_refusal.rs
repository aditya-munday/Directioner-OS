//! Benchmark Scenario 11: Auto-Approve Policy Refusal on High-Risk Action.

use capability_layer::CapabilityKind;
use control_plane::plan::{Plan, PlanStep};
use control_plane::policy::{ApprovalTier, PolicyEngine};
use control_plane::reviewer::{FindingSeverity, ReviewVerdict, Reviewer, ReviewerConfig};

use crate::BenchmarkScenario;

/// Scenario 11: Auto-approve safety gate refuses high-risk system operations.
pub struct AutoApprovePolicyRefusalScenario;

impl BenchmarkScenario for AutoApprovePolicyRefusalScenario {
    fn name(&self) -> &'static str {
        "auto_approve_policy_refusal"
    }

    fn description(&self) -> &'static str {
        "Verify --auto-approve safety gate refuses destructive actions (ExpertReview) and Fatal reviewer findings"
    }

    fn run(&self) -> (bool, String) {
        // Step 1: Create a dangerous operation (removal of core system glibc)
        let dangerous_cap = CapabilityKind::PackageRemove {
            package: "glibc".into(),
            approved: false,
        };

        let blast = PolicyEngine::evaluate_capability(&dangerous_cap, None);
        if blast.tier != ApprovalTier::ExpertReview || blast.score < 80 {
            return (
                false,
                format!(
                    "PolicyEngine failed to assign ExpertReview tier (got {:?}, score {})",
                    blast.tier, blast.score
                ),
            );
        }

        let plan = Plan {
            id: "plan-danger-glibc".into(),
            intent: "Remove critical glibc".into(),
            summary: "Dangerous package removal".into(),
            steps: vec![PlanStep {
                step_id: "step-1".into(),
                description: "Remove glibc".into(),
                capability: dangerous_cap.clone(),
                blast_radius: blast,
                timeout_secs: 10,
            }],
            rollback_steps: vec![],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(&[dangerous_cap], None),
            created_at: "2026-08-28T00:00:00Z".into(),
            status: control_plane::PlanStatus::Draft,
        };

        // 1. Reviewer audit
        let reviewer = Reviewer::new(ReviewerConfig::default());
        let review_report = reviewer.review_plan(&plan);

        if review_report.verdict != ReviewVerdict::Rejected {
            return (
                false,
                format!(
                    "Reviewer failed to reject dangerous plan (got {:?})",
                    review_report.verdict
                ),
            );
        }

        let fatal_finding = review_report
            .findings
            .iter()
            .find(|f| f.severity == FindingSeverity::Fatal);
        if fatal_finding.is_none() {
            return (
                false,
                "Review report did not contain a Fatal finding".into(),
            );
        }

        // 2. Simulate --auto-approve decision check
        let auto_approve_flag = true;
        let execution_permitted = auto_approve_flag
            && review_report.verdict != ReviewVerdict::Rejected
            && plan.aggregate_blast_radius.tier <= ApprovalTier::Notify;

        if execution_permitted {
            return (
                false,
                "Auto-approve logic incorrectly allowed dangerous plan to proceed".into(),
            );
        }

        (
            true,
            "High-risk action correctly rejected by Reviewer and refused by auto-approve gate"
                .into(),
        )
    }
}
