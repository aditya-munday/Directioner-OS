//! Tests for socket protocol wire limits (TODO 001).
//!
//! Subtasks:
//! 1.1 Read current request framing code; document unbounded allocation vulnerability.
//! 1.2 Enforce hard max request size (64 KiB default) before allocation.
//! 1.3 Per-connection read timeout (5s default) mitigating slowloris attacks.
//! 1.4 Max concurrent connections cap (64 default) with structured error response.
//! 1.5 Structured CapabilityResponse::Error response on limit breaches.

use capability_layer::server::{
    DEFAULT_MAX_CONCURRENT_CONNECTIONS, DEFAULT_MAX_REQUEST_BYTES, DEFAULT_READ_TIMEOUT,
    ServerConfig,
};
use std::time::Duration;

#[test]
fn test_wire_limit_config_defaults() {
    let config = ServerConfig::default();
    assert_eq!(config.max_request_bytes, DEFAULT_MAX_REQUEST_BYTES);
    assert_eq!(config.max_request_bytes, 64 * 1024);
    assert_eq!(config.read_timeout, DEFAULT_READ_TIMEOUT);
    assert_eq!(config.read_timeout, Duration::from_secs(5));
    assert_eq!(
        config.max_concurrent_connections,
        DEFAULT_MAX_CONCURRENT_CONNECTIONS
    );
    assert_eq!(config.max_concurrent_connections, 64);
}

#[test]
fn test_wire_limit_custom_config() {
    let config = ServerConfig {
        max_request_bytes: 4096,
        read_timeout: Duration::from_millis(500),
        max_concurrent_connections: 10,
        ..ServerConfig::default()
    };

    assert_eq!(config.max_request_bytes, 4096);
    assert_eq!(config.read_timeout, Duration::from_millis(500));
    assert_eq!(config.max_concurrent_connections, 10);
}
