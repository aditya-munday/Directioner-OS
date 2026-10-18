use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncidentRecord {
    pub incident_id: String,
    pub boot_id: String,
    pub timestamp: String,
    pub class: String,
    pub summary: String,
    pub evidence_json: String,
    pub prev_hash: Option<String>,
    pub entry_hash: Option<String>,
}

impl IncidentRecord {
    pub fn new(class: &str, summary: &str, evidence_json: &str) -> Self {
        let boot_id = read_boot_id();
        let now = Utc::now().to_rfc3339();
        let incident_id = format!("{}-{}", &boot_id[..8.min(boot_id.len())], Utc::now().timestamp());
        Self {
            incident_id,
            boot_id,
            timestamp: now,
            class: class.to_string(),
            summary: summary.to_string(),
            evidence_json: evidence_json.to_string(),
            prev_hash: None,
            entry_hash: None,
        }
    }
}

fn read_boot_id() -> String {
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .unwrap_or_else(|_| "unknown-boot-id".to_string())
        .trim()
        .to_string()
}

pub struct IncidentPersistence {
    path: PathBuf,
    max_records: usize,
}

impl IncidentPersistence {
    pub fn new(path: impl AsRef<Path>, max_records: usize) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            max_records,
        }
    }

    pub fn append(&self, mut record: IncidentRecord) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let prev_hash = self.last_hash();
        record.prev_hash = prev_hash.clone();

        let canoncial = format!(
            "{}|{}|{}|{}",
            record.incident_id, record.boot_id, record.timestamp, record.class
        );
        let mut hasher_input = prev_hash.unwrap_or_else(|| "GENESIS".to_string());
        hasher_input.push('|');
        hasher_input.push_str(&canoncial);
        record.entry_hash = Some(format!("{:x}", {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(hasher_input.as_bytes());
            h.finalize()
        }));

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let mut line = serde_json::to_vec(&record).map_err(std::io::Error::other)?;
        line.push(b'\n');
        file.write_all(&line)?;
        file.flush()?;

        self.rotate_if_needed()?;
        Ok(())
    }

    pub fn load_all(&self) -> std::io::Result<Vec<IncidentRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let f = File::open(&self.path)?;
        let reader = BufReader::new(f);
        let mut records = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(r) = serde_json::from_str::<IncidentRecord>(line) {
                records.push(r);
            }
        }
        Ok(records)
    }

    pub fn load_previous_boot(&self, current_boot_id: &str) -> std::io::Result<Vec<IncidentRecord>> {
        Ok(self
            .load_all()?
            .into_iter()
            .filter(|r| r.boot_id != current_boot_id)
            .collect())
    }

    fn last_hash(&self) -> Option<String> {
        let records = self.load_all().unwrap_or_default();
        records.last().and_then(|r| r.entry_hash.clone())
    }

    fn rotate_if_needed(&self) -> std::io::Result<()> {
        let records = self.load_all()?;
        if records.len() <= self.max_records {
            return Ok(());
        }
        let keep = &records[records.len() - self.max_records..];
        let mut f = File::create(&self.path)?;
        for r in keep {
            let mut line = serde_json::to_vec(r).map_err(std::io::Error::other)?;
            line.push(b'\n');
            f.write_all(&line)?;
        }
        f.flush()
    }
}
