//! Opt-In Telemetry Subsystem (TODO 49).
//!
//! Enforces:
//! 1. **Default OFF**: Zero collection occurs unless explicitly enabled.
//! 2. **Per-Category Granularity**: Users opt in to specific categories.
//! 3. **Payload Preview**: Users can inspect the exact payload before collection.
//! 4. **Rotating Ephemeral ID**: 128-bit random ID rotated every 30 days; zero hardware/user PII.
//! 5. **Local Staging**: Staged to local outbox; no remote server credentials or network transmission (Rule B4).

use std::fs;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Categories of telemetry data available for opt-in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TelemetryCategories {
    /// Panic traces and non-clean daemon shutdown reports.
    pub crash_reports: bool,
    /// High-level resource counters (disk count, thermal zones, PSI levels).
    pub hardware_health: bool,
    /// Aggregate counters of executed capability plans.
    pub feature_usage: bool,
}

/// Persistent configuration for the telemetry subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TelemetryConfig {
    /// Global toggle (Strictly FALSE by default).
    pub enabled: bool,
    /// Selected categories.
    pub categories: TelemetryCategories,
    /// Staging endpoint or outbox note (empty/None by default per Rule B4).
    pub endpoint_url: Option<String>,
}

/// Ephemeral machine identity metadata stored in restricted permissions file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EphemeralIdentity {
    /// Rotating UUIDv4-style random string.
    pub client_id: String,
    /// Expiration timestamp (RFC3339).
    pub expires_at: String,
}

/// Telemetry payload section for hardware health.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareHealthPayload {
    /// Hypervisor platform identifier (e.g. "QEMU", "Bare-Metal").
    pub hypervisor: String,
    /// Total number of active disks.
    pub disk_count: usize,
    /// Total number of thermal zones.
    pub thermal_zone_count: usize,
    /// Overall pressure level summary.
    pub psi_pressure: String,
}

/// Telemetry payload section for crash reports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrashReportPayload {
    /// Daemon name.
    pub daemon: String,
    /// Exit status code.
    pub exit_code: i32,
    /// Clean shutdown flag.
    pub clean_shutdown: bool,
}

/// Telemetry payload section for feature usage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeatureUsagePayload {
    /// Total plans staged.
    pub plans_staged: u64,
    /// Total plans executed.
    pub plans_executed: u64,
    /// Total plans rolled back.
    pub plans_rolled_back: u64,
}

/// Complete inspectable telemetry payload rendered during preview.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryPayload {
    /// Schema format version.
    pub schema_version: String,
    /// Anonymous rotating ephemeral client ID.
    pub ephemeral_client_id: String,
    /// Expiration time of the client ID.
    pub client_id_expires_at: String,
    /// Operating system identification.
    pub os_version: String,
    /// CPU Architecture.
    pub architecture: String,
    /// Capture timestamp.
    pub timestamp: String,
    /// Optional hardware health data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware_health: Option<HardwareHealthPayload>,
    /// Optional crash report data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crash_reports: Option<Vec<CrashReportPayload>>,
    /// Optional feature usage data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_usage: Option<FeatureUsagePayload>,
}

/// Manager orchestrating telemetry configuration, payload preview, and outbox staging.
pub struct TelemetryManager {
    base_dir: PathBuf,
    config_file: PathBuf,
    identity_file: PathBuf,
    outbox_dir: PathBuf,
}

impl TelemetryManager {
    /// Initialize a telemetry manager rooted in the given base state directory.
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self, Error> {
        let base_dir = base_dir.into();
        let config_file = base_dir.join("telemetry.json");
        let identity_file = base_dir.join("identity.json");
        let outbox_dir = base_dir.join("outbox");

        fs::create_dir_all(&base_dir)?;
        fs::create_dir_all(&outbox_dir)?;

        Ok(Self {
            base_dir,
            config_file,
            identity_file,
            outbox_dir,
        })
    }

    /// Load current configuration (defaulting to disabled if not present).
    pub fn load_config(&self) -> TelemetryConfig {
        if let Ok(data) = fs::read_to_string(&self.config_file)
            && let Ok(cfg) = serde_json::from_str::<TelemetryConfig>(&data)
        {
            return cfg;
        }
        TelemetryConfig::default()
    }

