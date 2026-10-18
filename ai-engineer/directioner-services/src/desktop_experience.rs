use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
    HighContrast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AriaEdgeGlowState {
    Off,
    Listening,
    Thinking,
    Responding,
    Alert,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlowParameters {
    pub color_hex: String,
    pub pulse_rate_hz: f32,
    pub intensity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrgencyLevel {
    Low,
    Normal,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationCategory {
    System,
    Security,
    AriaAssistant,
    Application,
    Update,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopNotification {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub urgency: UrgencyLevel,
    pub category: NotificationCategory,
    pub timestamp_epoch: i64,
    pub action_label: Option<String>,
}

pub struct DesktopCoordinator {
    pub theme: ThemeMode,
    pub glow_state: AriaEdgeGlowState,
    notifications: Vec<DesktopNotification>,
    next_notification_id: u64,
    dnd_enabled: bool,
    active_workspace: u32,
    total_workspaces: u32,
}

impl Default for DesktopCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopCoordinator {
    pub fn new() -> Self {
        DesktopCoordinator {
            theme: ThemeMode::Dark,
            glow_state: AriaEdgeGlowState::Off,
            notifications: Vec::new(),
            next_notification_id: 1,
            dnd_enabled: false,
            active_workspace: 0,
            total_workspaces: 4,
        }
    }

    pub fn set_theme(&mut self, mode: ThemeMode) {
        self.theme = mode;
    }

    pub fn set_glow_state(&mut self, state: AriaEdgeGlowState) {
        self.glow_state = state;
    }

    pub fn glow_parameters(&self) -> GlowParameters {
        match self.glow_state {
            AriaEdgeGlowState::Off => GlowParameters {
                color_hex: "#00000000".to_string(),
                pulse_rate_hz: 0.0,
                intensity: 0.0,
            },
            AriaEdgeGlowState::Listening => GlowParameters {
                color_hex: "#38bdf8".to_string(),
                pulse_rate_hz: 0.8,
                intensity: 0.6,
            },
            AriaEdgeGlowState::Thinking => GlowParameters {
                color_hex: "#818cf8".to_string(),
                pulse_rate_hz: 1.5,
                intensity: 0.8,
            },
            AriaEdgeGlowState::Responding => GlowParameters {
                color_hex: "#34d399".to_string(),
                pulse_rate_hz: 0.5,
                intensity: 0.7,
            },
            AriaEdgeGlowState::Alert => GlowParameters {
                color_hex: "#fbbf24".to_string(),
                pulse_rate_hz: 2.0,
                intensity: 0.9,
            },
            AriaEdgeGlowState::Error => GlowParameters {
                color_hex: "#f87171".to_string(),
                pulse_rate_hz: 3.0,
                intensity: 1.0,
            },
        }
    }

    pub fn set_do_not_disturb(&mut self, enabled: bool) {
        self.dnd_enabled = enabled;
    }

    pub fn is_do_not_disturb(&self) -> bool {
        self.dnd_enabled
    }

    pub fn post_notification(
        &mut self,
        title: &str,
        body: &str,
        urgency: UrgencyLevel,
        now: i64,
    ) -> u64 {
        self.post_notification_with_category(
            title,
            body,
            urgency,
            NotificationCategory::System,
            None,
            now,
        )
    }

    pub fn post_notification_with_category(
        &mut self,
        title: &str,
        body: &str,
        urgency: UrgencyLevel,
        category: NotificationCategory,
        action_label: Option<&str>,
        now: i64,
    ) -> u64 {
        let id = self.next_notification_id;
        self.next_notification_id += 1;

        let n = DesktopNotification {
            id,
            title: title.to_string(),
            body: body.to_string(),
            urgency,
            category,
            timestamp_epoch: now,
            action_label: action_label.map(|s| s.to_string()),
        };
        self.notifications.push(n);
        id
    }

    pub fn get_notifications(&self) -> &[DesktopNotification] {
        &self.notifications
    }

    pub fn get_visible_notifications(&self) -> Vec<DesktopNotification> {
        if self.dnd_enabled {
            self.notifications
                .iter()
                .filter(|n| n.urgency == UrgencyLevel::Critical)
                .cloned()
                .collect()
        } else {
            self.notifications.clone()
        }
    }

    pub fn clear_notification(&mut self, id: u64) -> bool {
        let prev = self.notifications.len();
        self.notifications.retain(|n| n.id != id);
        self.notifications.len() < prev
    }

    pub fn switch_workspace(&mut self, idx: u32) -> Result<(), String> {
        if idx < self.total_workspaces {
            self.active_workspace = idx;
            Ok(())
        } else {
            Err(format!(
                "workspace index {} exceeds total {}",
                idx, self.total_workspaces
            ))
        }
    }

    pub fn active_workspace(&self) -> u32 {
        self.active_workspace
    }

    pub fn render_theme_css_variables(&self) -> String {
        match self.theme {
            ThemeMode::Light => {
                ":root { --bg-primary: #f8fafc; --text-primary: #0f172a; --accent: #6366f1; }"
                    .to_string()
            }
            ThemeMode::Dark | ThemeMode::Auto => {
                ":root { --bg-primary: #0b0f19; --text-primary: #f1f5f9; --accent: #818cf8; }"
                    .to_string()
            }
            ThemeMode::HighContrast => {
                ":root { --bg-primary: #000000; --text-primary: #ffffff; --accent: #ffff00; }"
                    .to_string()
            }
        }
    }
}
