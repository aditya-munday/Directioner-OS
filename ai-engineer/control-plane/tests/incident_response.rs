//! Integration tests for IncidentResponseAgent (Phase 3.0.3).

use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

use control_plane::health_model::HealthStatus;
use control_plane::incident_response::{IncidentResponseAgent, IncidentResponseConfig};

#[test]
fn test_incident_response_agent_authority_boundary_doc_and_invariants() {
    let agent = IncidentResponseAgent::new(IncidentResponseConfig::default());
    let boundary = agent.authority_boundary();

    // Verify exact contract wording
    assert!(
        boundary.contains("Tier 1 recovery is fully autonomous at the bootloader/greenboot layer")
    );
    assert!(boundary.contains("requires no action from this agent"));
    assert!(boundary.contains("post-hoc observation and logging"));
    assert!(boundary.contains("zero write/mutating capability access as of Phase 3.0.3"));
}

#[test]
fn test_incident_response_agent_surfaces_tier1_observation_and_posture() {
    let mut log_file = NamedTempFile::new().unwrap();
    writeln!(
        log_file,
        r#"{{"seq":5,"timestamp":"2026-08-29T17:48:55Z","caller":"vm-smoke","capability":"boot_slot.pin","args":{{"slot":0}}}}"#
    ).unwrap();
    writeln!(
        log_file,
        r#"{{"seq":99,"timestamp":"2026-08-29T17:56:39Z","caller":"incident-response-agent:tier1-observer","capability":"boot_watchdog.tier1_rollback_observed","args":{{"trigger":"watchdog: required milestone health-check failed 3 consecutive attempts","mechanism":"bootcounter exhaustion + autonomous bootc rollback","action":"automatic rollback to pinned deployment 0"}},"result":{{"active_deployment_restored":true}}}}"#
    ).unwrap();

    let agent = IncidentResponseAgent::new(IncidentResponseConfig {
        db_path: PathBuf::from("/tmp/nonexistent.sqlite"),
        worklog_path: Some(log_file.path().to_path_buf()),
    });

    let obs = agent
        .observe_tier1_recovery()
        .expect("should observe tier 1 rollback");
    assert_eq!(obs.audit_seq, 99);
    assert_eq!(obs.observer, "incident-response-agent:tier1-observer");
    assert!(obs.trigger.contains("milestone health-check failed"));
    assert!(
        obs.action
            .contains("automatic rollback to pinned deployment 0")
    );
    assert!(obs.active_deployment_restored);

    let posture = agent.evaluate_posture();
    assert!(posture.latest_tier1_recovery.is_some());
    assert_eq!(posture.health_report.overall_status, HealthStatus::Degraded);
    assert_eq!(posture.health_report.composite_score, Some(65));
    assert!(
        posture
            .active_incidents
            .iter()
            .any(|i| i.contains("Tier 1 autonomous rollback"))
    );
}
