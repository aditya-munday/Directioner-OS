//! Directioner-OS AI Engineer CLI.
//!
//! Control-plane interface for planning, dry-running, applying, and explaining
//! autonomous system changes.

use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::exit;

use control_plane::client::CapabilityClient;
use control_plane::implementer::Implementer;
use control_plane::planner::{Planner, PlannerConfig};
use control_plane::simulation::Simulator;
use control_plane::verifier::Verifier;
use rusqlite::Connection;

fn print_usage() {
    eprintln!(
        r#"Directioner-OS AI Engineer CLI (Phase 1, 2, 3 & 4 Control Plane)

USAGE:
    ai-engineer <COMMAND> [OPTIONS]

COMMANDS:
    plan <INTENT>        Generate a structured multi-step plan with blast-radius scoring
    review <INTENT>      Run independent multi-agent Reviewer audit on synthesized plan
    dry-run <INTENT>     Simulate plan and generate unified diffs without modifying state
    apply <INTENT>       Plan, review, preview, prompt for confirmation, and execute via Capability Layer
    ask <QUERY>          Grounded conversational intent parsing and interactive plan negotiation
    chat                 Interactive conversational operator shell
    explain <INTENT>     Provide 3-depth explanation (summary, engineering plan, raw diffs)
    diagnose [PATH]      Scan workspace, parse compiler diagnostics, and localize cross-crate root causes
    fix [PATH]           Synthesize multi-crate repair plan, audit with Reviewer, preview diffs, and apply with verification
    self-heal            Run a single cycle of the closed-loop self-healing engine (evaluates HealthModel -> Remediation)
    daemon               Run the continuous background self-healing evaluation runner

OPTIONS:
    --config <PATH>      Path to self-healing TOML configuration (default: /etc/ai-engineer/self-healing.toml)
    --socket <PATH>      Path to ai-capabilityd Unix domain socket (default: /tmp/live-cap.sock or /tmp/ai-engineer/ai-capabilityd.sock)
    --db <PATH>          Path to world-model SQLite DB (default: /tmp/wm.sqlite or /tmp/ai-engineer/worldmodel.db)
    --lock <PATH>        Path to killswitch lockfile (default: /tmp/live-killswitch.lock or /etc/ai-engineer/killswitch.lock)
    --worklog <PATH>     Path to append-only Work Log (default: /var/log/live-worklog.jsonl)
    --work-root <PATH>   Path root for workspace source operations (default: current directory or /tmp/ai-engineer/scope)
    --apply              Approve and execute conversational plan draft through the Capability Layer
    -y, --yes            Bypass interactive confirmation prompt (strictly requires non-rejected Reviewer verdict)
    --confirm-hash <HEX> Verify and pin cryptographic plan hash before execution
    --interval <SECS>    Evaluation interval in seconds for background daemon [default: 30]
    --once               Run one self-healing cycle and exit immediately
    --auto-approve       Bypass interactive confirmation prompt (strictly limited to low-risk Notify tier)
    --depth <1|2|3>      Explanation depth (1=Summary, 2=Engineering, 3=Raw) [default: 2]
    --json               Output raw JSON format
    -h, --help           Print help information
"#
    );
}

