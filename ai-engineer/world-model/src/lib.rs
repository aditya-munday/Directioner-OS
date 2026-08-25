pub mod db;
pub mod model;
pub mod query;
pub mod sources;
pub mod sync;

use chrono::{SecondsFormat, Utc};

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn cutoff_rfc3339(duration: std::time::Duration) -> String {
    let delta = chrono::Duration::from_std(duration).unwrap_or_else(|_| chrono::Duration::hours(1));
    (Utc::now() - delta).to_rfc3339_opts(SecondsFormat::Millis, true)
}
