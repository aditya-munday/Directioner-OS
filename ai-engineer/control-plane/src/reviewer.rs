//! Multi-Agent Reviewer Role (Phase 2).
//!
//! An independent audit and verification pass that reviews synthesized [`Plan`]s
//! before implementation. The Reviewer enforces architectural invariants, verifies
//! rollback completeness, inspects blast-radius calibration, and flags safety anti-patterns.

use capability_layer::{ALLOWED_KERNEL_MODULES, CapabilityKind, PROHIBITED_KERNEL_PARAMS};
pub use capability_layer::{
    ALLOWED_KERNEL_MODULES as REVIEWER_ALLOWED_MODULES,
    PROHIBITED_KERNEL_PARAMS as REVIEWER_PROHIBITED_PARAMS,
};
use serde::{Deserialize, Serialize};

use crate::plan::{Plan, PlanStep};
use crate::policy::{ApprovalTier, CRITICAL_SYSTEM_PACKAGES, CRITICAL_SYSTEM_PATHS};

/// The outcome verdict produced by the Reviewer Agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    /// Plan satisfies all safety, rollback, and architectural invariants.
    Approved,
    /// Plan is acceptable but has non-blocking advisory warnings.
    ApprovedWithWarnings,
    /// Plan violates safety invariants and execution MUST be blocked.
    Rejected,
}

/// Detailed review report produced by the Reviewer Agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewReport {
    /// Overall verdict.
    pub verdict: ReviewVerdict,
    /// Detailed findings and rule violations (if rejected or warned).
    pub findings: Vec<ReviewFinding>,
    /// Summary commentary from the reviewer.
    pub summary: String,
}

/// A specific finding or violation identified during plan review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewFinding {
    /// Severity level of the finding.
    pub severity: FindingSeverity,
    /// Category of the rule.
    pub category: String,
    /// Human-readable explanation.
    pub message: String,
    /// Specific step_id associated with the finding, if applicable.
    pub step_id: Option<String>,
}

/// Severity of a review finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    /// Informational advisory.
    Info,
    /// Non-fatal warning.
    Warning,
    /// Critical blocker that prevents execution.
    Fatal,
}

/// Configuration for the Reviewer Agent.
#[derive(Debug, Clone)]
pub struct ReviewerConfig {
    /// Whether to require rollbacks for all state-mutating steps.
    pub require_rollbacks: bool,
    /// Maximum allowed aggregate blast radius score for automatic approval without review.
    pub max_auto_score: u32,
}

impl Default for ReviewerConfig {
    fn default() -> Self {
        Self {
            require_rollbacks: true,
            max_auto_score: 20,
        }
    }
}

/// The Reviewer Agent.
pub struct Reviewer {
    config: ReviewerConfig,
}

impl Reviewer {
    /// Create a new Reviewer instance with specified configuration.
    pub fn new(config: ReviewerConfig) -> Self {
        Self { config }
    }

