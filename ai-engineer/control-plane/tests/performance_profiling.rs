//! Performance Profiling & Baseline Benchmark Suite.
//!
//! Measures and records baseline performance metrics under realistic heavy load for:
//! 1. World Model Sync & Reverse-Dependency Graph Queries (1,000 packages, 5,000 edges, 200 units).
//! 2. Incremental Symbol Graph Scanning & Re-indexing (100 files, 5 crates).
//! 3. PlanDraftValidator Layer 1-4 Validation Pipeline (1,000 complex draft payloads).

use std::fs;
use std::thread::sleep;
use std::time::{Duration, Instant};

use control_plane::draft_validator::PlanDraftValidator;
use control_plane::plan::PlanStatus;
use control_plane::symbol_graph::IncrementalSymbolGraph;
use tempfile::tempdir;

#[test]
fn test_performance_profile_world_model_large_inventory() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("profile_wm.db");

    let conn = world_model::db::open(&db_path).unwrap();

    // 1. Ingest 1,000 packages with 5,000 dependency edges
    let total_pkgs = 1000;
    let total_deps_per_pkg = 5;
    let sync_start = Instant::now();

    conn.execute("BEGIN TRANSACTION;", []).unwrap();
    for i in 0..total_pkgs {
        let pkg_name = format!("pkg-service-{:04}", i);
        conn.execute(
            "INSERT INTO packages (name, evr, arch, summary, synced_at) VALUES (?1, '1.0.0', 'x86_64', 'Synthetic profile package', datetime('now'))",
            [&pkg_name],
        ).unwrap();

        for d in 0..total_deps_per_pkg {
            let dep_idx = (i + d * 37) % total_pkgs;
            let dep_name = format!("pkg-service-{:04}", dep_idx);
            let _ = conn.execute(
                "INSERT OR IGNORE INTO pkg_requires (pkg, dep, flags, version) VALUES (?1, ?2, 'GE', '1.0')",
                [&pkg_name, &dep_name],
            );
        }
    }

    // Ingest 200 systemd units
    for u in 0..200 {
        let unit_name = format!("daemon-{:03}.service", u);
        conn.execute(
            "INSERT INTO units (name, load_state, active_state, sub_state, enabled, since_ts) VALUES (?1, 'loaded', 'active', 'running', 'enabled', '1700000000')",
            [&unit_name],
        ).unwrap();
    }
    conn.execute("COMMIT;", []).unwrap();

    let sync_duration = sync_start.elapsed();
    println!(
        "\n[PERF PROFILE] World Model Batch Ingestion (1,000 pkgs, 5,000 edges, 200 units): {:.2} ms",
        sync_duration.as_secs_f64() * 1000.0
    );
    assert!(
        sync_duration.as_millis() < 500,
        "World Model batch sync must complete within 500ms (took {:?})",
        sync_duration
    );

    // 2. Profile reverse-dependency queries
    let query_iterations = 500;
    let query_start = Instant::now();
    for i in 0..query_iterations {
        let target_dep = format!("pkg-service-{:04}", i % total_pkgs);
        let mut stmt = conn
            .prepare("SELECT pkg FROM pkg_requires WHERE dep = ?1")
            .unwrap();
        let rows = stmt
            .query_map([&target_dep], |row| row.get::<_, String>(0))
            .unwrap();
        let count = rows.count();
        assert!(count > 0, "Dependency should resolve dependents");
    }
    let query_duration = query_start.elapsed();
    let avg_query_us = query_duration.as_micros() as f64 / query_iterations as f64;
    println!(
        "[PERF PROFILE] Reverse-Dependency Graph Queries (500 queries): {:.2} µs/query (total: {:.2} ms)",
        avg_query_us,
        query_duration.as_secs_f64() * 1000.0
    );
    assert!(
        avg_query_us < 2000.0,
        "Average reverse dependency query must be <2ms (took {:.2} µs)",
        avg_query_us
    );
}

