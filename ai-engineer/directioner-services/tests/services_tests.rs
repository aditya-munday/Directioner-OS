use directioner_services::{
    AriaEdgeGlowState, BootSlot, CircuitBreaker, CircuitState, CliIntegrationBroker,
    CloudSyncEngine, DeploymentState, DesktopCoordinator, HardwarePowerManager, ImpactRating,
    IntentBroker, IntentKind, IntentRequest, IntentResult, PowerProfile, PowerSource,
    QaConformanceRunner, ReleaseManifest, SandboxLevel, SecureBootMode, SecurityTrustEvaluator,
    SyncStatus, SystemActionsManager, ThemeMode, ThermalSeverity, UpdateController, UrgencyLevel,
};
use std::collections::HashMap;

#[test]
fn test_domain_8_cloud_sync_and_circuit_breaker() {
    let mut breaker = CircuitBreaker::new(2, 60);
    assert_eq!(breaker.state, CircuitState::Closed);
    assert!(breaker.allow_request(100));

    breaker.record_failure(100);
    assert_eq!(breaker.state, CircuitState::Closed);
    breaker.record_failure(101);
    assert_eq!(breaker.state, CircuitState::Open);
    assert!(!breaker.allow_request(102));
    assert!(breaker.allow_request(162));

    breaker.record_success();
    assert_eq!(breaker.state, CircuitState::Closed);

    let mut sync = CloudSyncEngine::new();
    sync.register_local_file("docs/notes.txt", "hash-v1", 100, 1000);
    assert_eq!(sync.pending_count(), 1);

    let status = sync.reconcile_remote("docs/notes.txt", "hash-v1", 1000);
    assert_eq!(status, SyncStatus::Synced);

    let status_conflict = sync.reconcile_remote("docs/notes.txt", "hash-remote", 1000);
    assert_eq!(status_conflict, SyncStatus::Conflict);

    let forked = sync.resolve_conflict("docs/notes.txt", false);
    assert!(forked.is_some());
}

#[test]
fn test_domain_9_cli_integration_broker() {
    let mut broker = CliIntegrationBroker::new();
    let session_id = "sess-123".to_string();

    broker
        .create_session(
            session_id.clone(),
            1234,
            1000,
            "/home/user".to_string(),
            vec!["unit.restart".to_string(), "logs.read".to_string()],
            SandboxLevel::Standard,
        )
        .unwrap();

    assert_eq!(broker.active_session_count(), 1);
    assert!(broker.authorize_action(&session_id, "unit.restart"));
    assert!(!broker.authorize_action(&session_id, "package.install"));

    assert!(broker.terminate_session(&session_id));
    assert_eq!(broker.active_session_count(), 0);
}

#[test]
fn test_domain_10_intent_broker_and_rule_b1_guard() {
    let broker = IntentBroker::new(0.6);

    assert_eq!(
        broker.classify_impact(&IntentKind::QuerySystemStatus),
        ImpactRating::Readonly
    );
    assert_eq!(
        broker.classify_impact(&IntentKind::ExecuteAction),
        ImpactRating::PrivilegedMutation
    );

    let mut slots = HashMap::new();
    slots.insert("app_name".to_string(), "terminal".to_string());
    let req_launch = IntentRequest {
        raw_utterance: "open terminal".to_string(),
        kind: IntentKind::LaunchApplication,
        slots,
        confidence: 0.9,
        execution_authorized: false,
    };
    let res = broker.dispatch(req_launch);
    assert!(matches!(res, IntentResult::Executed { .. }));

    let mut slots_exec = HashMap::new();
    slots_exec.insert("target".to_string(), "network".to_string());
    let req_unauth = IntentRequest {
        raw_utterance: "restart network".to_string(),
        kind: IntentKind::ExecuteAction,
        slots: slots_exec.clone(),
        confidence: 0.95,
        execution_authorized: false,
    };
    let res_unauth = broker.dispatch(req_unauth);
    match res_unauth {
        IntentResult::DraftPreview {
            action_summary,
            dry_run_output,
            ..
        } => {
            assert!(action_summary.contains("Draft execution"));
            assert!(dry_run_output.contains("Rule B1"));
        }
        _ => panic!("Expected DraftPreview under Rule B1"),
    }

    let req_auth = IntentRequest {
        raw_utterance: "restart network".to_string(),
        kind: IntentKind::ExecuteAction,
        slots: slots_exec,
        confidence: 0.95,
        execution_authorized: true,
    };
    let res_auth = broker.dispatch(req_auth);
    assert!(matches!(res_auth, IntentResult::Executed { .. }));
}

