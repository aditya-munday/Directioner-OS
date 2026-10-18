use crate::types::{AccountState, AuthToken, DeviceRegistration, Entitlements, UserProfile};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub state: AccountState,
    pub profile: Option<UserProfile>,
    pub token: Option<AuthToken>,
    pub entitlements: Option<Entitlements>,
    pub current_device: DeviceRegistration,
    pub updated_at: i64,
}

struct Inner {
    session: Option<SessionRecord>,
    devices: Vec<DeviceRegistration>,
}

pub struct StateStore {
    inner: Arc<RwLock<Inner>>,
}

impl StateStore {
    pub fn new() -> Self {
        StateStore {
            inner: Arc::new(RwLock::new(Inner {
                session: None,
                devices: Vec::new(),
            })),
        }
    }

    pub fn save_session(&self, record: &SessionRecord) -> Result<(), String> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| "store lock poisoned".to_string())?;
        guard.session = Some(record.clone());
        Ok(())
    }

    pub fn load_session(&self) -> Result<Option<SessionRecord>, String> {
        let guard = self
            .inner
            .read()
            .map_err(|_| "store lock poisoned".to_string())?;
        Ok(guard.session.clone())
    }

    pub fn clear_session(&self) -> Result<(), String> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| "store lock poisoned".to_string())?;
        guard.session = None;
        Ok(())
    }

    pub fn save_devices(&self, devices: &[DeviceRegistration]) -> Result<(), String> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| "store lock poisoned".to_string())?;
        guard.devices = devices.to_vec();
        Ok(())
    }

    pub fn load_devices(&self) -> Result<Vec<DeviceRegistration>, String> {
        let guard = self
            .inner
            .read()
            .map_err(|_| "store lock poisoned".to_string())?;
        Ok(guard.devices.clone())
    }
}

impl Default for StateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dummy_device() -> DeviceRegistration {
        DeviceRegistration {
            device_id: "test-device".to_string(),
            device_name: "Test Device".to_string(),
            os_version: "0.0.1".to_string(),
            variant_id: "test".to_string(),
            registered_at: 0,
            is_current: true,
            revoked: false,
        }
    }

    fn dummy_token() -> AuthToken {
        AuthToken {
            access_token: "secret-value".to_string(),
            refresh_token: "refresh-secret".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            created_at: 0,
            audience: "os".to_string(),
            scopes: vec!["os:read".to_string()],
        }
    }

    fn dummy_record(token: Option<AuthToken>) -> SessionRecord {
        SessionRecord {
            state: AccountState::SignedOut,
            profile: None,
            token,
            entitlements: None,
            current_device: dummy_device(),
            updated_at: 0,
        }
    }

    #[test]
    fn nothing_is_written_to_disk() {
        let sentinel: PathBuf =
            std::env::temp_dir().join(format!("store_disk_test_{}", std::process::id()));

        let store = StateStore::new();
        store
            .save_session(&dummy_record(Some(dummy_token())))
            .unwrap();
        store.save_devices(&[dummy_device()]).unwrap();

        assert!(
            !sentinel.exists(),
            "StateStore must not create or write any files: {:?} must not exist",
            sentinel
        );

        let session_json = sentinel.join("session.json");
        let devices_json = sentinel.join("devices.json");
        let store_key = sentinel.join(".store.key");

        assert!(
            !session_json.exists(),
            "session.json must not exist on disk"
        );
        assert!(
            !devices_json.exists(),
            "devices.json must not exist on disk"
        );
        assert!(!store_key.exists(), ".store.key must not exist on disk");
    }

    #[test]
    fn round_trip_session_in_memory() {
        let store = StateStore::new();

        assert!(store.load_session().unwrap().is_none());

        store
            .save_session(&dummy_record(Some(dummy_token())))
            .unwrap();

        let loaded = store.load_session().unwrap().unwrap();
        assert_eq!(loaded.token.unwrap().access_token, "secret-value");

        store.clear_session().unwrap();
        assert!(store.load_session().unwrap().is_none());
    }

    #[test]
    fn round_trip_devices_in_memory() {
        let store = StateStore::new();
        assert!(store.load_devices().unwrap().is_empty());
        store.save_devices(&[dummy_device()]).unwrap();
        assert_eq!(store.load_devices().unwrap().len(), 1);
    }
}
