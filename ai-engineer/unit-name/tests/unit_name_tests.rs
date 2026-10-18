use unit_name::{is_protected_unit, matches_unit, normalize};

#[test]
fn test_normalize_ssh_sshd_aliases() {
    assert_eq!(normalize("ssh"), "sshd");
    assert_eq!(normalize("sshd"), "sshd");
    assert_eq!(normalize("ssh.service"), "sshd");
    assert_eq!(normalize("sshd.service"), "sshd");
    assert_eq!(normalize("  ssh.service  "), "sshd");
    assert_eq!(normalize(" SSH.SERVICE "), "sshd");
    assert_eq!(normalize("SSHD"), "sshd");
}

#[test]
fn test_normalize_systemd_udevd() {
    assert_eq!(normalize("systemd-udevd"), "systemd-udevd");
    assert_eq!(normalize("systemd-udevd.service"), "systemd-udevd");
    assert_eq!(normalize(" SYSTEMD-UDEVD.SERVICE "), "systemd-udevd");
}

#[test]
fn test_normalize_templated_units() {
    assert_eq!(normalize("getty@tty1.service"), "getty@tty1");
    assert_eq!(normalize("user@1000.service"), "user@1000");
}

#[test]
fn test_matches_unit() {
    assert!(matches_unit("ssh", "sshd.service"));
    assert!(matches_unit("ssh.service", "sshd"));
    assert!(matches_unit("systemd-udevd", "systemd-udevd.service"));
    assert!(!matches_unit("nginx", "sshd"));
}

#[test]
fn test_is_protected_unit() {
    let protected = &["sshd", "dbus", "firewalld", "systemd-udevd"];

    assert!(is_protected_unit("ssh", protected));
    assert!(is_protected_unit("ssh.service", protected));
    assert!(is_protected_unit("sshd.service", protected));
    assert!(is_protected_unit("systemd-udevd.service", protected));
    assert!(is_protected_unit("systemd-udevd", protected));
    assert!(is_protected_unit("dbus.service", protected));
    assert!(!is_protected_unit("custom-app.service", protected));
}