#[test]
fn test_domain_11_desktop_coordinator() {
    let mut coordinator = DesktopCoordinator::new();
    assert_eq!(coordinator.theme, ThemeMode::Dark);

    coordinator.set_theme(ThemeMode::Light);
    assert_eq!(coordinator.theme, ThemeMode::Light);
    let css = coordinator.render_theme_css_variables();
    assert!(css.contains("--bg-primary"));

    coordinator.set_glow_state(AriaEdgeGlowState::Thinking);
    assert_eq!(coordinator.glow_state, AriaEdgeGlowState::Thinking);
    let params = coordinator.glow_parameters();
    assert_eq!(params.color_hex, "#818cf8");

    let id =
        coordinator.post_notification("Alert", "System update ready", UrgencyLevel::Normal, 1000);
    assert_eq!(coordinator.get_notifications().len(), 1);

    coordinator.set_do_not_disturb(true);
    assert!(coordinator.is_do_not_disturb());
    assert_eq!(coordinator.get_visible_notifications().len(), 0);

    coordinator.set_do_not_disturb(false);
    assert_eq!(coordinator.get_visible_notifications().len(), 1);

    assert!(coordinator.clear_notification(id));
    assert_eq!(coordinator.get_notifications().len(), 0);

    assert_eq!(coordinator.active_workspace(), 0);
    coordinator.switch_workspace(2).unwrap();
    assert_eq!(coordinator.active_workspace(), 2);
    assert!(coordinator.switch_workspace(99).is_err());
}

#[test]
fn test_domain_12_system_actions() {
    let mut actions = SystemActionsManager::new();
    assert_eq!(
        actions.get_system_setting("volume"),
        Some(&"70".to_string())
    );

    actions.set_system_setting("volume", "95").unwrap();
    assert_eq!(
        actions.get_system_setting("volume"),
        Some(&"95".to_string())
    );
    assert!(actions.set_system_setting("volume", "150").is_err());

    actions.set_alarm("alarm-1", "Wakeup", 200000);
    assert_eq!(actions.list_alarms().len(), 1);
    assert!(actions.cancel_alarm("alarm-1"));
    assert_eq!(actions.list_alarms().len(), 0);

    actions.set_timer("t1", "Tea Timer", 60);
    let finished_first = actions.tick_timers(30);
    assert!(finished_first.is_empty());
    let finished_second = actions.tick_timers(35);
    assert_eq!(finished_second, vec!["t1".to_string()]);

    assert!(!actions.a11y.high_contrast);
    assert!(actions.toggle_high_contrast());
    assert!(actions.a11y.high_contrast);

    actions.set_font_scale(1.5).unwrap();
    assert!(actions.a11y.large_text);
    assert!(actions.set_font_scale(5.0).is_err());

    assert!(!actions.audit_log().is_empty());
}

#[test]
fn test_domain_13_security_and_trust() {
    let mut sec = SecurityTrustEvaluator::new();
    sec.set_golden_pcr(1, "digest-abc-123");

    assert!(sec.verify_pcr(1, "digest-abc-123"));
    assert!(!sec.verify_pcr(1, "digest-tampered"));

    let audit_trail = [
        "booted kernel",
        "initialized ai-capabilityd",
        "started accountd",
    ];
    assert!(sec.evaluate_audit_chain(&audit_trail));

    let root = sec.compute_merkle_root(&["hash1", "hash2", "hash3"]);
    assert_eq!(root.len(), 64);

    assert_eq!(sec.get_secure_boot_mode(), SecureBootMode::DeployedMode);
    assert!(sec.is_secure_boot_active());

    sec.revoke_binary_hash("DEADBEEF1234");
    assert!(sec.is_binary_revoked("deadbeef1234"));
    assert!(!sec.is_binary_revoked("cafebeef1234"));

    let posture = sec.generate_posture_report();
    assert_eq!(posture.overall_trust_score, 100);
    assert_eq!(posture.revocation_db_count, 1);
}

