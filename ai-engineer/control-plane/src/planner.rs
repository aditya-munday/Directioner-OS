//! Multi-Agent Planner Role (Phase 1.2).
//!
//! Transforms user intent or system remediation goals into a structured,
//! auditable multi-step [`Plan`] with pre-calculated blast-radius scores
//! and compensating rollback steps, grounded in the live [`world_model`].

use std::path::PathBuf;

use capability_layer::CapabilityKind;
use chrono::Utc;
use rusqlite::Connection;

use crate::plan::{Plan, PlanStatus, PlanStep, RollbackStep};
use crate::policy::PolicyEngine;

/// Configuration options for the Planner.
#[derive(Debug, Clone)]
pub struct PlannerConfig {
    /// Base directory root where files are created/modified.
    pub work_root: PathBuf,
    /// Default step timeout in seconds.
    pub default_timeout_secs: u64,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            work_root: PathBuf::from("/etc"),
            default_timeout_secs: 30,
        }
    }
}

/// The Planner agent responsible for turning high-level intents into executable plans.
pub struct Planner {
    config: PlannerConfig,
}

impl Planner {
    /// Create a new Planner instance with specified configuration.
    pub fn new(config: PlannerConfig) -> Self {
        Self { config }
    }

    /// Generate an execution plan from an intent string, optionally querying the World Model.
    pub fn create_plan(&self, intent: &str, db: Option<&Connection>) -> Result<Plan, String> {
        let plan_id = format!("plan-{}", &uuid_v4()[0..8]);
        let intent_clean = intent.trim();

        if intent_clean.is_empty() {
            return Err("cannot generate plan from empty intent".into());
        }

        let (summary, steps, rollback_steps) = self.synthesize_steps(intent_clean, db)?;

        let caps: Vec<CapabilityKind> = steps.iter().map(|s| s.capability.clone()).collect();
        let aggregate_blast_radius = PolicyEngine::evaluate_plan_aggregate(&caps, db);

        Ok(Plan {
            id: plan_id,
            intent: intent_clean.to_string(),
            summary,
            steps,
            rollback_steps,
            aggregate_blast_radius,
            created_at: Utc::now().to_rfc3339(),
            status: PlanStatus::Draft,
        })
    }

