use std::path::Path;
use sha2::{Digest, Sha256};
use crate::config::PlatformConfig;
use crate::mock_backend::MockBackend;
use crate::protocol::{AccountRequest, AccountResponse, ResponseData};
use crate::rate_limit::RateLimiter;
use crate::store::{SessionRecord, StateStore};
use crate::types::{AccountEvent, AccountState, Audience, AuthToken, DeviceRegistration, UserProfile};

pub struct AccountEngine {
    pub config: PlatformConfig,
    pub store: StateStore,
    pub backend: MockBackend,
    pub rate_limiter: RateLimiter,
    pub events: Vec<AccountEvent>,
    pub current_device: DeviceRegistration,
    pub clock_skew_secs: i64,
}

impl AccountEngine {
    pub fn new<P: AsRef<Path>>(data_dir: P, config: PlatformConfig, backend: MockBackend) -> Result<Self, String> {
        let store = StateStore::new(data_dir)?;
        let device = DeviceRegistration {
            device_id: "dev-local-001".to_string(),
            device_name: "directioner-workstation".to_string(),
            os_version: "42.0.0".to_string(),
            variant_id: "server".to_string(),
            registered_at: 1700000000,
            is_current: true,
            revoked: false,
        };

        let rate_limiter = RateLimiter::new(100, 10);

        Ok(AccountEngine {
            config,
            store,
            backend,
            rate_limiter,
            events: Vec::new(),
            current_device: device,
            clock_skew_secs: 0,
        })
    }

