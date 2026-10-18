use control_plane::diagnosis::{
    DiagnosisClass, JournalMessage, JournalSnapshot, diagnose_boot_loop, diagnose_disk_full,
    diagnose_failed_unit, diagnose_network_loss, diagnose_oom, run_all_diagnoses,
};
use std::collections::HashMap;

fn ts() -> String {
    "2026-10-18T00:00:00Z".to_string()
}

fn journal_msg(msg: &str) -> JournalMessage {
    JournalMessage {
        timestamp: ts(),
        unit: None,
        message: msg.to_string(),
        priority: 3,
    }
}

fn journal_msg_unit(unit: &str, msg: &str) -> JournalMessage {
    JournalMessage {
        timestamp: ts(),
        unit: Some(unit.to_string()),
        message: msg.to_string(),
        priority: 3,
    }
}

#[test]
fn test_diagnose_failed_unit_detects_failed_state() {
    let mut snapshot = JournalSnapshot::default();
    snapshot
        .unit_states
        .insert("nginx.service".to_string(), "failed".to_string());
    snapshot
        .unit_states
        .insert("sshd.service".to_string(), "running".to_string());

    let report = diagnose_failed_unit(&snapshot);
    assert_eq!(report.class, DiagnosisClass::FailedUnit);
    assert!(report.confidence > 0.0);
    assert!(report.summary.contains("nginx.service"));
    assert_eq!(report.evidence.len(), 1);
}

#[test]
fn test_diagnose_failed_unit_clean_system_returns_unknown() {
    let mut snapshot = JournalSnapshot::default();
    snapshot
        .unit_states
        .insert("sshd.service".to_string(), "running".to_string());

    let report = diagnose_failed_unit(&snapshot);
    assert_eq!(report.class, DiagnosisClass::Unknown);
    assert_eq!(report.confidence, 0.0);
}

#[test]
fn test_diagnose_boot_loop_detects_restart_signals() {
    let snapshot = JournalSnapshot {
        unit_states: HashMap::new(),
        disk_usage_percent: None,
        memory_pressure_avg10: None,
        recent_messages: vec![
            journal_msg_unit("myapp.service", "Start request repeated too quickly."),
            journal_msg_unit("myapp.service", "Start request repeated too quickly."),
        ],
    };

    let report = diagnose_boot_loop(&snapshot);
    assert_eq!(report.class, DiagnosisClass::BootLoop);
    assert!(report.confidence >= 0.8);
    assert!(report.summary.contains("restart loop"));
}

#[test]
fn test_diagnose_disk_full_high_usage() {
    let snapshot = JournalSnapshot {
        unit_states: HashMap::new(),
        disk_usage_percent: Some(99.0),
        memory_pressure_avg10: None,
        recent_messages: Vec::new(),
    };

    let report = diagnose_disk_full(&snapshot);
    assert_eq!(report.class, DiagnosisClass::DiskFull);
    assert!(report.confidence >= 0.9);
    assert!(!report.evidence.is_empty());
}

#[test]
fn test_diagnose_oom_detects_oom_killer_messages() {
    let snapshot = JournalSnapshot {
        unit_states: HashMap::new(),
        disk_usage_percent: None,
        memory_pressure_avg10: None,
        recent_messages: vec![
            journal_msg("Out of memory: kill process 1234 (myapp) score 800"),
            journal_msg("oom-killer: gfp_mask=0x100cca(GFP_HIGHUSER_MOVABLE)"),
        ],
    };

    let report = diagnose_oom(&snapshot);
    assert_eq!(report.class, DiagnosisClass::Oom);
    assert!(report.confidence >= 0.8);
    assert_eq!(report.evidence.len(), 2);
}

#[test]
fn test_diagnose_network_loss_detects_link_down() {
    let snapshot = JournalSnapshot {
        unit_states: HashMap::new(),
        disk_usage_percent: None,
        memory_pressure_avg10: None,
        recent_messages: vec![
            journal_msg("eth0: link is down"),
            journal_msg("SIOCDELRT: Network is unreachable"),
        ],
    };

    let report = diagnose_network_loss(&snapshot);
    assert_eq!(report.class, DiagnosisClass::NetworkLoss);
    assert!(report.confidence >= 0.7);
    assert_eq!(report.evidence.len(), 2);
}

#[test]
fn test_run_all_diagnoses_returns_sorted_by_confidence() {
    let mut snapshot = JournalSnapshot::default();
    snapshot
        .unit_states
        .insert("broken.service".to_string(), "failed".to_string());
    snapshot.disk_usage_percent = Some(99.0);

    let reports = run_all_diagnoses(&snapshot);
    assert!(!reports.is_empty());
    for w in reports.windows(2) {
        assert!(w[0].confidence >= w[1].confidence);
    }
}
