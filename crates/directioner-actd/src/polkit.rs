/// Polkit action mapping table for Directioner OS actions (U073)
pub struct PolkitMap;

impl PolkitMap {
    /// Return the corresponding PolicyKit action ID for a capability, if required
    pub fn action_for_capability(capability_id: &str) -> Option<&'static str> {
        match capability_id {
            "power.system.reboot" => Some("org.freedesktop.login1.reboot"),
            "power.system.shutdown" => Some("org.freedesktop.login1.power-off"),
            "power.system.suspend" => Some("org.freedesktop.login1.suspend"),
            "network.wifi.connect" => Some("org.freedesktop.NetworkManager.network-control"),
            "network.connection.modify" => {
                Some("org.freedesktop.NetworkManager.settings.modify.system")
            }
            "system.package.install" => Some("org.freedesktop.packagekit.package-install"),
            "system.service.restart" => Some("org.freedesktop.systemd1.manage-units"),
            _ => None,
        }
    }

    /// Check if capability requires elevated administrative consent
    pub fn requires_privilege(capability_id: &str) -> bool {
        Self::action_for_capability(capability_id).is_some()
    }
}
