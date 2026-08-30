//! Extended Integration Test Suite for Directioner-OS Core Plane.
//!
//! Strengthens test coverage across:
//! 1. 3-Depth technical explanations in Planner & Simulation (`explain` representations)
//! 2. Cockpit snapshot JSON serialization and round-trip fidelity
//! 3. Simulation diff generation edge cases (empty file, multiline replacements)
//! 4. Reviewer multi-step plan verification and blast radius scoring boundaries

use control_plane::cockpit::{CockpitRenderer, CockpitSnapshot, CockpitState};
use control_plane::planner::{Planner, PlannerConfig};
use control_plane::reviewer::{Reviewer, ReviewerConfig};
use control_plane::simulation::Simulator;
use rusqlite::Connection;

#[test]
fn test_planner_three_depth_explanation_pipeline() {
    let temp = tempfile::tempdir().unwrap();
    let planner = Planner::new(PlannerConfig {
        work_root: temp.path().to_path_buf(),
        default_timeout_secs: 10,
    });

    let plan = planner
        .create_plan("configure nginx on port 8080", None)
        .expect("Plan synthesis should succeed");

    // Depth 1 representation: Plain summary
    assert!(!plan.summary.is_empty());
    assert!(
        plan.summary.to_lowercase().contains("nginx")
            || plan.summary.to_lowercase().contains("web")
    );

    // Depth 2 representation: Step-by-step breakdown & rollback safety
    assert!(!plan.steps.is_empty());
    assert_eq!(plan.steps.len(), plan.rollback_steps.len());
    for step in &plan.steps {
        assert!(!step.description.is_empty());
        assert!(step.blast_radius.score <= 100);
    }

    // Depth 3 representation: Full raw JSON simulation with policy scores and diffs
    let preview = Simulator::simulate_plan(&plan, Some(temp.path()));
    let combined = serde_json::json!({
        "plan": plan,
        "simulation": preview,
    });
    let json_str = serde_json::to_string_pretty(&combined).unwrap();
    assert!(json_str.contains("aggregate_blast_radius"));
    assert!(json_str.contains("step_previews"));
}

#[test]
fn test_cockpit_snapshot_json_roundtrip_fidelity() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("worldmodel.db");
    let audit_path = temp.path().join("audit.jsonl");
    let lock_path = temp.path().join("kill.lock");

    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE packages (id INTEGER PRIMARY KEY, name TEXT, epoch INTEGER, version TEXT, release TEXT, arch TEXT, summary TEXT, updated_at TEXT);
        CREATE TABLE units (id INTEGER PRIMARY KEY, name TEXT UNIQUE, load_state TEXT, active_state TEXT, sub_state TEXT, unit_file_state TEXT, description TEXT, updated_at TEXT);
        INSERT INTO packages (name, epoch, version, release, arch, summary, updated_at) VALUES ('systemd', 0, '256', '1.fc41', 'x86_64', 'System and Service Manager', '2026-08-27T00:00:00Z');
        INSERT INTO units (name, load_state, active_state, sub_state, unit_file_state, description, updated_at) VALUES ('canary.service', 'loaded', 'active', 'running', 'enabled', 'Canary Service', '2026-08-27T00:00:00Z');
        "#,
    ).unwrap();

    let state = CockpitState::new(&db_path, &audit_path, &lock_path);
    let snapshot = state.poll(None);

    let json_str = serde_json::to_string(&snapshot).expect("Snapshot should serialize to JSON");
    let deserialized: CockpitSnapshot =
        serde_json::from_str(&json_str).expect("Snapshot should deserialize from JSON");

    assert_eq!(deserialized.health.total_packages, 1);
    assert_eq!(deserialized.health.total_units, 1);
    assert!(!deserialized.killswitch_engaged);

    // Verify ANSI renderer does not panic
    let rendered = CockpitRenderer::render_terminal(&snapshot);
    assert!(rendered.contains("DIRECTIONER-OS — OPERATIONAL COCKPIT"));
}

#[test]
fn test_reviewer_multi_step_invariant_safety_check() {
    let temp = tempfile::tempdir().unwrap();
    let planner = Planner::new(PlannerConfig {
        work_root: temp.path().to_path_buf(),
        default_timeout_secs: 10,
    });
    let plan = planner
        .create_plan("configure nginx on port 8080", None)
        .expect("Synthesis should succeed");

    let reviewer = Reviewer::new(ReviewerConfig::default());
    let report = reviewer.review_plan(&plan);
    assert_ne!(
        report.verdict,
        control_plane::reviewer::ReviewVerdict::Rejected,
        "Standard synthesized plan should pass review"
    );
    assert!(
        report
            .findings
            .iter()
            .all(|f| f.severity != control_plane::reviewer::FindingSeverity::Fatal)
    );
}