#[test]
fn test_domain_14_release_installer_and_rollback() {
    let mut updater = UpdateController::new("42.0.0");
    assert_eq!(updater.boot_counter, 3);
    assert!(!updater.rolled_back);
    assert_eq!(updater.active_slot(), BootSlot::SlotA);

    let manifest_same = ReleaseManifest {
        os_version: "42.0.0".to_string(),
        image_digest: "sha256:111".to_string(),
        build_id: "b1".to_string(),
        channel: "stable".to_string(),
        min_upgrade_version: "41.0.0".to_string(),
        changelog_summary: "summary".to_string(),
    };
    assert!(updater.validate_manifest(&manifest_same).is_err());

    let manifest_new = ReleaseManifest {
        os_version: "42.1.0".to_string(),
        image_digest: "sha256:222".to_string(),
        build_id: "b2".to_string(),
        channel: "stable".to_string(),
        min_upgrade_version: "42.0.0".to_string(),
        changelog_summary: "summary".to_string(),
    };
    assert!(updater.validate_manifest(&manifest_new).is_ok());

    let target = updater.stage_update(&manifest_new).unwrap();
    assert_eq!(target, BootSlot::SlotB);
    assert_eq!(updater.deployment_state(), DeploymentState::Staged);

    updater.pin_deployment("deploy-42.0.0");
    assert!(updater.is_pinned("deploy-42.0.0"));
    assert!(!updater.is_pinned("deploy-unknown"));

    assert!(!updater.record_boot_failure());
    assert_eq!(updater.boot_counter, 2);
    assert!(!updater.record_boot_failure());
    assert_eq!(updater.boot_counter, 1);
    assert!(updater.record_boot_failure());
    assert_eq!(updater.boot_counter, 0);
    assert!(updater.rolled_back);
    assert_eq!(updater.deployment_state(), DeploymentState::RolledBack);
}

#[test]
fn test_domain_15_hardware_power() {
    let mut hw = HardwarePowerManager::new();
    assert_eq!(hw.get_profile(), PowerProfile::Balanced);

    hw.set_profile(PowerProfile::Performance);
    assert_eq!(hw.get_profile(), PowerProfile::Performance);

    assert_eq!(hw.get_battery_charge_limit(), 80);
    hw.set_battery_charge_limit(85).unwrap();
    assert_eq!(hw.get_battery_charge_limit(), 85);
    assert!(hw.set_battery_charge_limit(30).is_err());

    assert!(!hw.is_thermal_throttled());
    hw.set_thermal_throttle(true);
    assert!(hw.is_thermal_throttled());

    hw.record_thermal_reading("cpu-die", 65.0, 1000);
    assert_eq!(hw.evaluate_thermal_severity(), ThermalSeverity::Normal);
    hw.record_thermal_reading("cpu-die", 88.0, 1001);
    assert_eq!(hw.evaluate_thermal_severity(), ThermalSeverity::Throttled);
    assert!(hw.is_thermal_throttled());

    hw.update_battery_telemetry(98, 25, PowerSource::Battery);
    assert_eq!(hw.get_power_source(), PowerSource::Battery);
    assert_eq!(hw.get_battery_health(), 98);
    assert_eq!(hw.get_battery_cycles(), 25);

    let lease = hw.acquire_sleep_inhibition("backup-tool", "System backup in progress", 300, 1000);
    assert_eq!(hw.active_inhibition_count(1000), 1);
    assert!(hw.is_sleep_inhibited(1050));
    assert!(hw.release_sleep_inhibition(&lease));
    assert_eq!(hw.active_inhibition_count(1000), 0);
    assert!(!hw.is_sleep_inhibited(1050));
}

#[test]
fn test_domain_16_qa_conformance() {
    let report = QaConformanceRunner::run_full_suite();
    assert_eq!(report.failed_checks, 0);
    assert_eq!(report.passed_checks, 12);
    assert_eq!(report.details.len(), 12);
    assert_eq!(report.checks.len(), 12);
}
