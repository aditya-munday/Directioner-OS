use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkBlackholeClass {
    NoLink,
    NoRoute,
    NoDns,
    NoUpstream,
    Ok,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkBlackholeReport {
    pub link_state: NetworkLinkState,
    pub default_route: Option<String>,
    pub gateway_arp_reachable: Option<bool>,
    pub dns_resolver_reachable: Option<bool>,
    pub class: NetworkBlackholeClass,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkLinkState {
    Up,
    Down,
    Unknown,
}

pub fn check_link_state() -> NetworkLinkState {
    let path = Path::new("/sys/class/net");
    if !path.exists() {
        return NetworkLinkState::Unknown;
    }

    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return NetworkLinkState::Unknown,
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "lo" {
            continue;
        }
        let operstate_path = path.join(name_str.as_ref()).join("operstate");
        if let Ok(state) = std::fs::read_to_string(&operstate_path) {
            let state = state.trim();
            if state == "up" {
                return NetworkLinkState::Up;
            }
        }
    }
    NetworkLinkState::Down
}

pub fn read_default_route() -> Option<String> {
    let output = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()?;
    let out = String::from_utf8_lossy(&output.stdout);
    let line = out.lines().next()?;
    Some(line.trim().to_string())
}

fn extract_gateway(route: &str) -> Option<String> {
    let tokens: Vec<&str> = route.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        if *token == "via" {
            return tokens.get(i + 1).map(|s| s.to_string());
        }
    }
    None
}

pub fn check_gateway_arp(gateway: &str) -> bool {
    let output = Command::new("arping")
        .args(["-c", "1", "-w", "2", gateway])
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => {
            let ping_out = Command::new("ping")
                .args(["-c", "1", "-W", "2", gateway])
                .output();
            ping_out.map(|o| o.status.success()).unwrap_or(false)
        }
    }
}

pub fn check_dns_reachability(resolver_ip: &str) -> bool {
    let output = Command::new("dig")
        .args([
            "+short",
            "+timeout=2",
            "+tries=1",
            "@",
            resolver_ip,
            "localhost",
        ])
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => {
            let nc_out = Command::new("nc")
                .args(["-z", "-w", "2", resolver_ip, "53"])
                .output();
            nc_out.map(|o| o.status.success()).unwrap_or(false)
        }
    }
}

pub fn evaluate_network_signals(
    link_state: NetworkLinkState,
    default_route: Option<String>,
    gw_reachable: Option<bool>,
    dns_reachable: Option<bool>,
) -> NetworkBlackholeReport {
    if link_state == NetworkLinkState::Down || link_state == NetworkLinkState::Unknown {
        return NetworkBlackholeReport {
            link_state: NetworkLinkState::Down,
            default_route: None,
            gateway_arp_reachable: None,
            dns_resolver_reachable: None,
            class: NetworkBlackholeClass::NoLink,
            summary: "Network link is down — no physical carrier detected".to_string(),
        };
    }

    if default_route.is_none() || default_route.as_deref() == Some("") {
        return NetworkBlackholeReport {
            link_state,
            default_route: None,
            gateway_arp_reachable: None,
            dns_resolver_reachable: None,
            class: NetworkBlackholeClass::NoRoute,
            summary: "Link is up but no default route found".to_string(),
        };
    }

    let class = match (gw_reachable, dns_reachable) {
        (Some(false), _) => NetworkBlackholeClass::NoRoute,
        (_, Some(false)) => NetworkBlackholeClass::NoDns,
        (Some(true), Some(true)) => NetworkBlackholeClass::Ok,
        _ => NetworkBlackholeClass::Ok,
    };

    let summary = match &class {
        NetworkBlackholeClass::Ok => "Network connectivity checks passed".to_string(),
        NetworkBlackholeClass::NoRoute => "Gateway unreachable (ARP/ping failed)".to_string(),
        NetworkBlackholeClass::NoDns => "DNS resolver unreachable on port 53".to_string(),
        other => format!("{other:?}"),
    };

    NetworkBlackholeReport {
        link_state,
        default_route,
        gateway_arp_reachable: gw_reachable,
        dns_resolver_reachable: dns_reachable,
        class,
        summary,
    }
}

pub fn detect_network_blackhole() -> NetworkBlackholeReport {
    let link_state = check_link_state();
    if link_state == NetworkLinkState::Down || link_state == NetworkLinkState::Unknown {
        return evaluate_network_signals(link_state, None, None, None);
    }

    let default_route = read_default_route();
    if default_route.is_none() || default_route.as_deref() == Some("") {
        return evaluate_network_signals(link_state, None, None, None);
    }

    let route = default_route.clone().unwrap();
    let gateway = extract_gateway(&route);
    let gw_reachable = gateway.as_deref().map(check_gateway_arp);

    let resolv_conf = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let resolver_ip = resolv_conf
        .lines()
        .filter_map(|l| l.strip_prefix("nameserver "))
        .map(|s| s.trim().to_string())
        .next();
    let dns_reachable = resolver_ip.as_deref().map(check_dns_reachability);

    evaluate_network_signals(link_state, default_route, gw_reachable, dns_reachable)
}

pub fn detect_network_blackhole_from_netns(netns: &str) -> NetworkBlackholeReport {
    let in_ns_link = Command::new("ip")
        .args(["netns", "exec", netns, "ip", "link"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let link_state = if in_ns_link.contains("state UP") {
        NetworkLinkState::Up
    } else if in_ns_link.contains("state DOWN") {
        NetworkLinkState::Down
    } else {
        NetworkLinkState::Unknown
    };

    NetworkBlackholeReport {
        link_state: link_state.clone(),
        default_route: None,
        gateway_arp_reachable: None,
        dns_resolver_reachable: None,
        class: if link_state == NetworkLinkState::Down {
            NetworkBlackholeClass::NoLink
        } else {
            NetworkBlackholeClass::Ok
        },
        summary: format!("Network namespace {netns} link check complete"),
    }
}
