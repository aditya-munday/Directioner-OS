use crate::types::{AuthToken, DeviceRegistration, Entitlements};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct MockBackend {
    pub fault_slow: bool,
    pub fault_status: Option<u16>,
    pub fault_rate_limit: Option<u64>,
    pub server_time_offset_secs: i64,
    device_flows: HashMap<String, DeviceFlowState>,
    refresh_tokens: HashMap<String, String>,
    used_refresh_tokens: HashSet<String>,
    devices: Vec<DeviceRegistration>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DeviceFlowState {
    user_code: String,
    device_code: String,
    approved: bool,
    expires_at: i64,
    interval: u64,
}

impl MockBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn server_time(&self, local_now: i64) -> i64 {
        local_now + self.server_time_offset_secs
    }

    pub fn start_device_code_flow(
        &mut self,
        now: i64,
    ) -> Result<(String, String, String, u64), String> {
        self.check_faults()?;

        let device_code = format!("devcode-{}", now);
        let user_code = format!("DIR-{:04}", (now % 10000).abs());
        let verification_uri = "https://auth.directioner.internal/activate".to_string();
        let interval = 5;

        self.device_flows.insert(
            device_code.clone(),
            DeviceFlowState {
                user_code: user_code.clone(),
                device_code: device_code.clone(),
                approved: false,
                expires_at: now + 300,
                interval,
            },
        );

        Ok((device_code, user_code, verification_uri, interval))
    }

    pub fn approve_device_code(&mut self, device_code: &str) {
        if let Some(flow) = self.device_flows.get_mut(device_code) {
            flow.approved = true;
        }
    }

    pub fn poll_device_code_token(
        &mut self,
        device_code: &str,
        now: i64,
    ) -> Result<AuthToken, String> {
        self.check_faults()?;

        let flow = self
            .device_flows
            .get(device_code)
            .ok_or_else(|| "invalid_request".to_string())?;
        if now > flow.expires_at {
            return Err("expired_token".to_string());
        }
        if !flow.approved {
            return Err("authorization_pending".to_string());
        }

        let at = format!("at-{}", now);
        let rt = format!("rt-{}", now);
        self.refresh_tokens
            .insert(rt.clone(), "user-123".to_string());

        Ok(AuthToken {
            access_token: at,
            refresh_token: rt,
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            created_at: now,
            audience: "os".to_string(),
            scopes: vec!["os:read".to_string(), "os:status".to_string()],
        })
    }

    pub fn exchange_pkce(
        &mut self,
        verifier: &str,
        challenge: &str,
        now: i64,
    ) -> Result<AuthToken, String> {
        self.check_faults()?;

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let computed = hex::encode(hasher.finalize());

        if computed != challenge {
            return Err("invalid_grant: code_verifier mismatch".to_string());
        }

        let at = format!("at-pkce-{}", now);
        let rt = format!("rt-pkce-{}", now);
        self.refresh_tokens
            .insert(rt.clone(), "user-123".to_string());

        Ok(AuthToken {
            access_token: at,
            refresh_token: rt,
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            created_at: now,
            audience: "os".to_string(),
            scopes: vec!["os:read".to_string()],
        })
    }

    pub fn refresh_token(&mut self, old_rt: &str, now: i64) -> Result<AuthToken, String> {
        self.check_faults()?;

        if self.used_refresh_tokens.contains(old_rt) {
            return Err("token_compromised: refresh token reuse detected".to_string());
        }

        let uid = self
            .refresh_tokens
            .remove(old_rt)
            .ok_or_else(|| "invalid_grant".to_string())?;
        self.used_refresh_tokens.insert(old_rt.to_string());

        let new_at = format!("at-refreshed-{}", now);
        let new_rt = format!("rt-rotated-{}", now);
        self.refresh_tokens.insert(new_rt.clone(), uid);

        Ok(AuthToken {
            access_token: new_at,
            refresh_token: new_rt,
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            created_at: now,
            audience: "os".to_string(),
            scopes: vec!["os:read".to_string()],
        })
    }

    pub fn get_entitlements(&self, now: i64) -> Result<Entitlements, String> {
        self.check_faults()?;

        Ok(Entitlements {
            plan: "pro".to_string(),
            features: vec![
                "cloud_sync".to_string(),
                "aria_assist".to_string(),
                "diagnostics".to_string(),
            ],
            quota_remaining: 50000,
            reset_epoch: now + 86400,
            grace_expiry: now + (7 * 86400),
            signature: "sig-valid-99".to_string(),
            last_verified_clock: now,
        })
    }

    pub fn register_device(&mut self, device: DeviceRegistration) {
        self.devices.push(device);
    }

    pub fn list_devices(&self) -> Vec<DeviceRegistration> {
        self.devices.clone()
    }

    pub fn revoke_device(&mut self, device_id: &str) -> bool {
        for d in self.devices.iter_mut() {
            if d.device_id == device_id {
                d.revoked = true;
                return true;
            }
        }
        false
    }

    fn check_faults(&self) -> Result<(), String> {
        if let Some(status) = self.fault_status {
            return Err(format!("server_error: HTTP {}", status));
        }
        if let Some(retry_after) = self.fault_rate_limit {
            return Err(format!("rate_limited: retry after {} seconds", retry_after));
        }
        Ok(())
    }
}

mod hex {
    pub fn encode(data: impl AsRef<[u8]>) -> String {
        let mut s = String::new();
        for b in data.as_ref() {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }
}