    /// Perform an independent review pass over a synthesized plan.
    pub fn review_plan(&self, plan: &Plan) -> ReviewReport {
        let mut findings = Vec::new();

        // 1. Check for empty plan
        if plan.steps.is_empty() {
            findings.push(ReviewFinding {
                severity: FindingSeverity::Fatal,
                category: "structure".into(),
                message: "Plan contains no steps to execute".into(),
                step_id: None,
            });
        }

        // 2. Check each step for safety and traversal violations
        for step in &plan.steps {
            self.review_step(step, &mut findings);
        }

        // 3. Verify Rollback Completeness
        if self.config.require_rollbacks {
            self.verify_rollback_coverage(plan, &mut findings);
        }

        // 4. Verify Step Ordering (e.g. Package install before service restart)
        self.verify_step_ordering(&plan.steps, &mut findings);

        // 5. Check Approval Tier Consistency
        if plan.aggregate_blast_radius.tier == ApprovalTier::ExpertReview
            && plan.aggregate_blast_radius.score < 80
        {
            findings.push(ReviewFinding {
                severity: FindingSeverity::Warning,
                category: "policy_calibration".into(),
                message: "Plan marked as ExpertReview but has blast radius score < 80".into(),
                step_id: None,
            });
        }

        // 6. Check Cross-Crate Dispersion (>=3 distinct crates)
        let mut unique_crates = std::collections::HashSet::new();
        for step in &plan.steps {
            if (matches!(step.capability, CapabilityKind::FileWrite { .. })
                || matches!(step.capability, CapabilityKind::FileDelete { .. })
                || matches!(step.capability, CapabilityKind::FileRead { .. }))
                && let Some(path) = match &step.capability {
                    CapabilityKind::FileWrite { path, .. }
                    | CapabilityKind::FileDelete { path }
                    | CapabilityKind::FileRead { path } => Some(path),
                    _ => None,
                }
                && let Some(crate_name) = crate::policy::extract_crate_from_path(path)
            {
                unique_crates.insert(crate_name);
            }
        }
        if unique_crates.len() >= 3 {
            let mut sorted: Vec<String> = unique_crates.into_iter().collect();
            sorted.sort();
            findings.push(ReviewFinding {
                severity: FindingSeverity::Warning,
                category: "cross_crate_dispersion".into(),
                message: format!(
                    "Plan modifies files across {} distinct crates ({}). High dispersion increases integration risk.",
                    sorted.len(),
                    sorted.join(", ")
                ),
                step_id: None,
            });
        }

        // Calculate final verdict
        let has_fatal = findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Fatal);
        let has_warning = findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Warning);

        let verdict = if has_fatal {
            ReviewVerdict::Rejected
        } else if has_warning {
            ReviewVerdict::ApprovedWithWarnings
        } else {
            ReviewVerdict::Approved
        };

        let summary = match verdict {
            ReviewVerdict::Approved => {
                format!(
                    "Review passed: Plan '{}' meets all safety invariants and rollback requirements.",
                    plan.id
                )
            }
            ReviewVerdict::ApprovedWithWarnings => {
                format!(
                    "Review passed with {} warnings: Plan '{}' is executable with caveats.",
                    findings.len(),
                    plan.id
                )
            }
            ReviewVerdict::Rejected => {
                let fatal_count = findings
                    .iter()
                    .filter(|f| f.severity == FindingSeverity::Fatal)
                    .count();
                format!(
                    "Review REJECTED: Plan '{}' has {} fatal invariant violations and cannot proceed.",
                    plan.id, fatal_count
                )
            }
        };

        ReviewReport {
            verdict,
            findings,
            summary,
        }
    }

    fn review_step(&self, step: &PlanStep, findings: &mut Vec<ReviewFinding>) {
        match &step.capability {
            CapabilityKind::FileWrite { path, content, .. } => {
                // Check path traversal
                if path.contains("..") {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "path_security".into(),
                        message: format!("Path traversal '..' detected in file target '{path}'"),
                        step_id: Some(step.step_id.clone()),
                    });
                }

                // Check critical system files
                if CRITICAL_SYSTEM_PATHS
                    .iter()
                    .any(|crit| path.starts_with(crit))
                    && step.blast_radius.tier < ApprovalTier::ExpertReview
                {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "policy_mismatch".into(),
                        message: format!(
                            "Writing critical path '{path}' without ExpertReview approval tier"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }

                // Warning on empty write without explicit deletion intent
                if content.is_empty()
                    && !step.description.to_lowercase().contains("delete")
                    && !step.description.to_lowercase().contains("truncate")
                {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Warning,
                        category: "data_loss_risk".into(),
                        message: format!("Step writes 0 bytes to '{path}'; file will be emptied"),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::FileDelete { path } => {
                if path.contains("..") {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "path_security".into(),
                        message: format!(
                            "Path traversal '..' detected in deletion target '{path}'"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }

                // Check critical system files
                if CRITICAL_SYSTEM_PATHS
                    .iter()
                    .any(|crit| path.starts_with(crit))
                    && step.blast_radius.tier < ApprovalTier::ExpertReview
                {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "policy_mismatch".into(),
                        message: format!(
                            "Deleting critical path '{path}' without ExpertReview approval tier"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::PackageInstall { package, .. } => {
                if package.trim().is_empty() {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "validation".into(),
                        message: "Package name cannot be empty".into(),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::PackageRemove { package, .. } => {
                let pkg_lower = package.to_lowercase();
                if package == "glibc" || package == "systemd" || package == "kernel" {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "destructive_action".into(),
                        message: format!("Refusing removal of vital system package '{package}'"),
                        step_id: Some(step.step_id.clone()),
                    });
                } else if CRITICAL_SYSTEM_PACKAGES
                    .iter()
                    .any(|crit| pkg_lower == *crit || pkg_lower.starts_with(&format!("{crit}-")))
                    && step.blast_radius.tier < ApprovalTier::ExpertReview
                {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "policy_mismatch".into(),
                        message: format!(
                            "Removing core system package '{package}' without ExpertReview approval tier"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::ServiceRestart { unit, .. } => {
                let protected = [
                    "ssh.service",
                    "sshd.service",
                    "systemd-udevd.service",
                    "dbus.service",
                    "firewalld.service",
                ];
                if unit == "systemd-udevd.service" {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "critical_unit".into(),
                        message: "Restarting systemd-udevd is forbidden".into(),
                        step_id: Some(step.step_id.clone()),
                    });
                } else if is_ssh_unit_match(&protected, unit)
                    && step.blast_radius.tier < ApprovalTier::ExpertReview
                {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "policy_mismatch".into(),
                        message: format!(
                            "Restarting protected daemon '{unit}' without ExpertReview approval tier"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::ServiceStop { unit, .. } => {
                let protected = [
                    "ssh.service",
                    "sshd.service",
                    "systemd-udevd.service",
                    "dbus.service",
                    "firewalld.service",
                ];
                if unit == "systemd-udevd.service" {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "critical_unit".into(),
                        message: "Stopping systemd-udevd is forbidden".into(),
                        step_id: Some(step.step_id.clone()),
                    });
                } else if is_ssh_unit_match(&protected, unit)
                    && step.blast_radius.tier < ApprovalTier::ExpertReview
                {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "policy_mismatch".into(),
                        message: format!(
                            "Stopping protected daemon '{unit}' without ExpertReview approval tier"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::FileRead { path } => {
                if path.contains("..") {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Warning,
                        category: "path_hygiene".into(),
                        message: format!("Path traversal '..' in read path '{path}'"),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::KernelParamSet { key, value, .. } => {
                // Key syntax check
                let is_valid_key = !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
                    && key.contains('.')
                    && !key.starts_with('.')
                    && !key.ends_with('.');
                if !is_valid_key {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "syntax_validation".into(),
                        message: format!("Malformed kernel parameter key format '{key}'"),
                        step_id: Some(step.step_id.clone()),
                    });
                }

                // Prohibited security-critical keys
                if PROHIBITED_KERNEL_PARAMS
                    .iter()
                    .any(|p| key == *p || key.starts_with(&format!("{p}.")))
                {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "prohibited_security_parameter".into(),
                        message: format!(
                            "Kernel parameter '{key}' is on the prohibited security-critical denylist"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }

                // Arbitrary executable / pipe redirection in core_pattern
                if key == "kernel.core_pattern" && (value.starts_with('|') || value.contains('/')) {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "privilege_escalation_risk".into(),
                        message: "Redirecting kernel.core_pattern to executable pipe or path is strictly forbidden".into(),
                        step_id: Some(step.step_id.clone()),
                    });
                }

                // Sanity check for swappiness
                if key == "vm.swappiness"
                    && let Ok(swap_val) = value.parse::<u32>()
                    && swap_val > 200
                {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Warning,
                        category: "sanity_check".into(),
                        message: format!(
                            "vm.swappiness value '{value}' exceeds standard maximum (200)"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::KernelModuleLoad { module, .. } => {
                if !ALLOWED_KERNEL_MODULES.contains(&module.as_str()) {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "unauthorized_kernel_module".into(),
                        message: format!(
                            "Module '{module}' is not in the approved kernel module allowlist"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::KernelModuleUnload { module, .. } => {
                if !ALLOWED_KERNEL_MODULES.contains(&module.as_str()) {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Fatal,
                        category: "unauthorized_kernel_module".into(),
                        message: format!(
                            "Unloading unapproved kernel module '{module}' is disallowed"
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
            CapabilityKind::ServiceStatus { .. }
            | CapabilityKind::Ping
            | CapabilityKind::BootSlotPin { .. }
            | CapabilityKind::BootcRollback => {}
        }
    }

    fn verify_rollback_coverage(&self, plan: &Plan, findings: &mut Vec<ReviewFinding>) {
        for step in &plan.steps {
            let is_mutating = matches!(
                step.capability,
                CapabilityKind::FileWrite { .. }
                    | CapabilityKind::FileDelete { .. }
                    | CapabilityKind::PackageInstall { .. }
                    | CapabilityKind::PackageRemove { .. }
                    | CapabilityKind::ServiceRestart { .. }
                    | CapabilityKind::ServiceStop { .. }
                    | CapabilityKind::KernelParamSet { .. }
                    | CapabilityKind::KernelModuleLoad { .. }
                    | CapabilityKind::KernelModuleUnload { .. }
            );

            if is_mutating {
                let has_rollback = plan
                    .rollback_steps
                    .iter()
                    .any(|rb| rb.triggers.iter().any(|trig| trig == &step.step_id));

                if !has_rollback {
                    findings.push(ReviewFinding {
                        severity: FindingSeverity::Warning,
                        category: "rollback_coverage".into(),
                        message: format!(
                            "Mutating step '{}' ({}) has no registered compensating rollback step",
                            step.step_id, step.description
                        ),
                        step_id: Some(step.step_id.clone()),
                    });
                }
            }
        }
    }

    fn verify_step_ordering(&self, steps: &[PlanStep], findings: &mut Vec<ReviewFinding>) {
        // If package is installed after restart of that service, flag ordering error
        for (idx, step) in steps.iter().enumerate() {
            if let CapabilityKind::ServiceRestart { unit, .. } = &step.capability {
                let unit_base = unit.trim_end_matches(".service");
                for subsequent_step in &steps[idx + 1..] {
                    if let CapabilityKind::PackageInstall { package, .. } =
                        &subsequent_step.capability
                        && package == unit_base
                    {
                        findings.push(ReviewFinding {
                            severity: FindingSeverity::Fatal,
                            category: "ordering_anomaly".into(),
                            message: format!(
                                "Service '{}' restarted before package '{}' is installed",
                                unit, package
                            ),
                            step_id: Some(step.step_id.clone()),
                        });
                    }
                }
            }
        }
    }
}

/// Helper for protected-unit matching that normalises the SSH service naming gap
/// between Debian/Ubuntu (`ssh.service`) and Fedora (`sshd.service`), as well as
/// the `ssh`/`sshd` bare forms, so that all four forms match the same protected entry.
fn is_ssh_unit_match(protected: &[&str], unit: &str) -> bool {
    let u = unit.trim();
    let u_base = u.strip_suffix(".service").unwrap_or(u);
    let is_u_ssh = u_base == "ssh" || u_base == "sshd";
    protected.iter().any(|p| {
        let p_trim = p.trim();
        if p_trim == u {
            return true;
        }
        let p_base = p_trim.strip_suffix(".service").unwrap_or(p_trim);
        if p_base == u_base {
            return true;
        }
        let is_p_ssh = p_base == "ssh" || p_base == "sshd";
        is_p_ssh && is_u_ssh
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PolicyEngine;

    #[test]
    fn test_reviewer_approves_clean_plan() {
        let planner = crate::planner::Planner::new(crate::planner::PlannerConfig::default());
        let plan = planner
            .create_plan("install nginx on port 8080", None)
            .unwrap();

        let reviewer = Reviewer::new(ReviewerConfig::default());
        let report = reviewer.review_plan(&plan);

        assert_eq!(report.verdict, ReviewVerdict::Approved);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn test_reviewer_rejects_path_traversal() {
        let step = PlanStep {
            step_id: "step-1".into(),
            description: "Escaping path".into(),
            capability: CapabilityKind::FileWrite {
                path: "/etc/../root/.ssh/authorized_keys".into(),
                content: "ssh-rsa ...".into(),
                append: false,
            },
            blast_radius: PolicyEngine::evaluate_capability(
                &CapabilityKind::FileWrite {
                    path: "/etc/../root/.ssh/authorized_keys".into(),
                    content: "ssh-rsa ...".into(),
                    append: false,
                },
                None,
            ),
            timeout_secs: 30,
        };

        let plan = Plan {
            id: "plan-bad".into(),
            intent: "Exploit".into(),
            summary: "Bad plan".into(),
            steps: vec![step],
            rollback_steps: vec![],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(&[], None),
            created_at: "2026-08-26T00:00:00Z".into(),
            status: crate::plan::PlanStatus::Draft,
        };

        let reviewer = Reviewer::new(ReviewerConfig::default());
        let report = reviewer.review_plan(&plan);

        assert_eq!(report.verdict, ReviewVerdict::Rejected);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == "path_security")
        );
    }

    #[test]
    fn test_reviewer_rejects_glibc_removal() {
        let step = PlanStep {
            step_id: "step-1".into(),
            description: "Remove vital system glibc".into(),
            capability: CapabilityKind::PackageRemove {
                package: "glibc".into(),
                approved: true,
            },
            blast_radius: PolicyEngine::evaluate_capability(
                &CapabilityKind::PackageRemove {
                    package: "glibc".into(),
                    approved: true,
                },
                None,
            ),
            timeout_secs: 30,
        };

        let plan = Plan {
            id: "plan-destroy".into(),
            intent: "Remove glibc".into(),
            summary: "Destructive plan".into(),
            steps: vec![step],
            rollback_steps: vec![],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(&[], None),
            created_at: "2026-08-26T00:00:00Z".into(),
            status: crate::plan::PlanStatus::Draft,
        };

        let reviewer = Reviewer::new(ReviewerConfig::default());
        let report = reviewer.review_plan(&plan);

        assert_eq!(report.verdict, ReviewVerdict::Rejected);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == "destructive_action")
        );
    }

    #[test]
    fn test_reviewer_rejects_protected_service_stop_without_expert_review() {
        let mut rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::ServiceStop {
                unit: "ssh.service".into(),
                approved: true,
            },
            None,
        );
        // Force tier to Confirm to simulate policy calibration mismatch
        rep.tier = ApprovalTier::Confirm;

        let step = PlanStep {
            step_id: "step-1".into(),
            description: "Stop SSH daemon".into(),
            capability: CapabilityKind::ServiceStop {
                unit: "ssh.service".into(),
                approved: true,
            },
            blast_radius: rep,
            timeout_secs: 30,
        };

        let plan = Plan {
            id: "plan-stop-ssh".into(),
            intent: "Stop ssh".into(),
            summary: "Dangerous stop plan".into(),
            steps: vec![step],
            rollback_steps: vec![],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(&[], None),
            created_at: "2026-08-26T00:00:00Z".into(),
            status: crate::plan::PlanStatus::Draft,
        };

        let reviewer = Reviewer::new(ReviewerConfig::default());
        let report = reviewer.review_plan(&plan);

        assert_eq!(report.verdict, ReviewVerdict::Rejected);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.category == "policy_mismatch")
        );
    }

    #[test]
    fn test_reviewer_warns_on_high_cross_crate_dispersion() {
        let steps = vec![
            PlanStep {
                step_id: "step-1".into(),
                description: "Patch crate-a".into(),
                capability: CapabilityKind::FileWrite {
                    path: "/workspace/crate-a/src/lib.rs".into(),
                    content: "a".into(),
                    append: false,
                },
                blast_radius: PolicyEngine::evaluate_capability(
                    &CapabilityKind::FileWrite {
                        path: "/workspace/crate-a/src/lib.rs".into(),
                        content: "a".into(),
                        append: false,
                    },
                    None,
                ),
                timeout_secs: 5,
            },
            PlanStep {
                step_id: "step-2".into(),
                description: "Patch crate-b".into(),
                capability: CapabilityKind::FileWrite {
                    path: "/workspace/crate-b/src/lib.rs".into(),
                    content: "b".into(),
                    append: false,
                },
                blast_radius: PolicyEngine::evaluate_capability(
                    &CapabilityKind::FileWrite {
                        path: "/workspace/crate-b/src/lib.rs".into(),
                        content: "b".into(),
                        append: false,
                    },
                    None,
                ),
                timeout_secs: 5,
            },
            PlanStep {
                step_id: "step-3".into(),
                description: "Patch crate-c".into(),
                capability: CapabilityKind::FileWrite {
                    path: "/workspace/crate-c/src/lib.rs".into(),
                    content: "c".into(),
                    append: false,
                },
                blast_radius: PolicyEngine::evaluate_capability(
                    &CapabilityKind::FileWrite {
                        path: "/workspace/crate-c/src/lib.rs".into(),
                        content: "c".into(),
                        append: false,
                    },
                    None,
                ),
                timeout_secs: 5,
            },
        ];

        let plan = Plan {
            id: "plan-cross-dispersion".into(),
            intent: "Cross-crate patch".into(),
            summary: "Plan touching 3 crates".into(),
            steps,
            rollback_steps: vec![],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(
                &[
                    CapabilityKind::FileWrite {
                        path: "/workspace/crate-a/src/lib.rs".into(),
                        content: "a".into(),
                        append: false,
                    },
                    CapabilityKind::FileWrite {
                        path: "/workspace/crate-b/src/lib.rs".into(),
                        content: "b".into(),
                        append: false,
                    },
                    CapabilityKind::FileWrite {
                        path: "/workspace/crate-c/src/lib.rs".into(),
                        content: "c".into(),
                        append: false,
                    },
                ],
                None,
            ),
            created_at: "2026-08-28T00:00:00Z".into(),
            status: crate::plan::PlanStatus::Draft,
        };

        let reviewer = Reviewer::new(ReviewerConfig::default());
        let report = reviewer.review_plan(&plan);

        // Warning finding emitted on 3-crate dispersion
        assert_eq!(report.verdict, ReviewVerdict::ApprovedWithWarnings);
        let dispersion_finding = report
            .findings
            .iter()
            .find(|f| f.category == "cross_crate_dispersion");
        assert!(dispersion_finding.is_some());
        assert_eq!(
            dispersion_finding.unwrap().severity,
            FindingSeverity::Warning
        );
        assert!(
            dispersion_finding
                .unwrap()
                .message
                .contains("3 distinct crates")
        );
    }

    #[test]
    fn test_reviewer_flags_protected_unit_variants() {
        let reviewer = Reviewer::new(ReviewerConfig::default());
        let test_units = [
            "ssh.service",
            "ssh",
            "sshd.service",
            "sshd",
            "systemd-udevd.service",
            "systemd-udevd",
            "dbus.service",
            "dbus",
            "firewalld.service",
            "firewalld",
        ];

        for unit in test_units {
            // Build a plan with low approval tier (e.g. Notify) to assert rejection
            let mut fake_blast = PolicyEngine::evaluate_capability(
                &CapabilityKind::ServiceRestart {
                    unit: unit.into(),
                    approved: true,
                },
                None,
            );
            fake_blast.tier = ApprovalTier::Notify;

            let plan = Plan {
                id: "plan-prot".into(),
                intent: "restart protected unit".into(),
                summary: "restart".into(),
                steps: vec![PlanStep {
                    step_id: "step-1".into(),
                    description: format!("restart {unit}"),
                    capability: CapabilityKind::ServiceRestart {
                        unit: unit.into(),
                        approved: true,
                    },
                    blast_radius: fake_blast,
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
                status: crate::plan::PlanStatus::Draft,
            };

            let report = reviewer.review_plan(&plan);
            assert_eq!(
                report.verdict,
                ReviewVerdict::Rejected,
                "Reviewer should reject restart of protected unit '{unit}' when tier is Notify"
            );
            assert!(
                report
                    .findings
                    .iter()
                    .any(|f| f.category == "policy_mismatch" || f.category == "critical_unit"),
                "Finding should be policy_mismatch or critical_unit for '{unit}'"
            );
        }
    }
}