struct CliArgs {
    command: String,
    intent: String,
    config_path: PathBuf,
    socket_path: PathBuf,
    db_path: PathBuf,
    lock_path: PathBuf,
    worklog_path: PathBuf,
    work_root: PathBuf,
    interval_secs: u64,
    once: bool,
    auto_approve: bool,
    apply: bool,
    yes: bool,
    confirm_hash: Option<String>,
    depth: u8,
    json: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        exit(1);
    }

    let mut command = String::new();
    let mut intent_parts = Vec::new();
    let mut config_path = PathBuf::from("/etc/ai-engineer/self-healing.toml");
    let mut socket_path = if Path::new("/tmp/live-cap.sock").exists() {
        PathBuf::from("/tmp/live-cap.sock")
    } else {
        PathBuf::from("/tmp/ai-engineer/ai-capabilityd.sock")
    };
    let mut db_path = if Path::new("/tmp/wm.sqlite").exists() {
        PathBuf::from("/tmp/wm.sqlite")
    } else {
        PathBuf::from("/tmp/ai-engineer/worldmodel.db")
    };
    let mut lock_path = if Path::new("/tmp/live-killswitch.lock").exists() {
        PathBuf::from("/tmp/live-killswitch.lock")
    } else {
        PathBuf::from("/etc/ai-engineer/killswitch.lock")
    };
    let mut worklog_path = PathBuf::from("/var/log/live-worklog.jsonl");
    let mut work_root = PathBuf::from("/tmp/ai-engineer/scope");
    let mut interval_secs = 30;
    let mut once = false;
    let mut auto_approve = false;
    let mut apply = false;
    let mut yes = false;
    let mut confirm_hash = None;
    let mut depth = 2;
    let mut json = false;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                exit(0);
            }
            "--config" => {
                i += 1;
                if i >= args.len() {
                    return Err("--config requires an argument".into());
                }
                config_path = PathBuf::from(&args[i]);
            }
            "--socket" => {
                i += 1;
                if i >= args.len() {
                    return Err("--socket requires an argument".into());
                }
                socket_path = PathBuf::from(&args[i]);
            }
            "--db" => {
                i += 1;
                if i >= args.len() {
                    return Err("--db requires an argument".into());
                }
                db_path = PathBuf::from(&args[i]);
            }
            "--lock" => {
                i += 1;
                if i >= args.len() {
                    return Err("--lock requires an argument".into());
                }
                lock_path = PathBuf::from(&args[i]);
            }
            "--worklog" => {
                i += 1;
                if i >= args.len() {
                    return Err("--worklog requires an argument".into());
                }
                worklog_path = PathBuf::from(&args[i]);
            }
            "--work-root" => {
                i += 1;
                if i >= args.len() {
                    return Err("--work-root requires an argument".into());
                }
                work_root = PathBuf::from(&args[i]);
            }
            "--apply" => {
                apply = true;
            }
            "-y" | "--yes" => {
                yes = true;
            }
            "--confirm-hash" => {
                i += 1;
                if i >= args.len() {
                    return Err("--confirm-hash requires an argument".into());
                }
                confirm_hash = Some(args[i].clone());
            }
            "--interval" => {
                i += 1;
                if i >= args.len() {
                    return Err("--interval requires an integer argument".into());
                }
                interval_secs = args[i].parse::<u64>().unwrap_or(30);
            }
            "--once" => {
                once = true;
            }
            "--auto-approve" => {
                auto_approve = true;
            }
            "--depth" => {
                i += 1;
                if i >= args.len() {
                    return Err("--depth requires an argument (1, 2, or 3)".into());
                }
                depth = args[i].parse::<u8>().unwrap_or(2);
            }
            "--json" => {
                json = true;
            }
            _ => {
                if command.is_empty() {
                    command = arg.clone();
                } else {
                    intent_parts.push(arg.clone());
                }
            }
        }
        i += 1;
    }

    let intent = intent_parts.join(" ");

    Ok(CliArgs {
        command,
        intent,
        config_path,
        socket_path,
        db_path,
        lock_path,
        worklog_path,
        work_root,
        interval_secs,
        once,
        auto_approve,
        apply,
        yes,
        confirm_hash,
        depth,
        json,
    })
}

