use std::collections::HashMap;

/// Client consent ledger tracking authorization levels per caller UID (U068)
#[derive(Debug, Clone, Default)]
pub struct ConsentLedger {
    /// Grants mapping caller UID -> authorized consent level (0..=2)
    grants: HashMap<u32, u8>,
}

impl ConsentLedger {
    /// Create a new empty consent ledger
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant a specific consent level to a caller UID
    pub fn grant(&mut self, uid: u32, level: u8) {
        let capped = level.min(2); // Secrets (3) cannot be granted via consent ledger
        self.grants.insert(uid, capped);
    }

    /// Revoke all elevated grants for a caller UID
    pub fn revoke(&mut self, uid: u32) {
        self.grants.remove(&uid);
    }

    /// Resolve effective consent level for caller UID.
    /// Invariant: Unknown clients receive level 0 (public facts only) per U068.
    pub fn resolve_level(&self, uid: u32) -> u8 {
        self.grants.get(&uid).copied().unwrap_or(0)
    }
}
