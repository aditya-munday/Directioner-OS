// IN-MEMORY MODEL — no real OS integration. No sysfs, TPM, bootc, or kernel calls. Simulation only.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootSlot {
    SlotA,
    SlotB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentState {
    Idle,
    Downloading,
    Staged,
    Testing,
    Committed,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelKind {
    Stable,
    Beta,
    Nightly,
}

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
    active_slot: BootSlot,
    staged_slot: Option<BootSlot>,
    state: DeploymentState,
    pub boot_counter: u32,
    pub max_boot_attempts: u32,
    pub rolled_back: bool,
    pinned_deployments: Vec<String>,
}

impl UpdateController {
    pub fn new(current_version: &str) -> Self {
        UpdateController {
            current_version: current_version.to_string(),
            active_slot: BootSlot::SlotA,
            staged_slot: None,
            state: DeploymentState::Idle,
            boot_counter: 3,
            max_boot_attempts: 3,
            rolled_back: false,
            pinned_deployments: Vec::new(),
        }
    }

    pub fn validate_manifest(&self, manifest: &ReleaseManifest) -> Result<(), String> {
        if manifest.image_digest.is_empty() {
            return Err("missing_digest".to_string());
        }
        if manifest.os_version == self.current_version {
            return Err("already_installed".to_string());
        }
        if manifest.channel != "stable"
            && manifest.channel != "beta"
            && manifest.channel != "nightly"
        {
            return Err("invalid_channel".to_string());
        }
        Ok(())
    }

    pub fn stage_update(&mut self, manifest: &ReleaseManifest) -> Result<BootSlot, String> {
        self.validate_manifest(manifest)?;
        let target_slot = match self.active_slot {
            BootSlot::SlotA => BootSlot::SlotB,
            BootSlot::SlotB => BootSlot::SlotA,
        };
        self.staged_slot = Some(target_slot);
        self.state = DeploymentState::Staged;
        self.boot_counter = self.max_boot_attempts;
        self.rolled_back = false;
        Ok(target_slot)
    }

    pub fn record_boot_failure(&mut self) -> bool {
        if self.boot_counter > 0 {
            self.boot_counter -= 1;
        }
        if self.boot_counter == 0 {
            self.rolled_back = true;
            self.state = DeploymentState::RolledBack;
            self.staged_slot = None;
            true
        } else {
            false
        }
    }

    pub fn commit_update(&mut self) {
        if let Some(staged) = self.staged_slot.take() {
            self.active_slot = staged;
        }
        self.boot_counter = self.max_boot_attempts;
        self.rolled_back = false;
        self.state = DeploymentState::Committed;
    }

    pub fn pin_deployment(&mut self, deployment_id: &str) {
        if !self.pinned_deployments.iter().any(|d| d == deployment_id) {
            self.pinned_deployments.push(deployment_id.to_string());
        }
    }

    pub fn unpin_deployment(&mut self, deployment_id: &str) -> bool {
        let prev = self.pinned_deployments.len();
        self.pinned_deployments.retain(|d| d != deployment_id);
        self.pinned_deployments.len() < prev
    }

    pub fn is_pinned(&self, deployment_id: &str) -> bool {
        self.pinned_deployments.iter().any(|d| d == deployment_id)
    }

    pub fn active_slot(&self) -> BootSlot {
        self.active_slot
    }

    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    pub fn deployment_state(&self) -> DeploymentState {
        self.state
    }
}
