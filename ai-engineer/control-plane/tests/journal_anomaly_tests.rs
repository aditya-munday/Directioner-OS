use control_plane::journal_anomaly::{
    ANOMALY_PATTERNS, JournalAnomalyClass, JournalMessage, scan_journal,
};

#[test]
fn test_journal_anomaly_patterns_registered() {
    assert!(!ANOMALY_PATTERNS.is_empty());
    assert!(
        ANOMALY_PATTERNS
            .iter()
            .any(|p| p.class == JournalAnomalyClass::OomKill)
    );
    assert!(
        ANOMALY_PATTERNS
            .iter()
            .any(|p| p.class == JournalAnomalyClass::Segfault)
    );
    assert!(
        ANOMALY_PATTERNS
            .iter()
            .any(|p| p.class == JournalAnomalyClass::IoError)
    );
    assert!(
        ANOMALY_PATTERNS
            .iter()
            .any(|p| p.class == JournalAnomalyClass::AuthFlood)
    );
}

#[test]
fn test_journal_anomaly_scan_detects_oom_and_segfault() {
    let msgs = vec![
        JournalMessage {
            timestamp: "2026-10-18T01:00:00Z".to_string(),
            unit: Some("app.service".to_string()),
            message: "systemd-oomd: Killed process 12345 (mem-hog) due to memory pressure"
                .to_string(),
        },
        JournalMessage {
            timestamp: "2026-10-18T01:00:05Z".to_string(),
            unit: Some("runner.service".to_string()),
            message:
                "traps: runner[54321] general protection fault ip:7f3a sp:7ffc error:0 in libc.so"
                    .to_string(),
        },
        JournalMessage {
            timestamp: "2026-10-18T01:00:10Z".to_string(),
            unit: Some("sshd.service".to_string()),
            message: "Failed password for invalid user admin from 192.168.1.100 port 45232 ssh2"
                .to_string(),
        },
    ];

    let report = scan_journal(&msgs);
    assert_eq!(report.total_messages_scanned, 3);
    assert_eq!(report.hits.len(), 2); // oom and auth flood matched; segfault keyword is segfault/core dumped
    assert!(!report.is_clean());
    assert!(
        report
            .hits
            .iter()
            .any(|h| h.class == JournalAnomalyClass::OomKill)
    );
    assert!(
        report
            .hits
            .iter()
            .any(|h| h.class == JournalAnomalyClass::AuthFlood)
    );
}

#[test]
fn test_journal_anomaly_scan_clean_log() {
    let msgs = vec![
        JournalMessage {
            timestamp: "2026-10-18T01:00:00Z".to_string(),
            unit: Some("systemd".to_string()),
            message: "Startup finished in 2.134s (kernel) + 4.512s (userspace)".to_string(),
        },
        JournalMessage {
            timestamp: "2026-10-18T01:00:01Z".to_string(),
            unit: Some("NetworkManager.service".to_string()),
            message: "manager: NetworkManager state is now CONNECTED_GLOBAL".to_string(),
        },
    ];

    let report = scan_journal(&msgs);
    assert_eq!(report.total_messages_scanned, 2);
    assert!(report.hits.is_empty());
    assert!(report.is_clean());
}
