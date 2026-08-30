//! Comprehensive Integration Tests for Phase 3.1 Closed-Loop Self-Healing Engine.
//!
//! Validates:
//! 1. Anti-Flap Duration Filter (transient single-failure blip suppressed; 2 consecutive cycles trigger).
//! 2. Notify-Tier Execution & 3-Stage Audit Sequence (pre-dispatch intent -> execution -> verified).
//! 3. Per-Entity 3-Attempt Budget & Hard Lockout Escalation on 4th trigger.
//! 4. Global Remediation Throttle (5 actions in window -> 6th hard-halted by circuit breaker).
//! 5. KillSwitch Engagement Prior to Plan Synthesis (complete silencing).
//! 6. Confirm-Tier Boundary Holding (sysctl / high-blast changes proposal-only, never auto-executed).

use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use capability_layer::audit::AuditLog;
use capability_layer::handler::{Handler, HandlerConfig};
use capability_layer::killswitch::KillSwitch;
use capability_layer::scope::ScopedPath;
use capability_layer::server::Server;
use control_plane::client::CapabilityClient;
use control_plane::policy::ApprovalTier;
use control_plane::remediation::{RemediationAgent, RemediationConfig};
use control_plane::self_healing::{
    EntityIncidentState, SelfHealingConfig, SelfHealingEngine, SelfHealingOutcome,
};
use parking_lot::Mutex;
use rusqlite::Connection;
use tempfile::tempdir;

fn setup_test_daemon(
    temp: &tempfile::TempDir,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let sock = temp.path().join("cap.sock");
    let audit_log = temp.path().join("audit.jsonl");
    let lock_file = temp.path().join("kill.lock");
    let work_root = temp.path().join("scope");
    fs::create_dir_all(&work_root).unwrap();

    let h_scope = ScopedPath::new(&work_root).unwrap();
    let h_audit = AuditLog::open(&audit_log).unwrap();
    let h_kill = KillSwitch::new(&lock_file);
    let handler = Handler::new(
        h_scope,
        h_audit,
        h_kill,
        HandlerConfig {
            approval_required: false,
            protected_units: vec!["ssh.service".into()],
            dry_run: true,
        },
    );

    let srv_sock = sock.clone();
    thread::spawn(move || {
        let server = Server::bind(srv_sock, Arc::new(Mutex::new(handler))).unwrap();
        server.serve().ok();
    });

    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    (sock, audit_log, lock_file)
}

fn create_test_db_with_service(
    conn: &Connection,
    unit: &str,
    active_state: &str,
    sub_state: &str,
    failure_count: usize,
) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS units (
            name TEXT PRIMARY KEY,
            load_state TEXT,
            active_state TEXT,
            sub_state TEXT,
            enabled TEXT,
            unit_file_preset TEXT,
            description TEXT,
            since_ts TEXT,
            synced_at TEXT
        );
        CREATE TABLE IF NOT EXISTS unit_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            unit TEXT,
            kind TEXT,
            old_value TEXT,
            new_value TEXT,
            ts TEXT
        );
        CREATE TABLE IF NOT EXISTS packages (
            name TEXT PRIMARY KEY,
            version TEXT,
            release TEXT,
            arch TEXT,
            summary TEXT,
            install_time INTEGER,
            synced_at TEXT
        );
        CREATE TABLE IF NOT EXISTS pkg_requires (
            package TEXT,
            capability TEXT,
            version_constraint TEXT
        );
        CREATE TABLE IF NOT EXISTS pkg_provides (
            package TEXT,
            capability TEXT,
            version_constraint TEXT
        );",
    )
    .unwrap();

    let now_ts = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO units (name, load_state, active_state, sub_state, enabled, since_ts, synced_at)
         VALUES (?1, 'loaded', ?2, ?3, 'enabled', ?4, ?4)",
        rusqlite::params![unit, active_state, sub_state, now_ts],
    )
    .unwrap();

    for i in 0..failure_count {
        let event_ts = (chrono::Utc::now()
            - chrono::Duration::seconds((failure_count - i) as i64 * 5))
        .to_rfc3339();
        conn.execute(
            "INSERT INTO unit_events (unit, kind, old_value, new_value, ts)
             VALUES (?1, 'sub_state', 'running', 'failed', ?2)",
            rusqlite::params![unit, event_ts],
        )
        .unwrap();
    }
}

