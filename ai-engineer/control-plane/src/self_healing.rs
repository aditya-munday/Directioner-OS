//! Directioner-OS Closed-Loop Self-Healing & Automated Remediation Engine (Phase 3.1).
//!
//! Orchestrates closed-loop self-healing by consuming signals from HealthModel,
//! enforcing safety boundaries, circuit breakers, anti-flap timers, and audit sequences:
//! 1. KillSwitch: checked FIRST before any evaluation or plan synthesis.
//! 2. Anti-Flap: requires >= 2 consecutive degraded cycles before acting.
//! 3. Per-Entity Lockout: max 3 attempts per incident entity with hard stop + CRITICAL alert on 4th.
//! 4. Global Remediation Throttle: max 5 actions in 10-min sliding window across all entities.
//! 5. Audit Ordering: Pre-dispatch intent -> Capability execution -> Post-verification outcome.
//! 6. Tier Ceiling: autonomous execution restricted strictly to Notify tier (scores 20-39).
//!    Confirm/ExpertReview tier changes (scores >= 40) stop at proposal + preview diff.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use capability_layer::killswitch::KillSwitch;
use crate::client::CapabilityClient;
use crate::health_model::{HealthModel, HealthModelConfig, HealthStatus, SystemHealthReport};
use crate::implementer::Implementer;
use crate::plan::PlanStatus;
use crate::policy::ApprovalTier;
use crate::remediation::RemediationAgent;
use crate::simulation::Simulator;
use crate::verifier::Verifier;

/// Configuration for the SelfHealingEngine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingConfig {
    /// Path to World Model SQLite database.
    pub db_path: PathBuf,
    /// Maximum remediation attempts allowed per entity within window (default: 3).
    pub max_entity_attempts: u32,
    /// Sliding window in seconds for per-entity attempt budget (default: 3600s / 1hr).
    pub entity_window_secs: u64,
    /// Global maximum autonomous actions across all entities in sliding window (default: 5).
    pub global_action_cap: u32,
    /// Sliding window in seconds for global action cap (default: 600s / 10min).
    pub global_window_secs: u64,
    /// Number of consecutive degraded evaluations required before triggering (default: 2).
    pub anti_flap_consecutive_cycles: u32,
    /// WorkLog path for writing auditable lifecycle events.
    pub worklog_path: Option<PathBuf>,
}

impl Default for SelfHealingConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("/tmp/wm.sqlite"),
            max_entity_attempts: 3,
            entity_window_secs: 3600,
            global_action_cap: 5,
            global_window_secs: 600,
            anti_flap_consecutive_cycles: 2,
            worklog_path: Some(PathBuf::from("/var/log/live-worklog.jsonl")),
        }
    }
}

/// Tracking state for an individual degraded entity (service, parameter, module).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityIncidentState {
    /// Entity identifier (e.g. "canary-crashloop.service", "net.ipv4.ip_forward").
    pub entity: String,
    /// Number of consecutive cycles this entity has evaluated as degraded.
    pub consecutive_degraded_cycles: u32,
    /// Timestamps of remediation attempts within sliding window.
    pub attempt_timestamps: Vec<DateTime<Utc>>,
    /// Whether this entity is locked out from further autonomous actions.
    pub locked_out: bool,
    /// Timestamp when lockout was engaged.
    pub locked_out_at: Option<DateTime<Utc>>,
}

impl EntityIncidentState {
    /// Create new tracking state for an entity.
    pub fn new(entity: &str) -> Self {
        Self {
            entity: entity.to_string(),
            consecutive_degraded_cycles: 0,
            attempt_timestamps: Vec::new(),
            locked_out: false,
            locked_out_at: None,
        }
    }

    /// Clean up expired attempt timestamps outside the window.
    pub fn clean_attempts(&mut self, window: Duration) {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(window).unwrap_or_else(|_| chrono::Duration::hours(1));
        self.attempt_timestamps.retain(|&ts| ts >= cutoff);
    }
}

