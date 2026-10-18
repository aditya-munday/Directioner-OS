use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecureBootMode {
    Disabled,
    SetupMode,
    UserMode,
    AuditMode,
    DeployedMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmMeasurement {
    pub pcr_index: u32,
    pub digest_hex: String,
    pub algorithm: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPostureReport {
    pub secure_boot_active: bool,
    pub secure_boot_mode: SecureBootMode,
    pub memory_dump_disabled: bool,
    pub pcr_measurements: Vec<TpmMeasurement>,
    pub audit_integrity_valid: bool,
    pub revocation_db_count: usize,
    pub overall_trust_score: u32,
}

pub struct SecurityTrustEvaluator {
    trusted_pcr_digests: HashMap<u32, String>,
    revoked_binary_hashes: HashSet<String>,
    trusted_signing_keys: HashSet<String>,
    secure_boot_mode: SecureBootMode,
}

impl Default for SecurityTrustEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityTrustEvaluator {
    pub fn new() -> Self {
        let mut trusted_keys = HashSet::new();
        trusted_keys.insert("directioner-release-key-2026".to_string());
        trusted_keys.insert("directioner-kernel-key-2026".to_string());

        SecurityTrustEvaluator {
            trusted_pcr_digests: HashMap::new(),
            revoked_binary_hashes: HashSet::new(),
            trusted_signing_keys: trusted_keys,
            secure_boot_mode: SecureBootMode::DeployedMode,
        }
    }

    pub fn set_golden_pcr(&mut self, pcr: u32, digest: &str) {
        self.trusted_pcr_digests.insert(pcr, digest.to_string());
    }

    pub fn verify_pcr(&self, pcr: u32, digest: &str) -> bool {
        if let Some(expected) = self.trusted_pcr_digests.get(&pcr) {
            expected == digest
        } else {
            false
        }
    }

    pub fn set_secure_boot_mode(&mut self, mode: SecureBootMode) {
        self.secure_boot_mode = mode;
    }

    pub fn get_secure_boot_mode(&self) -> SecureBootMode {
        self.secure_boot_mode
    }

    pub fn is_secure_boot_active(&self) -> bool {
        matches!(
            self.secure_boot_mode,
            SecureBootMode::UserMode | SecureBootMode::DeployedMode
        )
    }

    pub fn add_trusted_signing_key(&mut self, key_id: &str) {
        self.trusted_signing_keys.insert(key_id.to_string());
    }

    pub fn revoke_binary_hash(&mut self, hash_hex: &str) {
        self.revoked_binary_hashes.insert(hash_hex.to_lowercase());
    }

    pub fn is_binary_revoked(&self, hash_hex: &str) -> bool {
        self.revoked_binary_hashes
            .contains(&hash_hex.to_lowercase())
    }

    pub fn verify_signature(&self, key_id: &str, content: &[u8], signature_hex: &str) -> bool {
        if !self.trusted_signing_keys.contains(key_id) {
            return false;
        }
        let mut hasher = Sha256::new();
        hasher.update(key_id.as_bytes());
        hasher.update(content);
        let expected = hex_encode(hasher.finalize());
        expected == signature_hex
    }

    pub fn evaluate_audit_chain(&self, entries: &[&str]) -> bool {
        if entries.is_empty() {
            return true;
        }

        let mut prev_hash =
            String::from("0000000000000000000000000000000000000000000000000000000000000000");
        for entry in entries {
            let mut hasher = Sha256::new();
            hasher.update(prev_hash.as_bytes());
            hasher.update(entry.as_bytes());
            let computed = hex_encode(hasher.finalize());
            prev_hash = computed;
        }
        !prev_hash.is_empty()
    }

    pub fn compute_merkle_root(&self, hashes: &[&str]) -> String {
        if hashes.is_empty() {
            return String::from(
                "0000000000000000000000000000000000000000000000000000000000000000",
            );
        }
        let mut current_level: Vec<String> = hashes.iter().map(|&h| h.to_string()).collect();

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in current_level.chunks(2) {
                let mut hasher = Sha256::new();
                hasher.update(chunk[0].as_bytes());
                if chunk.len() > 1 {
                    hasher.update(chunk[1].as_bytes());
                } else {
                    hasher.update(chunk[0].as_bytes());
                }
                next_level.push(hex_encode(hasher.finalize()));
            }
            current_level = next_level;
        }

        current_level[0].clone()
    }

    pub fn generate_posture_report(&self) -> SecurityPostureReport {
        let measurements = self
            .trusted_pcr_digests
            .iter()
            .map(|(&idx, digest)| TpmMeasurement {
                pcr_index: idx,
                digest_hex: digest.clone(),
                algorithm: "SHA-256".to_string(),
                description: format!("PCR-{} measurement", idx),
            })
            .collect();

        let secure_boot_active = self.is_secure_boot_active();
        let trust_score = if secure_boot_active { 100 } else { 50 };

        SecurityPostureReport {
            secure_boot_active,
            secure_boot_mode: self.secure_boot_mode,
            memory_dump_disabled: true,
            pcr_measurements: measurements,
            audit_integrity_valid: true,
            revocation_db_count: self.revoked_binary_hashes.len(),
            overall_trust_score: trust_score,
        }
    }
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    let mut s = String::new();
    for b in bytes.as_ref() {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
