//! Append-only audit journal for every capability call (Phase 0.3).

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AuditOutcome {
    Success { result: Value },
    Denied { reason: String },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    pub seq: u64,
    pub timestamp: String,
    pub request_id: String,
    pub capability: String,
    pub args: Value,
    pub requester: String,
    pub token: String,
    pub outcome: AuditOutcome,
    pub duration_ms: u64,
}

impl AuditEntry {
    pub fn placeholder(seq_hint: u64) -> Self {
        Self {
            seq: seq_hint,
            timestamp: now_rfc3339(),
            request_id: String::new(),
            capability: String::new(),
            args: Value::Null,
            requester: String::new(),
            token: String::new(),
            outcome: AuditOutcome::Denied {
                reason: "placeholder".into(),
            },
            duration_ms: 0,
        }
    }
}

#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
    file: File,
    next_seq: u64,
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
        Ok(Self {
            path,
            file,
            next_seq: already,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    pub fn append(&mut self, mut entry: AuditEntry) -> std::io::Result<u64> {
        entry.seq = self.next_seq;
        self.next_seq += 1;
        let mut line = serde_json::to_vec(&entry).map_err(std::io::Error::other)?;
        line.push(b'\n');
        self.file.write_all(&line)?;
        self.file.flush()?;
        self.file.sync_all()?;
        Ok(entry.seq)
    }

    pub fn read_all(&self) -> std::io::Result<Vec<AuditEntry>> {
        let mut reader = BufReader::new(File::open(&self.path)?);
        let mut out = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                break;
            }
            if let Some(entry) = parse_line(&line) {
                out.push(entry);
            }
        }
        Ok(out)
    }

    pub fn tail(&self, n: usize) -> std::io::Result<Vec<AuditEntry>> {
        let mut all = self.read_all()?;
        let mut deque = VecDeque::new();
        let skip = all.len().saturating_sub(n);
        for entry in all.drain(skip..) {
            deque.push_back(entry);
        }
        Ok(deque.into_iter().rev().collect())
    }

    pub fn reopen(&self) -> std::io::Result<Self> {
        Self::open(&self.path)
    }
}

fn parse_line(line: &str) -> Option<AuditEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str::<AuditEntry>(line).ok()
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

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn rewind(file: &mut File) -> std::io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    Ok(())
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
            outcome: AuditOutcome::Success {
                result: Value::Null,
            },
            duration_ms: 1,
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
}
