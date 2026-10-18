use control_plane::incident_persistence::{IncidentPersistence, IncidentRecord};
use tempfile::NamedTempFile;

#[test]
fn test_incident_persistence_append_and_load() {
    let tmp = NamedTempFile::new().unwrap();
    let persistence = IncidentPersistence::new(tmp.path(), 100);

    let r1 = IncidentRecord {
        incident_id: "test-001".to_string(),
        boot_id: "boot-abc123".to_string(),
        timestamp: "2026-10-18T00:00:00Z".to_string(),
        class: "oom".to_string(),
        summary: "OOM killer triggered".to_string(),
        evidence_json: r#"{"killed_process":"myapp"}"#.to_string(),
        prev_hash: None,
        entry_hash: None,
    };

    persistence.append(r1.clone()).unwrap();

    let records = persistence.load_all().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].incident_id, "test-001");
    assert_eq!(records[0].class, "oom");
    assert!(records[0].entry_hash.is_some());
}

#[test]
fn test_incident_persistence_rotation() {
    let tmp = NamedTempFile::new().unwrap();
    let persistence = IncidentPersistence::new(tmp.path(), 3);

    for i in 0..6 {
        let r = IncidentRecord {
            incident_id: format!("test-{i:03}"),
            boot_id: "boot-abc123".to_string(),
            timestamp: "2026-10-18T00:00:00Z".to_string(),
            class: "failed_unit".to_string(),
            summary: format!("Incident {i}"),
            evidence_json: "{}".to_string(),
            prev_hash: None,
            entry_hash: None,
        };
        persistence.append(r).unwrap();
    }

    let records = persistence.load_all().unwrap();
    assert!(records.len() <= 3, "expected at most 3 records after rotation, got {}", records.len());
}

#[test]
fn test_incident_persistence_load_previous_boot() {
    let tmp = NamedTempFile::new().unwrap();
    let persistence = IncidentPersistence::new(tmp.path(), 100);

    let r_old = IncidentRecord {
        incident_id: "old-001".to_string(),
        boot_id: "old-boot-id".to_string(),
        timestamp: "2026-10-17T00:00:00Z".to_string(),
        class: "failed_unit".to_string(),
        summary: "Old incident".to_string(),
        evidence_json: "{}".to_string(),
        prev_hash: None,
        entry_hash: None,
    };
    let r_current = IncidentRecord {
        incident_id: "new-001".to_string(),
        boot_id: "current-boot-id".to_string(),
        timestamp: "2026-10-18T00:00:00Z".to_string(),
        class: "oom".to_string(),
        summary: "Current incident".to_string(),
        evidence_json: "{}".to_string(),
        prev_hash: None,
        entry_hash: None,
    };

    persistence.append(r_old).unwrap();
    persistence.append(r_current).unwrap();

    let prev_boot = persistence.load_previous_boot("current-boot-id").unwrap();
    assert_eq!(prev_boot.len(), 1);
    assert_eq!(prev_boot[0].incident_id, "old-001");
}

#[test]
fn test_incident_persistence_hash_chain() {
    let tmp = NamedTempFile::new().unwrap();
    let persistence = IncidentPersistence::new(tmp.path(), 100);

    for i in 0..3 {
        persistence.append(IncidentRecord {
            incident_id: format!("chain-{i}"),
            boot_id: "boot-x".to_string(),
            timestamp: "2026-10-18T00:00:00Z".to_string(),
            class: "test".to_string(),
            summary: format!("entry {i}"),
            evidence_json: "{}".to_string(),
            prev_hash: None,
            entry_hash: None,
        }).unwrap();
    }

    let records = persistence.load_all().unwrap();
    assert_eq!(records.len(), 3);
    assert!(records[0].prev_hash.is_none() || records[0].prev_hash.as_deref() == Some("GENESIS") || records[0].entry_hash.is_some());
    assert!(records[1].entry_hash.is_some());
    assert!(records[2].entry_hash.is_some());
    assert_ne!(records[0].entry_hash, records[1].entry_hash);
}