fn run_conversational_preview(
    intent: &str,
    session: &mut control_plane::conversational::ConversationSession,
    args: &CliArgs,
    db_conn: Option<&Connection>,
) {
    session.add_user_message(intent);

    let grounding = if let Some(conn) = db_conn {
        if let Ok(stats) = world_model::query::graph_stats(conn) {
            format!(
                "World Model online: {} packages indexed, {} units tracked.",
                stats.packages, stats.units
            )
        } else {
            "World Model online (query active).".to_string()
        }
    } else {
        "World Model offline (using local fixtures).".to_string()
    };

    let socket_path = PathBuf::from("/run/ai-engineer/llm.sock");
    let plan = if socket_path.exists() {
        let backend = control_plane::conversational::LocalSocketBackend::new(socket_path);
        match control_plane::conversational::IntentParser::parse_intent(
            session,
            &backend,
            Some(&grounding),
        ) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Conversational intent parsing error: {e}");
                exit(1);
            }
        }
    } else {
        let planner = Planner::new(PlannerConfig {
            work_root: args.work_root.clone(),
            default_timeout_secs: 30,
        });
        let generated_plan = match planner.create_plan(intent, db_conn) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Planning error: {e}");
                exit(1);
            }
        };

        let candidate_dto = control_plane::draft_validator::CandidatePlanDraft {
            intent: generated_plan.intent.clone(),
            summary: generated_plan.summary.clone(),
            steps: generated_plan
                .steps
                .iter()
                .map(|s| control_plane::draft_validator::CandidateStepDraft {
                    step_id: s.step_id.clone(),
                    description: s.description.clone(),
                    capability: match &s.capability {
                        capability_layer::CapabilityKind::Ping => {
                            control_plane::draft_validator::CandidateCapabilityDraft::Ping
                        }
                        capability_layer::CapabilityKind::FileRead { path } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::FileRead {
                                path: path.clone(),
                            }
                        }
                        capability_layer::CapabilityKind::FileWrite {
                            path,
                            content,
                            append,
                        } => control_plane::draft_validator::CandidateCapabilityDraft::FileWrite {
                            path: path.clone(),
                            content: content.clone(),
                            append: *append,
                        },
                        capability_layer::CapabilityKind::FileDelete { path } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::FileDelete {
                                path: path.clone(),
                            }
                        }
                        capability_layer::CapabilityKind::PackageInstall { package, approved } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::PackageInstall {
                                package: package.clone(),
                                approved: *approved,
                            }
                        }
                        capability_layer::CapabilityKind::PackageRemove { package, approved } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::PackageRemove {
                                package: package.clone(),
                                approved: *approved,
                            }
                        }
                        capability_layer::CapabilityKind::ServiceStatus { unit } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::ServiceStatus {
                                unit: unit.clone(),
                            }
                        }
                        capability_layer::CapabilityKind::ServiceRestart {
                            unit,
                            approved,
                        } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::ServiceRestart {
                                unit: unit.clone(),
                                approved: *approved,
                            }
                        }
                        capability_layer::CapabilityKind::ServiceStop { unit, approved } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::ServiceStop {
                                unit: unit.clone(),
                                approved: *approved,
                            }
                        }
                        capability_layer::CapabilityKind::BootSlotPin { slot, approved } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::BootSlotPin {
                                slot: *slot,
                                approved: *approved,
                            }
                        }
                        capability_layer::CapabilityKind::KernelParamSet {
                            key,
                            value,
                            persistent,
                            approved,
                        } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::KernelParamSet {
                                key: key.clone(),
                                value: value.clone(),
                                persistent: *persistent,
                                approved: *approved,
                            }
                        }
                        capability_layer::CapabilityKind::KernelModuleLoad {
                            module,
                            args,
                            approved,
                        } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::KernelModuleLoad {
                                module: module.clone(),
                                args: args.clone(),
                                approved: *approved,
                            }
                        }
                        capability_layer::CapabilityKind::KernelModuleUnload {
                            module,
                            approved,
                        } => {
                            control_plane::draft_validator::CandidateCapabilityDraft::KernelModuleUnload {
                                module: module.clone(),
                                approved: *approved,
                            }
                        }
                        capability_layer::CapabilityKind::BootcRollback => {
                            control_plane::draft_validator::CandidateCapabilityDraft::BootcRollback
                        }
                    },
                    timeout_secs: Some(s.timeout_secs),
                })
                .collect(),
            rollback_steps: Some(
                generated_plan
                    .rollback_steps
                    .iter()
                    .map(|rb| control_plane::draft_validator::CandidateRollbackDraft {
                        step_id: rb.step_id.clone(),
                        description: rb.description.clone(),
                        capability: match &rb.capability {
                            capability_layer::CapabilityKind::Ping => {
                                control_plane::draft_validator::CandidateCapabilityDraft::Ping
                            }
                            capability_layer::CapabilityKind::FileRead { path } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::FileRead {
                                    path: path.clone(),
                                }
                            }
                            capability_layer::CapabilityKind::FileWrite {
                                path,
                                content,
                                append,
                            } => control_plane::draft_validator::CandidateCapabilityDraft::FileWrite {
                                path: path.clone(),
                                content: content.clone(),
                                append: *append,
                            },
                            capability_layer::CapabilityKind::FileDelete { path } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::FileDelete {
                                    path: path.clone(),
                                }
                            }
                            capability_layer::CapabilityKind::PackageInstall { package, approved } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::PackageInstall {
                                    package: package.clone(),
                                    approved: *approved,
                                }
                            }
                            capability_layer::CapabilityKind::PackageRemove { package, approved } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::PackageRemove {
                                    package: package.clone(),
                                    approved: *approved,
                                }
                            }
                            capability_layer::CapabilityKind::ServiceStatus { unit } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::ServiceStatus {
                                    unit: unit.clone(),
                                }
                            }
                            capability_layer::CapabilityKind::ServiceRestart {
                                unit,
                                approved,
                            } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::ServiceRestart {
                                    unit: unit.clone(),
                                    approved: *approved,
                                }
                            }
                            capability_layer::CapabilityKind::ServiceStop { unit, approved } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::ServiceStop {
                                    unit: unit.clone(),
                                    approved: *approved,
                                }
                            }
                            capability_layer::CapabilityKind::BootSlotPin { slot, approved } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::BootSlotPin {
                                    slot: *slot,
                                    approved: *approved,
                                }
                            }
                            capability_layer::CapabilityKind::KernelParamSet {
                                key,
                                value,
                                persistent,
                                approved,
                            } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::KernelParamSet {
                                    key: key.clone(),
                                    value: value.clone(),
                                    persistent: *persistent,
                                    approved: *approved,
                                }
                            }
                            capability_layer::CapabilityKind::KernelModuleLoad {
                                module,
                                args,
                                approved,
                            } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::KernelModuleLoad {
                                    module: module.clone(),
                                    args: args.clone(),
                                    approved: *approved,
                                }
                            }
                            capability_layer::CapabilityKind::KernelModuleUnload {
                                module,
                                approved,
                            } => {
                                control_plane::draft_validator::CandidateCapabilityDraft::KernelModuleUnload {
                                    module: module.clone(),
                                    approved: *approved,
                                }
                            }
                            capability_layer::CapabilityKind::BootcRollback => {
                                control_plane::draft_validator::CandidateCapabilityDraft::BootcRollback
                            }
                        },
                        triggers: rb.triggers.clone(),
                    })
                    .collect(),
            ),
        };

        let raw_dto_json = serde_json::to_string(&candidate_dto).unwrap();
        match control_plane::draft_validator::PlanDraftValidator::validate_and_convert(
            &raw_dto_json,
        ) {
            Ok(mut p) => {
                for step in &mut p.steps {
                    step.blast_radius = control_plane::policy::PolicyEngine::evaluate_capability(
                        &step.capability,
                        db_conn,
                    );
                }
                let caps: Vec<_> = p.steps.iter().map(|s| s.capability.clone()).collect();
                p.aggregate_blast_radius =
                    control_plane::policy::PolicyEngine::evaluate_plan_aggregate(&caps, db_conn);
                p
            }
            Err(e) => {
                eprintln!("Plan draft validation error: {e}");
                exit(1);
            }
        }
    };

    let reviewer =
        control_plane::reviewer::Reviewer::new(control_plane::reviewer::ReviewerConfig::default());
    let review_report = reviewer.review_plan(&plan);
    let preview = Simulator::simulate_plan(&plan, Some(&args.work_root));
    let gate = control_plane::conversational::PlanExecutionGate::evaluate(&plan, &review_report);

    if args.json {
        let combined = serde_json::json!({
            "plan_draft": plan,
            "review": review_report,
            "simulation": preview,
            "gate": gate,
            "execution_status": if args.apply { "EXECUTING" } else { "PROPOSED" },
        });
        println!("{}", serde_json::to_string_pretty(&combined).unwrap());
    } else {
        println!("══════════════════════════════════════════════════════════════════════");
        println!("  PHASE 4.1 CONVERSATIONAL OPERATOR — PLAN REVIEW & EXECUTION GATE");
        println!("══════════════════════════════════════════════════════════════════════");
        println!("  Plan ID:     {}", plan.id);
        println!("  Intent:      \"{}\"", plan.intent);
        println!("  Status:      {:?}", plan.status);
        println!(
            "  Blast Score: {}/100 (Tier: {:?})",
            plan.aggregate_blast_radius.score, plan.aggregate_blast_radius.tier
        );
        println!("  Summary:     {}", plan.summary);
        println!("──────────────────────────────────────────────────────────────────────");
        println!("  Reviewer Verdict: {:?}", review_report.verdict);
        if review_report.findings.is_empty() {
            println!("    ✔ No invariant violations or warnings found.");
        } else {
            for (idx, f) in review_report.findings.iter().enumerate() {
                let icon = match f.severity {
                    control_plane::reviewer::FindingSeverity::Fatal => "✘ [FATAL]",
                    control_plane::reviewer::FindingSeverity::Warning => "▲ [WARN]",
                    control_plane::reviewer::FindingSeverity::Info => "ℹ [INFO]",
                };
                println!("    {}. {} [{}] {}", idx + 1, icon, f.category, f.message);
            }
        }
        println!("──────────────────────────────────────────────────────────────────────");
        println!("  Execution Steps ({}):", plan.steps.len());
        for (idx, step) in plan.steps.iter().enumerate() {
            println!(
                "    {}. [{:?}] {} (Blast: {}/100)",
                idx + 1,
                step.capability,
                step.description,
                step.blast_radius.score
            );
        }
        if !plan.rollback_steps.is_empty() {
            println!("  Compensating Rollbacks ({}):", plan.rollback_steps.len());
            for (idx, rb) in plan.rollback_steps.iter().enumerate() {
                println!("    {}. {}", idx + 1, rb.description);
            }
        }
        println!("──────────────────────────────────────────────────────────────────────");
        println!("{}", preview.render_terminal());
        println!("──────────────────────────────────────────────────────────────────────");
    }

    if args.apply {
        match gate {
            control_plane::conversational::GateVerdict::Rejected { reason, findings } => {
                eprintln!("\nExecution REFUSED: {reason}");
                for f in findings {
                    eprintln!("  - {f}");
                }
                exit(1);
            }
            control_plane::conversational::GateVerdict::InvalidStatus(st) => {
                eprintln!("\nExecution REFUSED: Plan status '{st}' is not eligible for execution.");
                exit(1);
            }
            control_plane::conversational::GateVerdict::Ready {
                plan_hash,
                tier,
                requires_human_confirmation,
            } => {
                if let Some(ref expected) = args.confirm_hash
                    && expected != &plan_hash
                {
                    eprintln!(
                        "Execution REFUSED: Cryptographic plan hash mismatch (expected {expected}, calculated {plan_hash})"
                    );
                    exit(1);
                }

                if requires_human_confirmation && !args.yes {
                    print!(
                        "\nApply this plan [hash: {}] (Tier: {:?})? [y/N]: ",
                        &plan_hash[..8],
                        tier
                    );
                    std::io::stdout().flush().unwrap();
                    let mut line = String::new();
                    if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
                        println!("\nPlan execution cancelled by operator.");
                        exit(0);
                    }
                    let resp = line.trim().to_lowercase();
                    if resp != "y" && resp != "yes" {
                        println!("Plan execution cancelled by operator.");
                        exit(0);
                    }
                }

                let killswitch = capability_layer::killswitch::KillSwitch::new(&args.lock_path);
                if killswitch.is_engaged() {
                    eprintln!("Execution BLOCKED: Killswitch is engaged.");
                    exit(1);
                }

                let approved_plan =
                    match control_plane::conversational::PlanExecutionGate::approve_plan(
                        plan, &plan_hash,
                    ) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("Plan approval failed: {e}");
                            exit(1);
                        }
                    };

                let client = CapabilityClient::new(&args.socket_path, "ai-engineer-conversational");
                let implementer = Implementer::new(&client);
                let report = implementer.execute_plan(&approved_plan);

                if report.final_status != control_plane::PlanStatus::Completed {
                    eprintln!(
                        "\nExecution aborted during step application: {:?}",
                        report.final_status
                    );
                    eprintln!("Message: {}", report.message);
                    exit(1);
                }

                println!("══════════════════════════════════════════════════════════════════════");
                println!(
                    "  EXECUTION SUCCESSFUL: All {} steps verified and executed.",
                    approved_plan.steps.len()
                );
                println!("══════════════════════════════════════════════════════════════════════");
            }
        }
    } else if !args.json {
        println!("  [!] DRAFT ONLY — PASS '--apply' TO APPROVE AND EXECUTE THIS PLAN");
        println!("══════════════════════════════════════════════════════════════════════");
    }
}

