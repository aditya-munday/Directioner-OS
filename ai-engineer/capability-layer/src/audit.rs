use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Outcome status of an audited capability invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AuditOutcome {
    /// Capability succeeded returning output payload.
    Success {
        /// Serialized return value.
        result: Value,
    },
    /// Capability was denied by policy, killswitch, or verification.
    Denied {
        /// Reason for denial.
        reason: String,
    },
    /// Capability failed with a runtime error.
    Error {
        /// Error details.
        message: String,
    },
}

/// Strongly-typed journal entry recorded in the append-only audit stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// RFC3339 timestamp.
    pub timestamp: String,
    /// Client-supplied request ID.
    pub request_id: String,
    /// Name of the invoked capability.
    pub capability: String,
    /// Capability parameters.
    pub args: Value,
    /// Identity of the requesting service or agent.
    pub requester: String,
    /// Cryptographic or security token.
    pub token: String,
    /// Indicates whether this invocation was a compensating rollback.
    #[serde(default)]
    pub is_rollback: bool,
    /// Result outcome of the invocation.
    pub outcome: AuditOutcome,
    /// Execution latency in milliseconds.
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_hash: Option<String>,
}

impl AuditEntry {
    /// Generates a placeholder entry with a given sequence number.
    pub fn placeholder(seq_hint: u64) -> Self {
        Self {
            seq: seq_hint,
            timestamp: now_rfc3339(),
            request_id: String::new(),
            capability: String::new(),
            args: Value::Null,
            requester: String::new(),
            token: String::new(),
            is_rollback: false,
            outcome: AuditOutcome::Denied {
                reason: "placeholder".into(),
            },
            duration_ms: 0,
            prev_hash: None,
            entry_hash: None,
        }
    }
}

#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
    file: File,
    next_seq: u64,
    last_hash: Option<String>,
}

