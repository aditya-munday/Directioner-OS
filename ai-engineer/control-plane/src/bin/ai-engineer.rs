//! Directioner-OS AI Engineer CLI.
//!
//! Control-plane interface for planning, dry-running, applying, and explaining
//! autonomous system changes.

use std::env;
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
        r#"Directioner-OS AI Engineer CLI (Phase 1 & Phase 2 Control Plane)

USAGE:
    ai-engineer <COMMAND> [OPTIONS]

COMMANDS:
    plan <INTENT>        Generate a structured multi-step plan with blast-radius scoring
    review <INTENT>      Run independent multi-agent Reviewer audit on synthesized plan
    dry-run <INTENT>     Simulate plan and generate unified diffs without modifying state
    apply <INTENT>       Plan, review, preview, prompt for confirmation, and execute via Capability Layer
    explain <INTENT>     Provide 3-depth explanation (summary, engineering plan, raw diffs)
    diagnose [PATH]      Scan workspace, parse compiler diagnostics, and localize cross-crate root causes
    fix [PATH]           Synthesize multi-crate repair plan, audit with Reviewer, preview diffs, and apply with verification

OPTIONS:
    --socket <PATH>      Path to ai-capabilityd Unix domain socket (default: /tmp/ai-engineer/ai-capabilityd.sock)
    --db <PATH>          Path to world-model SQLite DB (default: /tmp/ai-engineer/worldmodel.db)
    --work-root <PATH>   Path root for workspace source operations (default: current directory or /tmp/ai-engineer/scope)
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
    socket_path: PathBuf,
    db_path: PathBuf,
    work_root: PathBuf,
    auto_approve: bool,
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
    let mut socket_path = PathBuf::from("/tmp/ai-engineer/ai-capabilityd.sock");
    let mut db_path = PathBuf::from("/tmp/ai-engineer/worldmodel.db");
    let mut work_root = PathBuf::from("/tmp/ai-engineer/scope");
    let mut auto_approve = false;
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
            "--work-root" => {
                i += 1;
                if i >= args.len() {
                    return Err("--work-root requires an argument".into());
                }
                work_root = PathBuf::from(&args[i]);
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
        socket_path,
        db_path,
        work_root,
        auto_approve,
        depth,
        json,
    })
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

        let plan = match engine
            .synthesize_cross_crate_plan(&initial_report.diagnostics, db_conn.as_ref())
        {
            Some(p) => p,
            None => {
                eprintln!("Could not synthesize automated remediation plan from diagnostics.");
                exit(1);
            }
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
                use std::io::{self, BufRead};
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
