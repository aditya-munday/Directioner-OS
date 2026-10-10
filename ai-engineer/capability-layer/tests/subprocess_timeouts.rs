//! Test suite for subprocess execution timeouts and process group management (TODO 002).
//!
//! Subtasks:
//! 2.1 List every place the daemon spawns a subprocess.
//! 2.2 Wrap each in a timeout with a per-capability default.
//! 2.3 On timeout: kill the process group (killpg), record in Work Log, return a typed error.
//! 2.4 Keep the killswitch as IMMEDIATE SIGKILL — must not use timeout or grace period.
//! 2.5 Make timeouts configurable but clamped to a sane maximum.

use std::time::{Duration, Instant};

use capability_layer::{
    audit::AuditLog,
    handler::{Handler, HandlerConfig, DEFAULT_SUBPROCESS_TIMEOUT, MAX_SUBPROCESS_TIMEOUT},
    killswitch::KillSwitch,
    scope::ScopedPath,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn make_handler(timeout: Duration) -> (Handler, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let scope = ScopedPath::new(tmp.path()).expect("scope");
    let audit = AuditLog::open(tmp.path().join("audit.log")).expect("audit");
    let ks = KillSwitch::new(tmp.path().join("ks"));
    let config = HandlerConfig {
        subprocess_timeout: timeout,
        ..HandlerConfig::default()
    };
    let h = Handler::new(scope, audit, ks, config);
    (h, tmp) // return tmp so it lives long enough
}

// ─── 2.5 – configuration & clamping ──────────────────────────────────────────

#[test]
fn default_subprocess_timeout_is_30s() {
    let config = HandlerConfig::default();
    assert_eq!(config.subprocess_timeout, DEFAULT_SUBPROCESS_TIMEOUT);
    assert_eq!(config.subprocess_timeout, Duration::from_secs(30));
}

#[test]
fn max_subprocess_timeout_is_300s() {
    assert_eq!(MAX_SUBPROCESS_TIMEOUT, Duration::from_secs(300));
}

#[test]
fn custom_timeout_is_preserved_in_config() {
    let config = HandlerConfig {
        subprocess_timeout: Duration::from_secs(15),
        ..HandlerConfig::default()
    };
    assert_eq!(config.subprocess_timeout, Duration::from_secs(15));
}

/// If caller sets timeout > MAX, run_command clamps it internally — the process
/// still finishes fast so we only verify the clamp doesn't panic and returns Ok.
#[test]
fn oversized_timeout_is_clamped_and_does_not_crash() {
    let (h, _tmp) = make_handler(Duration::from_secs(9999));
    let result = h.run_command(&mut std::process::Command::new("true"));
    assert!(result.is_ok(), "true should succeed: {result:?}");
}

// ─── 2.2 / 2.3 – timeout fires and kills the process ─────────────────────────

/// A subprocess that sleeps longer than the configured timeout must be killed
/// and the returned error kind must be `TimedOut`.
#[test]
fn slow_subprocess_is_killed_on_timeout() {
    // 200 ms timeout; `sleep 10` would run for 10 s without the timeout.
    let (h, _tmp) = make_handler(Duration::from_millis(200));
    let t0 = Instant::now();
    let err = h
        .run_command(&mut std::process::Command::new("sleep").arg("10"))
        .expect_err("expected timeout error");
    let elapsed = t0.elapsed();

    assert_eq!(
        err.kind(),
        std::io::ErrorKind::TimedOut,
        "error kind should be TimedOut, got: {:?}",
        err.kind()
    );
    // Should have returned well within 3 s (generous upper bound for slow CI).
    assert!(
        elapsed < Duration::from_secs(3),
        "took too long ({elapsed:?}); timeout not firing promptly"
    );
}

/// The error message must indicate a timeout occurred.
#[test]
fn timeout_error_message_contains_timed_out() {
    let (h, _tmp) = make_handler(Duration::from_millis(100));
    let err = h
        .run_command(&mut std::process::Command::new("sleep").arg("10"))
        .expect_err("expected timeout");
    let msg = err.to_string();
    assert!(
        msg.contains("timed out"),
        "expected 'timed out' in message, got: {msg}"
    );
    // Must mention the max cap (300s).
    assert!(
        msg.contains("300s"),
        "expected '300s' (max cap) in message, got: {msg}"
    );
}

// ─── 2.4 – killswitch still fires immediately, independent of timeout ─────────

/// When the kill-switch is engaged the subprocess must be terminated with
/// `ErrorKind::Interrupted` — not `TimedOut` — even if the timeout hasn't elapsed.
#[test]
fn killswitch_fires_before_timeout() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ks_path = tmp.path().join("killswitch");
    let scope = ScopedPath::new(tmp.path()).expect("scope");
    let audit = AuditLog::open(tmp.path().join("audit.log")).expect("audit");
    let mut ks = KillSwitch::new(&ks_path);

    // Give a very generous timeout so only the killswitch path is exercised.
    let config = HandlerConfig {
        subprocess_timeout: Duration::from_secs(60),
        ..HandlerConfig::default()
    };

    // Engage the killswitch BEFORE building the handler so it is read as engaged.
    ks.engage().expect("engage");

    let handler = Handler::new(scope, audit, ks, config);

    let err = handler
        .run_command(&mut std::process::Command::new("sleep").arg("10"))
        .expect_err("expected killswitch interrupt");

    assert_eq!(
        err.kind(),
        std::io::ErrorKind::Interrupted,
        "killswitch should produce Interrupted, got: {:?}",
        err.kind()
    );
    let msg = err.to_string();
    assert!(
        msg.contains("killswitch"),
        "error message should mention killswitch: {msg}"
    );
}

// ─── 2.1 – fast-path: commands that finish before deadline succeed ─────────────

#[test]
fn fast_command_succeeds_within_timeout() {
    let (h, _tmp) = make_handler(Duration::from_secs(5));
    let out = h
        .run_command(&mut std::process::Command::new("echo").arg("hello"))
        .expect("echo should succeed");
    assert!(out.status.success());
    assert_eq!(out.stdout.trim_ascii_end(), b"hello");
}

#[test]
fn failed_command_returns_non_zero_status() {
    let (h, _tmp) = make_handler(DEFAULT_SUBPROCESS_TIMEOUT);
    let out = h
        .run_command(&mut std::process::Command::new("false"))
        .expect("run_command itself should not error");
    assert!(!out.status.success());
}
