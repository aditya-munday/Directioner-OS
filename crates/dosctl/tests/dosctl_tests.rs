use dosctl::{DbusIntrospector, FabricClient};
use std::env;

#[test]
fn test_dbus_tree_introspection() {
    let nodes = DbusIntrospector::inspect_system();
    assert!(
        !nodes.is_empty(),
        "D-Bus tree should discover core system services"
    );
    assert!(
        nodes.iter().any(|n| n.service == "org.freedesktop.login1"),
        "login1 should be in D-Bus tree"
    );
    assert!(
        nodes
            .iter()
            .any(|n| n.service == "org.freedesktop.NetworkManager"),
        "NetworkManager should be in D-Bus tree"
    );

    let tree_output = DbusIntrospector::render_tree();
    assert!(tree_output.contains("D-Bus System Bus Introspection Tree"));
    assert!(tree_output.contains("org.freedesktop.login1"));
}

#[test]
fn test_fabric_client_socket_resolution() {
    unsafe {
        env::set_var("DIRECTIONER_STATD_SOCKET", "/tmp/custom_statd.sock");
    }
    assert_eq!(
        FabricClient::statd_socket_path().to_str().unwrap(),
        "/tmp/custom_statd.sock"
    );

    unsafe {
        env::set_var("XDG_RUNTIME_DIR", "/tmp/user_runtime");
    }
    assert_eq!(
        FabricClient::actd_socket_path().to_str().unwrap(),
        "/tmp/user_runtime/directioner/actd.sock"
    );
}
