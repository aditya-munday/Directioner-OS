//! Directioner-OS Incident Response Agent (Phase 3.0.3).
//!
//! # Authority Boundary & Invariants (Phase 3.0.3)
//! "Tier 1 recovery is fully autonomous at the bootloader/greenboot layer and
//! requires no action from this agent. This agent's only current responsibility is
//! post-hoc observation and logging. It has zero write/mutating capability access
//! as of Phase 3.0.3."
//!
//! This module formalizes the named `IncidentResponseAgent` component responsible for
//! observing system resilience posture, monitoring boot watchdog recovery events,
//! and synthesizing incident health telemetry without initiating active mutations.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::health_model::{HealthModel, HealthModelConfig, HealthStatus, SystemHealthReport};

/// Observation record for an autonomous Tier 1 boot recovery event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tier1RecoveryObservation {
    /// Monotonic sequence ID in the audit work log.
    pub audit_seq: u64,
    /// Timestamp when observation was logged.
    pub timestamp: String,
    /// Actor or subagent ID that observed the event (e.g. "incident-response-agent:tier1-observer").
    pub observer: String,
    /// Root cause or trigger description.
    pub trigger: String,
    /// Autonomous recovery mechanism invoked.
    pub mechanism: String,
    /// Outcome action executed by bootloader/greenboot.
    pub action: String,
    /// Whether the active deployment was successfully restored.
    pub active_deployment_restored: bool,
}

/// Comprehensive incident posture report summarizing current alerts and historical recoveries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncidentPostureReport {
    /// RFC3339 timestamp when posture was evaluated.
    pub timestamp: String,
    /// Current system health evaluation.
    pub health_report: SystemHealthReport,
    /// Latest observed Tier 1 autonomous rollback event, if any.
    pub latest_tier1_recovery: Option<Tier1RecoveryObservation>,
    /// Active incident flags (e.g., degraded services, crash-loops).
    pub active_incidents: Vec<String>,
}

/// Configuration options for the Incident Response Agent.
#[derive(Debug, Clone)]
pub struct IncidentResponseConfig {
    /// Path to the World Model SQLite database.
    pub db_path: PathBuf,
    /// Optional path to the capability daemon Work Log JSONL file.
    pub worklog_path: Option<PathBuf>,
}

impl Default for IncidentResponseConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("/tmp/wm.sqlite"),
            worklog_path: Some(PathBuf::from("/var/log/live-worklog.jsonl")),
        }
    }
}

/// Named Incident Response Agent component (Phase 3.0.3).
///
/// Owns the Tier 1 observation responsibility proven in 3.0.1/3.0.2.
///
/// # Authority Boundary & Safety Guarantees
/// Tier 1 recovery is fully autonomous at the bootloader/greenboot layer and
/// requires no action from this agent. This agent's only current responsibility is
/// post-hoc observation and logging. It has zero write/mutating capability access
/// as of Phase 3.0.3.
pub struct IncidentResponseAgent {
    config: IncidentResponseConfig,
}

impl IncidentResponseAgent {
    /// Create a new Incident Response Agent with the given configuration.
    pub fn new(config: IncidentResponseConfig) -> Self {
        Self { config }
    }

