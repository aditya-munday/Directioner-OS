use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Sensitivity classification for state tree nodes (U065, U068)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum SensitivityLevel {
    /// Level 0: Public system facts (OS name, version, architecture, uptime)
    Public = 0,
    /// Level 1: Device and hardware state (battery level, volume percentage, interface list)
    DeviceState = 1,
    /// Level 2: Personal identifiers (SSIDs, IP addresses, MACs, usernames, file paths)
    Personal = 2,
    /// Level 3: Secrets (cryptographic keys, credentials, tokens - NEVER exposed)
    Secret = 3,
}

/// A node within the Observe Plane State Tree (U065)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateNode {
    /// Canonical node path, e.g. "network.wifi.current"
    pub path: String,
    /// JSON value payload
    pub value: serde_json::Value,
    /// Sensitivity tier
    pub sensitivity: SensitivityLevel,
    /// Source provider
    pub source: String,
    /// Timestamp when this node was refreshed
    pub updated_at: DateTime<Utc>,
}

/// The complete versioned Observe Plane State Tree v1 (U065)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTree {
    /// Monotonically increasing tree version
    pub version: u64,
    /// Snapshot timestamp
    pub timestamp: DateTime<Utc>,
    /// State nodes indexed by path
    pub nodes: HashMap<String, StateNode>,
}

impl Default for StateTree {
    fn default() -> Self {
        Self {
            version: 1,
            timestamp: Utc::now(),
            nodes: HashMap::new(),
        }
    }
}

impl StateTree {
    /// Create a new empty state tree
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update a node in the state tree
    pub fn insert(
        &mut self,
        path: impl Into<String>,
        value: serde_json::Value,
        sensitivity: SensitivityLevel,
        source: impl Into<String>,
    ) {
        let p = path.into();
        let node = StateNode {
            path: p.clone(),
            value,
            sensitivity,
            source: source.into(),
            updated_at: Utc::now(),
        };
        self.nodes.insert(p, node);
        self.version = self.version.saturating_add(1);
        self.timestamp = Utc::now();
    }

    /// Retrieve a node by path
    pub fn get(&self, path: &str) -> Option<&StateNode> {
        self.nodes.get(path)
    }

    /// Retrieve all nodes matching a path prefix
    pub fn get_prefix(&self, prefix: &str) -> Vec<&StateNode> {
        let p = if prefix.ends_with('.') {
            prefix.to_string()
        } else {
            format!("{prefix}.")
        };
        self.nodes
            .iter()
            .filter(|(k, _)| k.starts_with(&p) || *k == prefix)
            .map(|(_, v)| v)
            .collect()
    }
}
