//! Directioner-OS Tier 2 Self-Healing & Remediation Engine (Phase 3.0.4).
//!
//! Orchestrates Tier 2 active remediation through the multi-agent control plane:
//! 1. Planner captures pre-existing system/kernel state (e.g. from `/proc/sys/`) and synthesizes forward + rollback steps.
//! 2. PolicyEngine calculates blast-radius and applies the strict conservative floor (no action below `Notify`).
//! 3. Reviewer executes independent invariant checks (syntax, allowlist, prohibited key denylist, rollback coverage).
//! 4. Implementer executes the plan through the Capability Daemon with atomic rollback if verification fails.

use std::path::PathBuf;

use crate::plan::{Plan, PlanStep, RollbackStep};
use crate::policy::PolicyEngine;
use crate::reviewer::{FindingSeverity, ReviewVerdict, Reviewer, ReviewerConfig};
use capability_layer::CapabilityKind;

use std::fmt;

/// Remediation errors.
#[derive(Debug)]
pub enum RemediationError {
    /// Synthesis or parameter error.
    SynthesisError(String),
    /// Plan was rejected by the independent Reviewer agent.
    ReviewRejected(String),
    /// Execution error.
    ExecutionError(String),
}

impl fmt::Display for RemediationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemediationError::SynthesisError(msg) => {
                write!(f, "failed to synthesize remediation plan: {msg}")
            }
            RemediationError::ReviewRejected(msg) => {
                write!(f, "remediation plan rejected by reviewer: {msg}")
            }
            RemediationError::ExecutionError(msg) => {
                write!(f, "remediation execution failed: {msg}")
            }
        }
    }
}

impl std::error::Error for RemediationError {}

/// Configuration for Tier 2 Remediation Engine.
#[derive(Debug, Clone)]
pub struct RemediationConfig {
    /// Work root or procfs mount path for reading current state.
    pub proc_root: PathBuf,
    /// Maximum allowed blast radius score.
    pub max_blast_radius: u32,
}

impl Default for RemediationConfig {
    fn default() -> Self {
        Self {
            proc_root: PathBuf::from("/proc/sys"),
            max_blast_radius: 80,
        }
    }
}

/// Tier 2 Self-Healing & Remediation Agent (Phase 3.0.4).
pub struct RemediationAgent {
    config: RemediationConfig,
    reviewer: Reviewer,
}

impl RemediationAgent {
    /// Create a new remediation agent with specified configuration.
    pub fn new(config: RemediationConfig) -> Self {
        Self {
            config,
            reviewer: Reviewer::new(ReviewerConfig::default()),
        }
    }

    /// Synthesize an auditable remediation plan for tuning a kernel sysctl parameter.
    pub fn plan_kernel_param_tuning(
        &self,
        key: &str,
        target_value: &str,
        persistent: bool,
        conn: Option<&rusqlite::Connection>,
    ) -> Result<Plan, RemediationError> {
        // Query current value from /proc/sys
        let proc_file = self.config.proc_root.join(key.replace('.', "/"));
        let current_value = if proc_file.exists() {
            std::fs::read_to_string(&proc_file)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "0".to_string())
        } else {
            "0".to_string()
        };

        let step_id = format!("tune_{}", key.replace('.', "_"));
        let cap_forward = CapabilityKind::KernelParamSet {
            key: key.to_string(),
            value: target_value.to_string(),
            persistent,
            approved: false,
        };
        let cap_rollback = CapabilityKind::KernelParamSet {
            key: key.to_string(),
            value: current_value,
            persistent,
            approved: true,
        };

        let blast = PolicyEngine::evaluate_capability(&cap_forward, conn);

        let forward_step = PlanStep {
            step_id: step_id.clone(),
            description: format!("Tune kernel parameter '{key}' to '{target_value}'"),
            capability: cap_forward.clone(),
            blast_radius: blast.clone(),
            timeout_secs: 30,
        };

        let rollback_step = RollbackStep {
            step_id: format!("rollback_{step_id}"),
            description: format!("Revert kernel parameter '{key}' to previous value"),
            capability: cap_rollback,
            triggers: vec![step_id.clone()],
        };

        let plan = Plan {
            id: format!("plan_tune_{}", key.replace('.', "_")),
            intent: format!("Tier 2 kernel parameter remediation for {key}"),
            summary: format!("Adjust {key} to {target_value} (persistent={persistent})"),
            steps: vec![forward_step],
            rollback_steps: vec![rollback_step],
            aggregate_blast_radius: blast,
            created_at: chrono::Utc::now().to_rfc3339(),
            status: crate::plan::PlanStatus::Draft,
        };

