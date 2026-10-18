use crate::types::{AccountState, AuthToken, DeviceRegistration, Entitlements, UserProfile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub state: AccountState,
    pub profile: Option<UserProfile>,
    pub token: Option<AuthToken>,
    pub entitlements: Option<Entitlements>,
    pub current_device: DeviceRegistration,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSessionRecord {
    state: AccountState,
    profile: Option<UserProfile>,
    encrypted_token_envelope: Option<EncryptedEnvelope>,
    entitlements: Option<Entitlements>,
    current_device: DeviceRegistration,
    updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedEnvelope {
    nonce: String,
    ciphertext_hex: String,
}

pub struct StateStore {
    root_dir: PathBuf,
    key_material: Vec<u8>,
}

impl StateStore {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> Result<Self, String> {
        let path = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;

        let key_file = path.join(".store.key");
        let key = if key_file.exists() {
            fs::read(&key_file).map_err(|e| e.to_string())?
        } else {
            let mut k = Vec::with_capacity(32);
            let seed = format!("{}-{}", path.display(), std::process::id());
            let mut hasher = Sha256::new();
            hasher.update(seed.as_bytes());
            k.extend_from_slice(&hasher.finalize());

            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&key_file)
                .map_err(|e| e.to_string())?;
            f.write_all(&k).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            k
        };

        Ok(StateStore {
            root_dir: path,
            key_material: key,
        })
    }

    fn session_file_path(&self) -> PathBuf {
        self.root_dir.join("session.json")
    }

    fn devices_file_path(&self) -> PathBuf {
        self.root_dir.join("devices.json")
    }

    pub fn save_session(&self, record: &SessionRecord) -> Result<(), String> {
        let env = match &record.token {
            Some(tok) => {
                let serialized = serde_json::to_string(tok).map_err(|e| e.to_string())?;
                let nonce = format!("{:016x}", record.updated_at);
                let encrypted = self.crypt_bytes(&nonce, serialized.as_bytes());
                Some(EncryptedEnvelope {
                    nonce,
                    ciphertext_hex: hex_encode(&encrypted),
                })
            }
            None => None,
        };

        let stored = StoredSessionRecord {
            state: record.state,
            profile: record.profile.clone(),
            encrypted_token_envelope: env,
            entitlements: record.entitlements.clone(),
            current_device: record.current_device.clone(),
            updated_at: record.updated_at,
        };

        let serialized = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
        self.atomic_write(&self.session_file_path(), serialized.as_bytes())
    }

    pub fn load_session(&self) -> Result<Option<SessionRecord>, String> {
        let path = self.session_file_path();
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let stored: StoredSessionRecord = serde_json::from_str(&data).map_err(|e| e.to_string())?;

        let token = match stored.encrypted_token_envelope {
            Some(env) => {
                let cipher_bytes = hex_decode(&env.ciphertext_hex)?;
                let plain_bytes = self.crypt_bytes(&env.nonce, &cipher_bytes);
                let tok: AuthToken =
                    serde_json::from_slice(&plain_bytes).map_err(|e| e.to_string())?;
                Some(tok)
            }
            None => None,
        };

        Ok(Some(SessionRecord {
            state: stored.state,
            profile: stored.profile,
            token,
            entitlements: stored.entitlements,
            current_device: stored.current_device,
            updated_at: stored.updated_at,
        }))
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
        let list: Vec<DeviceRegistration> =
            serde_json::from_str(&data).map_err(|e| e.to_string())?;
        Ok(list)
    }

    fn crypt_bytes(&self, nonce: &str, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len());
        let mut block_idx: u32 = 0;

        while out.len() < input.len() {
            let mut hasher = Sha256::new();
            hasher.update(&self.key_material);
            hasher.update(nonce.as_bytes());
            hasher.update(block_idx.to_be_bytes());
            let block = hasher.finalize();

            for &b in block.iter() {
                if out.len() < input.len() {
                    out.push(input[out.len()] ^ b);
                } else {
                    break;
                }
            }
            block_idx = block_idx.saturating_add(1);
        }

        out
    }

    fn atomic_write(&self, target: &Path, content: &[u8]) -> Result<(), String> {
        let temp_name = format!(
            "{}.tmp.{}",
            target.file_name().unwrap().to_str().unwrap(),
            std::process::id()
        );
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

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::new();
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("invalid hex length".to_string());
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte = u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string())?;
        out.push(byte);
    }
    Ok(out)
}