    pub fn handle_request(&mut self, req: AccountRequest, caller_uid: u32, now: i64) -> AccountResponse {
        if !self.rate_limiter.check_and_consume(caller_uid, now) {
            return AccountResponse::Error {
                code: "rate_limited".to_string(),
                message: "too many requests from local uid".to_string(),
            };
        }

        match req {
            AccountRequest::LoginStartDevice => {
                match self.backend.start_device_code_flow(now) {
                    Ok((device_code, user_code, verification_uri, interval)) => {
                        let rec = SessionRecord {
                            state: AccountState::SigningIn,
                            profile: None,
                            token: None,
                            entitlements: None,
                            current_device: self.current_device.clone(),
                            updated_at: now,
                        };
                        let _ = self.store.save_session(&rec);

                        AccountResponse::Ok(ResponseData::DeviceCodeInitiated {
                            device_code,
                            user_code,
                            verification_uri,
                            interval,
                        })
                    }
                    Err(e) => AccountResponse::Error {
                        code: "backend_error".to_string(),
                        message: e,
                    },
                }
            }

            AccountRequest::LoginPollDevice { device_code } => {
                match self.backend.poll_device_code_token(&device_code, now) {
                    Ok(token) => {
                        let profile = UserProfile {
                            user_id: "usr-42".to_string(),
                            user_hash: sha256_hex("usr-42"),
                            email_hash: sha256_hex("user@directioner.internal"),
                            org_id: None,
                        };
                        let ents = self.backend.get_entitlements(now).ok();

                        let rec = SessionRecord {
                            state: AccountState::SignedIn,
                            profile: Some(profile.clone()),
                            token: Some(token.clone()),
                            entitlements: ents,
                            current_device: self.current_device.clone(),
                            updated_at: now,
                        };
                        let _ = self.store.save_session(&rec);

                        self.events.push(AccountEvent::LoggedIn {
                            user_hash: profile.user_hash,
                            device_id: self.current_device.device_id.clone(),
                        });

                        AccountResponse::Ok(ResponseData::TokenIssued(token))
                    }
                    Err(e) => AccountResponse::Error {
                        code: e.clone(),
                        message: e,
                    },
                }
            }

            AccountRequest::LoginPkce { verifier, challenge } => {
                match self.backend.exchange_pkce(&verifier, &challenge, now) {
                    Ok(token) => {
                        let profile = UserProfile {
                            user_id: "usr-42".to_string(),
                            user_hash: sha256_hex("usr-42"),
                            email_hash: sha256_hex("user@directioner.internal"),
                            org_id: None,
                        };
                        let ents = self.backend.get_entitlements(now).ok();

                        let rec = SessionRecord {
                            state: AccountState::SignedIn,
                            profile: Some(profile.clone()),
                            token: Some(token.clone()),
                            entitlements: ents,
                            current_device: self.current_device.clone(),
                            updated_at: now,
                        };
                        let _ = self.store.save_session(&rec);

                        self.events.push(AccountEvent::LoggedIn {
                            user_hash: profile.user_hash,
                            device_id: self.current_device.device_id.clone(),
                        });

                        AccountResponse::Ok(ResponseData::TokenIssued(token))
                    }
                    Err(e) => AccountResponse::Error {
                        code: "pkce_error".to_string(),
                        message: e,
                    },
                }
            }

            AccountRequest::RefreshToken => {
                let session = match self.store.load_session() {
                    Ok(Some(s)) => s,
                    _ => return AccountResponse::Error {
                        code: "not_signed_in".to_string(),
                        message: "no active session".to_string(),
                    },
                };

                let old_rt = match session.token.as_ref() {
                    Some(t) => t.refresh_token.clone(),
                    None => return AccountResponse::Error {
                        code: "no_token".to_string(),
                        message: "session has no token".to_string(),
                    },
                };

                match self.backend.refresh_token(&old_rt, now) {
                    Ok(new_token) => {
                        let mut updated = session;
                        updated.token = Some(new_token.clone());
                        updated.updated_at = now;
                        let _ = self.store.save_session(&updated);

                        self.events.push(AccountEvent::TokenRefreshed {
                            audience: new_token.audience.clone(),
                        });

                        AccountResponse::Ok(ResponseData::TokenIssued(new_token))
                    }
                    Err(e) if e.contains("token_compromised") => {
                        let _ = self.store.clear_session();
                        self.events.push(AccountEvent::Revoked {
                            reason: "refresh token reuse detected".to_string(),
                        });
                        AccountResponse::Error {
                            code: "token_compromised".to_string(),
                            message: "compromise detected; session revoked".to_string(),
                        }
                    }
                    Err(e) => AccountResponse::Error {
                        code: "refresh_failed".to_string(),
                        message: e,
                    },
                }
            }

            AccountRequest::Logout => {
                let _ = self.store.clear_session();
                self.events.push(AccountEvent::LoggedOut {
                    device_id: self.current_device.device_id.clone(),
                });
                AccountResponse::Ok(ResponseData::LoggedOut)
            }

            AccountRequest::Status => {
                let (state, profile, plan, remaining) = match self.store.load_session() {
                    Ok(Some(s)) => {
                        let plan_name = s.entitlements.as_ref().map(|e| e.plan.clone());
                        let grace = if s.state == AccountState::OfflineGrace {
                            s.entitlements.as_ref().map(|e| (e.grace_expiry - now).max(0))
                        } else {
                            None
                        };
                        (s.state, s.profile, plan_name, grace)
                    }
                    _ => (AccountState::SignedOut, None, None, None),
                };

                AccountResponse::Ok(ResponseData::StatusInfo {
                    state,
                    user: profile,
                    device_id: self.current_device.device_id.clone(),
                    plan,
                    offline_grace_remaining_secs: remaining,
                })
            }

            AccountRequest::Whoami => {
                match self.store.load_session() {
                    Ok(Some(s)) if s.profile.is_some() => {
                        let p = s.profile.unwrap();
                        AccountResponse::Ok(ResponseData::WhoamiInfo {
                            user_id: p.user_id,
                            user_hash: p.user_hash,
                            device_name: self.current_device.device_name.clone(),
                        })
                    }
                    _ => AccountResponse::Error {
                        code: "unauthenticated".to_string(),
                        message: "not logged in".to_string(),
                    },
                }
            }

            AccountRequest::TokenGet { audience } => {
                let aud = match Audience::from_str(&audience) {
                    Some(a) => a,
                    None => return AccountResponse::Error {
                        code: "invalid_audience".to_string(),
                        message: format!("unknown audience {}", audience),
                    },
                };

                match self.store.load_session() {
                    Ok(Some(s)) if s.token.is_some() => {
                        let base = s.token.unwrap();
                        let scoped_token = AuthToken {
                            access_token: format!("{}:{}", base.access_token, aud.as_str()),
                            refresh_token: String::new(),
                            token_type: "Bearer".to_string(),
                            expires_in: 900,
                            created_at: now,
                            audience: aud.as_str().to_string(),
                            scopes: aud.allowed_scopes().iter().map(|s| s.to_string()).collect(),
                        };
                        AccountResponse::Ok(ResponseData::TokenIssued(scoped_token))
                    }
                    _ => AccountResponse::Error {
                        code: "no_token".to_string(),
                        message: "must be logged in".to_string(),
                    },
                }
            }

            AccountRequest::DeviceList => {
                let mut list = self.backend.list_devices();
                if !list.iter().any(|d| d.device_id == self.current_device.device_id) {
                    list.push(self.current_device.clone());
                }
                AccountResponse::Ok(ResponseData::DeviceCollection(list))
            }

            AccountRequest::DeviceRevoke { device_id } => {
                let revoked = self.backend.revoke_device(&device_id);
                if device_id == self.current_device.device_id {
                    let _ = self.store.clear_session();
                    self.events.push(AccountEvent::Revoked {
                        reason: "device revoked by user".to_string(),
                    });
                }
                if revoked || device_id == self.current_device.device_id {
                    AccountResponse::Ok(ResponseData::DeviceRevoked { device_id })
                } else {
                    AccountResponse::Error {
                        code: "device_not_found".to_string(),
                        message: "device id not recognized".to_string(),
                    }
                }
            }

            AccountRequest::EntitlementsGet => {
                match self.backend.get_entitlements(now) {
                    Ok(ents) => AccountResponse::Ok(ResponseData::EntitlementsInfo(ents)),
                    Err(e) => {
                        if let Ok(Some(s)) = self.store.load_session() {
                            if let Some(cached) = s.entitlements {
                                if cached.is_valid_offline(now) {
                                    return AccountResponse::Ok(ResponseData::EntitlementsInfo(cached));
                                }
                            }
                        }
                        AccountResponse::Error {
                            code: "offline_and_expired".to_string(),
                            message: e,
                        }
                    }
                }
            }

            AccountRequest::ExportData => {
                let payload = serde_json::json!({
                    "device": self.current_device,
                    "session": self.store.load_session().ok().flatten(),
                    "exported_at": now
                });
                AccountResponse::Ok(ResponseData::ExportArchive {
                    json_payload: payload.to_string(),
                })
            }

            AccountRequest::DeleteAccount => {
                let _ = self.store.clear_session();
                self.events.push(AccountEvent::Revoked {
                    reason: "account deletion requested".to_string(),
                });
                AccountResponse::Ok(ResponseData::AccountDeleted)
            }
        }
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let mut out = String::new();
    for b in hasher.finalize() {
        out.push_str(&format!("{:02x}", b));
    }
    out
}
