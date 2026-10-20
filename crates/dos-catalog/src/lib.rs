//! Directioner OS Capability Catalog (dos-catalog)
//!
//! Provides the schema (U062), loader, registry service (U063), and
//! linter (U064) for all Directioner OS capabilities.

pub mod lint;
pub mod loader;
pub mod registry;
pub mod schema;

pub use lint::{CatalogLinter, LintErrorKind, LintViolation};
pub use loader::{CatalogLoader, LoaderError};
pub use registry::CatalogRegistry;
pub use schema::{BackendKind, CatalogEntry, DomainCatalogFile, EntryStatus, Scope, TierHint};