fn open_db(path: &Path) -> Option<Connection> {
    if path.exists() {
        Connection::open(path).ok()
    } else {
        None
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {e}");
            exit(1);
        }
    };

    let target_dir = if !args.intent.is_empty() && Path::new(&args.intent).exists() {
        PathBuf::from(&args.intent)
    } else {
        args.work_root.clone()
    };

    let db_conn = open_db(&args.db_path);

    if args.command == "daemon" || args.command == "self-heal" {
        let once = args.once || args.command == "self-heal";
        let mut config = control_plane::self_healing::SelfHealingConfig::load_from_file_or_default(
            &args.config_path,
        );
        config.db_path.clone_from(&args.db_path);
        config.worklog_path = Some(args.worklog_path.clone());

        let killswitch = capability_layer::killswitch::KillSwitch::new(&args.lock_path);
        let client = CapabilityClient::new(&args.socket_path, "ai-engineer-daemon");
        let rem_agent = control_plane::remediation::RemediationAgent::new(
            control_plane::remediation::RemediationConfig::default(),
        );

        let mut engine = control_plane::self_healing::SelfHealingEngine::new(
            config.clone(),
            killswitch,
            client,
            rem_agent,
        );

        if config.enabled {
            println!(
                "[Self-Healing] Closed-loop self-healing engine ACTIVE (interval: {}s, max_entity_attempts: {}, global_action_cap: {}).",
                args.interval_secs, config.max_entity_attempts, config.global_action_cap
            );
        } else {
            println!(
                "[Self-Healing] Closed-loop self-healing is DISABLED by configuration (enabled = false in {}).",
                args.config_path.display()
            );
            println!("[Self-Healing] Zero autonomous remediation actions will be dispatched.");
            if once {
                let report = engine.evaluate_and_heal();
                if args.json {
                    println!("{}", serde_json::to_string_pretty(&report).unwrap());
                }
                exit(0);
            }
        }

        loop {
            let report = engine.evaluate_and_heal();
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!(
                    "[{}] Self-Healing Cycle Evaluated: overall_status={}, outcomes={}",
                    report.evaluated_at,
                    report.health_report.overall_status,
                    report.outcomes.len()
                );
                for outcome in &report.outcomes {
                    match outcome {
                        control_plane::self_healing::SelfHealingOutcome::DisabledByConfig => {
                            println!("  • Self-healing disabled by configuration.");
                        }
                        control_plane::self_healing::SelfHealingOutcome::DisabledByKillswitch => {
                            println!("  • KillSwitch engaged: Evaluation and dispatches halted.");
                        }
                        control_plane::self_healing::SelfHealingOutcome::HealthyOrUnobserved { score } => {
                            println!("  • System Healthy/Unobserved (score: {score:?}).");
                        }
                        control_plane::self_healing::SelfHealingOutcome::AntiFlapSuppressed { entity, consecutive_cycles, required_cycles } => {
                            println!("  • Anti-Flap Filter: Transient failure suppressed for '{entity}' ({consecutive_cycles}/{required_cycles} cycles).");
                        }
                        control_plane::self_healing::SelfHealingOutcome::EntityLockedOut { entity, attempts, message } => {
                            println!("  • Entity Locked Out: '{entity}' (attempts: {attempts}): {message}");
                        }
                        control_plane::self_healing::SelfHealingOutcome::GlobalThrottleEngaged { actions_in_window, cap, message } => {
                            println!("  • Global Throttle Engaged ({actions_in_window}/{cap} actions): {message}");
                        }
                        control_plane::self_healing::SelfHealingOutcome::AwaitingConfirmation { entity, plan_id, tier, summary, .. } => {
                            println!("  • [Awaiting Operator Confirmation] Entity '{entity}' -> Plan '{plan_id}' ({tier:?} tier): {summary}");
                        }
                        control_plane::self_healing::SelfHealingOutcome::ExecutedNotifyTier { entity, plan_id, action, verified, .. } => {
                            println!("  • [Executed Notify Tier] Entity '{entity}' -> Plan '{plan_id}': {action} (verified: {verified})");
                        }
                        control_plane::self_healing::SelfHealingOutcome::ExecutionFailedRolledBack { entity, plan_id, attempt, error } => {
                            println!("  • [Failed & Rolled Back] Entity '{entity}' -> Plan '{plan_id}' (attempt {attempt}): {error}");
                        }
                    }
                }
            }

            if once {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(args.interval_secs));
        }
        return;
    }

    if args.command == "diagnose" {
        let mut engine = control_plane::CodeEngine::new(&target_dir);
        let _ = engine.build_symbol_graph();

        let report = match engine.run_cargo_check() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Diagnostic execution failed: {e}");
                exit(1);
            }
        };

        if args.json {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        } else {
            println!("══════════════════════════════════════════════════════════════════════");
            println!("  DIRECTIONER-OS CODE DIAGNOSTICS & SYMBOL LOCALIZATION");
            println!("  Target: {}", target_dir.display());
            println!(
                "  Status: {}",
                if report.success {
                    "PASSED (Clean)"
                } else {
                    "ERRORS DETECTED"
                }
            );
            println!("──────────────────────────────────────────────────────────────────────");

            if report.diagnostics.is_empty() {
                println!("  ✔ No compiler errors or warnings detected in workspace.");
            } else {
                for (i, diag) in report.diagnostics.iter().enumerate() {
                    let code_display = diag.code.as_deref().unwrap_or("error");
                    let loc_display = match (&diag.file_path, diag.line_number) {
                        (Some(f), Some(l)) => format!("{f}:{l}"),
                        (Some(f), None) => f.clone(),
                        _ => "workspace".into(),
                    };

                    println!(
                        "  {}. ✘ [{}] {}: {}",
                        i + 1,
                        code_display,
                        loc_display,
                        diag.message
                    );

                    if let Some(root) = engine.localize_diagnostic_root(diag) {
                        let sym_str = root.symbol_name.as_deref().unwrap_or("<anonymous>");
                        println!(
                            "     ↳ Cross-Crate Root Cause: References '{}' defined in crate '{}' at {}:{}",
                            sym_str,
                            root.upstream_crate,
                            root.definition_file.display(),
                            root.definition_line
                        );
                    }
                }
            }
            println!("══════════════════════════════════════════════════════════════════════");
        }
        if !report.success {
            exit(1);
        }
        return;
    }

    if args.command == "fix" {
        let mut engine = control_plane::CodeEngine::new(&target_dir);
        let _ = engine.build_symbol_graph();

        let initial_report = match engine.run_cargo_check() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Diagnostic check failed: {e}");
                exit(1);
            }
        };

        if initial_report.success {
            println!(
                "Workspace at {} is clean; no compiler fixes required.",
                target_dir.display()
            );
            return;
        }

        let Some(plan) =
            engine.synthesize_cross_crate_plan(&initial_report.diagnostics, db_conn.as_ref())
        else {
            eprintln!(
                "Could not synthesize automated remediation plan from diagnostics (no machine-applicable compiler suggestions available)."
            );
            exit(1);
        };

        // 1. Reviewer audit
        let reviewer = control_plane::reviewer::Reviewer::new(
            control_plane::reviewer::ReviewerConfig::default(),
        );
        let review_report = reviewer.review_plan(&plan);
        if review_report.verdict == control_plane::reviewer::ReviewVerdict::Rejected {
            eprintln!("\nRemediation plan BLOCKED by Reviewer Agent:");
            for f in &review_report.findings {
                eprintln!("  ✘ [{:?}] {}: {}", f.severity, f.category, f.message);
            }
            exit(1);
        }

        // 2. Simulation preview (mandatory)
        let preview = Simulator::simulate_plan(&plan, Some(&target_dir));
        println!("{}", preview.render_terminal());

        // 3. Strict auto-approve policy enforcement
        if plan.aggregate_blast_radius.tier.requires_confirmation() {
            if args.auto_approve {
                eprintln!(
                    "\nAuto-approve REFUSED: Plan requires {:?} confirmation (exceeds auto-approvable low-risk threshold).",
                    plan.aggregate_blast_radius.tier
                );
                exit(1);
            }

            eprint!(
                "\nApproval required for Tier {:?}. Proceed with execution? [y/N]: ",
                plan.aggregate_blast_radius.tier
            );
            use std::io::{self, BufRead};
            let mut line = String::new();
            let stdin = io::stdin();
            stdin.lock().read_line(&mut line).unwrap();
            if line.trim().to_lowercase() != "y" && line.trim().to_lowercase() != "yes" {
                eprintln!("Execution cancelled by user.");
                exit(0);
            }
        }

        // 4. Implement plan
        let client = CapabilityClient::new(&args.socket_path, "ai-engineer-cli");
        let implementer = Implementer::new(&client);
        let report = implementer.execute_plan(&plan);

        if report.final_status != control_plane::PlanStatus::Completed {
            eprintln!(
                "\nExecution aborted during step application: {:?}",
                report.final_status
            );
            eprintln!("Message: {}", report.message);
            exit(1);
        }

        // 5. Re-verify
        let re_check = match engine.run_cargo_check() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Re-verification command failed: {e}");
                exit(1);
            }
        };

        if !re_check.success {
            eprintln!(
                "\nRe-verification FAILED. Initiating atomic compensating rollback in reverse order..."
            );
            let rb_report = implementer.rollback_all(&plan);
            eprintln!("Rollback status: {:?}", rb_report.final_status);
            exit(1);
        }

        println!("══════════════════════════════════════════════════════════════════════");
        println!(
            "  REMEDIATION SUCCESSFUL: All {} patch steps verified.",
            plan.steps.len()
        );
        println!("══════════════════════════════════════════════════════════════════════");
        return;
    }

    if args.command == "ask" {
        if args.intent.is_empty() {
            eprintln!(
                "Error: question/intent string is required for 'ai-engineer ask \"<query>\"'"
            );
            exit(1);
        }
        let mut session = control_plane::conversational::ConversationSession::new(10);
        run_conversational_preview(&args.intent, &mut session, &args, db_conn.as_ref());
        return;
    }

    if args.command == "chat" {
        println!("══════════════════════════════════════════════════════════════════════");
        println!("  Directioner-OS Conversational Operator Shell (Phase 4.1)");
        println!("  Type your operational requests below (Ctrl+C or 'exit' to quit).");
        println!("  [!] Use '--apply' flag to enable execution upon confirmation.");
        println!("══════════════════════════════════════════════════════════════════════");
        let mut session = control_plane::conversational::ConversationSession::new(20);
        let stdin = std::io::stdin();
        let mut reader = std::io::BufReader::new(stdin);
        let mut line = String::new();
        loop {
            print!("\noperator> ");
            std::io::stdout().flush().unwrap();
            line.clear();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            let query = line.trim();
            if query.is_empty() {
                continue;
            }
            if query == "exit" || query == "quit" {
                println!("Exiting conversational shell.");
                break;
            }
            run_conversational_preview(query, &mut session, &args, db_conn.as_ref());
        }
        return;
    }

    if args.intent.is_empty() && args.command != "help" {
        eprintln!(
            "Error: intent string is required for command '{}'",
            args.command
        );
        exit(1);
    }

    let planner = Planner::new(PlannerConfig {
        work_root: args.work_root.clone(),
        default_timeout_secs: 30,
    });

    let plan = match planner.create_plan(&args.intent, db_conn.as_ref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Planning error: {e}");
            exit(1);
        }
    };

    match args.command.as_str() {
        "plan" => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&plan).unwrap());
            } else {
                println!("Plan ID: {}", plan.id);
                println!("Intent:  \"{}\"", plan.intent);
                println!(
                    "Tier:    {:?} (Blast Score: {}/100)",
                    plan.aggregate_blast_radius.tier, plan.aggregate_blast_radius.score
                );
                println!("Summary: {}", plan.summary);
                println!("\nSteps ({}) :", plan.steps.len());
                for (i, step) in plan.steps.iter().enumerate() {
                    println!(
                        "  {}. {} (Risk: {}/100)",
                        i + 1,
                        step.description,
                        step.blast_radius.score
                    );
                }
                println!("\nRollback steps available: {}", plan.rollback_steps.len());
            }
        }
        "review" => {
            let reviewer = control_plane::reviewer::Reviewer::new(
                control_plane::reviewer::ReviewerConfig::default(),
            );
            let report = reviewer.review_plan(&plan);

            if args.json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("══════════════════════════════════════════════════════════════════════");
                println!("  REVIEWER AGENT AUDIT REPORT");
                println!("  Plan:    {}", plan.id);
                println!("  Verdict: {:?}", report.verdict);
                println!("  Summary: {}", report.summary);
                println!("──────────────────────────────────────────────────────────────────────");
                if report.findings.is_empty() {
                    println!("  ✔ No invariant violations or warnings found.");
                } else {
                    println!("  Findings ({}) :", report.findings.len());
                    for (i, f) in report.findings.iter().enumerate() {
                        let icon = match f.severity {
                            control_plane::reviewer::FindingSeverity::Fatal => "✘ [FATAL]",
                            control_plane::reviewer::FindingSeverity::Warning => "▲ [WARN]",
                            control_plane::reviewer::FindingSeverity::Info => "ℹ [INFO]",
                        };
                        println!("    {}. {} [{}] {}", i + 1, icon, f.category, f.message);
                    }
                }
                println!("══════════════════════════════════════════════════════════════════════");
            }
            if report.verdict == control_plane::reviewer::ReviewVerdict::Rejected {
                exit(1);
            }
        }
        "dry-run" => {
            let preview = Simulator::simulate_plan(&plan, Some(&args.work_root));
            if args.json {
                println!("{}", serde_json::to_string_pretty(&preview).unwrap());
            } else {
                println!("{}", preview.render_terminal());
            }
        }
        "apply" => {
            // Independent Reviewer Agent pass before presentation/execution
            let reviewer = control_plane::reviewer::Reviewer::new(
                control_plane::reviewer::ReviewerConfig::default(),
            );
            let review_report = reviewer.review_plan(&plan);
            if review_report.verdict == control_plane::reviewer::ReviewVerdict::Rejected {
                eprintln!("\nExecution BLOCKED by Reviewer Agent:");
                for f in &review_report.findings {
                    eprintln!("  ✘ [{:?}] {}: {}", f.severity, f.category, f.message);
                }
                exit(1);
            }

            let preview = Simulator::simulate_plan(&plan, Some(&args.work_root));
            if !args.json {
                println!("{}", preview.render_terminal());
            }

            if plan.aggregate_blast_radius.tier.requires_confirmation() && !args.auto_approve {
                eprint!(
                    "\nApproval required for Tier {:?}. Proceed with execution? [y/N]: ",
                    plan.aggregate_blast_radius.tier
                );
                let mut line = String::new();
                let stdin = io::stdin();
                stdin.lock().read_line(&mut line).unwrap();
                if line.trim().to_lowercase() != "y" && line.trim().to_lowercase() != "yes" {
                    eprintln!("Execution cancelled by user.");
                    exit(0);
                }
            }

            let client = CapabilityClient::new(&args.socket_path, "ai-engineer-cli");
            let implementer = Implementer::new(&client);
            let report = implementer.execute_plan(&plan);

            let verifier = Verifier::new(&client);
            let verify_report = verifier.verify_plan(&plan);

            if args.json {
                let out = serde_json::json!({
                    "execution": report,
                    "verification": verify_report,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                println!(
                    "\n══════════════════════════════════════════════════════════════════════"
                );
                println!("  EXECUTION RESULT: {:?}", report.final_status);
                println!("  Message: {}", report.message);
                println!("  Total time: {} ms", report.total_duration_ms);
                println!("──────────────────────────────────────────────────────────────────────");
                println!(
                    "  Verification: {}",
                    if verify_report.all_passed {
                        "PASSED (100%)"
                    } else {
                        "FAILED"
                    }
                );
                for check in &verify_report.checks {
                    println!(
                        "    [{}] {} - {}",
                        if check.passed { "✔" } else { "✘" },
                        check.target,
                        check.message
                    );
                }
                println!("══════════════════════════════════════════════════════════════════════");
            }

            if report.final_status != control_plane::PlanStatus::Completed
                || !verify_report.all_passed
            {
                exit(1);
            }
        }
        "explain" => {
            match args.depth {
                1 => {
                    // 1-line plain English summary
                    println!("{}", plan.summary);
                }
                2 => {
                    // Engineering design & steps
                    println!(
                        "══════════════════════════════════════════════════════════════════════"
                    );
                    println!("ENGINEERING PLAN: {}", plan.intent);
                    println!(
                        "Tier: {:?} | Blast Score: {}/100",
                        plan.aggregate_blast_radius.tier, plan.aggregate_blast_radius.score
                    );
                    println!(
                        "──────────────────────────────────────────────────────────────────────"
                    );
                    println!("{}", plan.summary);
                    println!("\nAction Breakdown:");
                    for (i, step) in plan.steps.iter().enumerate() {
                        println!(
                            "  {}. {} (Blast Score: {})",
                            i + 1,
                            step.description,
                            step.blast_radius.score
                        );
                    }
                    println!("\nRollback Safety:");
                    for (i, rb) in plan.rollback_steps.iter().enumerate() {
                        println!("  {}. {}", i + 1, rb.description);
                    }
                    println!(
                        "══════════════════════════════════════════════════════════════════════"
                    );
                }
                3 => {
                    // Raw capabilities, policy scores, and diff projections
                    let preview = Simulator::simulate_plan(&plan, Some(&args.work_root));
                    let combined = serde_json::json!({
                        "plan": plan,
                        "simulation": preview,
                    });
                    println!("{}", serde_json::to_string_pretty(&combined).unwrap());
                }
                _ => {
                    eprintln!("Invalid depth {}. Supported depths: 1, 2, 3", args.depth);
                    exit(1);
                }
            }
        }
        cmd => {
            eprintln!("Unknown command: '{cmd}'");
            print_usage();
            exit(1);
        }
    }
}