#[test]
fn test_1_anti_flap_single_blip_suppressed_consecutive_triggers() {
    let temp = tempdir().unwrap();
    let (sock, audit_log, lock_file) = setup_test_daemon(&temp);
    let db_path = temp.path().join("wm.db");
    let conn = Connection::open(&db_path).unwrap();

    create_test_db_with_service(&conn, "transient-worker.service", "failed", "failed", 2);

    let client = CapabilityClient::new(&sock, "self-heal-test");
    let killswitch = KillSwitch::new(&lock_file);
    let rem_agent = RemediationAgent::new(RemediationConfig::default());
    let mut engine = SelfHealingEngine::new(
        SelfHealingConfig {
            db_path: db_path.clone(),
            anti_flap_consecutive_cycles: 2,
            worklog_path: Some(audit_log.clone()),
            ..Default::default()
        },
        killswitch,
        client,
        rem_agent,
    );

    // CYCLE 1: First observation of degraded state -> MUST be suppressed by anti-flap
    let report_1 = engine.evaluate_and_heal();
    assert_eq!(report_1.outcomes.len(), 1);
    match &report_1.outcomes[0] {
        SelfHealingOutcome::AntiFlapSuppressed {
            entity,
            consecutive_cycles,
            required_cycles,
        } => {
            assert_eq!(entity, "transient-worker.service");
            assert_eq!(*consecutive_cycles, 1);
            assert_eq!(*required_cycles, 2);
        }
        other => panic!("Expected AntiFlapSuppressed on cycle 1, got {:?}", other),
    }

    // CYCLE 2: Second consecutive observation of degraded state -> MUST trigger remediation
    let report_2 = engine.evaluate_and_heal();
    assert_eq!(report_2.outcomes.len(), 1);
    match &report_2.outcomes[0] {
        SelfHealingOutcome::ExecutedNotifyTier {
            entity, verified, ..
        } => {
            assert_eq!(entity, "transient-worker.service");
            assert!(*verified);
        }
        other => panic!("Expected ExecutedNotifyTier on cycle 2, got {:?}", other),
    }
}

#[test]
fn test_2_notify_tier_execution_and_three_stage_audit_sequence() {
    let temp = tempdir().unwrap();
    let (sock, audit_log, lock_file) = setup_test_daemon(&temp);
    let db_path = temp.path().join("wm.db");
    let conn = Connection::open(&db_path).unwrap();

    create_test_db_with_service(&conn, "failing-app.service", "failed", "failed", 3);

    let client = CapabilityClient::new(&sock, "self-heal-test");
    let killswitch = KillSwitch::new(&lock_file);
    let rem_agent = RemediationAgent::new(RemediationConfig::default());
    let mut engine = SelfHealingEngine::new(
        SelfHealingConfig {
            db_path: db_path.clone(),
            anti_flap_consecutive_cycles: 1, // trigger on cycle 1 for audit inspection
            worklog_path: Some(audit_log.clone()),
            ..Default::default()
        },
        killswitch,
        client,
        rem_agent,
    );

    let report = engine.evaluate_and_heal();
    assert_eq!(report.outcomes.len(), 1);

    // Verify outcome was executed notify tier
    assert!(matches!(
        &report.outcomes[0],
        SelfHealingOutcome::ExecutedNotifyTier {
            entity,
            verified: true,
            ..
        } if entity == "failing-app.service"
    ));

    // Verify 3-stage audit ordering in worklog file
    let worklog_content = fs::read_to_string(&audit_log).unwrap();
    assert!(
        worklog_content.contains("remediation.intent_dispatched"),
        "Missing Stage 1 pre-dispatch audit"
    );
    assert!(
        worklog_content.contains("service.restart")
            || worklog_content.contains("failing-app.service"),
        "Missing Stage 2 capability execution"
    );
    assert!(
        worklog_content.contains("remediation.completed_verified"),
        "Missing Stage 3 post-verification audit"
    );

    // Assert ordering: intent_dispatched appears before completed_verified
    let idx_intent = worklog_content
        .find("remediation.intent_dispatched")
        .unwrap();
    let idx_verified = worklog_content
        .find("remediation.completed_verified")
        .unwrap();
    assert!(
        idx_intent < idx_verified,
        "Stage 1 pre-dispatch audit must appear before Stage 3 verification audit"
    );
}