        let review = self.reviewer.review_plan(&plan);
        if review.verdict == ReviewVerdict::Rejected {
            let fatal_messages: Vec<String> = review
                .findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::Fatal)
                .map(|f| f.message.clone())
                .collect();
            return Err(RemediationError::ReviewRejected(fatal_messages.join("; ")));
        }

        Ok(plan)
    }

    /// Synthesize an auditable remediation plan for dynamically loading an authorized kernel module.
    pub fn plan_module_load(
        &self,
        module: &str,
        args: Vec<String>,
        conn: Option<&rusqlite::Connection>,
    ) -> Result<Plan, RemediationError> {
        let step_id = format!("load_mod_{module}");
        let cap_forward = CapabilityKind::KernelModuleLoad {
            module: module.to_string(),
            args: args.clone(),
            approved: false,
        };
        let cap_rollback = CapabilityKind::KernelModuleUnload {
            module: module.to_string(),
            approved: true,
        };

        let blast = PolicyEngine::evaluate_capability(&cap_forward, conn);

        let forward_step = PlanStep {
            step_id: step_id.clone(),
            description: format!("Load kernel module '{module}' with args: {:?}", args),
            capability: cap_forward.clone(),
            blast_radius: blast.clone(),
            timeout_secs: 30,
        };

        let rollback_step = RollbackStep {
            step_id: format!("rollback_{step_id}"),
            description: format!("Unload dynamically loaded kernel module '{module}'"),
            capability: cap_rollback,
            triggers: vec![step_id.clone()],
        };

        let plan = Plan {
            id: format!("plan_load_mod_{module}"),
            intent: format!("Tier 2 kernel module self-healing for {module}"),
            summary: format!("Load module {module} to resolve missing subsystem dependency"),
            steps: vec![forward_step],
            rollback_steps: vec![rollback_step],
            aggregate_blast_radius: blast,
            created_at: chrono::Utc::now().to_rfc3339(),
            status: crate::plan::PlanStatus::Draft,
        };

        let review = self.reviewer.review_plan(&plan);
        if review.verdict == ReviewVerdict::Rejected {
            let fatal_messages: Vec<String> = review
                .findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::Fatal)
                .map(|f| f.message.clone())
                .collect();
            return Err(RemediationError::ReviewRejected(fatal_messages.join("; ")));
        }

        Ok(plan)
    }

    /// Synthesize an auditable remediation plan for restarting a failed or crashlooping systemd service.
    pub fn plan_service_restart(
        &self,
        unit: &str,
        conn: Option<&rusqlite::Connection>,
    ) -> Result<Plan, RemediationError> {
        let step_id = format!("restart_{}", unit.replace('.', "_"));
        let cap_forward = CapabilityKind::ServiceRestart {
            unit: unit.to_string(),
            approved: false,
        };
        let cap_rollback = CapabilityKind::ServiceStatus {
            unit: unit.to_string(),
        };

        let blast = PolicyEngine::evaluate_capability(&cap_forward, conn);

        let forward_step = PlanStep {
            step_id: step_id.clone(),
            description: format!("Restart systemd service '{unit}' to recover from failed state"),
            capability: cap_forward.clone(),
            blast_radius: blast.clone(),
            timeout_secs: 30,
        };

        let rollback_step = RollbackStep {
            step_id: format!("rollback_{step_id}"),
            description: format!("Query service status for '{unit}' if restart fails"),
            capability: cap_rollback,
            triggers: vec![step_id.clone()],
        };

        let plan = Plan {
            id: format!("plan_restart_{}", unit.replace('.', "_")),
            intent: format!("Tier 2 service self-healing for {unit}"),
            summary: format!("Restart service {unit} to recover from crashloop or failure"),
            steps: vec![forward_step],
            rollback_steps: vec![rollback_step],
            aggregate_blast_radius: blast,
            created_at: chrono::Utc::now().to_rfc3339(),
            status: crate::plan::PlanStatus::Draft,
        };

        let review = self.reviewer.review_plan(&plan);
        if review.verdict == ReviewVerdict::Rejected {
            let fatal_messages: Vec<String> = review
                .findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::Fatal)
                .map(|f| f.message.clone())
                .collect();
            return Err(RemediationError::ReviewRejected(fatal_messages.join("; ")));
        }

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::ApprovalTier;
    use tempfile::tempdir;

    #[test]
    fn test_plan_kernel_param_tuning_creates_revertible_plan() {
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
            .expect("should synthesize plan");

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.rollback_steps.len(), 1);

        // Verify conservative floor: tier is at least Notify
        assert!(plan.aggregate_blast_radius.tier >= ApprovalTier::Notify);
        assert!(plan.aggregate_blast_radius.score >= 20);

        // Verify rollback step restores captured "0"
        match &plan.rollback_steps[0].capability {
            CapabilityKind::KernelParamSet { key, value, .. } => {
                assert_eq!(key, "net.ipv4.ip_forward");
                assert_eq!(value, "0");
            }
            _ => panic!("expected KernelParamSet rollback"),
        }
    }

    #[test]
    fn test_plan_prohibited_parameter_is_rejected_by_reviewer() {
        let dir = tempdir().unwrap();
        let agent = RemediationAgent::new(RemediationConfig {
            proc_root: dir.path().to_path_buf(),
            max_blast_radius: 80,
        });

        let err = agent
            .plan_kernel_param_tuning("kernel.core_pattern", "|/tmp/bad", false, None)
            .expect_err("should reject prohibited parameter");

        match err {
            RemediationError::ReviewRejected(msg) => {
                assert!(
                    msg.contains("prohibited security-critical denylist")
                        || msg.contains("forbidden")
                );
            }
            _ => panic!("expected ReviewRejected error"),
        }
    }

    #[test]
    fn test_plan_unauthorized_module_is_rejected_by_reviewer() {
        let dir = tempdir().unwrap();
        let agent = RemediationAgent::new(RemediationConfig {
            proc_root: dir.path().to_path_buf(),
            max_blast_radius: 80,
        });

        let err = agent
            .plan_module_load("evil_module", vec![], None)
            .expect_err("should reject unlisted module");

        match err {
            RemediationError::ReviewRejected(msg) => {
                assert!(msg.contains("not in the approved kernel module allowlist"));
            }
            _ => panic!("expected ReviewRejected error"),
        }
    }
}
