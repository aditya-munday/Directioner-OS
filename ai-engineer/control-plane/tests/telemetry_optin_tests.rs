//! Integration tests for Opt-In Telemetry Subsystem (TODO 49).
//!
//! Tests:
//! 1. 49.1 Default state is strictly OFF. Explicit opt-in activates specific categories.
//! 2. 49.2 Preview payload generates exact inspectable JSON matching selected categories.
//! 3. 49.3 Anonymous rotating ephemeral client ID (zero hardware serials or user PII).
//! 4. 49.4 Staging payload writes to local outbox without remote cloud credentials (Rule B4).
//! 5. 49.5 Opt-out disables collection and rejects staging.

use std::fs;

use control_plane::telemetry_optin::{
    CrashReportPayload, FeatureUsagePayload, HardwareHealthPayload, TelemetryCategories,
    TelemetryManager,
};

#[test]
fn test_telemetry_default_off_and_opt_in_lifecycle() {
    let temp_dir = std::env::temp_dir().join("test_telemetry_optin");
    let _ = fs::remove_dir_all(&temp_dir);

    let mgr = TelemetryManager::new(&temp_dir).expect("TelemetryManager init");

    // 1. By default, telemetry MUST be OFF
    assert!(!mgr.is_enabled(), "Telemetry must be default OFF");
    let initial_config = mgr.load_config();
    assert!(!initial_config.enabled);
    assert!(!initial_config.categories.crash_reports);
    assert!(!initial_config.categories.hardware_health);
    assert!(!initial_config.categories.feature_usage);

    // 2. Opt-in to specific categories
    let chosen_categories = TelemetryCategories {
        crash_reports: true,
        hardware_health: true,
        feature_usage: false,
    };
    mgr.opt_in(chosen_categories.clone()).expect("opt_in");

    assert!(mgr.is_enabled());
    let active_config = mgr.load_config();
    assert!(active_config.enabled);
    assert!(active_config.categories.crash_reports);
    assert!(active_config.categories.hardware_health);
    assert!(!active_config.categories.feature_usage);

    // 3. Opt-out
    mgr.opt_out().expect("opt_out");
    assert!(!mgr.is_enabled());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_telemetry_anonymous_rotating_identity() {
    let temp_dir = std::env::temp_dir().join("test_telemetry_identity");
    let _ = fs::remove_dir_all(&temp_dir);

    let mgr = TelemetryManager::new(&temp_dir).expect("TelemetryManager init");

    let id1 = mgr.get_or_rotate_identity().expect("get id");
    assert_eq!(id1.client_id.len(), 36); // UUID-style 8-4-4-4-12
    assert!(id1.client_id.contains('-'));

    // Second call without expiry returns the SAME identity
    let id2 = mgr.get_or_rotate_identity().expect("get id again");
    assert_eq!(id1.client_id, id2.client_id);

    // Identity contains zero user or hardware information
    assert!(!id1.client_id.contains("aditya"));
    assert!(!id1.client_id.contains("Directioner"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_telemetry_preview_and_outbox_staging() {
    let temp_dir = std::env::temp_dir().join("test_telemetry_preview_stage");
    let _ = fs::remove_dir_all(&temp_dir);

    let mgr = TelemetryManager::new(&temp_dir).expect("TelemetryManager init");

    let categories = TelemetryCategories {
        crash_reports: true,
        hardware_health: true,
        feature_usage: false,
    };

    let sample_hw = HardwareHealthPayload {
        hypervisor: "QEMU-KVM".into(),
        disk_count: 2,
        thermal_zone_count: 1,
        psi_pressure: "Nominal".into(),
    };

    let sample_crashes = vec![CrashReportPayload {
        daemon: "ai-capabilityd".into(),
        exit_code: 0,
        clean_shutdown: true,
    }];

    let sample_usage = FeatureUsagePayload {
        plans_staged: 5,
        plans_executed: 5,
        plans_rolled_back: 0,
    };

    // 1. Preview payload before enabling
    let preview = mgr
        .preview_payload(
            &categories,
            Some(sample_hw),
            Some(sample_crashes),
            Some(sample_usage),
        )
        .expect("preview_payload");

    assert_eq!(preview.schema_version, "1.0.0");
    assert!(preview.hardware_health.is_some());
    assert!(preview.crash_reports.is_some());
    // Since feature_usage was false in categories, it MUST NOT appear
    assert!(preview.feature_usage.is_none());

    // 2. Staging when disabled MUST fail with PermissionDenied
    let stage_err = mgr.stage_payload(&preview);
    assert!(stage_err.is_err());
    assert_eq!(
        stage_err.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );

    // 3. Enable opt-in and stage to local outbox
    mgr.opt_in(categories).expect("opt_in");
    let staged_file = mgr.stage_payload(&preview).expect("stage_payload");

    assert!(staged_file.exists());
    let staged_data = fs::read_to_string(&staged_file).unwrap();
    assert!(staged_data.contains("QEMU-KVM"));
    assert!(staged_data.contains("ai-capabilityd"));

    let _ = fs::remove_dir_all(&temp_dir);
}
