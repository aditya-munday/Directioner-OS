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
pub struct TimerRecord {
    pub id: String,
    pub label: String,
    pub duration_secs: u64,
    pub remaining_secs: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionAuditEvent {
    pub action_name: String,
    pub caller_uid: u32,
    pub success: bool,
    pub timestamp_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySettings {
    pub high_contrast: bool,
    pub screen_reader_active: bool,
    pub large_text: bool,
    pub animations_enabled: bool,
    pub font_scale: f32,
    pub cursor_scale: f32,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        AccessibilitySettings {
            high_contrast: false,
            screen_reader_active: false,
            large_text: false,
            animations_enabled: true,
            font_scale: 1.0,
            cursor_scale: 1.0,
        }
    }
}

pub struct SystemActionsManager {
    alarms: HashMap<String, AlarmRecord>,
    timers: HashMap<String, TimerRecord>,
    settings: HashMap<String, String>,
    audit_events: Vec<ActionAuditEvent>,
    pub a11y: AccessibilitySettings,
}

impl Default for SystemActionsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemActionsManager {
    pub fn new() -> Self {
        let mut mgr = SystemActionsManager {
            alarms: HashMap::new(),
            timers: HashMap::new(),
            settings: HashMap::new(),
            audit_events: Vec::new(),
            a11y: AccessibilitySettings::default(),
        };
        mgr.settings.insert("volume".to_string(), "70".to_string());
        mgr.settings
            .insert("brightness".to_string(), "80".to_string());
        mgr.settings
            .insert("wifi".to_string(), "enabled".to_string());
        mgr.settings
            .insert("bluetooth".to_string(), "enabled".to_string());
        mgr.settings
            .insert("night_light".to_string(), "disabled".to_string());
        mgr
    }

    pub fn set_alarm(&mut self, id: &str, label: &str, target_epoch: i64) {
        self.alarms.insert(
            id.to_string(),
            AlarmRecord {
                id: id.to_string(),
                label: label.to_string(),
                target_epoch,
                enabled: true,
            },
        );
        self.record_audit("set_alarm", 1000, true, target_epoch);
    }

    pub fn cancel_alarm(&mut self, id: &str) -> bool {
        let removed = self.alarms.remove(id).is_some();
        self.record_audit("cancel_alarm", 1000, removed, 0);
        removed
    }

    pub fn list_alarms(&self) -> Vec<AlarmRecord> {
        self.alarms.values().cloned().collect()
    }

    pub fn set_timer(&mut self, id: &str, label: &str, duration_secs: u64) {
        self.timers.insert(
            id.to_string(),
            TimerRecord {
                id: id.to_string(),
                label: label.to_string(),
                duration_secs,
                remaining_secs: duration_secs,
                active: true,
            },
        );
        self.record_audit("set_timer", 1000, true, 0);
    }

    pub fn cancel_timer(&mut self, id: &str) -> bool {
        self.timers.remove(id).is_some()
    }

    pub fn tick_timers(&mut self, elapsed_secs: u64) -> Vec<String> {
        let mut completed = Vec::new();
        for timer in self.timers.values_mut() {
            if timer.active {
                if timer.remaining_secs <= elapsed_secs {
                    timer.remaining_secs = 0;
                    timer.active = false;
                    completed.push(timer.id.clone());
                } else {
                    timer.remaining_secs -= elapsed_secs;
                }
            }
        }
        completed
    }

    pub fn set_system_setting(&mut self, key: &str, value: &str) -> Result<(), String> {
        if key == "volume" || key == "brightness" {
            let val: u32 = value
                .parse()
                .map_err(|_| "must be integer 0..100".to_string())?;
            if val > 100 {
                return Err("value out of range 0..100".to_string());
            }
        }
        self.settings.insert(key.to_string(), value.to_string());
        self.record_audit(&format!("set_setting:{}", key), 1000, true, 0);
        Ok(())
    }

    pub fn get_system_setting(&self, key: &str) -> Option<&String> {
        self.settings.get(key)
    }

    pub fn toggle_high_contrast(&mut self) -> bool {
        self.a11y.high_contrast = !self.a11y.high_contrast;
        self.record_audit("toggle_high_contrast", 1000, true, 0);
        self.a11y.high_contrast
    }

    pub fn set_font_scale(&mut self, scale: f32) -> Result<(), String> {
        if (0.5..=3.0).contains(&scale) {
            self.a11y.font_scale = scale;
            self.a11y.large_text = scale >= 1.25;
            Ok(())
        } else {
            Err("font scale must be between 0.5 and 3.0".to_string())
        }
    }

    pub fn audit_log(&self) -> &[ActionAuditEvent] {
        &self.audit_events
    }

    fn record_audit(&mut self, action: &str, uid: u32, success: bool, epoch: i64) {
        self.audit_events.push(ActionAuditEvent {
            action_name: action.to_string(),
            caller_uid: uid,
            success,
            timestamp_epoch: epoch,
        });
    }
}
