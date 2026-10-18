use control_plane::network_blackhole::{
    evaluate_network_signals, NetworkBlackholeClass, NetworkLinkState,
};

#[test]
fn test_blackhole_no_operstate_up_reports_no_link() {
    let report = evaluate_network_signals(NetworkLinkState::Down, None, None, None);
    assert_eq!(report.class, NetworkBlackholeClass::NoLink);
    assert_eq!(report.link_state, NetworkLinkState::Down);
}

#[test]
fn test_blackhole_no_default_route() {
    let report = evaluate_network_signals(NetworkLinkState::Up, None, None, None);
    assert_eq!(report.class, NetworkBlackholeClass::NoRoute);
    assert_eq!(report.link_state, NetworkLinkState::Up);
    assert!(report.summary.contains("no default route"));
}

#[test]
fn test_blackhole_no_dns() {
    let report = evaluate_network_signals(
        NetworkLinkState::Up,
        Some("default via 192.168.1.1 dev eth0".to_string()),
        Some(true),
        Some(false),
    );
    assert_eq!(report.class, NetworkBlackholeClass::NoDns);
    assert!(report.summary.contains("DNS"));
}

#[test]
fn test_blackhole_gateway_unreachable() {
    let report = evaluate_network_signals(
        NetworkLinkState::Up,
        Some("default via 192.168.1.1 dev eth0".to_string()),
        Some(false),
        Some(true),
    );
    assert_eq!(report.class, NetworkBlackholeClass::NoRoute);
    assert!(report.summary.contains("Gateway unreachable"));
}

#[test]
fn test_blackhole_healthy_link() {
    let report = evaluate_network_signals(
        NetworkLinkState::Up,
        Some("default via 192.168.1.1 dev eth0".to_string()),
        Some(true),
        Some(true),
    );
    assert_eq!(report.class, NetworkBlackholeClass::Ok);
    assert_eq!(report.summary, "Network connectivity checks passed");
}
