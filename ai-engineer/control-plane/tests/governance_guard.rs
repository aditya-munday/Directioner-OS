//! Governance Guard Regression & Adversarial Reintroduction Test Suite.
//!
//! Enforces permanent code-level invariants preventing unauthorized reintroduction of
//! conversational execution wiring, un-gated `--apply`/`--confirm-hash` flags, or
//! connection of `CapabilityClient`/`Implementer` to conversational intent parsing pathways.
//!
//! If any test in this file fails, CI must hard-halt immediately.

use std::fs;
use std::path::{Path, PathBuf};

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

#[test]
fn test_adversarial_no_aliased_conversational_execution_functions() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files_to_scan = [
        manifest_dir.join("src/bin/ai-engineer.rs"),
        manifest_dir.join("src/conversational.rs"),
        manifest_dir.join("src/lib.rs"),
    ];

    let forbidden_function_names = [
        "execute_conversational_plan",
        "apply_conversational_intent",
        "dispatch_unverified_plan",
        "run_live_intent",
        "apply_intent_direct",
        "execute_chat_plan",
        "execute_dialogue_plan",
        "apply_conversational_draft",
    ];

    for file_path in &files_to_scan {
        if let Ok(content) = fs::read_to_string(file_path) {
            for fn_name in &forbidden_function_names {
                assert!(
                    !content.contains(fn_name),
                    "Adversarial alias violation: found forbidden symbol '{}' in {}",
                    fn_name,
                    file_path.display()
                );
            }
        }
    }
}

#[test]
fn test_adversarial_chat_subcommand_has_no_execution_wiring() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cli_path = manifest_dir.join("src/bin/ai-engineer.rs");
    let content = fs::read_to_string(&cli_path).expect("Failed to read src/bin/ai-engineer.rs");

    let chat_marker = r#"if args.command == "chat" {"#;
    let chat_start = content
        .find(chat_marker)
        .expect("Could not locate chat command handler in ai-engineer.rs");
    let chat_body = &content[chat_start..];
    let chat_end = chat_body.find("return;").unwrap_or(300);
    let chat_scope = &chat_body[..chat_end];

    assert!(
        !chat_scope.contains("Implementer"),
        "Forbidden Implementer found inside chat command handler"
    );
    assert!(
        !chat_scope.contains("CapabilityClient"),
        "Forbidden CapabilityClient found inside chat command handler"
    );
    assert!(
        !chat_scope.contains("dispatch_execution"),
        "Forbidden dispatch_execution found inside chat command handler"
    );
}

#[test]
fn test_adversarial_no_backdoor_conversational_subcommands() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cli_path = manifest_dir.join("src/bin/ai-engineer.rs");
    let content = fs::read_to_string(&cli_path).expect("Failed to read src/bin/ai-engineer.rs");

    let forbidden_subcommands = [
        "apply-conversational",
        "chat-apply",
        "ask-apply",
        "run-dialogue",
        "interactive-exec",
        "exec-chat",
        "conversational-apply",
    ];

    for subcmd in &forbidden_subcommands {
        let pattern = format!("\"{}\"", subcmd);
        assert!(
            !content.contains(&pattern),
            "Adversarial backdoor violation: found forbidden subcommand '{}' in {}",
            subcmd,
            cli_path.display()
        );
    }
}

#[test]
fn test_adversarial_no_dynamic_capability_injection_in_conversational() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let module_path = manifest_dir.join("src/conversational.rs");
    let content = fs::read_to_string(&module_path).expect("Failed to read src/conversational.rs");

    let forbidden_signatures = [
        "&Implementer",
        "&mut Implementer",
        "&CapabilityClient",
        "Arc<CapabilityClient>",
        "CapabilityClient::new",
    ];

    for sig in &forbidden_signatures {
        assert!(
            !content.contains(sig),
            "Adversarial injection violation: found privileged signature '{}' in {}",
            sig,
            module_path.display()
        );
    }
}

#[test]
fn test_adversarial_no_unauthorized_phase_documentation_files() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new("."));
    let docs_dir = root_dir.join("docs");

    if let Ok(entries) = fs::read_dir(&docs_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Disallow phase numbers like phase-4.1, phase-5.0 unless explicitly created in roadmap
            assert!(
                !name.starts_with("phase-4.1")
                    && !name.starts_with("phase-4.2")
                    && !name.starts_with("phase-5"),
                "Adversarial documentation violation: unauthorized phase file '{}' found in docs/",
                name
            );
        }
    }
}