/// Specific outcome of a single self-healing evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SelfHealingOutcome {
    /// Silenced immediately because killswitch is engaged.
    DisabledByKillswitch,
    /// System is healthy or unobserved; no self-healing required.
    HealthyOrUnobserved {
        /// Composite health score if available.
        score: Option<u32>,
    },
    /// Transient failure suppressed by anti-flap duration filter.
    AntiFlapSuppressed {
        /// Degraded entity name.
        entity: String,
        /// Current observed consecutive cycles.
        consecutive_cycles: u32,
        /// Required consecutive cycles before triggering.
        required_cycles: u32,
    },
    /// Entity has exceeded attempt budget and is hard locked out.
    EntityLockedOut {
        /// Locked-out entity name.
        entity: String,
        /// Attempt count within sliding window.
        attempts: u32,
        /// Human-readable explanation.
        message: String,
    },
    /// Global velocity circuit breaker engaged (max actions in window hit).
    GlobalThrottleEngaged {
        /// Actions recorded in sliding window.
        actions_in_window: u32,
        /// Maximum allowed global action cap.
        cap: u32,
        /// Human-readable explanation.
        message: String,
    },
    /// Remediation requires Confirm/ExpertReview tier; proposal synthesized, awaiting confirmation.
    AwaitingConfirmation {
        /// Incident entity name.
        entity: String,
        /// Synthesized plan ID.
        plan_id: String,
        /// Evaluated approval tier.
        tier: ApprovalTier,
        /// Human-readable plan summary.
        summary: String,
        /// Unified diff preview of proposed changes.
        preview_diff: String,
    },
    /// Notify-tier action executed autonomously and verified clean.
    ExecutedNotifyTier {
        /// Remediated entity name.
        entity: String,
        /// Executed plan ID.
        plan_id: String,
        /// Description of executed action.
        action: String,
        /// Total execution duration in milliseconds.
        duration_ms: u64,
        /// Whether post-execution verification passed.
        verified: bool,
    },
    /// Execution failed; compensating rollback was executed.
    ExecutionFailedRolledBack {
        /// Degraded entity name.
        entity: String,
        /// Executed plan ID.
        plan_id: String,
        /// Failed attempt number.
        attempt: u32,
        /// Error message.
        error: String,
    },
}

/// Comprehensive report of a SelfHealingEngine evaluation cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingReport {
    /// Timestamp of evaluation (RFC3339).
    pub evaluated_at: String,
    /// Health report from HealthModel.
    pub health_report: SystemHealthReport,
    /// Whether killswitch was engaged at time of evaluation.
    pub killswitch_engaged: bool,
    /// List of outcomes produced during this cycle.
    pub outcomes: Vec<SelfHealingOutcome>,
}

/// Closed-Loop Self-Healing & Remediation Engine.
pub struct SelfHealingEngine {
    config: SelfHealingConfig,
    killswitch: KillSwitch,
    client: CapabilityClient,
    remediation_agent: RemediationAgent,
    incident_states: HashMap<String, EntityIncidentState>,
    global_actions_history: Vec<DateTime<Utc>>,
}

impl SelfHealingEngine {
    /// Create a new SelfHealingEngine.
    pub fn new(
        config: SelfHealingConfig,
        killswitch: KillSwitch,
        client: CapabilityClient,
        remediation_agent: RemediationAgent,
    ) -> Self {
        Self {
            config,
            killswitch,
            client,
            remediation_agent,
            incident_states: HashMap::new(),
            global_actions_history: Vec::new(),
        }
    }

    /// Explicitly get mutable access to incident states (for testing/inspection).
    pub fn incident_states_mut(&mut self) -> &mut HashMap<String, EntityIncidentState> {
        &mut self.incident_states
    }

    /// Explicitly get mutable access to global action history (for testing/inspection).
    pub fn global_actions_history_mut(&mut self) -> &mut Vec<DateTime<Utc>> {
        &mut self.global_actions_history
    }

    /// Explicitly clear lockout on an entity (operator action).
    pub fn clear_entity_lockout(&mut self, entity: &str) {
        if let Some(state) = self.incident_states.get_mut(entity) {
            state.locked_out = false;
            state.locked_out_at = None;
            state.attempt_timestamps.clear();
            state.consecutive_degraded_cycles = 0;
        }
    }

    /// Reset global throttle history (operator action).
    pub fn reset_global_throttle(&mut self) {
        self.global_actions_history.clear();
    }

