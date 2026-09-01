//! Autonomous Semantic Code Synthesis Test Suite (Phase 2 Deferred Item).
//!
//! Verifies that `CodeEngine` synthesizes semantically valid Rust repair plans
//! even when compiler diagnostics lack direct machine-applicable suggestions,
//! by leveraging AST symbol graph lookups, Levenshtein typo corrections, and type coercions.

use control_plane::code_engine::{CodeEngine, Diagnostic, DiagnosticSpan};
use control_plane::symbol_graph::{SymbolDefinition, SymbolKind, WorkspaceSymbolGraph};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_synthesize_semantic_patch_missing_import_e0425() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Create workspace structure
    let crate_a = root.join("crate_a/src");
    let crate_b = root.join("crate_b/src");
    fs::create_dir_all(&crate_a).unwrap();
    fs::create_dir_all(&crate_b).unwrap();

    let def_file = crate_a.join("lib.rs");
    fs::write(&def_file, "pub struct HealthChecker;\n").unwrap();

    let usage_file = crate_b.join("lib.rs");
    fs::write(
        &usage_file,
        "//! Crate B\n\npub fn run() {\n    let _ = HealthChecker;\n}\n",
    )
    .unwrap();

    // Construct SymbolGraph
    let mut graph = WorkspaceSymbolGraph::new();
    graph.symbols.push(SymbolDefinition {
        name: "HealthChecker".to_string(),
        crate_name: "crate_a".to_string(),
        module_path: "crate_a::HealthChecker".to_string(),
        kind: SymbolKind::Struct,
        file_path: def_file.clone(),
        line_start: 1,
        line_end: 1,
        signature: Some("pub struct HealthChecker;".to_string()),
        visibility: "pub".to_string(),
    });

    let engine = CodeEngine::new(root).with_symbol_graph(graph);

    let diag = Diagnostic {
        level: "error".to_string(),
        code: Some("E0425".to_string()),
        message: "cannot find value `HealthChecker` in this scope".to_string(),
        file_path: Some("crate_b/src/lib.rs".to_string()),
        line_number: Some(4),
        column_number: Some(13),
        suggested_replacement: None,
        spans: vec![DiagnosticSpan {
            file_path: "crate_b/src/lib.rs".to_string(),
            line_start: 4,
            line_end: 4,
            column_start: 13,
            column_end: 26,
            is_primary: true,
            label: Some("not found in this scope".to_string()),
            suggested_replacement: None,
        }],
        notes: vec![],
    };

    let plan = engine.synthesize_cross_crate_plan(&[diag], None);
    assert!(plan.is_some(), "Expected plan synthesized for E0425");
    let p = plan.unwrap();
    assert_eq!(p.steps.len(), 1);

    match &p.steps[0].capability {
        capability_layer::CapabilityKind::FileWrite { content, .. } => {
            assert!(
                content.contains("use crate_a::HealthChecker;"),
                "Synthesized content should include import statement:\n{content}"
            );
        }
        other => panic!("Expected FileWrite capability, got {other:?}"),
    }
}

#[test]
fn test_synthesize_semantic_patch_typo_correction() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();

    let file_path = src.join("main.rs");
    fs::write(
        &file_path,
        "fn main() {\n    let status = healt_status();\n}\n",
    )
    .unwrap();

    let mut graph = WorkspaceSymbolGraph::new();
    graph.symbols.push(SymbolDefinition {
        name: "health_status".to_string(),
        crate_name: "app".to_string(),
        module_path: "app::health_status".to_string(),
        kind: SymbolKind::Function,
        file_path: src.join("health.rs"),
        line_start: 1,
        line_end: 5,
        signature: Some("pub fn health_status() -> bool".to_string()),
        visibility: "pub".to_string(),
    });

    let engine = CodeEngine::new(root).with_symbol_graph(graph);

    let diag = Diagnostic {
        level: "error".to_string(),
        code: Some("E0425".to_string()),
        message: "cannot find function `healt_status` in this scope".to_string(),
        file_path: Some("src/main.rs".to_string()),
        line_number: Some(2),
        column_number: Some(18),
        suggested_replacement: None,
        spans: vec![DiagnosticSpan {
            file_path: "src/main.rs".to_string(),
            line_start: 2,
            line_end: 2,
            column_start: 18,
            column_end: 30,
            is_primary: true,
            label: Some("not found in this scope".to_string()),
            suggested_replacement: None,
        }],
        notes: vec![],
    };

    let plan = engine.synthesize_cross_crate_plan(&[diag], None);
    assert!(plan.is_some(), "Expected typo correction plan synthesized");
    let p = plan.unwrap();

    match &p.steps[0].capability {
        capability_layer::CapabilityKind::FileWrite { content, .. } => {
            assert!(
                content.contains("health_status()"),
                "Synthesized content should correct typo identifier:\n{content}"
            );
        }
        other => panic!("Expected FileWrite capability, got {other:?}"),
    }
}