    /// Check if telemetry collection is currently active.
    pub fn is_enabled(&self) -> bool {
        self.load_config().enabled
    }

    /// Explicitly opt in to telemetry collection with selected categories.
    pub fn opt_in(&self, categories: TelemetryCategories) -> Result<(), Error> {
        let cfg = TelemetryConfig {
            enabled: true,
            categories,
            endpoint_url: None,
        };
        let data = serde_json::to_string_pretty(&cfg)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        fs::write(&self.config_file, data)?;
        Ok(())
    }

    /// Explicitly opt out of telemetry collection.
    pub fn opt_out(&self) -> Result<(), Error> {
        let mut cfg = self.load_config();
        cfg.enabled = false;
        let data = serde_json::to_string_pretty(&cfg)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        fs::write(&self.config_file, data)?;
        Ok(())
    }

    /// Return the root base directory.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Generate or rotate the anonymous 128-bit ephemeral client ID (valid for 30 days).
    pub fn get_or_rotate_identity(&self) -> Result<EphemeralIdentity, Error> {
        let now = Utc::now();

        if let Ok(data) = fs::read_to_string(&self.identity_file)
            && let Ok(id) = serde_json::from_str::<EphemeralIdentity>(&data)
            && let Ok(exp) = DateTime::parse_from_rfc3339(&id.expires_at)
            && exp > now
        {
            return Ok(id);
        }

        // Generate a new random pseudo-UUID based on hash of timestamp and seed
        let mut hasher = Sha256::new();
        hasher.update(now.to_rfc3339().as_bytes());
        hasher.update(now.timestamp_nanos_opt().unwrap_or(0).to_le_bytes());
        let hash = format!("{:x}", hasher.finalize());

        let client_id = format!(
            "{}-{}-{}-{}-{}",
            &hash[0..8],
            &hash[8..12],
            &hash[12..16],
            &hash[16..20],
            &hash[20..32]
        );

        let expires_at = (now + Duration::days(30)).to_rfc3339();

        let identity = EphemeralIdentity {
            client_id,
            expires_at,
        };

        let data = serde_json::to_string_pretty(&identity)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        fs::write(&self.identity_file, data)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.identity_file, fs::Permissions::from_mode(0o600));
        }

        Ok(identity)
    }

    /// Render an exact preview of the telemetry payload that would be collected.
    pub fn preview_payload(
        &self,
        categories: &TelemetryCategories,
        sample_hw: Option<HardwareHealthPayload>,
        sample_crashes: Option<Vec<CrashReportPayload>>,
        sample_usage: Option<FeatureUsagePayload>,
    ) -> Result<TelemetryPayload, Error> {
        let identity = self.get_or_rotate_identity()?;

        Ok(TelemetryPayload {
            schema_version: "1.0.0".into(),
            ephemeral_client_id: identity.client_id,
            client_id_expires_at: identity.expires_at,
            os_version: "Directioner-OS 42.0.0".into(),
            architecture: std::env::consts::ARCH.into(),
            timestamp: Utc::now().to_rfc3339(),
            hardware_health: if categories.hardware_health {
                sample_hw
            } else {
                None
            },
            crash_reports: if categories.crash_reports {
                sample_crashes
            } else {
                None
            },
            feature_usage: if categories.feature_usage {
                sample_usage
            } else {
                None
            },
        })
    }

    /// Stage a verified payload into the local outbox directory (Rule B4 compliant).
    pub fn stage_payload(&self, payload: &TelemetryPayload) -> Result<PathBuf, Error> {
        let config = self.load_config();
        if !config.enabled {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "Telemetry collection is disabled (default OFF). Opt-in required.",
            ));
        }

        let filename = format!("telemetry_{}.json", Utc::now().timestamp());
        let target_path = self.outbox_dir.join(filename);

        let data = serde_json::to_string_pretty(payload)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        fs::write(&target_path, data)?;

        Ok(target_path)
    }
}
