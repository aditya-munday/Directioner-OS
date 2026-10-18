use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSessionContext {
    pub session_id: String,
    pub caller_pid: u32,
    pub caller_uid: u32,
    pub working_dir: String,
    pub allowed_capabilities: Vec<String>,
    pub sandbox_level: SandboxLevel,
    pub created_epoch: i64,
    pub last_activity_epoch: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxLevel {
    Strict,
    Standard,
    Unrestricted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliAuditEvent {
    pub session_id: String,
    pub action: String,
    pub capability: String,
    pub outcome: AuditOutcome,
    pub epoch: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditOutcome {
    Allowed,
    Denied,
    DraftProduced,
}

pub struct CliIntegrationBroker {
    active_sessions: HashMap<String, CliSessionContext>,
    audit_log: Vec<CliAuditEvent>,
    max_sessions: usize,
}

impl Default for CliIntegrationBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl CliIntegrationBroker {
    pub fn new() -> Self {
        CliIntegrationBroker {
            active_sessions: HashMap::new(),
            audit_log: Vec::new(),
            max_sessions: 64,
        }
    }

    pub fn with_max_sessions(max: usize) -> Self {
        CliIntegrationBroker {
            active_sessions: HashMap::new(),
            audit_log: Vec::new(),
            max_sessions: max,
        }
    }

    pub fn create_session(
        &mut self,
        session_id: String,
        pid: u32,
        uid: u32,
        cwd: String,
        caps: Vec<String>,
        sandbox: SandboxLevel,
    ) -> Result<(), String> {
        if self.active_sessions.len() >= self.max_sessions {
            return Err("max_sessions_reached".to_string());
        }
        if self.active_sessions.contains_key(&session_id) {
            return Err("session_already_exists".to_string());
        }

        let ctx = CliSessionContext {
            session_id: session_id.clone(),
            caller_pid: pid,
            caller_uid: uid,
            working_dir: cwd,
            allowed_capabilities: caps,
            sandbox_level: sandbox,
            created_epoch: epoch_now(),
            last_activity_epoch: epoch_now(),
        };

        self.active_sessions.insert(session_id, ctx);
        Ok(())
    }

    pub fn authorize_action(&mut self, session_id: &str, required_cap: &str) -> bool {
        if let Some(session) = self.active_sessions.get_mut(session_id) {
            let allowed = session
                .allowed_capabilities
                .iter()
                .any(|c| c == required_cap);

            let outcome = if allowed {
                AuditOutcome::Allowed
            } else {
                AuditOutcome::Denied
            };

            session.last_activity_epoch = epoch_now();

            self.audit_log.push(CliAuditEvent {
                session_id: session_id.to_string(),
                action: "authorize".to_string(),
                capability: required_cap.to_string(),
                outcome,
                epoch: epoch_now(),
            });

            allowed
        } else {
            self.audit_log.push(CliAuditEvent {
                session_id: session_id.to_string(),
                action: "authorize".to_string(),
                capability: required_cap.to_string(),
                outcome: AuditOutcome::Denied,
                epoch: epoch_now(),
            });
            false
        }
    }

    pub fn terminate_session(&mut self, session_id: &str) -> bool {
        self.active_sessions.remove(session_id).is_some()
    }

    pub fn active_session_count(&self) -> usize {
        self.active_sessions.len()
    }

    pub fn get_session(&self, session_id: &str) -> Option<&CliSessionContext> {
        self.active_sessions.get(session_id)
    }

    pub fn audit_log(&self) -> &[CliAuditEvent] {
        &self.audit_log
    }

    pub fn update_capabilities(&mut self, session_id: &str, caps: Vec<String>) -> bool {
        if let Some(session) = self.active_sessions.get_mut(session_id) {
            session.allowed_capabilities = caps;
            session.last_activity_epoch = epoch_now();
            true
        } else {
            false
        }
    }

    pub fn sessions_for_uid(&self, uid: u32) -> Vec<&CliSessionContext> {
        self.active_sessions
            .values()
            .filter(|s| s.caller_uid == uid)
            .collect()
    }
}

fn epoch_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
