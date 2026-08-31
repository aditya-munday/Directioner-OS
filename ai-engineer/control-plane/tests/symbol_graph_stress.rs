//! Comprehensive Stress Test for IncrementalSymbolWatcher & WorkspaceSymbolGraph (Phase 2.2 / Hardening).
//!
//! Generates a synthetic workspace with 60+ files across 6 distinct crates, validating:
//! 1. Initial bulk indexing performance and symbol completeness (>300 symbols).
//! 2. Rapid sequential file modifications with precise $O(1)$ incremental re-indexing.
//! 3. Dynamic file additions and deletions correctly invalidating the cache.
//! 4. Symbol resolution accuracy across cross-crate dependencies.

use std::fs;
use std::path::Path;
use std::time::Instant;

use control_plane::symbol_graph::{IncrementalSymbolGraph, SymbolKind};
use tempfile::tempdir;

fn generate_synthetic_workspace(root: &Path) -> Vec<String> {
    let crates = [
        "core-lib",
        "network-stack",
        "auth-engine",
        "storage-driver",
        "telemetry-agent",
        "api-gateway",
    ];

    let mut all_files = Vec::new();

    for crate_name in crates {
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

        // lib.rs
        let mut lib_rs = String::from("//! Root crate module\n");
        for file_idx in 0..10 {
            lib_rs.push_str(&format!("pub mod mod_{file_idx};\n"));
        }
        fs::write(src_dir.join("lib.rs"), lib_rs).unwrap();
        all_files.push(src_dir.join("lib.rs").to_string_lossy().to_string());

        // 10 modules per crate = 60 modules total
        for file_idx in 0..10 {
            let mod_name = format!("mod_{file_idx}");
            let file_path = src_dir.join(format!("{mod_name}.rs"));

            let content = format!(
                r#"//! Module {mod_name} in {crate_name}

pub struct Config{file_idx} {{
    pub id: u64,
    pub active: bool,
}}

pub enum Status{file_idx} {{
    Init,
    Running,
    Stopped,
}}

pub trait Handler{file_idx} {{
    fn handle(&self) -> bool;
}}

impl Config{file_idx} {{
    pub fn new(id: u64) -> Self {{
        Self {{ id, active: true }}
    }}

    pub fn is_valid(&self) -> bool {{
        self.active
    }}
}}

pub fn execute_{mod_name}() -> u64 {{
    42
}}

pub type ResultCode{file_idx} = Result<Config{file_idx}, String>;
"#
            );

            fs::write(&file_path, content).unwrap();
            all_files.push(file_path.to_string_lossy().to_string());
        }
    }

    all_files
}

