use std::env;
use std::ffi::OsStr;
use std::os::unix::net::UnixDatagram;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub fn notify(state: &str) -> bool {
    let socket_path = env::var_os("NOTIFY_SOCKET");
    notify_to_socket(state, socket_path.as_deref())
}

pub fn notify_to_socket(state: &str, socket_path: Option<&OsStr>) -> bool {
    let path = match socket_path {
        Some(p) => p,
        None => return false,
    };

    let sock = match UnixDatagram::unbound() {
        Ok(s) => s,
        Err(_) => return false,
    };

    sock.send_to(state.as_bytes(), path).is_ok()
}

pub fn notify_ready() -> bool {
    notify("READY=1")
}

pub fn notify_stopping() -> bool {
    notify("STOPPING=1")
}

pub fn notify_watchdog() -> bool {
    notify("WATCHDOG=1")
}

pub fn parse_watchdog_usec(raw: Option<&str>) -> Option<Duration> {
    raw.and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_micros)
}

pub fn watchdog_interval() -> Option<Duration> {
    env::var("WATCHDOG_USEC")
        .ok()
        .and_then(|v| parse_watchdog_usec(Some(&v)))
}

pub fn spawn_watchdog_thread() -> Option<Arc<AtomicBool>> {
    let interval = watchdog_interval()?;
    let socket_path = env::var_os("NOTIFY_SOCKET")?;
    spawn_watchdog_thread_target(socket_path, interval)
}

pub fn spawn_watchdog_thread_target(
    socket_path: std::ffi::OsString,
    interval: Duration,
) -> Option<Arc<AtomicBool>> {
    let half_interval = interval / 2;
    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_signal.clone();

    thread::Builder::new()
        .name("ai-capabilityd-watchdog".into())
        .spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                notify_to_socket("WATCHDOG=1", Some(&socket_path));
                thread::sleep(half_interval);
            }
        })
        .ok()?;

    Some(stop_signal)
}
