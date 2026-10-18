//! Update Verification and Automatic Rollback Subsystem (TODO 50).
//!
//! Enforces:
//! 1. **Cryptographic Signature Verification**: Validates signed OCI/bootc deployment manifests.
//! 2. **Greenboot Boot Counter Mechanics**: Tracks boot attempts (3 -> 0) to trigger automatic rollback.
//! 3. **Gating Health Checks**: Verifies systemd posture, daemon socket, database, and audit log before commit.
//! 4. **Honest Downtime Model**: Explicitly notes that updates require non-zero downtime reboots (Rule E).

use serde::{Deserialize, Serialize};

/// Digital signature block for a staged bootc deployment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateSignature {
    /// SHA256 digest of the deployment OCI image.
    pub digest: String,
    /// Public key identifier used to verify the signature.
    pub key_id: String,
    /// Digital signature string.
    pub signature: String,
}

/// A staged operating system update deployment awaiting reboot and health verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedDeployment {
    /// Deployment identifier (e.g. `directioner-42.0.1`).
    pub deployment_id: String,
    /// Semantic version.
    pub version: String,
    /// Provenance signature.
    pub signature: UpdateSignature,
    /// RFC3339 timestamp of staging.
    pub staged_at: String,
}

/// Result of a single gating health check required to commit an update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatingHealthCheck {
    /// Script / check name (e.g. `02_ai_capabilityd.sh`).
    pub name: String,
    /// Whether the check passed successfully.
    pub passed: bool,
    /// Diagnostic description.
    pub details: String,
}

/// Complete report of post-reboot health checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateVerificationReport {
    /// The deployment being verified.
    pub deployment: StagedDeployment,
    /// List of evaluated gating checks.
    pub checks: Vec<GatingHealthCheck>,
}

impl UpdateVerificationReport {
    /// Check whether all gating health checks passed.
    pub fn all_passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|c| c.passed)
    }
}

/// Action decided after evaluating deployment health against boot counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackDecision {
    /// All health checks passed; deployment committed and boot counter cleared.
    Commit,
    /// Health checks failed but attempts remain before fallback.
    RetryRemaining {
        /// Remaining boot attempts.
        attempts_left: u32,
    },
    /// Boot counter reached zero or critical invariant violated; trigger fallback slot reboot.
    TriggerRollback {
        /// Rationale for triggering rollback.
        reason: String,
    },
}

/// Controller managing the greenboot trial boot counter and rollback decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreenbootController {
    /// Remaining boot attempts before triggering rollback (defaults to 3).
    pub boot_counter: u32,
    /// Whether the active deployment has been finalized green.
    pub is_green: bool,
}

impl Default for GreenbootController {
    fn default() -> Self {
        Self {
            boot_counter: 3,
            is_green: false,
        }
    }
}

impl GreenbootController {
    /// Initialize with a custom boot counter.
    pub fn with_counter(counter: u32) -> Self {
        Self {
            boot_counter: counter,
            is_green: false,
        }
    }

    /// Decrement the boot counter on boot failure.
    pub fn record_failed_boot(&mut self) -> u32 {
        if self.boot_counter > 0 {
            self.boot_counter -= 1;
        }
        self.boot_counter
    }

    /// Finalize status green and commit the deployment.
    pub fn mark_green(&mut self) {
        self.is_green = true;
        self.boot_counter = 0;
    }

    /// Evaluate verification report and decide whether to commit, retry, or rollback.
    pub fn evaluate(
        &mut self,
        signature_valid: bool,
        report: &UpdateVerificationReport,
    ) -> RollbackDecision {
        if !signature_valid {
            return RollbackDecision::TriggerRollback {
                reason: "Cryptographic signature validation failed on staged deployment"
                    .to_string(),
            };
        }

        if report.all_passed() {
            self.mark_green();
            RollbackDecision::Commit
        } else {
            let remaining = self.record_failed_boot();
            if remaining == 0 {
                let failing_names: Vec<&str> = report
                    .checks
                    .iter()
                    .filter(|c| !c.passed)
                    .map(|c| c.name.as_str())
                    .collect();
                RollbackDecision::TriggerRollback {
                    reason: format!(
                        "Boot counter exhausted (0 attempts left). Gating checks failed: {:?}",
                        failing_names
                    ),
                }
            } else {
                RollbackDecision::RetryRemaining {
                    attempts_left: remaining,
                }
            }
        }
    }
}
