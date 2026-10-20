//! Directioner OS Observe Plane Daemon (directioner-statd)
//!
//! Provides real-time read-only system telemetry aggregation (U066),
//! provider polling framework (U067), sensitivity/consent classification (U068),
//! and golden redaction engine (U069).

pub mod consent;
pub mod providers;
pub mod redact;
pub mod server;
pub mod tree;

pub use consent::ConsentLedger;
pub use providers::{NetworkProvider, PowerProvider, StateProvider, SystemProvider};
pub use redact::RedactionEngine;
pub use server::{StatDaemon, StatRequest, StatResponse};
pub use tree::{SensitivityLevel, StateNode, StateTree};
