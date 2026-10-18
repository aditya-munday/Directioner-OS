use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub os_version: String,
    pub image_digest: String,
    pub build_id: String,
    pub channel: String,
    pub min_upgrade_version: String,
    pub changelog_summary: String,
}

pub struct UpdateController {
    current_version: String,
    pub boot_counter: u32,
    pub max_boot_attempts: u32,
    pub rolled_back: bool,
}

impl UpdateController {
    pub fn new(current_version: &str) -> Self {
        UpdateController {
            current_version: current_version.to_string(),
            boot_counter: 3,
            max_boot_attempts: 3,
            rolled_back: false,
        }
    }

    pub fn validate_manifest(&self, manifest: &ReleaseManifest) -> Result<(), String> {
        if manifest.image_digest.is_empty() {
            return Err("missing_digest".to_string());
        }
        if manifest.os_version == self.current_version {
            return Err("already_installed".to_string());
        }
        Ok(())
    }

    pub fn record_boot_failure(&mut self) -> bool {
        if self.boot_counter > 0 {
            self.boot_counter -= 1;
        }
        if self.boot_counter == 0 {
            self.rolled_back = true;
            true
        } else {
            false
        }
    }

    pub fn commit_update(&mut self) {
        self.boot_counter = self.max_boot_attempts;
        self.rolled_back = false;
    }
}