#[test]
fn test_3_persistent_failure_budget_lockout_after_three_attempts() {
    let temp = tempdir().unwrap();
    let (sock, audit_log, lock_file) = setup_test_daemon(&temp);
    let db_path = temp.path().join("wm.db");
    let conn = Connection::open(&db_path).unwrap();

    create_test_db_with_service(&conn, "permanent-crash.service", "failed", "failed", 3);

    let client = CapabilityClient::new(&sock, "self-heal-test");
    let killswitch = KillSwitch::new(&lock_file);
    let rem_agent = RemediationAgent::new(RemediationConfig::default());
    let mut engine = SelfHealingEngine::new(
        SelfHealingConfig {
            db_path: db_path.clone(),
            anti_flap_consecutive_cycles: 1,
            max_entity_attempts: 3,
            worklog_path: Some(audit_log.clone()),
            ..Default::default()
        },
        killswitch,
        client,
        rem_agent,
    );

    // Simulate 3 failed remediation attempts
    for attempt in 1..=3 {
        let mut state = EntityIncidentState::new("permanent-crash.service");
        state.attempt_timestamps.push(chrono::Utc::now());
        state.consecutive_degraded_cycles = 1;
        engine
            .incident_states_mut()
            .entry("permanent-crash.service".into())
            .or_insert(state)
            .attempt_timestamps
            .push(chrono::Utc::now());
        println!("Recorded failed attempt #{attempt} for permanent-crash.service");
    }

    // 4th Trigger: MUST be hard locked out with CRITICAL status
    let report_4 = engine.evaluate_and_heal();
    assert_eq!(report_4.outcomes.len(), 1);
    match &report_4.outcomes[0] {
        SelfHealingOutcome::EntityLockedOut {
            entity,
            attempts,
            message,
        } => {
            assert_eq!(entity, "permanent-crash.service");
            assert!(*attempts >= 3);
            assert!(message.contains("exceeded attempt budget"));
        }
        other => panic!("Expected EntityLockedOut on 4th attempt, got {:?}", other),
    }

    // Verify CRITICAL audit entry in worklog
    let worklog_content = fs::read_to_string(&audit_log).unwrap();
    assert!(worklog_content.contains("remediation.entity_locked_out"));
    assert!(worklog_content.contains("Remediation budget exhausted (3 failed attempts)"));
}

#[test]
fn test_4_global_throttle_circuit_breaker_five_actions_cap() {
    let temp = tempdir().unwrap();
    let (sock, audit_log, lock_file) = setup_test_daemon(&temp);
    let db_path = temp.path().join("wm.db");
    let conn = Connection::open(&db_path).unwrap();

    create_test_db_with_service(&conn, "worker-6.service", "failed", "failed", 3);

    let client = CapabilityClient::new(&sock, "self-heal-test");
    let killswitch = KillSwitch::new(&lock_file);
    let rem_agent = RemediationAgent::new(RemediationConfig::default());
    let mut engine = SelfHealingEngine::new(
        SelfHealingConfig {
            db_path: db_path.clone(),
            anti_flap_consecutive_cycles: 1,
            global_action_cap: 5,
            global_window_secs: 600,
            worklog_path: Some(audit_log.clone()),
            ..Default::default()
        },
        killswitch,
        client,
        rem_agent,
    );

    // Simulate 5 successful autonomous actions across the system within window
    for _ in 0..5 {
        engine.global_actions_history_mut().push(chrono::Utc::now());
    }

    // 6th Action: MUST trigger GlobalThrottleEngaged circuit breaker
    let report_6 = engine.evaluate_and_heal();
    assert_eq!(report_6.outcomes.len(), 1);
    match &report_6.outcomes[0] {
        SelfHealingOutcome::GlobalThrottleEngaged {
            actions_in_window,
            cap,
            message,
        } => {
            assert_eq!(*actions_in_window, 5);
            assert_eq!(*cap, 5);
            assert!(message.contains("Global remediation throttle exceeded"));
        }
        other => panic!(
            "Expected GlobalThrottleEngaged on 6th action, got {:?}",
            other
        ),
    }

    // Verify circuit breaker CRITICAL alert written to worklog
    let worklog_content = fs::read_to_string(&audit_log).unwrap();
    assert!(worklog_content.contains("remediation.global_throttle_engaged"));
}

