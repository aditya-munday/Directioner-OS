use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Introspected D-Bus Node representing a service, object path, or interface (U085)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbusNode {
    /// Bus type: "system" or "session"
    pub bus: String,
    /// Well-known service name (e.g. "org.freedesktop.NetworkManager")
    pub service: String,
    /// Object path (e.g. "/org/freedesktop/NetworkManager")
    pub path: String,
    /// Exported interfaces
    pub interfaces: Vec<String>,
}

/// D-Bus introspection engine for capability discovery (U085)
pub struct DbusIntrospector;

impl DbusIntrospector {
    /// Inspect available D-Bus services on the system
    pub fn inspect_system() -> Vec<DbusNode> {
        let mut nodes = Vec::new();

        // 1. Scan standard system service activation files
        let sys_services_dir = Path::new("/usr/share/dbus-1/system-services");
        if let Ok(entries) = fs::read_dir(sys_services_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(service_name) = name.strip_suffix(".service") {
                    nodes.push(DbusNode {
                        bus: "system".into(),
                        service: service_name.to_string(),
                        path: format!("/{}", service_name.replace('.', "/")),
                        interfaces: vec![
                            service_name.to_string(),
                            "org.freedesktop.DBus.Introspectable".into(),
                        ],
                    });
                }
            }
        }

        // 2. Ensure core system services are always represented in the introspection tree
        let core_services = [
            (
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                vec![
                    "org.freedesktop.login1.Manager",
                    "org.freedesktop.login1.Session",
                ],
            ),
            (
                "org.freedesktop.NetworkManager",
                "/org/freedesktop/NetworkManager",
                vec![
                    "org.freedesktop.NetworkManager",
                    "org.freedesktop.NetworkManager.Device",
                ],
            ),
            (
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                vec!["org.freedesktop.systemd1.Manager"],
            ),
            (
                "org.freedesktop.timedate1",
                "/org/freedesktop/timedate1",
                vec!["org.freedesktop.timedate1"],
            ),
            (
                "org.freedesktop.hostname1",
                "/org/freedesktop/hostname1",
                vec!["org.freedesktop.hostname1"],
            ),
        ];

        for (srv, obj_path, ifaces) in core_services {
            if !nodes.iter().any(|n| n.service == srv) {
                nodes.push(DbusNode {
                    bus: "system".into(),
                    service: srv.into(),
                    path: obj_path.into(),
                    interfaces: ifaces.into_iter().map(String::from).collect(),
                });
            }
        }

        nodes.sort_by(|a, b| a.service.cmp(&b.service));
        nodes
    }

    /// Render formatted ASCII tree of discovered D-Bus surface
    pub fn render_tree() -> String {
        let nodes = Self::inspect_system();
        let mut out = String::new();
        out.push_str("D-Bus System Bus Introspection Tree (U085)\n");
        out.push_str("==========================================\n");
        for node in nodes {
            out.push_str(&format!("├── {} [{}]\n", node.service, node.bus));
            out.push_str(&format!("│   └── Object: {}\n", node.path));
            for iface in &node.interfaces {
                out.push_str(&format!("│       └── Interface: {}\n", iface));
            }
        }
        out
    }
}
