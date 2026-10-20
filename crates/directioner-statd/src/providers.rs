use crate::tree::SensitivityLevel;
use std::fs;
use std::path::Path;

/// Trait implemented by observe plane telemetry providers (U067)
pub trait StateProvider: Send + Sync {
    /// Unique provider name
    fn name(&self) -> &'static str;
    /// Collect current telemetry updates from the host environment
    fn poll(&mut self) -> Result<Vec<(String, serde_json::Value, SensitivityLevel)>, String>;
}

/// Provider reading host OS information, uptime, and kernel facts
pub struct SystemProvider;

impl StateProvider for SystemProvider {
    fn name(&self) -> &'static str {
        "system"
    }

    fn poll(&mut self) -> Result<Vec<(String, serde_json::Value, SensitivityLevel)>, String> {
        let mut updates = Vec::new();

        // 1. OS Release facts
        let os_release_path = Path::new("/etc/os-release");
        let mut os_id = "directioner".to_string();
        let mut os_version = "42".to_string();
        let mut pretty_name = "Directioner OS".to_string();

        if let Ok(content) = fs::read_to_string(os_release_path) {
            for line in content.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let clean_v = v.trim_matches('"');
                    match k.trim() {
                        "ID" => os_id = clean_v.to_string(),
                        "VERSION_ID" => os_version = clean_v.to_string(),
                        "PRETTY_NAME" => pretty_name = clean_v.to_string(),
                        _ => {}
                    }
                }
            }
        }

        updates.push((
            "system.os.info".into(),
            serde_json::json!({
                "id": os_id,
                "version": os_version,
                "pretty_name": pretty_name,
                "arch": std::env::consts::ARCH,
            }),
            SensitivityLevel::Public,
        ));

        // 2. Load average & Uptime
        if let Some(secs) = fs::read_to_string("/proc/uptime").ok().and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|f| f.parse::<f64>().ok())
        }) {
            updates.push((
                "system.uptime.seconds".into(),
                serde_json::json!(secs),
                SensitivityLevel::Public,
            ));
        }

        if let Ok(load_str) = fs::read_to_string("/proc/loadavg") {
            let parts: Vec<&str> = load_str.split_whitespace().collect();
            if parts.len() >= 3 {
                updates.push((
                    "system.load.average".into(),
                    serde_json::json!({
                        "1min": parts[0],
                        "5min": parts[1],
                        "15min": parts[2],
                    }),
                    SensitivityLevel::Public,
                ));
            }
        }

        Ok(updates)
    }
}

/// Provider reading power supply and battery telemetry from /sys/class/power_supply
pub struct PowerProvider;

impl StateProvider for PowerProvider {
    fn name(&self) -> &'static str {
        "power"
    }

    fn poll(&mut self) -> Result<Vec<(String, serde_json::Value, SensitivityLevel)>, String> {
        let mut updates = Vec::new();
        let psupply = Path::new("/sys/class/power_supply");

        let mut batteries = Vec::new();
        let mut ac_online = true;

        if let Ok(entries) = fs::read_dir(psupply) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                let type_path = path.join("type");

                if let Ok(supply_type) = fs::read_to_string(type_path) {
                    let st = supply_type.trim();
                    if st == "Mains" {
                        if let Ok(online_val) = fs::read_to_string(path.join("online")) {
                            ac_online = online_val.trim() == "1";
                        }
                    } else if st == "Battery" {
                        let cap = fs::read_to_string(path.join("capacity"))
                            .ok()
                            .and_then(|s| s.trim().parse::<u8>().ok())
                            .unwrap_or(100);
                        let status = fs::read_to_string(path.join("status"))
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|_| "Full".into());
                        batteries.push(serde_json::json!({
                            "device": name,
                            "capacity_percent": cap,
                            "status": status,
                        }));
                    }
                }
            }
        }

        updates.push((
            "power.supply.state".into(),
            serde_json::json!({
                "ac_online": ac_online,
                "batteries": batteries,
            }),
            SensitivityLevel::DeviceState,
        ));

        Ok(updates)
    }
}

/// Provider reading network interfaces and status from /sys/class/net
pub struct NetworkProvider;

impl StateProvider for NetworkProvider {
    fn name(&self) -> &'static str {
        "network"
    }

    fn poll(&mut self) -> Result<Vec<(String, serde_json::Value, SensitivityLevel)>, String> {
        let mut updates = Vec::new();
        let net_dir = Path::new("/sys/class/net");
        let mut interfaces = Vec::new();

        if let Ok(entries) = fs::read_dir(net_dir) {
            for entry in entries.flatten() {
                let ifname = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                let operstate = fs::read_to_string(path.join("operstate"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| "unknown".into());
                let is_up = operstate == "up";
                let is_wireless = path.join("wireless").exists() || path.join("phy80211").exists();

                interfaces.push(serde_json::json!({
                    "name": ifname,
                    "operstate": operstate,
                    "up": is_up,
                    "wireless": is_wireless,
                }));
            }
        }

        updates.push((
            "network.interfaces.list".into(),
            serde_json::json!(interfaces),
            SensitivityLevel::DeviceState,
        ));

        Ok(updates)
    }
}