#[test]
fn test_5_killswitch_engaged_silences_loop_before_synthesis() {
    let temp = tempdir().unwrap();
    let (sock, audit_log, lock_file) = setup_test_daemon(&temp);
    let db_path = temp.path().join("wm.db");
    let conn = Connection::open(&db_path).unwrap();

    // Create degraded state
    create_test_db_with_service(&conn, "unhealthy.service", "failed", "failed", 5);

    // Engage killswitch
    let mut killswitch = KillSwitch::new(&lock_file);
    killswitch.engage().unwrap();
    assert!(killswitch.is_engaged());

    let client = CapabilityClient::new(&sock, "self-heal-test");
    let rem_agent = RemediationAgent::new(RemediationConfig::default());
    let mut engine = SelfHealingEngine::new(
        SelfHealingConfig {
            db_path: db_path.clone(),
            anti_flap_consecutive_cycles: 1,
            worklog_path: Some(audit_log.clone()),
            ..Default::default()
        },
        killswitch,
        client,
        rem_agent,
    );

    let report = engine.evaluate_and_heal();
    assert!(report.killswitch_engaged);
    assert_eq!(
        report.outcomes,
        vec![SelfHealingOutcome::DisabledByKillswitch]
    );

    // Verify zero dispatches or plans were written to worklog
    if audit_log.exists() {
        let worklog_content = fs::read_to_string(&audit_log).unwrap();
        assert!(!worklog_content.contains("remediation.intent_dispatched"));
        assert!(!worklog_content.contains("service.restart"));
    }
}

#[test]
fn test_6_confirm_tier_boundary_holding_stops_at_proposal() {
    let temp = tempdir().unwrap();
    let (sock, audit_log, lock_file) = setup_test_daemon(&temp);

    // Create DB where a sysctl parameter remediation is indicated
    let db_path = temp.path().join("wm.db");
    let conn = Connection::open(&db_path).unwrap();
    create_test_db_with_service(&conn, "sysctl:net.ipv4.ip_forward", "failed", "failed", 3);

    let client = CapabilityClient::new(&sock, "self-heal-test");
    let killswitch = KillSwitch::new(&lock_file);
    let rem_agent = RemediationAgent::new(RemediationConfig::default());
    let mut engine = SelfHealingEngine::new(
        SelfHealingConfig {
            db_path: db_path.clone(),
            anti_flap_consecutive_cycles: 1,
            worklog_path: Some(audit_log.clone()),
            ..Default::default()
        },
        killswitch,
        client,
        rem_agent,
    );

    let report = engine.evaluate_and_heal();
    assert_eq!(report.outcomes.len(), 1);
    match &report.outcomes[0] {
        SelfHealingOutcome::AwaitingConfirmation {
            entity,
            tier,
            summary,
            preview_diff,
            ..
        } => {
            assert!(entity.contains("net.ipv4.ip_forward"));
            assert_eq!(*tier, ApprovalTier::Confirm);
            assert!(summary.contains("net.ipv4.ip_forward"));
            assert!(preview_diff.contains("DIRECTIONER-OS AI ENGINEER — PLAN PREVIEW"));
        }
        other => panic!(
            "Expected AwaitingConfirmation for Confirm-tier sysctl, got {:?}",
            other
        ),
    }

    // Verify proposal logged as awaiting confirmation and NOT executed
    let worklog_content = fs::read_to_string(&audit_log).unwrap();
    assert!(worklog_content.contains("remediation.proposal_awaiting_confirmation"));
    assert!(!worklog_content.contains("remediation.intent_dispatched"));
}
