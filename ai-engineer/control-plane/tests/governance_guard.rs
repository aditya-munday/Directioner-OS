//! Governance Guard Regression Test Suite.
//!
//! Enforces permanent code-level invariants preventing unauthorized reintroduction of
//! conversational execution wiring, un-gated `--apply`/`--confirm-hash` flags, or
//! connection of `CapabilityClient`/`Implementer` to conversational intent parsing pathways.
//!
//! If any test in this file fails, CI must hard-halt immediately.

use std::fs;
use std::path::PathBuf;

#[test]
fn test_governance_guard_comment_present_in_ai_engineer_cli() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cli_path = manifest_dir.join("src/bin/ai-engineer.rs");
    let content = fs::read_to_string(&cli_path)
        .expect("Failed to read src/bin/ai-engineer.rs for governance audit");

    // Assert that the exact governance guard banner exists verbatim
    assert!(
        content.contains("MANDATORY CODE-LEVEL GOVERNANCE GUARD: CONVERSATIONAL EXECUTION WIRING"),
        "Missing mandatory governance guard banner in src/bin/ai-engineer.rs"
    );

    assert!(
        content.contains("REQUIRES SPECIFIC, NAMED AUTHORIZATION FROM ADITYA"),
        "Missing named authorization clause in governance guard comment"
    );

    assert!(
        content.contains("EXACT COMMENT"),
        "Missing reference clause in governance guard comment"
    );

    assert!(
        content.contains("This function MUST remain strictly draft-only / proposal preview"),
        "Missing draft-only mandate in governance guard comment"
    );
}

#[test]
fn test_no_conversational_execution_dispatch_in_ai_engineer_cli() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cli_path = manifest_dir.join("src/bin/ai-engineer.rs");
    let content = fs::read_to_string(&cli_path)
        .expect("Failed to read src/bin/ai-engineer.rs for governance audit");

    // Extract run_conversational_preview function body
    let fn_marker = "fn run_conversational_preview(";
    let fn_start = content
        .find(fn_marker)
        .expect("Could not locate run_conversational_preview in ai-engineer.rs");
    let fn_body = &content[fn_start..];

    // Find end of run_conversational_preview function (before next fn or end of block)
    let fn_end = fn_body[100..]
        .find("\nfn ")
        .map(|idx| idx + 100)
        .unwrap_or(fn_body.len());
    let conversational_scope = &fn_body[..fn_end];

    // Assert absence of execution wiring within conversational preview scope
    assert!(
        !conversational_scope.contains("dispatch_execution"),
        "Forbidden call to dispatch_execution found in run_conversational_preview"
    );
    assert!(
        !conversational_scope.contains("execute_plan"),
        "Forbidden call to execute_plan found in run_conversational_preview"
    );
    assert!(
        !conversational_scope.contains("CapabilityClient::new"),
        "Forbidden CapabilityClient initialization found in run_conversational_preview"
    );
    assert!(
        !conversational_scope.contains("--apply execution pathway"),
        "Forbidden --apply execution pathway block found in run_conversational_preview"
    );

    // Assert draft-only indicator is present
    assert!(
        conversational_scope.contains("DRAFT-ONLY PROPOSAL")
            || conversational_scope.contains("DRAFT_ONLY"),
        "Conversational preview must explicitly declare DRAFT_ONLY execution status"
    );
}

#[test]
fn test_no_execution_dispatch_in_conversational_module() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let module_path = manifest_dir.join("src/conversational.rs");
    let content = fs::read_to_string(&module_path)
        .expect("Failed to read src/conversational.rs for governance audit");

    // Assert absence of execution dispatch methods in conversational module
    assert!(
        !content.contains("fn dispatch_execution"),
        "Forbidden dispatch_execution function found in control-plane::conversational"
    );
    assert!(
        !content.contains("ConversationalExecutionReport"),
        "Forbidden ConversationalExecutionReport struct found in control-plane::conversational"
    );
    assert!(
        !content.contains("crate::implementer::Implementer"),
        "Forbidden Implementer dependency found in control-plane::conversational"
    );
    assert!(
        !content.contains("crate::verifier::Verifier"),
        "Forbidden Verifier dependency found in control-plane::conversational"
    );
}

#[test]
fn test_cli_help_does_not_advertise_conversational_apply() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cli_path = manifest_dir.join("src/bin/ai-engineer.rs");
    let content = fs::read_to_string(&cli_path)
        .expect("Failed to read src/bin/ai-engineer.rs for governance audit");

    let usage_marker = "fn print_usage() {";
    let usage_start = content
        .find(usage_marker)
        .expect("Could not locate print_usage in ai-engineer.rs");
    let usage_body = &content[usage_start..];
    let usage_end = usage_body[50..]
        .find("\nfn ")
        .map(|idx| idx + 50)
        .unwrap_or(usage_body.len());
    let usage_scope = &usage_body[..usage_end];

    // Assert that 'ask' usage does NOT list --apply or execution capability
    assert!(
        !usage_scope.contains("ask <QUERY>          Grounded conversational intent parsing and interactive plan execution (with --apply)"),
        "print_usage must not advertise conversational execution via --apply"
    );
}