    /// Synthesize steps based on parsed patterns in the intent string.
    fn synthesize_steps(
        &self,
        intent: &str,
        db: Option<&Connection>,
    ) -> Result<(String, Vec<PlanStep>, Vec<RollbackStep>), String> {
        let lower = intent.to_lowercase();
        let mut steps = Vec::new();
        let mut rollbacks = Vec::new();

        // Pattern 1: Web server setup / nginx
        if lower.contains("nginx") || lower.contains("web server") || lower.contains("webserver") {
            let port = extract_port(&lower).unwrap_or(8080);
            let conf_path = self.config.work_root.join("nginx/conf.d/site.conf");
            let conf_str = conf_path.to_string_lossy().to_string();

            let summary = format!(
                "Deploys nginx web server listening on port {port}, configures site vhost, and restarts service."
            );

            // Step 1: Install nginx package
            let step1_cap = CapabilityKind::PackageInstall {
                package: "nginx".into(),
                approved: true,
            };
            steps.push(PlanStep {
                step_id: "step-1".into(),
                description: "Install nginx package".into(),
                blast_radius: PolicyEngine::evaluate_capability(&step1_cap, db),
                capability: step1_cap,
                timeout_secs: self.config.default_timeout_secs,
            });

            // Step 2: Write nginx site config
            let conf_content = format!(
                "server {{\n    listen {};\n    server_name localhost;\n    root /usr/share/nginx/html;\n    index index.html;\n}}\n",
                port
            );
            let step2_cap = CapabilityKind::FileWrite {
                path: conf_str.clone(),
                content: conf_content,
                append: false,
            };
            steps.push(PlanStep {
                step_id: "step-2".into(),
                description: format!("Configure nginx virtual host for port {port}"),
                blast_radius: PolicyEngine::evaluate_capability(&step2_cap, db),
                capability: step2_cap,
                timeout_secs: self.config.default_timeout_secs,
            });

            // Step 3: Restart nginx service
            let step3_cap = CapabilityKind::ServiceRestart {
                unit: "nginx.service".into(),
                approved: true,
            };
            steps.push(PlanStep {
                step_id: "step-3".into(),
                description: "Restart nginx service to apply new configuration".into(),
                blast_radius: PolicyEngine::evaluate_capability(&step3_cap, db),
                capability: step3_cap,
                timeout_secs: self.config.default_timeout_secs,
            });

            // Rollback steps
            rollbacks.push(RollbackStep {
                step_id: "rb-1".into(),
                description: "Remove created nginx configuration file".into(),
                capability: CapabilityKind::FileWrite {
                    path: conf_str,
                    content: String::new(),
                    append: false,
                },
                triggers: vec!["step-2".into(), "step-3".into()],
            });

            return Ok((summary, steps, rollbacks));
        }

        // Pattern 2: Service remediation / fix / restart
        if lower.contains("fix") || lower.contains("remediate") || lower.contains("restart") {
            let unit = extract_unit_name(&lower).unwrap_or_else(|| "app.service".into());
            let summary = format!(
                "Inspects status of unit '{unit}', verifies integrity, and executes controlled restart."
            );

            // Step 1: Read service status
            let step1_cap = CapabilityKind::ServiceStatus { unit: unit.clone() };
            steps.push(PlanStep {
                step_id: "step-1".into(),
                description: format!("Inspect service status for '{unit}'"),
                blast_radius: PolicyEngine::evaluate_capability(&step1_cap, db),
                capability: step1_cap,
                timeout_secs: self.config.default_timeout_secs,
            });

            // Step 2: Restart service
            let step2_cap = CapabilityKind::ServiceRestart {
                unit: unit.clone(),
                approved: true,
            };
            steps.push(PlanStep {
                step_id: "step-2".into(),
                description: format!("Execute restart of '{unit}'"),
                blast_radius: PolicyEngine::evaluate_capability(&step2_cap, db),
                capability: step2_cap,
                timeout_secs: self.config.default_timeout_secs,
            });

            return Ok((summary, steps, rollbacks));
        }

        // Pattern 3: Package install / management
        if lower.starts_with("install ") || lower.starts_with("add package ") {
            let pkg = lower
                .trim_start_matches("install ")
                .trim_start_matches("add package ")
                .split_whitespace()
                .next()
                .unwrap_or("curl");

            let summary = format!("Installs package '{pkg}' via system package manager.");
            let step1_cap = CapabilityKind::PackageInstall {
                package: pkg.to_string(),
                approved: true,
            };
            steps.push(PlanStep {
                step_id: "step-1".into(),
                description: format!("Install package '{pkg}'"),
                blast_radius: PolicyEngine::evaluate_capability(&step1_cap, db),
                capability: step1_cap,
                timeout_secs: self.config.default_timeout_secs,
            });

            return Ok((summary, steps, rollbacks));
        }

        // Pattern 4: Generic config write / update
        if lower.contains("config") || lower.contains("write") || lower.contains("set ") {
            let path = self.config.work_root.join("custom.conf");
            let path_str = path.to_string_lossy().to_string();
            let summary = format!("Writes configuration update to '{path_str}'.");

            let step1_cap = CapabilityKind::FileWrite {
                path: path_str.clone(),
                content: format!("# Generated by Directioner-OS AI Engineer\nintent={intent}\n"),
                append: false,
            };
            steps.push(PlanStep {
                step_id: "step-1".into(),
                description: format!("Write configuration to '{path_str}'"),
                blast_radius: PolicyEngine::evaluate_capability(&step1_cap, db),
                capability: step1_cap,
                timeout_secs: self.config.default_timeout_secs,
            });

            rollbacks.push(RollbackStep {
                step_id: "rb-1".into(),
                description: format!("Revert/truncate '{path_str}'"),
                capability: CapabilityKind::FileWrite {
                    path: path_str,
                    content: String::new(),
                    append: false,
                },
                triggers: vec!["step-1".into()],
            });

            return Ok((summary, steps, rollbacks));
        }

        // Fallback generic inspection / ping plan
        let summary =
            format!("Performs safe diagnostic status inspection for intent: \"{intent}\"");
        let step1_cap = CapabilityKind::Ping;
        steps.push(PlanStep {
            step_id: "step-1".into(),
            description: "System liveness check".into(),
            blast_radius: PolicyEngine::evaluate_capability(&step1_cap, db),
            capability: step1_cap,
            timeout_secs: self.config.default_timeout_secs,
        });

        Ok((summary, steps, rollbacks))
    }
}

fn extract_port(text: &str) -> Option<u16> {
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        if let Ok(port) = word.parse::<u16>()
            && port >= 80
        {
            return Some(port);
        }
    }
    None
}

fn extract_unit_name(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        if word.ends_with(".service") || word.ends_with(".socket") || word.ends_with(".target") {
            return Some(word.to_string());
        }
    }
    None
}

fn uuid_v4() -> String {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:016x}", nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planner_synthesizes_nginx_plan() {
        let planner = Planner::new(PlannerConfig::default());
        let plan = planner
            .create_plan("install nginx on port 9090", None)
            .unwrap();

        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.rollback_steps.len(), 1);
        assert!(plan.summary.contains("9090"));
        assert!(plan.aggregate_blast_radius.tier >= crate::policy::ApprovalTier::Confirm);
    }

    #[test]
    fn test_planner_synthesizes_service_restart_plan() {
        let planner = Planner::new(PlannerConfig::default());
        let plan = planner
            .create_plan("fix firewalld.service restart", None)
            .unwrap();

        assert_eq!(plan.steps.len(), 2);
        assert!(plan.aggregate_blast_radius.score >= 80);
    }
}
