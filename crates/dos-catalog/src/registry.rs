use crate::schema::{CatalogEntry, EntryStatus, Scope, TierHint};
use std::collections::HashMap;

/// In-memory catalog registry service (U063)
#[derive(Debug, Clone, Default)]
pub struct CatalogRegistry {
    entries_by_id: HashMap<String, CatalogEntry>,
    domain_index: HashMap<String, Vec<String>>,
}

impl CatalogRegistry {
    /// Create a new empty catalog registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a registry from an iterator of entries
    pub fn from_entries(entries: impl IntoIterator<Item = CatalogEntry>) -> Self {
        let mut reg = Self::new();
        for entry in entries {
            reg.register(entry);
        }
        reg
    }

    /// Register a single catalog entry
    pub fn register(&mut self, entry: CatalogEntry) {
        let id = entry.id.clone();
        let domain = entry.domain.clone();

        self.domain_index
            .entry(domain)
            .or_default()
            .push(id.clone());

        self.entries_by_id.insert(id, entry);
    }

    /// Look up an entry by canonical id
    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries_by_id.get(id)
    }

    /// Check if a capability id exists in the catalog
    pub fn contains(&self, id: &str) -> bool {
        self.entries_by_id.contains_key(id)
    }

    /// Total number of registered capabilities
    pub fn len(&self) -> usize {
        self.entries_by_id.len()
    }

    /// Returns true if no capabilities are registered
    pub fn is_empty(&self) -> bool {
        self.entries_by_id.is_empty()
    }

    /// Return all registered entries sorted by id
    pub fn all_entries(&self) -> Vec<&CatalogEntry> {
        let mut list: Vec<&CatalogEntry> = self.entries_by_id.values().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Find capabilities by domain
    pub fn by_domain(&self, domain: &str) -> Vec<&CatalogEntry> {
        match self.domain_index.get(domain) {
            Some(ids) => ids.iter().filter_map(|id| self.get(id)).collect(),
            None => Vec::new(),
        }
    }

    /// Filter capabilities by scope
    pub fn by_scope(&self, scope: Scope) -> Vec<&CatalogEntry> {
        self.entries_by_id
            .values()
            .filter(|e| e.scope == scope)
            .collect()
    }

    /// Filter capabilities by tier hint
    pub fn by_tier(&self, tier: TierHint) -> Vec<&CatalogEntry> {
        self.entries_by_id
            .values()
            .filter(|e| e.tier_hint == tier)
            .collect()
    }

    /// Filter capabilities by status
    pub fn by_status(&self, status: EntryStatus) -> Vec<&CatalogEntry> {
        self.entries_by_id
            .values()
            .filter(|e| e.status == status)
            .collect()
    }
}
