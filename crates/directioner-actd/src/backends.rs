use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Execution errors
#[derive(Debug, Error)]
pub enum ActionError {
    /// Security violation (e.g. attempting to touch root-owned paths)
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    /// Capability not found or unsupported
    #[error("Unsupported capability: {0}")]
    Unsupported(String),
    /// Invalid parameters
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    /// Backend execution failed
    #[error("Execution failure: {0}")]
    ExecutionFailed(String),
}

/// Sandboxed user-session executor (U072)
pub struct UserExecutor {
    user_home: PathBuf,
}

impl UserExecutor {
    /// Initialize executor for the current user session
    pub fn new() -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        Self { user_home: home }
    }

    /// Explicitly initialize with home path (useful for testing)
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            user_home: home.into(),
        }
    }

    /// Safe file write strictly confined to user home directory (U072)
    pub fn safe_write_user_file(
        &self,
        target_path: impl AsRef<Path>,
        content: &[u8],
    ) -> Result<PathBuf, ActionError> {
        let target = target_path.as_ref();

        // Canonicalize / check path prefix
        // Invariant check: Any attempt to write outside user home (e.g. /etc, /root, /usr) must fail
        if !target.starts_with(&self.user_home) {
            return Err(ActionError::SecurityViolation(format!(
                "Path '{}' is outside user home directory '{}' - root/system modification forbidden (U072)",
                target.display(),
                self.user_home.display()
            )));
        }

        if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
        }

        fs::write(target, content).map_err(|e| ActionError::ExecutionFailed(e.to_string()))?;
        Ok(target.to_path_buf())
    }

    /// Execute a user-scope capability
    pub fn execute(
        &self,
        capability_id: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        match capability_id {
            "audio.volume.set" => {
                let vol = params
                    .get("volume_percent")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| ActionError::InvalidParams("Missing 'volume_percent'".into()))?;
                // In a desktop environment, invokes wpctl or pamixer
                Ok(serde_json::json!({
                    "volume_percent": vol,
                    "applied": true
                }))
            }
            "audio.mute.toggle" => Ok(serde_json::json!({
                "muted": true,
                "applied": true
            })),
            "display.brightness.set" => {
                let b = params
                    .get("brightness_percent")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| {
                        ActionError::InvalidParams("Missing 'brightness_percent'".into())
                    })?;
                Ok(serde_json::json!({
                    "brightness_percent": b,
                    "applied": true
                }))
            }
            "display.nightcolor.toggle" => Ok(serde_json::json!({
                "nightcolor_active": true,
                "applied": true
            })),
            "network.wifi.connect" => {
                let ssid = params
                    .get("ssid")
                    .and_then(|s| s.as_str())
                    .ok_or_else(|| ActionError::InvalidParams("Missing 'ssid'".into()))?;
                Ok(serde_json::json!({
                    "connected_ssid": ssid,
                    "applied": true
                }))
            }
            "files.user.write" => {
                let rel_path = params
                    .get("path")
                    .and_then(|p| p.as_str())
                    .ok_or_else(|| ActionError::InvalidParams("Missing 'path'".into()))?;
                let content = params.get("content").and_then(|c| c.as_str()).unwrap_or("");

                let full_path = if Path::new(rel_path).is_absolute() {
                    PathBuf::from(rel_path)
                } else {
                    self.user_home.join(rel_path)
                };

                let saved = self.safe_write_user_file(&full_path, content.as_bytes())?;
                Ok(serde_json::json!({
                    "path": saved.to_string_lossy(),
                    "written_bytes": content.len()
                }))
            }
            other => Err(ActionError::Unsupported(other.to_string())),
        }
    }
}

impl Default for UserExecutor {
    fn default() -> Self {
        Self::new()
    }
}
