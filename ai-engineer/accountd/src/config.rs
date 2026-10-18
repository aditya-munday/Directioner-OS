use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformConfig {
    pub base_url: String,
    pub region: String,
    pub channel: String,
    pub require_https: bool,
    pub poll_interval_secs: u64,
    pub clock_skew_tolerance_secs: u64,
}

impl Default for PlatformConfig {
    fn default() -> Self {
        PlatformConfig {
            base_url: "https://auth.directioner.internal".to_string(),
            region: "global".to_string(),
            channel: "stable".to_string(),
            require_https: true,
            poll_interval_secs: 5,
            clock_skew_tolerance_secs: 60,
        }
    }
}

impl PlatformConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::from_str(&content)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(content: &str) -> Result<Self, String> {
        let cfg: PlatformConfig = toml::from_str(content).map_err(|e| e.to_string())?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.base_url.is_empty() {
            return Err("base_url cannot be empty".to_string());
        }
        if self.require_https && !self.base_url.starts_with("https://") {
            return Err("base_url must use https scheme".to_string());
        }
        if self.poll_interval_secs == 0 {
            return Err("poll_interval_secs must be greater than zero".to_string());
        }
        Ok(())
    }
}

impl std::str::FromStr for PlatformConfig {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s)
    }
}
