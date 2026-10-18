use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::types::{AccountState, AuthToken, DeviceRegistration, Entitlements, UserProfile};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub state: AccountState,
    pub profile: Option<UserProfile>,
    pub token: Option<AuthToken>,
    pub entitlements: Option<Entitlements>,
    pub current_device: DeviceRegistration,
    pub updated_at: i64,
}

pub struct StateStore {
    root_dir: PathBuf,
}

impl StateStore {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Result<Self, String> {
        let path = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        Ok(StateStore { root_dir: path })
    }

    fn session_file_path(&self) -> PathBuf {
        self.root_dir.join("session.json")
    }

    fn devices_file_path(&self) -> PathBuf {
        self.root_dir.join("devices.json")
    }

    pub fn save_session(&self, record: &SessionRecord) -> Result<(), String> {
        let serialized = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
        self.atomic_write(&self.session_file_path(), serialized.as_bytes())
    }

    pub fn load_session(&self) -> Result<Option<SessionRecord>, String> {
        let path = self.session_file_path();
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let rec: SessionRecord = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        Ok(Some(rec))
    }

    pub fn clear_session(&self) -> Result<(), String> {
        let path = self.session_file_path();
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn save_devices(&self, devices: &[DeviceRegistration]) -> Result<(), String> {
        let serialized = serde_json::to_string_pretty(devices).map_err(|e| e.to_string())?;
        self.atomic_write(&self.devices_file_path(), serialized.as_bytes())
    }

    pub fn load_devices(&self) -> Result<Vec<DeviceRegistration>, String> {
        let path = self.devices_file_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let list: Vec<DeviceRegistration> = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        Ok(list)
    }

    fn atomic_write(&self, target: &Path, content: &[u8]) -> Result<(), String> {
        let temp_name = format!("{}.tmp.{}", target.file_name().unwrap().to_str().unwrap(), std::process::id());
        let temp_path = self.root_dir.join(temp_name);

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp_path)
            .map_err(|e| e.to_string())?;

        file.write_all(content).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);

        fs::rename(temp_path, target).map_err(|e| e.to_string())?;
        Ok(())
    }
}
