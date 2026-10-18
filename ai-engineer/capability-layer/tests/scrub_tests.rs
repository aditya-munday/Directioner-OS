use capability_layer::scrub::{
    MAX_OUTPUT_BYTES, TRUNCATION_MARKER, read_bounded_output, scrub_secrets,
};
use std::io::Cursor;

#[test]
fn test_bounded_output_truncation_with_marker() {
    let large_data = vec![b'A'; MAX_OUTPUT_BYTES * 2];
    let cursor = Cursor::new(large_data);

    let (buf, truncated) = read_bounded_output(cursor, MAX_OUTPUT_BYTES);

    assert!(truncated);
    assert!(buf.ends_with(TRUNCATION_MARKER.as_bytes()));
    assert!(buf.len() <= MAX_OUTPUT_BYTES + TRUNCATION_MARKER.len());
}

#[test]
fn test_bounded_output_under_limit_no_truncation() {
    let data = b"normal command output under 64k";
    let cursor = Cursor::new(data);

    let (buf, truncated) = read_bounded_output(cursor, MAX_OUTPUT_BYTES);

    assert!(!truncated);
    assert_eq!(buf, data);
}

#[test]
fn test_scrub_private_key_blocks() {
    let raw = "Error occurred:\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0\n1234567890abcdef\n-----END RSA PRIVATE KEY-----\nEnd of log";
    let (scrubbed, modified) = scrub_secrets(raw);

    assert!(modified);
    assert!(!scrubbed.contains("MIIEowIBAAKCAQEA0"));
    assert!(!scrubbed.contains("1234567890abcdef"));
    assert!(scrubbed.contains("[REDACTED RSA PRIVATE KEY]"));
}

#[test]
fn test_scrub_tokens_and_passwords() {
    let raw = "config: password=super_secret_pass_123&token=ghp_abcdef1234567890&user=admin";
    let (scrubbed, modified) = scrub_secrets(raw);

    assert!(modified);
    assert!(!scrubbed.contains("super_secret_pass_123"));
    assert!(!scrubbed.contains("ghp_abcdef1234567890"));
    assert!(scrubbed.contains("password=[REDACTED]"));
    assert!(scrubbed.contains("token=[REDACTED]"));
    assert!(scrubbed.contains("user=admin"));
}

#[test]
fn test_scrubber_false_positive_preservation() {
    let raw = "The token_count was 42 and password_complexity is enforced for all users.";
    let (scrubbed, modified) = scrub_secrets(raw);

    assert!(!modified);
    assert_eq!(scrubbed, raw);
}