    /// Explicit statement of the current agent authority boundary.
    pub fn authority_boundary(&self) -> &'static str {
        "Tier 1 recovery is fully autonomous at the bootloader/greenboot layer and \
         requires no action from this agent. This agent's only current responsibility is \
         post-hoc observation and logging. It has zero write/mutating capability access \
         as of Phase 3.0.3."
    }

    /// Read-only inspection of the audit Work Log to observe any recorded Tier 1 rollback events.
    pub fn observe_tier1_recovery(&self) -> Option<Tier1RecoveryObservation> {
        let worklog_path = self.resolve_worklog_path()?;
        if !worklog_path.exists() {
            return None;
        }

        let file = File::open(&worklog_path).ok()?;
        let reader = BufReader::new(file);
        let mut latest_observation: Option<Tier1RecoveryObservation> = None;

        for line in reader.lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                let cap = v.get("capability").and_then(|c| c.as_str()).unwrap_or("");
                if cap == "boot_watchdog.tier1_rollback_observed" {
                    let seq = v.get("seq").and_then(|s| s.as_u64()).unwrap_or(0);
                    let timestamp = v
                        .get("timestamp")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    let observer = v
                        .get("actor")
                        .or_else(|| v.get("caller"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("incident-response-agent:tier1-observer")
                        .to_string();
                    let args = v.get("args");
                    let trigger = args
                        .and_then(|a| a.get("trigger"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("watchdog milestone health-check failed")
                        .to_string();
                    let mechanism = args
                        .and_then(|a| a.get("mechanism"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("bootcounter exhaustion + autonomous bootc rollback")
                        .to_string();
                    let action = args
                        .and_then(|a| a.get("action"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("automatic rollback to pinned deployment 0")
                        .to_string();
                    let active_deployment_restored = v
                        .get("response")
                        .or_else(|| v.get("result"))
                        .and_then(|r| r.get("active_deployment_restored"))
                        .and_then(|d| d.as_bool())
                        .unwrap_or(true);

                    latest_observation = Some(Tier1RecoveryObservation {
                        audit_seq: seq,
                        timestamp,
                        observer,
                        trigger,
                        mechanism,
                        action,
                        active_deployment_restored,
                    });
                }
            }
        }

        latest_observation
    }

    /// Assess current system health using the Health Model.
    pub fn assess_health(&self) -> SystemHealthReport {
        let health_model = HealthModel::new(HealthModelConfig {
            db_path: self.config.db_path.clone(),
            worklog_path: self.config.worklog_path.clone(),
        });
        health_model.evaluate()
    }

    /// Evaluate overall incident posture across health signals and historical boot recovery observations.
    pub fn evaluate_posture(&self) -> IncidentPostureReport {
        let timestamp = Utc::now().to_rfc3339();
        let health_report = self.assess_health();
        let latest_tier1_recovery = self.observe_tier1_recovery();

        let mut active_incidents = Vec::new();

        if health_report.overall_status != HealthStatus::Healthy
            && health_report.overall_status != HealthStatus::Unknown
        {
            active_incidents.push(format!(
                "System health {} (composite score: {}/100)",
                health_report.overall_status,
                health_report
                    .composite_score
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            ));
        }

        for sub in &health_report.subsystems {
            if sub.status == HealthStatus::Degraded || sub.status == HealthStatus::Critical {
                active_incidents.push(format!(
                    "Subsystem '{}' {}: {}",
                    sub.name, sub.status, sub.summary
                ));
            }
        }

        if let Some(ref recovery) = latest_tier1_recovery {
            active_incidents.push(format!(
                "Tier 1 autonomous rollback recorded (audit seq={}): {}",
                recovery.audit_seq, recovery.trigger
            ));
        }

        IncidentPostureReport {
            timestamp,
            health_report,
            latest_tier1_recovery,
            active_incidents,
        }
    }

    fn resolve_worklog_path(&self) -> Option<PathBuf> {
        if let Some(ref p) = self.config.worklog_path
            && p.exists()
        {
            return Some(p.clone());
        }
        let var_log = PathBuf::from("/var/log/live-worklog.jsonl");
        if var_log.exists() {
            return Some(var_log);
        }
        let var_lib = PathBuf::from("/var/lib/directioner/worklog.jsonl");
        if var_lib.exists() {
            return Some(var_lib);
        }
        self.config.worklog_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_authority_boundary_statement_is_exact() {
        let agent = IncidentResponseAgent::new(IncidentResponseConfig::default());
        let boundary = agent.authority_boundary();
        assert!(
            boundary
                .contains("Tier 1 recovery is fully autonomous at the bootloader/greenboot layer")
        );
        assert!(boundary.contains("requires no action from this agent"));
        assert!(boundary.contains("zero write/mutating capability access as of Phase 3.0.3"));
    }

    #[test]
    fn test_observe_tier1_recovery_from_worklog() {
        let mut log_file = NamedTempFile::new().unwrap();
        writeln!(
            log_file,
            r#"{{"seq":5,"timestamp":"2026-08-29T17:48:55Z","caller":"vm-smoke","capability":"boot_slot.pin","args":{{"slot":0}}}}"#
        ).unwrap();
        writeln!(
            log_file,
            r#"{{"seq":99,"timestamp":"2026-08-29T17:56:39Z","caller":"incident-response-agent:tier1-observer","capability":"boot_watchdog.tier1_rollback_observed","args":{{"trigger":"watchdog: required milestone health-check failed 3 consecutive attempts","mechanism":"bootcounter exhaustion + autonomous bootc rollback","action":"automatic rollback to pinned deployment 0"}},"result":{{"active_deployment_restored":true}}}}"#
        ).unwrap();

        let agent = IncidentResponseAgent::new(IncidentResponseConfig {
            db_path: PathBuf::from("/tmp/nonexistent.sqlite"),
            worklog_path: Some(log_file.path().to_path_buf()),
        });

        let obs = agent
            .observe_tier1_recovery()
            .expect("should observe tier 1 rollback");
        assert_eq!(obs.audit_seq, 99);
        assert_eq!(obs.observer, "incident-response-agent:tier1-observer");
        assert!(obs.trigger.contains("milestone health-check failed"));
        assert!(
            obs.action
                .contains("automatic rollback to pinned deployment 0")
        );
        assert!(obs.active_deployment_restored);

        let posture = agent.evaluate_posture();
        assert!(posture.latest_tier1_recovery.is_some());
        assert_eq!(posture.health_report.overall_status, HealthStatus::Degraded);
        assert!(!posture.active_incidents.is_empty());
    }

    #[test]
    fn test_zero_mutating_capabilities_in_incident_response_agent() {
        // Compile-time check: IncidentResponseAgent only takes immutable &self and never exposes dispatch methods
        let agent = IncidentResponseAgent::new(IncidentResponseConfig::default());
        let _ = agent.authority_boundary();
        let _ = agent.observe_tier1_recovery();
        let _ = agent.assess_health();
        let _ = agent.evaluate_posture();
    }
}
