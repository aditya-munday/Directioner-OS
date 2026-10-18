use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AriaEdgeGlowState {
    Off,
    Listening,
    Thinking,
    Responding,
    Alert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopNotification {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub urgency: UrgencyLevel,
    pub timestamp_epoch: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrgencyLevel {
    Low,
    Normal,
    Critical,
}

pub struct DesktopCoordinator {
    pub theme: ThemeMode,
    pub glow_state: AriaEdgeGlowState,
    notifications: Vec<DesktopNotification>,
    next_notification_id: u64,
}

impl DesktopCoordinator {
    pub fn new() -> Self {
        DesktopCoordinator {
            theme: ThemeMode::Dark,
            glow_state: AriaEdgeGlowState::Off,
            notifications: Vec::new(),
            next_notification_id: 1,
        }
    }

    pub fn set_theme(&mut self, mode: ThemeMode) {
        self.theme = mode;
    }

    pub fn set_glow_state(&mut self, state: AriaEdgeGlowState) {
        self.glow_state = state;
    }

    pub fn post_notification(&mut self, title: &str, body: &str, urgency: UrgencyLevel, now: i64) -> u64 {
        let id = self.next_notification_id;
        self.next_notification_id += 1;

        let n = DesktopNotification {
            id,
            title: title.to_string(),
            body: body.to_string(),
            urgency,
            timestamp_epoch: now,
        };
        self.notifications.push(n);
        id
    }

    pub fn get_notifications(&self) -> &[DesktopNotification] {
        &self.notifications
    }

    pub fn clear_notification(&mut self, id: u64) -> bool {
        let prev = self.notifications.len();
        self.notifications.retain(|n| n.id != id);
        self.notifications.len() < prev
    }
}