#[test]
fn test_synthesize_semantic_patch_missing_trait_e0599() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();

    let file_path = src.join("main.rs");
    fs::write(
        &file_path,
        "fn main() {\n    let unit = get_unit();\n    unit.audit_record();\n}\n",
    )
    .unwrap();

    let mut graph = WorkspaceSymbolGraph::new();
    graph.symbols.push(SymbolDefinition {
        name: "audit_record".to_string(),
        crate_name: "core".to_string(),
        module_path: "core::traits::Auditable".to_string(),
        kind: SymbolKind::Trait,
        file_path: src.join("traits.rs"),
        line_start: 1,
        line_end: 10,
        signature: Some("pub trait Auditable { fn audit_record(&self); }".to_string()),
        visibility: "pub".to_string(),
    });

    let engine = CodeEngine::new(root).with_symbol_graph(graph);

    let diag = Diagnostic {
        level: "error".to_string(),
        code: Some("E0599".to_string()),
        message: "no method named `audit_record` found for struct `Unit` in the current scope"
            .to_string(),
        file_path: Some("src/main.rs".to_string()),
        line_number: Some(3),
        column_number: Some(10),
        suggested_replacement: None,
        spans: vec![DiagnosticSpan {
            file_path: "src/main.rs".to_string(),
            line_start: 3,
            line_end: 3,
            column_start: 10,
            column_end: 22,
            is_primary: true,
            label: Some("method not found in `Unit`".to_string()),
            suggested_replacement: None,
        }],
        notes: vec![],
    };

    let plan = engine.synthesize_cross_crate_plan(&[diag], None);
    assert!(
        plan.is_some(),
        "Expected trait import synthesis plan for E0599"
    );
    let p = plan.unwrap();

    match &p.steps[0].capability {
        capability_layer::CapabilityKind::FileWrite { content, .. } => {
            assert!(
                content.contains("use core::traits::Auditable;"),
                "Synthesized content should import trait:\n{content}"
            );
        }
        other => panic!("Expected FileWrite capability, got {other:?}"),
    }
}

#[test]
fn test_synthesize_semantic_patch_type_coercion_e0308() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();

    let file_path = src.join("main.rs");
    fs::write(
        &file_path,
        "fn take_str(s: &str) {}\nfn main() {\n    let val = String::from(\"hello\");\n    take_str(val);\n}\n",
    )
    .unwrap();

    let engine = CodeEngine::new(root);

    let diag = Diagnostic {
        level: "error".to_string(),
        code: Some("E0308".to_string()),
        message: "mismatched types: expected `&str`, found `String`".to_string(),
        file_path: Some("src/main.rs".to_string()),
        line_number: Some(4),
        column_number: Some(14),
        suggested_replacement: None,
        spans: vec![DiagnosticSpan {
            file_path: "src/main.rs".to_string(),
            line_start: 4,
            line_end: 4,
            column_start: 14,
            column_end: 17,
            is_primary: true,
            label: Some("expected `&str`, found `String`".to_string()),
            suggested_replacement: None,
        }],
        notes: vec![],
    };

    let plan = engine.synthesize_plan_from_diagnostics(&[diag], None);
    assert!(
        plan.is_some(),
        "Expected type coercion plan synthesized for E0308"
    );
    let p = plan.unwrap();

    match &p.steps[0].capability {
        capability_layer::CapabilityKind::FileWrite { content, .. } => {
            assert!(
                content.contains("val.as_str()"),
                "Synthesized content should coerce String to &str:\n{content}"
            );
        }
        other => panic!("Expected FileWrite capability, got {other:?}"),
    }
}
