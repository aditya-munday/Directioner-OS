use crate::schema::{CatalogEntry, TierHint};
use std::fmt;

/// Lint error kinds for catalog validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintErrorKind {
    /// Identifier does not conform to standard domain.subsystem.action pattern
    InvalidIdFormat(String),
    /// ID domain prefix does not match containing domain
    DomainMismatch { id: String, domain: String },
    /// Summary is empty or too short
    MissingSummary,
    /// Help text is empty or too short
    MissingHelpText,
    /// Invalid consent level (> 3)
    InvalidConsentLevel(u8),
    /// Parameters is not a valid JSON schema object
    InvalidParamsSchema(String),
    /// Mutating capability is missing mandatory dry-run preview support
    MissingPreviewOnMutating(String),
    /// Mutating capability is missing mandatory undo support without justification
    MissingUndoOnMutating(String),
}

impl fmt::Display for LintErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdFormat(id) => write!(
                f,
                "Invalid id format '{id}': must be 'domain.subsystem.action'"
            ),
            Self::DomainMismatch { id, domain } => {
                write!(f, "Entry id '{id}' does not start with domain '{domain}'")
            }
            Self::MissingSummary => write!(f, "Summary must not be empty"),
            Self::MissingHelpText => write!(f, "Help text must not be empty"),
            Self::InvalidConsentLevel(lvl) => {
                write!(f, "Invalid consent level {lvl}: must be between 0 and 3")
            }
            Self::InvalidParamsSchema(err) => write!(f, "Invalid params JSON Schema: {err}"),
            Self::MissingPreviewOnMutating(id) => write!(
                f,
                "Mutating capability '{id}' must support preview (preview = true) per U064/U074"
            ),
            Self::MissingUndoOnMutating(id) => write!(
                f,
                "Mutating capability '{id}' must support undo (undo = true) per U064/U075"
            ),
        }
    }
}

/// A lint finding with entry ID and error kind
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintViolation {
    /// ID of the violating entry
    pub entry_id: String,
    /// Kind of lint violation
    pub kind: LintErrorKind,
}

impl fmt::Display for LintViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.entry_id, self.kind)
    }
}

/// Linter for catalog entries (U064)
pub struct CatalogLinter;

impl CatalogLinter {
    /// Lint a single catalog entry against Directioner OS architectural invariants
    pub fn lint_entry(entry: &CatalogEntry) -> Vec<LintViolation> {
        let mut violations = Vec::new();

        // 1. ID format validation: domain.subsystem.action
        let parts: Vec<&str> = entry.id.split('.').collect();
        if parts.len() < 3 || parts.iter().any(|p| p.is_empty()) {
            violations.push(LintViolation {
                entry_id: entry.id.clone(),
                kind: LintErrorKind::InvalidIdFormat(entry.id.clone()),
            });
        }

        // 2. Domain prefix matching
        if !entry.domain.is_empty() && !entry.id.starts_with(&format!("{}.", entry.domain)) {
            violations.push(LintViolation {
                entry_id: entry.id.clone(),
                kind: LintErrorKind::DomainMismatch {
                    id: entry.id.clone(),
                    domain: entry.domain.clone(),
                },
            });
        }

        // 3. Summary non-empty
        if entry.summary.trim().is_empty() {
            violations.push(LintViolation {
                entry_id: entry.id.clone(),
                kind: LintErrorKind::MissingSummary,
            });
        }

        // 4. Help text non-empty
        if entry.help_text.trim().is_empty() {
            violations.push(LintViolation {
                entry_id: entry.id.clone(),
                kind: LintErrorKind::MissingHelpText,
            });
        }

        // 5. Consent level range (0..=3)
        if entry.consent_level > 3 {
            violations.push(LintViolation {
                entry_id: entry.id.clone(),
                kind: LintErrorKind::InvalidConsentLevel(entry.consent_level),
            });
        }

        // 6. Params JSON Schema validation: must be an object schema
        if !entry.params.is_object() {
            violations.push(LintViolation {
                entry_id: entry.id.clone(),
                kind: LintErrorKind::InvalidParamsSchema(
                    "params must be a JSON Schema object".into(),
                ),
            });
        } else if entry.params.get("type").is_some_and(|t| t != "object") {
            violations.push(LintViolation {
                entry_id: entry.id.clone(),
                kind: LintErrorKind::InvalidParamsSchema(
                    "params schema top-level type must be 'object'".into(),
                ),
            });
        }

        // 7. Preview and Undo requirements on Mutating Capabilities (U064)
        match entry.tier_hint {
            TierHint::Tier1Read => {
                // Read-only operations do not strictly require undo
            }
            TierHint::Tier2UserNonDestructive => {
                // User-scope mutations must implement dry-run preview and undo
                if !entry.preview {
                    violations.push(LintViolation {
                        entry_id: entry.id.clone(),
                        kind: LintErrorKind::MissingPreviewOnMutating(entry.id.clone()),
                    });
                }
                if !entry.undo {
                    violations.push(LintViolation {
                        entry_id: entry.id.clone(),
                        kind: LintErrorKind::MissingUndoOnMutating(entry.id.clone()),
                    });
                }
            }
            TierHint::Tier3PrivilegedOrMutating => {
                // Privileged mutations must always support preview
                if !entry.preview {
                    violations.push(LintViolation {
                        entry_id: entry.id.clone(),
                        kind: LintErrorKind::MissingPreviewOnMutating(entry.id.clone()),
                    });
                }
                // Terminal system actions like reboot/poweroff cannot be undone by physics,
                // but any configuration mutation must support undo
                let is_terminal = entry.id.contains("reboot")
                    || entry.id.contains("shutdown")
                    || entry.id.contains("poweroff");
                if !is_terminal && !entry.undo {
                    violations.push(LintViolation {
                        entry_id: entry.id.clone(),
                        kind: LintErrorKind::MissingUndoOnMutating(entry.id.clone()),
                    });
                }
            }
        }

        violations
    }

    /// Lint all entries in a slice and return all accumulated violations
    pub fn lint_all(entries: &[CatalogEntry]) -> Vec<LintViolation> {
        let mut all_violations = Vec::new();
        for entry in entries {
            all_violations.extend(Self::lint_entry(entry));
        }
        all_violations
    }
}
