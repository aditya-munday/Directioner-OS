use serde::{Deserialize, Serialize};

/// Target execution scope for a catalog capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// System-wide mutation or observation (may require root or polkit)
    System,
    /// User session specific mutation or observation
    User,
    /// Active interactive session / seat context
    Session,
}

/// Tier hint for authorization, risk classification, and approval routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TierHint {
    /// Tier 1: Read-only safe observation
    Tier1Read,
    /// Tier 2: User-scope non-destructive or readily reversible modification
    Tier2UserNonDestructive,
    /// Tier 3: Privileged or potentially mutating system configuration
    Tier3PrivilegedOrMutating,
}

/// Backend mechanism through which the capability executes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// System or session D-Bus interface method
    Dbus,
    /// systemctl / loginctl / systemd user manager
    Systemd,
    /// Sandboxed execution script or helper
    Script,
    /// polkit-authorized action
    Polkit,
    /// Internal kernel / sysfs / procfs or native Directioner service
    Internal,
}

/// Implementation maturity status of the capability entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    /// Fully implemented and verified on the real desktop OS
    Real,
    /// Invariant/model scaffold pending underlying live daemon integration
    Scaffold,
    /// Planned capability definition in schema
    Planned,
}

/// A validated Directioner OS Capability Catalog Entry v1 (U062)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Canonical dotted identifier, e.g. "audio.volume.set"
    pub id: String,
    /// Capability domain, e.g. "audio", "network", "power", "display"
    pub domain: String,
    /// One-line human summary
    pub summary: String,
    /// JSON Schema defining valid input parameters
    pub params: serde_json::Value,
    /// Execution scope (system, user, session)
    pub scope: Scope,
    /// Risk tier hint (tier1_read, tier2_user_non_destructive, tier3_privileged_or_mutating)
    pub tier_hint: TierHint,
    /// Indicates whether capability implements dry-run preview
    pub preview: bool,
    /// Indicates whether capability supports undo or provides an inverse action
    pub undo: bool,
    /// Execution backend kind
    pub backend: BackendKind,
    /// Consent level required: 0 (public), 1 (device/state), 2 (personal/identity), 3 (secret)
    pub consent_level: u8,
    /// Maturity status: real, scaffold, planned
    pub status: EntryStatus,
    /// Human-readable explanation of what the capability does
    pub help_text: String,
    /// Optional GUI parity route (e.g. "settings/sound", "kcm_powerdevil")
    #[serde(default)]
    pub gui_route: Option<String>,
}

/// Top-level TOML document representing a domain capability definition file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainCatalogFile {
    /// Domain name, e.g. "audio", "power", "network"
    pub domain: String,
    /// List of capabilities defined within this domain
    #[serde(default)]
    pub entries: Vec<CatalogEntry>,
}
