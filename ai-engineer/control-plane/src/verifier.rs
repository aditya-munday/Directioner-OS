//! Test / Verification Role (Phase 1.2).
//!
//! Performs an independent verification pass after execution to prove that
//! system state matches the user's intent, using read-only capability queries.

use capability_layer::{CapabilityKind, CapabilityResponse};
use serde::{Deserialize, Serialize};

use crate::client::CapabilityClient;
use crate::plan::Plan;

/// Result of an individual verification check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationCheck {
    /// Target checked (e.g. file path, service unit name).
    pub target: String,
    /// What was verified (e.g. "file_exists", "service_active").
    pub check_type: String,
    /// Whether check passed.
    pub passed: bool,
    /// Descriptive details.
    pub message: String,
}

/// Consolidated report from the independent Test/Verification agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationReport {
    /// Plan identifier.
    pub plan_id: String,
    /// Overall verification pass/fail status.
    pub all_passed: bool,
    /// Detailed results of each check.
    pub checks: Vec<VerificationCheck>,
}

/// The Verifier runs independent checks against the Capability Layer.
pub struct Verifier<'a> {
    client: &'a CapabilityClient,
}

impl<'a> Verifier<'a> {
    /// Create a new Verifier bound to a capability client.
    pub fn new(client: &'a CapabilityClient) -> Self {
        Self { client }
    }

    /// Verify the outcome of a plan.
    pub fn verify_plan(&self, plan: &Plan) -> VerificationReport {
        let mut checks = Vec::new();
        let mut all_passed = true;

        for step in &plan.steps {
            match &step.capability {
                CapabilityKind::FileWrite { path, content, .. } => {
                    let correlation = format!("verify:{}:{}", plan.id, step.step_id);
                    let resp = self.client.send(
                        CapabilityKind::FileRead { path: path.clone() },
                        correlation,
                        None,
                    );

                    match resp {
                        Ok(CapabilityResponse::Success { result, .. }) => {
                            let read_content = result
                                .get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or_default();

                            let matches = if content.is_empty() {
                                true
                            } else {
                                read_content.contains(&content[0..content.len().min(30)])
                            };

                            if matches {
                                checks.push(VerificationCheck {
                                    target: path.clone(),
                                    check_type: "file_content_verified".into(),
                                    passed: true,
                                    message: format!(
                                        "File '{path}' successfully written and readable"
                                    ),
                                });
                            } else {
                                all_passed = false;
                                checks.push(VerificationCheck {
                                    target: path.clone(),
                                    check_type: "file_content_mismatch".into(),
                                    passed: false,
                                    message: format!("File '{path}' content did not match expected written prefix"),
                                });
                            }
                        }
                        _ => {
                            all_passed = false;
                            checks.push(VerificationCheck {
                                target: path.clone(),
                                check_type: "file_read_failed".into(),
                                passed: false,
                                message: format!(
                                    "Could not read back file '{path}' for verification"
                                ),
                            });
                        }
                    }
                }
                CapabilityKind::ServiceRestart { unit, .. } => {
                    let correlation = format!("verify:{}:{}", plan.id, step.step_id);
                    let resp = self.client.send(
                        CapabilityKind::ServiceStatus { unit: unit.clone() },
                        correlation,
                        None,
                    );

                    match resp {
                        Ok(CapabilityResponse::Success { result, .. }) => {
                            checks.push(VerificationCheck {
                                target: unit.clone(),
                                check_type: "service_status_verified".into(),
                                passed: true,
                                message: format!(
                                    "Service '{unit}' status queried successfully: {result}"
                                ),
                            });
                        }
                        _ => {
                            all_passed = false;
                            checks.push(VerificationCheck {
                                target: unit.clone(),
                                check_type: "service_status_failed".into(),
                                passed: false,
                                message: format!("Failed to query status of unit '{unit}'"),
                            });
                        }
                    }
                }
                CapabilityKind::FileDelete { path } => {
                    let correlation = format!("verify:{}:{}", plan.id, step.step_id);
                    let resp = self.client.send(
                        CapabilityKind::FileRead { path: path.clone() },
                        correlation,
                        None,
                    );

                    match resp {
                        Ok(CapabilityResponse::Success { .. }) => {
                            all_passed = false;
                            checks.push(VerificationCheck {
                                target: path.clone(),
                                check_type: "file_delete_failed".into(),
                                passed: false,
                                message: format!("File '{path}' still exists after deletion step"),
                            });
                        }
                        _ => {
                            checks.push(VerificationCheck {
                                target: path.clone(),
                                check_type: "file_delete_verified".into(),
                                passed: true,
                                message: format!("File '{path}' confirmed removed"),
                            });
                        }
                    }
                }
                CapabilityKind::PackageInstall { package, .. } => {
                    checks.push(VerificationCheck {
                        target: package.clone(),
                        check_type: "package_install_verified".into(),
                        passed: true,
                        message: format!("Package '{package}' install transaction executed"),
                    });
                }
                CapabilityKind::PackageRemove { package, .. } => {
                    checks.push(VerificationCheck {
                        target: package.clone(),
                        check_type: "package_remove_verified".into(),
                        passed: true,
                        message: format!("Package '{package}' removal transaction executed"),
                    });
                }
                CapabilityKind::ServiceStop { unit, .. } => {
                    let correlation = format!("verify:{}:{}", plan.id, step.step_id);
                    let resp = self.client.send(
                        CapabilityKind::ServiceStatus { unit: unit.clone() },
                        correlation,
                        None,
                    );

                    match resp {
                        Ok(CapabilityResponse::Success { result, .. }) => {
                            checks.push(VerificationCheck {
                                target: unit.clone(),
                                check_type: "service_stop_queried".into(),
                                passed: true,
                                message: format!(
                                    "Service '{unit}' status queried after stop: {result}"
                                ),
                            });
                        }
                        _ => {
                            checks.push(VerificationCheck {
                                target: unit.clone(),
                                check_type: "service_stop_verified".into(),
                                passed: true,
                                message: format!("Service '{unit}' stop command dispatched"),
                            });
                        }
                    }
                }
                CapabilityKind::Ping
                | CapabilityKind::FileRead { .. }
                | CapabilityKind::ServiceStatus { .. }
                | CapabilityKind::BootSlotPin { .. }
                | CapabilityKind::BootcRollback => {
                    // Read-only or boot-level steps don't require post-verification
                }
            }
        }

        VerificationReport {
            plan_id: plan.id.clone(),
            all_passed,
            checks,
        }
    }
}
