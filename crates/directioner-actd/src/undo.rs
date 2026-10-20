use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Recorded inverse action capable of rolling back a state mutation (U075)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoJournalEntry {
    /// Unique action identifier
    pub action_id: String,
    /// Timestamp when original mutation was executed
    pub timestamp: DateTime<Utc>,
    /// Expiry timestamp after which undo is no longer valid
    pub expires_at: DateTime<Utc>,
    /// Capability ID that was mutated (e.g. "audio.volume.set")
    pub capability_id: String,
    /// Inverse capability ID to execute (e.g. "audio.volume.set")
    pub inverse_capability_id: String,
    /// Inverse parameters to restore previous state (e.g. {"volume_percent": 65})
    pub inverse_params: serde_json::Value,
    /// Human-readable description of what undo will restore
    pub description: String,
}

/// Journal tracking undoable actions with persistent on-disk storage (U075)
pub struct UndoJournal {
    storage_dir: PathBuf,
    entries: HashMap<String, UndoJournalEntry>,
}

impl UndoJournal {
    /// Initialize journal with storage directory
    pub fn new(storage_dir: impl Into<PathBuf>) -> Self {
        let dir = storage_dir.into();
        let _ = fs::create_dir_all(&dir);
        let mut journal = Self {
            storage_dir: dir,
            entries: HashMap::new(),
        };
        journal.load_persisted();
        journal
    }

    fn load_persisted(&mut self) {
        if let Ok(entries) = fs::read_dir(&self.storage_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.extension().is_some_and(|ext| ext == "json") {
                    continue;
                }
                if let Some(journal_entry) = fs::read_to_string(&path)
                    .ok()
                    .and_then(|c| serde_json::from_str::<UndoJournalEntry>(&c).ok())
                {
                    if journal_entry.expires_at > Utc::now() {
                        self.entries
                            .insert(journal_entry.action_id.clone(), journal_entry);
                    } else {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
    }

    /// Record an action and its inverse operation
    pub fn record(
        &mut self,
        action_id: impl Into<String>,
        capability_id: impl Into<String>,
        inverse_capability_id: impl Into<String>,
        inverse_params: serde_json::Value,
        description: impl Into<String>,
        ttl: Duration,
    ) -> Result<UndoJournalEntry, std::io::Error> {
        let aid = action_id.into();
        let now = Utc::now();
        let entry = UndoJournalEntry {
            action_id: aid.clone(),
            timestamp: now,
            expires_at: now + ttl,
            capability_id: capability_id.into(),
            inverse_capability_id: inverse_capability_id.into(),
            inverse_params,
            description: description.into(),
        };

        // Persist to disk
        let file_path = self.storage_dir.join(format!("{aid}.json"));
        let json = serde_json::to_string_pretty(&entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(file_path, json)?;

        self.entries.insert(aid, entry.clone());
        Ok(entry)
    }

    /// Retrieve an unexpired undo entry
    pub fn get(&self, action_id: &str) -> Option<&UndoJournalEntry> {
        self.entries
            .get(action_id)
            .filter(|e| e.expires_at > Utc::now())
    }

    /// Remove an undo entry once consumed
    pub fn consume(&mut self, action_id: &str) -> Option<UndoJournalEntry> {
        let entry = self.entries.remove(action_id)?;
        let file_path = self.storage_dir.join(format!("{action_id}.json"));
        let _ = fs::remove_file(file_path);
        Some(entry)
    }

    /// Prune expired entries
    pub fn prune_expired(&mut self) {
        let now = Utc::now();
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.expires_at <= now)
            .map(|(k, _)| k.clone())
            .collect();

        for id in expired {
            self.consume(&id);
        }
    }

    /// List all active undoable actions
    pub fn active_entries(&self) -> Vec<&UndoJournalEntry> {
        let now = Utc::now();
        self.entries
            .values()
            .filter(|e| e.expires_at > now)
            .collect()
    }
}
