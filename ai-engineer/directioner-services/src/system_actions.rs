use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmRecord {
    pub id: String,
    pub label: String,
    pub target_epoch: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySettings {
    pub high_contrast: bool,
    pub screen_reader_active: bool,
    pub large_text: bool,
    pub animations_enabled: bool,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        AccessibilitySettings {
            high_contrast: false,
            screen_reader_active: false,
            large_text: false,
            animations_enabled: true,
        }
    }
}

pub struct SystemActionsManager {
    alarms: HashMap<String, AlarmRecord>,
    settings: HashMap<String, String>,
    pub a11y: AccessibilitySettings,
}

impl SystemActionsManager {
    pub fn new() -> Self {
        let mut mgr = SystemActionsManager {
            alarms: HashMap::new(),
            settings: HashMap::new(),
            a11y: AccessibilitySettings::default(),
        };
        mgr.settings.insert("volume".to_string(), "70".to_string());
        mgr.settings.insert("brightness".to_string(), "80".to_string());
        mgr.settings.insert("wifi".to_string(), "enabled".to_string());
        mgr.settings.insert("bluetooth".to_string(), "enabled".to_string());
        mgr
    }

    pub fn set_alarm(&mut self, id: &str, label: &str, target_epoch: i64) {
        self.alarms.insert(id.to_string(), AlarmRecord {
            id: id.to_string(),
            label: label.to_string(),
            target_epoch,
            enabled: true,
        });
    }

    pub fn cancel_alarm(&mut self, id: &str) -> bool {
        self.alarms.remove(id).is_some()
    }

    pub fn list_alarms(&self) -> Vec<AlarmRecord> {
        self.alarms.values().cloned().collect()
    }

    pub fn set_system_setting(&mut self, key: &str, value: &str) -> Result<(), String> {
        if key == "volume" || key == "brightness" {
            let val: u32 = value.parse().map_err(|_| "must be integer 0..100".to_string())?;
            if val > 100 {
                return Err("value out of range 0..100".to_string());
            }
        }
        self.settings.insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub fn get_system_setting(&self, key: &str) -> Option<&String> {
        self.settings.get(key)
    }

    pub fn toggle_high_contrast(&mut self) -> bool {
        self.a11y.high_contrast = !self.a11y.high_contrast;
        self.a11y.high_contrast
    }
}
