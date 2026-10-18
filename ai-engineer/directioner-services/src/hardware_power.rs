// IN-MEMORY MODEL — no real OS integration. No sysfs, TPM, bootc, or kernel calls. Simulation only.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
    UltraSaver,
    GamingBoost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerSource {
    Ac,
    Battery,
    UsbPd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThermalSeverity {
    Normal,
    Elevated,
    Throttled,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalZoneReading {
    pub zone: String,
    pub celsius: f32,
    pub recorded_epoch: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepInhibitionLease {
    pub lease_id: String,
    pub caller_id: String,
    pub reason: String,
    pub expires_epoch: i64,
}

pub struct HardwarePowerManager {
    current_profile: PowerProfile,
    battery_charge_limit_percent: u32,
    thermal_throttled: bool,
    power_source: PowerSource,
    battery_health_percentage: u32,
    battery_cycle_count: u32,
    brightness_percent: u32,
    thermal_readings: HashMap<String, ThermalZoneReading>,
    inhibition_leases: HashMap<String, SleepInhibitionLease>,
    next_lease_seq: u64,
}

impl Default for HardwarePowerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwarePowerManager {
    pub fn new() -> Self {
        HardwarePowerManager {
            current_profile: PowerProfile::Balanced,
            battery_charge_limit_percent: 80,
            thermal_throttled: false,
            power_source: PowerSource::Ac,
            battery_health_percentage: 100,
            battery_cycle_count: 12,
            brightness_percent: 75,
            thermal_readings: HashMap::new(),
            inhibition_leases: HashMap::new(),
            next_lease_seq: 1,
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

    pub fn set_brightness(&mut self, percent: u32) -> Result<(), String> {
        if percent <= 100 {
            self.brightness_percent = percent;
            Ok(())
        } else {
            Err("brightness percent must be between 0 and 100".to_string())
        }
    }

    pub fn get_brightness(&self) -> u32 {
        self.brightness_percent
    }

    pub fn record_thermal_reading(&mut self, zone: &str, celsius: f32, epoch: i64) {
        self.thermal_readings.insert(
            zone.to_string(),
            ThermalZoneReading {
                zone: zone.to_string(),
                celsius,
                recorded_epoch: epoch,
            },
        );
        let max_temp = self.highest_temperature().unwrap_or(0.0);
        self.thermal_throttled = max_temp >= 85.0;
    }

    pub fn highest_temperature(&self) -> Option<f32> {
        self.thermal_readings
            .values()
            .map(|r| r.celsius)
            .reduce(f32::max)
    }

    pub fn evaluate_thermal_severity(&self) -> ThermalSeverity {
        let max_temp = self.highest_temperature().unwrap_or(40.0);
        if max_temp >= 95.0 {
            ThermalSeverity::Critical
        } else if max_temp >= 85.0 {
            ThermalSeverity::Throttled
        } else if max_temp >= 70.0 {
            ThermalSeverity::Elevated
        } else {
            ThermalSeverity::Normal
        }
    }

    pub fn update_battery_telemetry(&mut self, health: u32, cycles: u32, source: PowerSource) {
        self.battery_health_percentage = health.min(100);
        self.battery_cycle_count = cycles;
        self.power_source = source;
    }

    pub fn get_power_source(&self) -> PowerSource {
        self.power_source
    }

    pub fn get_battery_health(&self) -> u32 {
        self.battery_health_percentage
    }

    pub fn get_battery_cycles(&self) -> u32 {
        self.battery_cycle_count
    }

    pub fn acquire_sleep_inhibition(
        &mut self,
        caller_id: &str,
        reason: &str,
        ttl_secs: u64,
        now: i64,
    ) -> String {
        let lease_id = format!("lease-{}-{}", caller_id, self.next_lease_seq);
        self.next_lease_seq = self.next_lease_seq.saturating_add(1);
        let lease = SleepInhibitionLease {
            lease_id: lease_id.clone(),
            caller_id: caller_id.to_string(),
            reason: reason.to_string(),
            expires_epoch: now.saturating_add(ttl_secs as i64),
        };
        self.inhibition_leases.insert(lease_id.clone(), lease);
        lease_id
    }

    pub fn release_sleep_inhibition(&mut self, lease_id: &str) -> bool {
        self.inhibition_leases.remove(lease_id).is_some()
    }

    pub fn is_sleep_inhibited(&mut self, now: i64) -> bool {
        self.inhibition_leases
            .retain(|_, lease| lease.expires_epoch > now);
        !self.inhibition_leases.is_empty()
    }

    pub fn active_inhibition_count(&self, now: i64) -> usize {
        self.inhibition_leases
            .values()
            .filter(|l| l.expires_epoch > now)
            .count()
    }
}