#[test]
fn test_symbol_graph_synthetic_workspace_stress_and_incremental_reindex() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // 1. Generate 6 crates x (1 lib.rs + 10 modules) = 66 Rust source files
    let file_paths = generate_synthetic_workspace(root);
    assert_eq!(file_paths.len(), 66);

    // 2. Initial Full Scan & Indexing
    let mut watcher = IncrementalSymbolGraph::new(root);
    let start = Instant::now();
    let reindexed_initial = watcher
        .sync_incremental()
        .expect("Initial workspace sync must succeed");
    let initial_duration = start.elapsed();

    println!(
        "Initial scan of 66 files completed in {:?} (re-indexed {} files)",
        initial_duration, reindexed_initial
    );

    assert_eq!(reindexed_initial, 66);
    let graph = watcher.build_graph();

    // Validate symbols indexed across crates
    assert_eq!(graph.crates.len(), 6);
    assert!(
        graph.symbols.len() >= 360,
        "Expected at least 360 symbols across 66 files, found {}",
        graph.symbols.len()
    );

    // Verify lookup by name
    let found_configs = graph.find_by_name("Config0");
    assert_eq!(
        found_configs.len(),
        6,
        "Config0 should exist in each of the 6 crates"
    );

    // Verify lookup by qualified path
    let qpath = "core_lib::execute_mod_3";
    let found_fn = graph
        .find_by_qualified_path(qpath)
        .expect("Qualified path lookup must find function");
    assert_eq!(found_fn.kind, SymbolKind::Function);
    assert_eq!(found_fn.crate_name, "core-lib");

    let method_qpath = "core_lib::Config3::new";
    let found_method = graph
        .find_by_qualified_path(method_qpath)
        .expect("Qualified path lookup must find method");
    assert_eq!(found_method.kind, SymbolKind::Method);
    assert_eq!(found_method.crate_name, "core-lib");

    // 3. No-op Sync Verification (should re-index 0 files)
    let reindexed_noop = watcher.sync_incremental().expect("No-op sync must succeed");
    assert_eq!(
        reindexed_noop, 0,
        "No-op sync without modifications must reindex 0 files"
    );

    // 4. Rapid Sequential Incremental Edits Stress Test
    // Perform 15 sequential edits to random files across different crates
    for edit_round in 0..15 {
        let target_crate = match edit_round % 6 {
            0 => "core-lib",
            1 => "network-stack",
            2 => "auth-engine",
            3 => "storage-driver",
            4 => "telemetry-agent",
            _ => "api-gateway",
        };
        let target_mod = edit_round % 10;
        let file_to_modify = root
            .join(target_crate)
            .join("src")
            .join(format!("mod_{target_mod}.rs"));

        // Append a new function definition
        let new_fn_name = format!("dynamically_added_fn_round_{edit_round}");
        let appended_code = format!("\npub fn {new_fn_name}() -> bool {{ true }}\n");
        let mut existing_content = fs::read_to_string(&file_to_modify).unwrap();
        existing_content.push_str(&appended_code);
        fs::write(&file_to_modify, existing_content).unwrap();

        // Single file explicit update
        let update_start = Instant::now();
        let updated = watcher
            .update_file(&file_to_modify)
            .expect("update_file should succeed");
        let update_duration = update_start.elapsed();

        assert!(updated);
        assert!(
            update_duration.as_millis() < 50,
            "Single file update must complete in <50ms, took {:?}",
            update_duration
        );

        let updated_graph = watcher.build_graph();
        let found_new = updated_graph.find_by_name(&new_fn_name);
        assert_eq!(
            found_new.len(),
            1,
            "Newly added function '{}' must be queryable immediately",
            new_fn_name
        );
        assert_eq!(found_new[0].crate_name, target_crate);
    }

    // 5. Batch Sync after Multiple Simultaneous Modifications
    // Modify 5 files at once
    for idx in 0..5 {
        let path = root
            .join("storage-driver")
            .join("src")
            .join(format!("mod_{idx}.rs"));
        let mut content = fs::read_to_string(&path).unwrap();
        content.push_str("\npub struct BatchAddedStruct;\n");
        fs::write(&path, content).unwrap();
    }

    let batch_start = Instant::now();
    let reindexed_batch = watcher.sync_incremental().expect("Batch sync must succeed");
    let batch_duration = batch_start.elapsed();

    assert_eq!(
        reindexed_batch, 5,
        "Batch sync must only re-index exactly the 5 modified files"
    );
    assert!(
        batch_duration.as_millis() < 100,
        "5-file batch sync must complete in <100ms, took {:?}",
        batch_duration
    );

    // 6. Dynamic File Deletion & Invalidation
    let file_to_delete = root.join("auth-engine").join("src").join("mod_7.rs");
    fs::remove_file(&file_to_delete).unwrap();

    let reindexed_after_delete = watcher
        .sync_incremental()
        .expect("Sync after delete must succeed");
    assert_eq!(reindexed_after_delete, 0);

    let graph_after_delete = watcher.build_graph();
    let deleted_symbol = graph_after_delete
        .symbols
        .iter()
        .find(|s| s.file_path == file_to_delete);
    assert!(
        deleted_symbol.is_none(),
        "Symbols from deleted file must be purged from graph"
    );

    // 7. Dynamic File Creation & Inclusion
    let new_file_path = root.join("api-gateway").join("src").join("new_feature.rs");
    fs::write(
        &new_file_path,
        "pub struct NewGatewayFeature { pub enabled: bool }\n",
    )
    .unwrap();

    let reindexed_after_add = watcher
        .sync_incremental()
        .expect("Sync after add must succeed");
    assert_eq!(
        reindexed_after_add, 1,
        "Sync must detect and index exactly 1 new file"
    );

    let graph_after_add = watcher.build_graph();
    let added_symbol = graph_after_add.find_by_name("NewGatewayFeature");
    assert_eq!(added_symbol.len(), 1);
    assert_eq!(added_symbol[0].crate_name, "api-gateway");
}
