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
            // Disallow unauthorized phase numbers like phase-4.1, phase-4.2, phase-5.1, phase-6
            // Only officially chartered phases (e.g. phase-0 through phase-5) are permitted
            if name.starts_with("phase-") {
                assert!(
                    name == "phase-0-checklist.md"
                        || name == "phase-1-checklist.md"
                        || name == "phase-2-checklist.md"
                        || name == "phase-3-checklist.md"
                        || name == "phase-4-checklist.md"
                        || name == "phase-4-proposal.md"
                        || name == "phase-5-checklist.md",
                    "Adversarial documentation violation: unauthorized phase file '{}' found in docs/",
                    name
                );
            }
        }
    }
}

#[test]
fn test_plugin_sdk_cannot_bypass_policy_or_reviewer() {
    use capability_layer::CapabilityKind;
    use control_plane::plan::{Plan, PlanStatus, PlanStep};
    use control_plane::plugin::{
        PluginContext, PluginError, PluginManifest, PluginMetadata, PluginPermissions,
        PluginSecurity,
    };
    use control_plane::policy::ApprovalTier;

    let manifest = PluginManifest {
        metadata: PluginMetadata {
            id: "test.plugin.adversarial".into(),
            name: "Adversarial Test Plugin".into(),
            version: "0.1.0".into(),
            author: "Tester".into(),
            description: "Tests policy and reviewer boundary enforcement".into(),
            license: "Apache-2.0".into(),
            min_os_version: "42.0.0".into(),
        },
        security: PluginSecurity {
            signing_key_id: "key-1".into(),
            signature: "sig-1".into(),
            manifest_hash: "hash-1".into(),
        },
        permissions: PluginPermissions {
            read_world_model: false,
            max_approval_tier: ApprovalTier::ExpertReview,
            allowed_capabilities: vec!["file_write".into()],
            allowed_paths: vec![],
        },
        entrypoint: "TestPlugin".into(),
    };

    let ctx = PluginContext::new(manifest, None, None);

    // 1. Reviewer invariant enforcement: Plan that targets /etc/sudoers.d MUST be rejected
    // even if plugin has ExpertReview tier permissions and provides an approval token!
    let sudoers_plan = Plan {
        id: "plan-sudoers-breach".into(),
        intent: "tamper with sudoers".into(),
        summary: "write sudoers".into(),
        steps: vec![PlanStep {
            step_id: "step-1".into(),
            description: "write sudoers file".into(),
            capability: CapabilityKind::FileWrite {
                path: "/etc/../etc/sudoers.d/malicious".into(),
                content: "ALL ALL=(ALL) NOPASSWD: ALL".into(),
                append: false,
            },
            blast_radius: control_plane::policy::BlastRadiusReport {
                score: 85,
                tier: ApprovalTier::ExpertReview,
                factors: vec![],
            },
            timeout_secs: 30,
        }],
        rollback_steps: vec![],
        aggregate_blast_radius: control_plane::policy::BlastRadiusReport {
            score: 85,
            tier: ApprovalTier::ExpertReview,
            factors: vec![],
        },
        created_at: "2026-09-03T00:00:00Z".into(),
        status: PlanStatus::Draft,
    };

    let err = ctx
        .propose_and_execute_plan(&sudoers_plan, Some("token-operator"))
        .unwrap_err();
    match err {
        PluginError::ReviewerRejected { ref summary } => {
            assert!(summary.contains("Path traversal") || summary.contains("fatal"));
        }
        other => {
            panic!("expected ReviewerRejected for /etc/../etc/sudoers.d breach, got {other:?}")
        }
    }

    // 2. Policy tier enforcement: Plan exceeding max_approval_tier MUST be rejected
    let notify_manifest = PluginPermissions {
        read_world_model: false,
        max_approval_tier: ApprovalTier::Notify,
        allowed_capabilities: vec!["file_write".into()],
        allowed_paths: vec![],
    };
    let mut notify_plugin_manifest = ctx.manifest().clone();
    notify_plugin_manifest.permissions = notify_manifest;
    let notify_ctx = PluginContext::new(notify_plugin_manifest, None, None);
    let err_tier = notify_ctx
        .propose_and_execute_plan(&sudoers_plan, None)
        .unwrap_err();
    assert!(matches!(err_tier, PluginError::TierExceeded { .. }));

    // 3. Capability allowlist enforcement: Plan with un-allowlisted capability MUST be rejected
    let high_blast_plan = Plan {
        id: "plan-high-tier".into(),
        intent: "remove vital package".into(),
        summary: "remove vital package".into(),
        steps: vec![PlanStep {
            step_id: "step-1".into(),
            description: "remove package".into(),
            capability: CapabilityKind::PackageRemove {
                package: "systemd".into(),
                approved: true,
            },
            blast_radius: control_plane::policy::BlastRadiusReport {
                score: 80,
                tier: ApprovalTier::ExpertReview,
                factors: vec![],
            },
            timeout_secs: 30,
        }],
        rollback_steps: vec![],
        aggregate_blast_radius: control_plane::policy::BlastRadiusReport {
            score: 80,
            tier: ApprovalTier::ExpertReview,
            factors: vec![],
        },
        created_at: "2026-09-03T00:00:00Z".into(),
        status: PlanStatus::Draft,
    };
    let err2 = ctx
        .propose_and_execute_plan(&high_blast_plan, None)
        .unwrap_err();
    match err2 {
        PluginError::CapabilityNotAllowlisted { .. } | PluginError::TierExceeded { .. } => {}
        other => panic!("expected CapabilityNotAllowlisted or TierExceeded, got {other:?}"),
    }
}
