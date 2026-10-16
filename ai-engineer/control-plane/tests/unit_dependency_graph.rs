use control_plane::unit_graph::{UnitDependencyGraph, UnitDependencies};

const PROTECTED_UNITS: &[&str] = &[
    "ssh",
    "sshd",
    "dbus",
    "dbus-broker",
    "firewalld",
    "systemd-udevd",
];

#[test]
fn test_parse_systemctl_show_properties() {
    let mock_show = r#"
Requires=basic.target systemd-journald.socket
RequiredBy=multi-user.target
Wants=systemd-logind.service
After=systemd-journald.socket basic.target
Before=shutdown.target
"#;

    let deps = UnitDependencyGraph::parse_systemctl_show(mock_show);
    assert_eq!(deps.requires, vec!["basic.target", "systemd-journald.socket"]);
    assert_eq!(deps.required_by, vec!["multi-user.target"]);
    assert_eq!(deps.wants, vec!["systemd-logind"]);
    assert_eq!(deps.after, vec!["systemd-journald.socket", "basic.target"]);
    assert_eq!(deps.before, vec!["shutdown.target"]);
}

#[test]
fn test_direct_stop_of_protected_unit() {
    let graph = UnitDependencyGraph::new();
    let impacts = graph.detect_protected_unit_impact("sshd.service", PROTECTED_UNITS);

    assert_eq!(impacts.len(), 1);
    assert_eq!(impacts[0].protected_unit, "sshd");
    assert_eq!(impacts[0].dependency_chain, vec!["sshd"]);
}

#[test]
fn test_transitive_protected_unit_impact() {
    let mut graph = UnitDependencyGraph::new();

    // mock-storage -> required by mock-auth-helper
    // mock-auth-helper -> required by sshd
    graph.add_unit(
        "mock-storage",
        UnitDependencies {
            requires: vec![],
            required_by: vec!["mock-auth-helper".to_string()],
            wants: vec![],
            after: vec![],
            before: vec![],
        },
    );

    graph.add_unit(
        "mock-auth-helper",
        UnitDependencies {
            requires: vec!["mock-storage".to_string()],
            required_by: vec!["sshd.service".to_string()],
            wants: vec![],
            after: vec![],
            before: vec![],
        },
    );

    graph.add_unit(
        "sshd.service",
        UnitDependencies {
            requires: vec!["mock-auth-helper".to_string()],
            required_by: vec![],
            wants: vec![],
            after: vec![],
            before: vec![],
        },
    );

    let impacts = graph.detect_protected_unit_impact("mock-storage", PROTECTED_UNITS);

    assert_eq!(impacts.len(), 1);
    assert_eq!(impacts[0].protected_unit, "sshd");
    assert_eq!(
        impacts[0].dependency_chain,
        vec!["mock-storage", "mock-auth-helper", "sshd"]
    );
    assert!(impacts[0].reason.contains("transitively stops protected unit 'sshd'"));
}

#[test]
fn test_isolated_unit_has_zero_protected_impacts() {
    let mut graph = UnitDependencyGraph::new();

    graph.add_unit(
        "isolated-batch-worker.service",
        UnitDependencies {
            requires: vec![],
            required_by: vec!["worker-pool.target".to_string()],
            wants: vec![],
            after: vec![],
            before: vec![],
        },
    );

    let impacts = graph.detect_protected_unit_impact("isolated-batch-worker", PROTECTED_UNITS);
    assert!(impacts.is_empty());
}
