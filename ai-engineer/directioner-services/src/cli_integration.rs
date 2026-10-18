use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSessionContext {
    pub session_id: String,
    pub caller_pid: u32,
    pub caller_uid: u32,
    pub working_dir: String,
    pub allowed_capabilities: Vec<String>,
    pub sandbox_level: SandboxLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxLevel {
    Strict,
    Standard,
    Unrestricted,
}

pub struct CliIntegrationBroker {
    active_sessions: std::collections::HashMap<String, CliSessionContext>,
}

impl CliIntegrationBroker {
    pub fn new() -> Self {
        CliIntegrationBroker {
            active_sessions: std::collections::HashMap::new(),
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
        };

        self.active_sessions.insert(session_id, ctx);
        Ok(())
    }

    pub fn authorize_action(&self, session_id: &str, required_cap: &str) -> bool {
        if let Some(session) = self.active_sessions.get(session_id) {
            session.allowed_capabilities.iter().any(|c| c == required_cap)
        } else {
            false
        }
    }

    pub fn terminate_session(&mut self, session_id: &str) -> bool {
        self.active_sessions.remove(session_id).is_some()
    }

    pub fn active_session_count(&self) -> usize {
        self.active_sessions.len()
    }
}
