use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

pub struct HardwarePowerManager {
    current_profile: PowerProfile,
    battery_charge_limit_percent: u32,
    thermal_throttled: bool,
}

impl HardwarePowerManager {
    pub fn new() -> Self {
        HardwarePowerManager {
            current_profile: PowerProfile::Balanced,
            battery_charge_limit_percent: 80,
            thermal_throttled: false,
        }
    }

    pub fn set_profile(&mut self, profile: PowerProfile) {
        self.current_profile = profile;
    }

    pub fn get_profile(&self) -> PowerProfile {
        self.current_profile
    }

    pub fn set_battery_charge_limit(&mut self, limit: u32) -> Result<(), String> {
        if (50..=100).contains(&limit) {
            self.battery_charge_limit_percent = limit;
            Ok(())
        } else {
            Err("limit must be between 50 and 100".to_string())
        }
    }

    pub fn get_battery_charge_limit(&self) -> u32 {
        self.battery_charge_limit_percent
    }

    pub fn set_thermal_throttle(&mut self, throttled: bool) {
        self.thermal_throttled = throttled;
    }

    pub fn is_thermal_throttled(&self) -> bool {
        self.thermal_throttled
    }
}
