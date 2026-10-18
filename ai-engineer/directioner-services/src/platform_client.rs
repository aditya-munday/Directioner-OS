use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    Synced,
    PendingUpload,
    PendingDownload,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudFileRecord {
    pub relative_path: String,
    pub content_hash: String,
    pub size_bytes: u64,
    pub modified_epoch: i64,
    pub status: SyncStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 2000,
        }
    }
}

pub struct CircuitBreaker {
    pub failure_threshold: u32,
    pub failure_count: u32,
    pub state: CircuitState,
    pub last_failure_epoch: i64,
    pub recovery_timeout_secs: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, recovery_secs: i64) -> Self {
        CircuitBreaker {
            failure_threshold: threshold,
            failure_count: 0,
            state: CircuitState::Closed,
            last_failure_epoch: 0,
            recovery_timeout_secs: recovery_secs,
        }
    }

    pub fn allow_request(&mut self, now: i64) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if now - self.last_failure_epoch >= self.recovery_timeout_secs {
                    self.state = CircuitState::HalfOpen;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitState::Closed;
    }

    pub fn record_failure(&mut self, now: i64) {
        self.failure_count += 1;
        self.last_failure_epoch = now;
        if self.failure_count >= self.failure_threshold {
            self.state = CircuitState::Open;
        }
    }
}

pub struct CloudSyncEngine {
    files: HashMap<String, CloudFileRecord>,
    offline_queue: Vec<String>,
}

impl CloudSyncEngine {
    pub fn new() -> Self {
        CloudSyncEngine {
            files: HashMap::new(),
            offline_queue: Vec::new(),
        }
    }

    pub fn register_local_file(&mut self, path: &str, hash: &str, size: u64, modified: i64) {
        let rec = CloudFileRecord {
            relative_path: path.to_string(),
            content_hash: hash.to_string(),
            size_bytes: size,
            modified_epoch: modified,
            status: SyncStatus::PendingUpload,
        };
        self.files.insert(path.to_string(), rec);
        self.offline_queue.push(path.to_string());
    }

    pub fn reconcile_remote(&mut self, path: &str, remote_hash: &str, remote_modified: i64) -> SyncStatus {
        if let Some(local) = self.files.get_mut(path) {
            if local.content_hash == remote_hash {
                local.status = SyncStatus::Synced;
            } else if local.modified_epoch > remote_modified {
                local.status = SyncStatus::PendingUpload;
            } else if local.modified_epoch < remote_modified {
                local.status = SyncStatus::PendingDownload;
            } else {
                local.status = SyncStatus::Conflict;
            }
            local.status
        } else {
            let rec = CloudFileRecord {
                relative_path: path.to_string(),
                content_hash: remote_hash.to_string(),
                size_bytes: 0,
                modified_epoch: remote_modified,
                status: SyncStatus::PendingDownload,
            };
            self.files.insert(path.to_string(), rec);
            SyncStatus::PendingDownload
        }
    }

    pub fn resolve_conflict(&mut self, path: &str, keep_local: bool) -> Option<String> {
        if let Some(rec) = self.files.get_mut(path) {
            if rec.status == SyncStatus::Conflict {
                if keep_local {
                    rec.status = SyncStatus::PendingUpload;
                    None
                } else {
                    let forked = format!("{}.conflict.{}", path, rec.modified_epoch);
                    rec.status = SyncStatus::PendingDownload;
                    Some(forked)
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn pending_count(&self) -> usize {
        self.files.values().filter(|f| f.status != SyncStatus::Synced).count()
    }
}
