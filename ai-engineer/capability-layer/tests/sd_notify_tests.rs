use capability_layer::notify::{
    notify_to_socket, parse_watchdog_usec, spawn_watchdog_thread_target,
};
use std::os::unix::net::UnixDatagram;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_sd_notify_messages_over_unix_datagram() {
    let dir = tempdir().unwrap();
    let notify_sock_path = dir.path().join("notify.sock");

    let receiver = UnixDatagram::bind(&notify_sock_path).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    assert!(notify_to_socket(
        "READY=1",
        Some(notify_sock_path.as_os_str())
    ));
    let mut buf = [0u8; 64];
    let (n, _) = receiver.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"READY=1");

    assert!(notify_to_socket(
        "WATCHDOG=1",
        Some(notify_sock_path.as_os_str())
    ));
    let (n, _) = receiver.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"WATCHDOG=1");

    assert!(notify_to_socket(
        "STOPPING=1",
        Some(notify_sock_path.as_os_str())
    ));
    let (n, _) = receiver.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"STOPPING=1");
}

#[test]
fn test_watchdog_interval_parsing() {
    assert_eq!(
        parse_watchdog_usec(Some("10000000")),
        Some(Duration::from_secs(10))
    );
    assert_eq!(parse_watchdog_usec(None), None);
    assert_eq!(parse_watchdog_usec(Some("invalid")), None);
}

#[test]
fn test_watchdog_thread_heartbeat() {
    let dir = tempdir().unwrap();
    let notify_sock_path = dir.path().join("watchdog.sock");

    let receiver = UnixDatagram::bind(&notify_sock_path).unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    let handle = spawn_watchdog_thread_target(
        notify_sock_path.as_os_str().to_os_string(),
        Duration::from_millis(40),
    )
    .expect("watchdog thread should spawn");

    let mut buf = [0u8; 64];
    let (n, _) = receiver.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"WATCHDOG=1");

    handle.store(true, Ordering::Relaxed);
}
