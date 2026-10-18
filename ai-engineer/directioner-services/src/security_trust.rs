use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmMeasurement {
    pub pcr_index: u32,
    pub digest_hex: String,
    pub algorithm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPostureReport {
    pub secure_boot_active: bool,
    pub memory_dump_disabled: bool,
    pub pcr_measurements: Vec<TpmMeasurement>,
    pub audit_integrity_valid: bool,
}

pub struct SecurityTrustEvaluator {
    trusted_pcr_digests: std::collections::HashMap<u32, String>,
}

impl SecurityTrustEvaluator {
    pub fn new() -> Self {
        SecurityTrustEvaluator {
            trusted_pcr_digests: std::collections::HashMap::new(),
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

    pub fn evaluate_audit_chain(&self, entries: &[&str]) -> bool {
        if entries.is_empty() {
            return true;
        }

        let mut prev_hash = String::from("0000000000000000000000000000000000000000000000000000000000000000");
        for entry in entries {
            let mut hasher = Sha256::new();
            hasher.update(prev_hash.as_bytes());
            hasher.update(entry.as_bytes());
            let computed = hex_encode(hasher.finalize());
            prev_hash = computed;
        }
        !prev_hash.is_empty()
    }
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    let mut s = String::new();
    for b in bytes.as_ref() {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