    /// Record structured audit line to worklog if path configured.
    fn write_worklog_audit(
        path: Option<&std::path::Path>,
        capability: &str,
        requester: &str,
        args: serde_json::Value,
        result: serde_json::Value,
        status: &str,
    ) {
        if let Some(p) = path {
            let entry = serde_json::json!({
                "timestamp": Utc::now().to_rfc3339(),
                "request_id": format!("self-heal-{}", Utc::now().timestamp_millis()),
                "capability": capability,
                "requester": requester,
                "args": args,
                "outcome": {
                    "status": status,
                    "result": result,
                }
            });
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(file, "{}", entry);
            }
        }
    }

    /// Clean global actions history outside the sliding window.
    fn clean_global_history(&mut self) {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(Duration::from_secs(self.config.global_window_secs))
                .unwrap_or_else(|_| chrono::Duration::minutes(10));
        self.global_actions_history.retain(|&ts| ts >= cutoff);
    }

    /// Run one complete self-healing evaluation cycle.
    pub fn evaluate_and_heal(&mut self) -> SelfHealingReport {
        let now_str = Utc::now().to_rfc3339();

        // 1. INVARIANT 1: KillSwitch check BEFORE any plan synthesis or evaluation
        if self.killswitch.is_engaged() {
            return SelfHealingReport {
                evaluated_at: now_str,
                health_report: SystemHealthReport {
                    evaluated_at: Utc::now().to_rfc3339(),
                    composite_score: None,
                    overall_status: HealthStatus::Unknown,
                    summary: "Evaluation bypassed: KillSwitch engaged".into(),
                    subsystems: Vec::new(),
                },
                killswitch_engaged: true,
                outcomes: vec![SelfHealingOutcome::DisabledByKillswitch],
            };
        }

        // 2. Health Model evaluation (pure read-only scoring)
        let health_model = HealthModel::new(HealthModelConfig {
            db_path: self.config.db_path.clone(),
            worklog_path: self.config.worklog_path.clone(),
        });
        let health = health_model.evaluate();
        let mut outcomes = Vec::new();

        if health.overall_status == HealthStatus::Healthy
            || health.overall_status == HealthStatus::Unknown
        {
            // Reset consecutive failure counters for recovered entities
            for state in self.incident_states.values_mut() {
                if !state.locked_out {
                    state.consecutive_degraded_cycles = 0;
                }
            }
            outcomes.push(SelfHealingOutcome::HealthyOrUnobserved {
                score: health.composite_score,
            });
            return SelfHealingReport {
                evaluated_at: now_str,
                health_report: health,
                killswitch_engaged: false,
                outcomes,
            };
        }

        // Clean global actions history
        self.clean_global_history();

        // 3. Extract degraded entities from health subsystem evidence
        let mut degraded_entities: Vec<(String, String, String)> = Vec::new(); // (entity, source, reason)
        for sub in &health.subsystems {
            for ev in &sub.evidence {
                if ev.severity == "critical" || ev.severity == "warning" {
                    if !ev.entity.is_empty() {
                        degraded_entities.push((
                            ev.entity.clone(),
                            ev.source.clone(),
                            ev.description.clone(),
                        ));
                    }
                }
            }
        }

        if degraded_entities.is_empty() {
            outcomes.push(SelfHealingOutcome::HealthyOrUnobserved {
                score: health.composite_score,
            });
            return SelfHealingReport {
                evaluated_at: now_str,
                health_report: health,
                killswitch_engaged: false,
                outcomes,
            };
        }

        // Track seen entities in this cycle
        let mut seen_entities = std::collections::HashSet::new();
        let worklog_p = self.config.worklog_path.as_deref();

        for (entity, _source, reason) in degraded_entities {
            if seen_entities.contains(&entity) {
                continue;
            }
            seen_entities.insert(entity.clone());

            let (consecutive_cycles, attempt_count, is_locked_out) = {
                let state = self
                    .incident_states
                    .entry(entity.clone())
                    .or_insert_with(|| EntityIncidentState::new(&entity));
                state.consecutive_degraded_cycles += 1;
                state.clean_attempts(Duration::from_secs(self.config.entity_window_secs));
                (
                    state.consecutive_degraded_cycles,
                    state.attempt_timestamps.len(),
                    state.locked_out,
                )
            };

            // 4. INVARIANT 2: Anti-Flap Duration Filter
            if consecutive_cycles < self.config.anti_flap_consecutive_cycles {
                outcomes.push(SelfHealingOutcome::AntiFlapSuppressed {
                    entity: entity.clone(),
                    consecutive_cycles,
                    required_cycles: self.config.anti_flap_consecutive_cycles,
                });
                continue;
            }

            // 5. INVARIANT 3: Per-Entity 3-Attempt Budget & Hard Lockout
            if is_locked_out || attempt_count >= self.config.max_entity_attempts as usize {
                if !is_locked_out {
                    if let Some(state) = self.incident_states.get_mut(&entity) {
                        state.locked_out = true;
                        state.locked_out_at = Some(Utc::now());
                    }
                    Self::write_worklog_audit(
                        worklog_p,
                        "remediation.entity_locked_out",
                        "self-healing-engine",
                        serde_json::json!({
                            "entity": entity,
                            "attempts": attempt_count,
                            "reason": "Remediation budget exhausted (3 failed attempts). Autonomous healing suspended for this entity. Operator intervention required.",
                        }),
                        serde_json::json!({ "locked_out": true }),
                        "critical",
                    );
                }
                outcomes.push(SelfHealingOutcome::EntityLockedOut {
                    entity: entity.clone(),
                    attempts: attempt_count as u32,
                    message: format!(
                        "Entity '{entity}' exceeded attempt budget (3 attempts). Locked out until manual clearance."
                    ),
                });
                continue;
            }

            // 6. INVARIANT 4: Global Throttle & Cascading Failure Circuit Breaker
            if self.global_actions_history.len() >= self.config.global_action_cap as usize {
                Self::write_worklog_audit(
                    worklog_p,
                    "remediation.global_throttle_engaged",
                    "self-healing-engine",
                    serde_json::json!({
                        "actions_in_window": self.global_actions_history.len(),
                        "cap": self.config.global_action_cap,
                        "reason": "Global remediation throttle exceeded (5 actions in 10m). System-wide autonomous healing suspended to prevent cascading thrashing. Escalating all pending incidents to Operator Confirmation.",
                    }),
                    serde_json::json!({ "circuit_breaker": "engaged" }),
                    "critical",
                );
                outcomes.push(SelfHealingOutcome::GlobalThrottleEngaged {
                    actions_in_window: self.global_actions_history.len() as u32,
                    cap: self.config.global_action_cap,
                    message: "Global remediation throttle exceeded (5 actions in 10m). Escalated to operator confirmation.".into(),
                });
                continue;
            }

            // 7. Plan Synthesis based on entity type
            let plan_res = if entity.ends_with(".service") {
                self.remediation_agent.plan_service_restart(&entity)
            } else if entity.starts_with("module:") {
                let mod_name = entity.trim_start_matches("module:");
                self.remediation_agent.plan_module_load(mod_name, Vec::new())
            } else if entity.starts_with("sysctl:") || entity.contains('.') {
                let key = entity.trim_start_matches("sysctl:");
                self.remediation_agent
                    .plan_kernel_param_tuning(key, "1", false)
            } else {
                self.remediation_agent.plan_service_restart(&entity)
            };

            let plan = match plan_res {
                Ok(p) => p,
                Err(e) => {
                    outcomes.push(SelfHealingOutcome::ExecutionFailedRolledBack {
                        entity: entity.clone(),
                        plan_id: "synthesis_failed".into(),
                        attempt: attempt_count as u32 + 1,
                        error: e.to_string(),
                    });
                    continue;
                }
            };

            // 8. INVARIANT 5: Notify Tier Ceiling & Audit Ordering
            if plan.aggregate_blast_radius.tier.requires_confirmation() {
                // Confirm or ExpertReview tier: Proposal only!
                let preview = Simulator::simulate_plan(&plan, None);
                Self::write_worklog_audit(
                    worklog_p,
                    "remediation.proposal_awaiting_confirmation",
                    "self-healing-engine",
                    serde_json::json!({
                        "entity": entity,
                        "plan_id": plan.id,
                        "tier": plan.aggregate_blast_radius.tier,
                        "summary": plan.summary,
                    }),
                    serde_json::json!({ "status": "awaiting_operator_confirmation" }),
                    "notify",
                );
                outcomes.push(SelfHealingOutcome::AwaitingConfirmation {
                    entity: entity.clone(),
                    plan_id: plan.id.clone(),
                    tier: plan.aggregate_blast_radius.tier,
                    summary: plan.summary.clone(),
                    preview_diff: preview.render_terminal(),
                });
                continue;
            }

            // Safe Notify Tier: Execute with 3-Stage Audit Ordering
            // STAGE 1: Pre-Dispatch Audit Record
            Self::write_worklog_audit(
                worklog_p,
                "remediation.intent_dispatched",
                "self-healing-engine",
                serde_json::json!({
                    "entity": entity,
                    "plan_id": plan.id,
                    "tier": "Notify",
                    "reason": reason,
                    "action": plan.steps.first().map(|s| s.description.clone()).unwrap_or_default(),
                }),
                serde_json::json!({ "dispatched": true }),
                "info",
            );

            // STAGE 2: Capability Execution
            let start = Instant::now();
            let implementer = Implementer::new(&self.client);
            let exec_report = implementer.execute_plan(&plan);
            let duration_ms = start.elapsed().as_millis() as u64;

            // STAGE 3: Post-Verification & Outcome
            let verifier = Verifier::new(&self.client);
            let verify_report = verifier.verify_plan(&plan);

            let success =
                exec_report.final_status == PlanStatus::Completed && verify_report.all_passed;

            if success {
                Self::write_worklog_audit(
                    worklog_p,
                    "remediation.completed_verified",
                    "self-healing-engine",
                    serde_json::json!({
                        "entity": entity,
                        "plan_id": plan.id,
                        "duration_ms": duration_ms,
                    }),
                    serde_json::json!({ "verified": true, "all_checks_passed": true }),
                    "success",
                );
                self.global_actions_history.push(Utc::now());
                if let Some(st) = self.incident_states.get_mut(&entity) {
                    st.consecutive_degraded_cycles = 0;
                }
                outcomes.push(SelfHealingOutcome::ExecutedNotifyTier {
                    entity: entity.clone(),
                    plan_id: plan.id.clone(),
                    action: plan
                        .steps
                        .first()
                        .map(|s| s.description.clone())
                        .unwrap_or_default(),
                    duration_ms,
                    verified: true,
                });
            } else {
                // Trigger atomic compensating rollback
                let rb_report = implementer.rollback_all(&plan);
                let attempt_num = if let Some(st) = self.incident_states.get_mut(&entity) {
                    st.attempt_timestamps.push(Utc::now());
                    st.attempt_timestamps.len() as u32
                } else {
                    1
                };

                Self::write_worklog_audit(
                    worklog_p,
                    "remediation.failed_rolled_back",
                    "self-healing-engine",
                    serde_json::json!({
                        "entity": entity,
                        "plan_id": plan.id,
                        "attempt": attempt_num,
                        "max_attempts": self.config.max_entity_attempts,
                        "rollback_status": rb_report.final_status,
                    }),
                    serde_json::json!({ "rolled_back": true, "error": exec_report.message }),
                    "warning",
                );

                if attempt_num >= self.config.max_entity_attempts {
                    if let Some(st) = self.incident_states.get_mut(&entity) {
                        st.locked_out = true;
                        st.locked_out_at = Some(Utc::now());
                    }
                    Self::write_worklog_audit(
                        worklog_p,
                        "remediation.entity_locked_out",
                        "self-healing-engine",
                        serde_json::json!({
                            "entity": entity,
                            "attempts": attempt_num,
                            "reason": "Remediation budget exhausted (3 failed attempts). Autonomous healing suspended for this entity. Operator intervention required.",
                        }),
                        serde_json::json!({ "locked_out": true }),
                        "critical",
                    );
                }

                outcomes.push(SelfHealingOutcome::ExecutionFailedRolledBack {
                    entity: entity.clone(),
                    plan_id: plan.id.clone(),
                    attempt: attempt_num,
                    error: format!(
                        "Execution or post-verification failed: {}",
                        exec_report.message
                    ),
                });
            }
        }

        SelfHealingReport {
            evaluated_at: now_str,
            health_report: health,
            killswitch_engaged: false,
            outcomes,
        }
    }
}