impl AuditLog {
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let already = count_lines(File::open(&path)?)?;
        let last_hash = scan_last_hash(&path)?;
        Ok(Self {
            path,
            file,
            next_seq: already,
            last_hash,
        })
    }

    pub fn open_read_only(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let already = count_lines(File::open(&path)?)?;
        let last_hash = scan_last_hash(&path)?;
        Ok(Self {
            path,
            file,
            next_seq: already,
            last_hash,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    pub fn last_hash(&self) -> Option<&str> {
        self.last_hash.as_deref()
    }

    pub fn append(&mut self, mut entry: AuditEntry) -> std::io::Result<u64> {
        entry.seq = self.next_seq;
        self.next_seq += 1;
        entry.prev_hash = self.last_hash.clone();
        let computed = compute_entry_hash(self.last_hash.as_deref(), &entry);
        entry.entry_hash = Some(computed.clone());
        self.last_hash = Some(computed);
        let mut line = serde_json::to_vec(&entry).map_err(std::io::Error::other)?;
        line.push(b'\n');
        self.file.write_all(&line)?;
        self.file.flush()?;
        self.file.sync_all()?;
        Ok(entry.seq)
    }

    pub fn verify(&self) -> std::io::Result<VerifyReport> {
        let entries = self.read_all()?;
        Ok(verify_entries(&entries))
    }

    /// Reads all entries from the audit log.
    pub fn read_all(&self) -> std::io::Result<Vec<AuditEntry>> {
        let f = File::open(&self.path)?;
        let reader = BufReader::new(f);
        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<AuditEntry>(line) {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    /// Returns the last `n` entries from the audit log, newest first.
    pub fn tail(&self, n: usize) -> std::io::Result<Vec<AuditEntry>> {
        let mut all = self.read_all()?;
        let skip = all.len().saturating_sub(n);
        let items: Vec<AuditEntry> = all.drain(skip..).rev().collect();
        Ok(items)
    }

    /// Re-opens the audit log file handle.
    pub fn reopen(&self) -> std::io::Result<Self> {
        Self::open(&self.path)
    }
}

fn count_lines(file: File) -> std::io::Result<u64> {
    let mut reader = BufReader::new(file);
    let mut n = 0u64;
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        if !line.trim().is_empty() {
            n += 1;
        }
    }
    Ok(n)
}

/// Returns the current UTC timestamp formatted as RFC3339 with millisecond precision.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Rewinds a file cursor to the beginning.
pub fn rewind(file: &mut File) -> std::io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}

pub fn compute_entry_hash(prev_hash: Option<&str>, entry: &AuditEntry) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    if let Some(prev) = prev_hash {
        hasher.update(prev.as_bytes());
    } else {
        hasher.update(b"GENESIS");
    }
    hasher.update(b"|");
    hasher.update(entry.seq.to_le_bytes());
    hasher.update(b"|");
    hasher.update(entry.timestamp.as_bytes());
    hasher.update(b"|");
    hasher.update(entry.request_id.as_bytes());
    hasher.update(b"|");
    hasher.update(entry.capability.as_bytes());
    hasher.update(b"|");
    let args_str = serde_json::to_string(&entry.args).unwrap_or_default();
    hasher.update(args_str.as_bytes());
    hasher.update(b"|");
    hasher.update(entry.requester.as_bytes());
    hasher.update(b"|");
    hasher.update(entry.token.as_bytes());
    hasher.update(b"|");
    hasher.update(if entry.is_rollback { b"1" } else { b"0" });
    hasher.update(b"|");
    let outcome_str = serde_json::to_string(&entry.outcome).unwrap_or_default();
    hasher.update(outcome_str.as_bytes());
    hasher.update(b"|");
    hasher.update(entry.duration_ms.to_le_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub count_verified: usize,
    pub count_unchained: usize,
    pub broken_index: Option<u64>,
    pub error_message: Option<String>,
}

impl VerifyReport {
    pub fn is_valid(&self) -> bool {
        self.broken_index.is_none()
    }
}

pub fn verify_entries(entries: &[AuditEntry]) -> VerifyReport {
    let mut count_verified = 0;
    let mut count_unchained = 0;
    let mut prev_expected_hash: Option<String> = None;

    for (idx, entry) in entries.iter().enumerate() {
        if entry.seq != idx as u64 {
            return VerifyReport {
                count_verified,
                count_unchained,
                broken_index: Some(entry.seq),
                error_message: Some(format!(
                    "sequence gap: expected seq {}, found {}",
                    idx, entry.seq
                )),
            };
        }
        match &entry.entry_hash {
            None => {
                count_unchained += 1;
                prev_expected_hash = None;
            }
            Some(actual_hash) => {
                if entry.prev_hash != prev_expected_hash {
                    return VerifyReport {
                        count_verified,
                        count_unchained,
                        broken_index: Some(entry.seq),
                        error_message: Some(format!(
                            "broken chain at seq {}: prev_hash {:?} != expected {:?}",
                            entry.seq, entry.prev_hash, prev_expected_hash
                        )),
                    };
                }
                let computed = compute_entry_hash(entry.prev_hash.as_deref(), entry);
                if computed != *actual_hash {
                    return VerifyReport {
                        count_verified,
                        count_unchained,
                        broken_index: Some(entry.seq),
                        error_message: Some(format!(
                            "tampered entry at seq {}: computed hash {} != stored hash {}",
                            entry.seq, computed, actual_hash
                        )),
                    };
                }
                count_verified += 1;
                prev_expected_hash = Some(actual_hash.clone());
            }
        }
    }

    VerifyReport {
        count_verified,
        count_unchained,
        broken_index: None,
        error_message: None,
    }
}

fn scan_last_hash(path: &Path) -> std::io::Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    let mut last = None;
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<AuditEntry>(line)
            && entry.entry_hash.is_some()
        {
            last = entry.entry_hash;
        }
    }
    Ok(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(cap: &str) -> AuditEntry {
        AuditEntry {
            seq: 0,
            timestamp: now_rfc3339(),
            request_id: format!("r-{cap}"),
            capability: cap.into(),
            args: Value::Null,
            requester: "test".into(),
            token: "tok".into(),
            is_rollback: false,
            outcome: AuditOutcome::Success {
                result: Value::Null,
            },
            duration_ms: 1,
            prev_hash: None,
            entry_hash: None,
        }
    }

    #[test]
    fn append_is_durable_and_sequential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worklog.jsonl");
        {
            let mut log = AuditLog::open(&path).unwrap();
            assert_eq!(log.next_seq(), 0);
            assert_eq!(log.append(entry("ping")).unwrap(), 0);
        }
        let log = AuditLog::open(&path).unwrap();
        assert_eq!(log.next_seq(), 1);
        let all = log.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].capability, "ping");
        assert!(all[0].entry_hash.is_some());
    }

    #[test]
    fn tail_returns_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("worklog.jsonl");
        let mut log = AuditLog::open(&path).unwrap();
        log.append(entry("a")).unwrap();
        log.append(entry("b")).unwrap();
        log.append(entry("c")).unwrap();
        let tail = log.tail(2).unwrap();
        let caps: Vec<&str> = tail.iter().map(|e| e.capability.as_str()).collect();
        assert_eq!(caps, vec!["c", "b"]);
    }

    #[test]
    fn placeholder_row_is_roundtrippable() {
        let p = AuditEntry::placeholder(7);
        let json = serde_json::to_string(&p).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn reopen_preserves_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("worklog.jsonl");
        let mut log = AuditLog::open(&path).unwrap();
        log.append(entry("x")).unwrap();
        let reopened = log.reopen().unwrap();
        assert_eq!(reopened.next_seq(), 1);
        assert_eq!(reopened.read_all().unwrap().len(), 1);
    }

    #[test]
    fn hash_chain_verification_detects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tamper.jsonl");
        let mut log = AuditLog::open(&path).unwrap();
        log.append(entry("cap1")).unwrap();
        log.append(entry("cap2")).unwrap();
        log.append(entry("cap3")).unwrap();

        let report = log.verify().unwrap();
        assert!(report.is_valid());
        assert_eq!(report.count_verified, 3);
        assert_eq!(report.count_unchained, 0);

        let mut entries = log.read_all().unwrap();
        entries[1].capability = "tampered".into();
        let tampered_report = verify_entries(&entries);
        assert!(!tampered_report.is_valid());
        assert_eq!(tampered_report.broken_index, Some(1));
    }
}
