//! Implementer Role (Phase 1.2).
//!
//! Executes approved plans step-by-step strictly through the Capability Layer
//! client (`ai-capabilityd`), monitoring progress and executing compensating
//! rollback actions if any forward step fails.

use std::time::Instant;

use capability_layer::CapabilityResponse;

use crate::client::CapabilityClient;
use crate::plan::{
    Plan, PlanExecutionReport, PlanStatus, PlanStep, RollbackStep, StepExecutionResult,
};

/// Implementer executor for running structured plans.
pub struct Implementer<'a> {
    client: &'a CapabilityClient,
}

impl<'a> Implementer<'a> {
    /// Create a new Implementer bound to a capability client.
    pub fn new(client: &'a CapabilityClient) -> Self {
        Self { client }
    }

    /// Execute an approved plan to completion or trigger rollback upon failure.
    pub fn execute_plan(&self, plan: &Plan) -> PlanExecutionReport {
        let total_start = Instant::now();
        let mut step_results = Vec::new();
        let mut failed_step_id: Option<String> = None;

        // Step execution loop
        for step in &plan.steps {
            let res = self.execute_step(step, &plan.id);
            let success = res.success;
            step_results.push(res);

            if !success {
                failed_step_id = Some(step.step_id.clone());
                break;
            }
        }

        // Check if any step failed
        if let Some(failed_id) = failed_step_id {
            // Execute rollback steps that are triggered by the failed step or prior steps
            let rollback_results =
                self.execute_rollback(&plan.rollback_steps, &plan.id, &failed_id);
            let total_duration_ms = total_start.elapsed().as_millis() as u64;

            let any_rollback_failed = rollback_results.iter().any(|r| !r.success);
            let (final_status, message) = if any_rollback_failed {
                (
                    PlanStatus::RollbackFailed,
                    format!(
                        "CRITICAL: Plan aborted at step '{failed_id}', and one or more compensating rollback steps FAILED. Manual remediation required."
                    ),
                )
            } else {
                (
                    PlanStatus::RolledBack,
                    format!(
                        "Plan aborted at step '{failed_id}'; all compensating rollback steps executed successfully."
                    ),
                )
            };

            return PlanExecutionReport {
                plan_id: plan.id.clone(),
                final_status,
                step_results,
                rollback_results,
                total_duration_ms,
                message,
            };
        }

        let total_duration_ms = total_start.elapsed().as_millis() as u64;
        PlanExecutionReport {
            plan_id: plan.id.clone(),
            final_status: PlanStatus::Completed,
            step_results,
            rollback_results: Vec::new(),
            total_duration_ms,
            message: "Plan executed and completed successfully.".into(),
        }
    }

    /// Execute a single forward step.
    fn execute_step(&self, step: &PlanStep, plan_id: &str) -> StepExecutionResult {
        let start = Instant::now();
        let correlation = format!("{}:{}", plan_id, step.step_id);

        let outcome = self.client.send(step.capability.clone(), correlation, None);
        let duration_ms = start.elapsed().as_millis() as u64;

        match outcome {
            Ok(CapabilityResponse::Success {
                result, audit_seq, ..
            }) => StepExecutionResult {
                step_id: step.step_id.clone(),
                success: true,
                details: result,
                audit_seq,
                duration_ms,
            },
            Ok(CapabilityResponse::Denied {
                reason, audit_seq, ..
            }) => StepExecutionResult {
                step_id: step.step_id.clone(),
                success: false,
                details: serde_json::json!({ "denied": reason }),
                audit_seq,
                duration_ms,
            },
            Ok(CapabilityResponse::Error {
                message, audit_seq, ..
            }) => StepExecutionResult {
                step_id: step.step_id.clone(),
                success: false,
                details: serde_json::json!({ "error": message }),
                audit_seq,
                duration_ms,
            },
            Err(e) => StepExecutionResult {
                step_id: step.step_id.clone(),
                success: false,
                details: serde_json::json!({ "client_error": e.to_string() }),
                audit_seq: 0,
                duration_ms,
            },
        }
    }

    /// Execute matching rollback steps in reverse order.
    fn execute_rollback(
        &self,
        rollbacks: &[RollbackStep],
        plan_id: &str,
        failed_step_id: &str,
    ) -> Vec<StepExecutionResult> {
        let mut results = Vec::new();

        for rb in rollbacks.iter().rev() {
            // Trigger if rollback specifies trigger or is universal
            let applies = rb.triggers.is_empty() || rb.triggers.iter().any(|t| t == failed_step_id);
            if !applies {
                continue;
            }

            let start = Instant::now();
            let correlation = format!("{}:{}", plan_id, rb.step_id);

            let outcome = self
                .client
                .send_rollback(rb.capability.clone(), correlation, None);
            let duration_ms = start.elapsed().as_millis() as u64;

            let res = match outcome {
                Ok(CapabilityResponse::Success {
                    result, audit_seq, ..
                }) => StepExecutionResult {
                    step_id: rb.step_id.clone(),
                    success: true,
                    details: result,
                    audit_seq,
                    duration_ms,
                },
                Ok(CapabilityResponse::Denied {
                    reason, audit_seq, ..
                }) => StepExecutionResult {
                    step_id: rb.step_id.clone(),
                    success: false,
                    details: serde_json::json!({ "denied": reason }),
                    audit_seq,
                    duration_ms,
                },
                Ok(CapabilityResponse::Error {
                    message, audit_seq, ..
                }) => StepExecutionResult {
                    step_id: rb.step_id.clone(),
                    success: false,
                    details: serde_json::json!({ "error": message }),
                    audit_seq,
                    duration_ms,
                },
                Err(e) => StepExecutionResult {
                    step_id: rb.step_id.clone(),
                    success: false,
                    details: serde_json::json!({ "client_error": e.to_string() }),
                    audit_seq: 0,
                    duration_ms,
                },
            };

            results.push(res);
        }

        results
    }
}
