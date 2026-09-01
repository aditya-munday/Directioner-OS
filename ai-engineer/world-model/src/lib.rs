//! SQLite-backed World Model for Directioner-OS (Phase 1.1).
//!
//! Maintains an offline, sub-10ms queryable graph index of installed packages,
//! reverse dependency trees, systemd unit states, transition event logs, and read-only
//! hardware/system telemetry.

pub mod db;
pub mod model;
pub mod query;
pub mod sources;
pub mod sync;

use chrono::{SecondsFormat, Utc};

/// Returns the current UTC timestamp formatted as RFC3339 with millisecond precision.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Computes an RFC3339 cutoff timestamp string subtracting `duration` from the current time.
pub fn cutoff_rfc3339(duration: std::time::Duration) -> String {
    let delta = chrono::Duration::from_std(duration).unwrap_or_else(|_| chrono::Duration::hours(1));
    (Utc::now() - delta).to_rfc3339_opts(SecondsFormat::Millis, true)
}