#[test]
fn test_performance_profile_symbol_graph_large_workspace() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Generate synthetic 100-file workspace across 5 crates
    let crates = ["core", "net", "storage", "auth", "crypto"];
    let files_per_crate = 20;

    for crate_name in &crates {
        let crate_dir = root.join(crate_name);
        let src_dir = crate_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        // Cargo.toml
        let cargo_toml = format!(
            r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"
"#
        );
        fs::write(crate_dir.join("Cargo.toml"), cargo_toml).unwrap();

        for f in 0..files_per_crate {
            let file_path = src_dir.join(format!("mod_{:02}.rs", f));
            let content = format!(
                r#"//! Module {} in crate {}
pub struct ServiceContext_{} {{
    pub id: u64,
    pub active: bool,
}}

impl ServiceContext_{} {{
    pub fn new(id: u64) -> Self {{
        Self {{ id, active: true }}
    }}

    pub fn execute_operation(&self) -> bool {{
        self.active
    }}
}}

pub fn init_subsystem_{}(val: u64) -> ServiceContext_{} {{
    ServiceContext_{}::new(val)
}}
"#,
                f, crate_name, f, f, f, f, f
            );
            fs::write(&file_path, content).unwrap();
        }
    }

    // 1. Initial full scan of 100 files
    let mut watcher = IncrementalSymbolGraph::new(root);
    let scan_start = Instant::now();
    let reindexed = watcher
        .sync_incremental()
        .expect("Initial sync must succeed");
    let scan_duration = scan_start.elapsed();

    let graph = watcher.build_graph();
    println!(
        "\n[PERF PROFILE] Symbol Graph Initial Full Scan (100 files, 5 crates): {} files / {} symbols indexed in {:.2} ms",
        reindexed,
        graph.symbols.len(),
        scan_duration.as_secs_f64() * 1000.0
    );
    assert_eq!(reindexed, 100);
    assert!(
        graph.symbols.len() >= 300,
        "Expected at least 300 symbols, found {}",
        graph.symbols.len()
    );
    assert!(
        scan_duration.as_millis() < 100,
        "100-file initial scan must complete under 100ms (took {:?})",
        scan_duration
    );

    // 2. Incremental re-index performance (20 sequential single-file updates across distinct files)
    let reindex_start = Instant::now();
    for i in 0..20 {
        sleep(Duration::from_millis(5));
        let target_file = root
            .join("core")
            .join("src")
            .join(format!("mod_{:02}.rs", i));
        let updated_content = format!(
            r#"//! Updated iteration {}
pub struct ServiceContext_{:02} {{
    pub id: u64,
    pub generation: u32,
}}

impl ServiceContext_{:02} {{
    pub fn new(id: u64) -> Self {{
        Self {{ id, generation: {} }}
    }}
}}
"#,
            i, i, i, i
        );
        fs::write(&target_file, updated_content).unwrap();
        let changed = watcher
            .sync_incremental()
            .expect("Incremental sync must succeed");
        assert_eq!(changed, 1, "Only modified file must be re-indexed");
    }
    let reindex_duration = reindex_start.elapsed();
    let avg_reindex_ms = (reindex_duration.as_secs_f64() * 1000.0) / 20.0;

    println!(
        "[PERF PROFILE] Incremental Re-indexing (20 distinct file updates): {:.2} ms/update (total: {:.2} ms)",
        avg_reindex_ms,
        reindex_duration.as_secs_f64() * 1000.0
    );
    assert!(
        avg_reindex_ms < 50.0,
        "Incremental symbol update must be <50ms (took {:.2} ms)",
        avg_reindex_ms
    );
}

#[test]
fn test_performance_profile_validator_pipeline_throughput() {
    let raw_payload = r#"{
        "intent": "Deploy Prometheus Node Exporter on port 9100",
        "summary": "Installs node_exporter package, configures options, opens firewall, and starts service",
        "steps": [
            {
                "step_id": "step-1",
                "description": "Install exporter package",
                "capability": {
                    "kind": "PackageInstall",
                    "package": "golang-github-prometheus-node-exporter",
                    "approved": false
                },
                "timeout_secs": 60
            },
            {
                "step_id": "step-2",
                "description": "Write node exporter configuration",
                "capability": {
                    "kind": "FileWrite",
                    "path": "/etc/sysconfig/node_exporter",
                    "content": "OPTIONS=\"--web.listen-address=:9100\"",
                    "append": false
                }
            },
            {
                "step_id": "step-3",
                "description": "Restart node_exporter daemon",
                "capability": {
                    "kind": "ServiceRestart",
                    "unit": "node_exporter.service",
                    "approved": false
                }
            }
        ],
        "rollback_steps": [
            {
                "step_id": "rb-1",
                "description": "Stop node exporter service",
                "capability": {
                    "kind": "ServiceStop",
                    "unit": "node_exporter.service",
                    "approved": false
                },
                "triggers": ["step-3"]
            },
            {
                "step_id": "rb-2",
                "description": "Remove exporter package",
                "capability": {
                    "kind": "PackageRemove",
                    "package": "golang-github-prometheus-node-exporter",
                    "approved": false
                },
                "triggers": ["step-1"]
            }
        ]
    }"#;

    let iterations = 1000;

    let val_start = Instant::now();
    for _ in 0..iterations {
        let plan = PlanDraftValidator::validate_and_convert(raw_payload)
            .expect("Valid draft must succeed");
        assert_eq!(plan.status, PlanStatus::Draft);
    }
    let val_duration = val_start.elapsed();
    let throughput_per_sec = iterations as f64 / val_duration.as_secs_f64();
    let latency_us = val_duration.as_micros() as f64 / iterations as f64;

    println!(
        "\n[PERF PROFILE] PlanDraftValidator Layer 1-4 Pipeline ({} complex drafts): {:.2} µs/draft | Throughput: {:.0} drafts/sec",
        iterations, latency_us, throughput_per_sec
    );
    assert!(
        latency_us < 200.0,
        "Validator latency per draft must be <200µs (took {:.2} µs)",
        latency_us
    );
    assert!(
        throughput_per_sec > 5000.0,
        "Validator throughput must exceed 5,000 drafts/sec (achieved {:.0} drafts/sec)",
        throughput_per_sec
    );
}
