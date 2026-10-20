use serde::{Deserialize, Serialize};

/// Risk classification of a planned action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRisk {
    /// Safe, minimal impact
    Low,
    /// Moderate change, affects current user session
    Medium,
    /// High impact or privileged mutation
    High,
}

/// Dry-run preview plan generated prior to execution (U074)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewPlan {
    /// Target capability identifier
    pub capability_id: String,
    /// Proposed parameters
    pub params: serde_json::Value,
    /// Plain-English explanation of what will change
    pub impact_summary: String,
    /// Existing state before mutation (if available)
    pub current_state: Option<serde_json::Value>,
    /// State that will result after mutation
    pub projected_state: Option<serde_json::Value>,
    /// Indicates whether action can be automatically undone
    pub reversible: bool,
    /// Indicates whether action will trigger polkit prompt
    pub requires_polkit: bool,
    /// Assessed risk level
    pub risk: ActionRisk,
}

/// Engine generating previews for proposed actions (U074)
pub struct PreviewEngine;

impl PreviewEngine {
    /// Generate a dry-run preview for an action
    pub fn generate_preview(
        capability_id: &str,
        params: &serde_json::Value,
        current_state: Option<serde_json::Value>,
    ) -> PreviewPlan {
        let (impact, reversible, polkit, risk) = match capability_id {
            "audio.volume.set" => {
                let vol = params
                    .get("volume_percent")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(50);
                (
                    format!("Change master audio output volume to {vol}%"),
                    true,
                    false,
                    ActionRisk::Low,
                )
            }
            "audio.mute.toggle" => (
                "Toggle master audio output mute state".to_string(),
                true,
                false,
                ActionRisk::Low,
            ),
            "network.wifi.connect" => {
                let ssid = params
                    .get("ssid")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                (
                    format!("Connect wireless interface to network '{ssid}'"),
                    true,
                    false,
                    ActionRisk::Medium,
                )
            }
            "display.brightness.set" => {
                let b = params
                    .get("brightness_percent")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(100);
                (
                    format!("Set screen brightness to {b}%"),
                    true,
                    false,
                    ActionRisk::Low,
                )
            }
            "display.nightcolor.toggle" => (
                "Toggle blue-light night color filter".to_string(),
                true,
                false,
                ActionRisk::Low,
            ),
            "power.system.reboot" => (
                "Reboot the operating system cleanly".to_string(),
                false,
                true,
                ActionRisk::High,
            ),
            "power.system.shutdown" => (
                "Power down hardware cleanly".to_string(),
                false,
                true,
                ActionRisk::High,
            ),
            other => (
                format!("Execute user-scope action '{other}'"),
                true,
                false,
                ActionRisk::Medium,
            ),
        };

        PreviewPlan {
            capability_id: capability_id.to_string(),
            params: params.clone(),
            impact_summary: impact,
            current_state,
            projected_state: Some(params.clone()),
            reversible,
            requires_polkit: polkit,
            risk,
        }
    }
}
