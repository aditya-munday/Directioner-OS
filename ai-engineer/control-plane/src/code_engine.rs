//! Code Engineering & Structured Diagnostics Integration (Phase 2).
//!
//! Provides structured runners and analyzers for compiler outputs (`cargo build/test --message-format=json`,
//! `pytest --json-report`), enabling automated error localization, code graph traversal, and remediation.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::symbol_graph::WorkspaceSymbolGraph;

/// A span reference in a compiler diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticSpan {
    /// File path where the span is located.
    pub file_path: String,
    /// 1-based start line.
    pub line_start: usize,
    /// 1-based end line.
    pub line_end: usize,
    /// 1-based start column.
    pub column_start: usize,
    /// 1-based end column.
    pub column_end: usize,
    /// Whether this is the primary error span.
    pub is_primary: bool,
    /// Diagnostic label or annotation on the span.
    pub label: Option<String>,
    /// Machine-applicable suggested replacement, if available.
    pub suggested_replacement: Option<String>,
}

/// A parsed compiler diagnostic or linter finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    /// Severity level ("error", "warning", "note", "help").
    pub level: String,
    /// Compiler error code (e.g. "E0308", "E0425", "E0061"), if present.
    pub code: Option<String>,
    /// Primary explanation message.
    pub message: String,
    /// Primary file path where the error occurred.
    pub file_path: Option<String>,
    /// Line number (1-based), if available.
    pub line_number: Option<usize>,
    /// Column number (1-based), if available.
    pub column_number: Option<usize>,
    /// Suggested replacement or code snippet, if provided by compiler.
    pub suggested_replacement: Option<String>,
    /// All spans (primary and secondary reference notes) associated with the diagnostic.
    pub spans: Vec<DiagnosticSpan>,
    /// Associated note / help messages from compiler children.
    pub notes: Vec<String>,
}

/// Cross-crate root cause analysis tracing a downstream error to an upstream definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrossCrateRootCause {
    /// File where the diagnostic error manifested (call/usage site).
    pub call_site_file: PathBuf,
    /// Line number of the call site.
    pub call_site_line: usize,
    /// Upstream file where the referenced item is defined.
    pub definition_file: PathBuf,
    /// Line number of the upstream definition.
    pub definition_line: usize,
    /// Name of the symbol involved.
    pub symbol_name: Option<String>,
    /// Upstream crate name.
    pub upstream_crate: String,
    /// Compiler diagnostic error code.
    pub diagnostic_code: Option<String>,
    /// Explanation of the cross-crate dependency relation.
    pub explanation: String,
}

/// Structured test failure report from a test harness execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestFailure {
    /// Name of the failing test (e.g. "tests::my_failing_test").
    pub name: String,
    /// Failure message or panic output.
    pub message: String,
    /// Execution duration in seconds.
    pub duration_secs: Option<f64>,
}

/// Comprehensive summary of a build or test run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuildTestReport {
    /// Whether the overall build/test succeeded.
    pub success: bool,
    /// Total number of tests executed.
    pub total_tests: usize,
    /// Total number of passed tests.
    pub passed_tests: usize,
    /// Total number of failed tests.
    pub failed_tests: usize,
    /// List of specific test failures.
    pub failures: Vec<TestFailure>,
    /// List of compiler/linter diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Raw output snippet if errors occurred.
    pub raw_snippet: String,
}

/// The Code Engine runner that executes builds and parses diagnostics.
pub struct CodeEngine {
    workspace_root: PathBuf,
    symbol_graph: Option<WorkspaceSymbolGraph>,
}

impl CodeEngine {
    /// Create a new CodeEngine for the given workspace root.
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
            symbol_graph: None,
        }
    }

    /// Set an existing symbol graph on the CodeEngine.
    pub fn with_symbol_graph(mut self, graph: WorkspaceSymbolGraph) -> Self {
        self.symbol_graph = Some(graph);
        self
    }

    /// Build or refresh the workspace symbol graph.
    pub fn build_symbol_graph(&mut self) -> Result<(), String> {
        let graph = WorkspaceSymbolGraph::from_workspace(&self.workspace_root)?;
        self.symbol_graph = Some(graph);
        Ok(())
    }

    /// Returns a reference to the symbol graph if initialized.
    pub fn symbol_graph(&self) -> Option<&WorkspaceSymbolGraph> {
        self.symbol_graph.as_ref()
    }

    /// Returns the workspace root path.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Parse cargo JSON message stream from compiler output.
    pub fn parse_cargo_json(json_stream: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for line in json_stream.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
                && v["reason"] == "compiler-message"
                && let Some(msg) = v.get("message")
            {
                let level = msg["level"].as_str().unwrap_or("error").to_string();
                let message = msg["message"].as_str().unwrap_or("").to_string();
                let code = msg["code"]["code"].as_str().map(|s| s.to_string());

                let mut spans = Vec::new();
                if let Some(raw_spans) = msg["spans"].as_array() {
                    for s in raw_spans {
                        let file_path = s["file_name"].as_str().unwrap_or("").to_string();
                        let line_start = s["line_start"].as_u64().unwrap_or(0) as usize;
                        let line_end = s["line_end"].as_u64().unwrap_or(0) as usize;
                        let column_start = s["column_start"].as_u64().unwrap_or(0) as usize;
                        let column_end = s["column_end"].as_u64().unwrap_or(0) as usize;
                        let is_primary = s["is_primary"].as_bool().unwrap_or(false);
                        let label = s["label"].as_str().map(|str| str.to_string());
                        let suggested_replacement = s["suggested_replacement"]
                            .as_str()
                            .map(|str| str.to_string());

                        spans.push(DiagnosticSpan {
                            file_path,
                            line_start,
                            line_end,
                            column_start,
                            column_end,
                            is_primary,
                            label,
                            suggested_replacement,
                        });
                    }
                }

                let mut notes = Vec::new();
                if let Some(children) = msg["children"].as_array() {
                    for c in children {
                        if let Some(child_msg) = c["message"].as_str() {
                            notes.push(child_msg.to_string());
                        }
                    }
                }

                let primary_span = spans.iter().find(|s| s.is_primary);

                let file_path = primary_span.map(|s| s.file_path.clone());
                let line_number = primary_span.map(|s| s.line_start);
                let column_number = primary_span.map(|s| s.column_start);
                let suggested_replacement =
                    primary_span.and_then(|s| s.suggested_replacement.clone());

                diagnostics.push(Diagnostic {
                    level,
                    code,
                    message,
                    file_path,
                    line_number,
                    column_number,
                    suggested_replacement,
                    spans,
                    notes,
                });
            }
        }

        diagnostics
    }

    /// Parse pytest json report or standard test summary.
    pub fn parse_pytest_json(report_json: &str) -> BuildTestReport {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(report_json) {
            let summary = &v["summary"];
            let passed = summary["passed"].as_u64().unwrap_or(0) as usize;
            let failed = summary["failed"].as_u64().unwrap_or(0) as usize;
            let total = summary["total"]
                .as_u64()
                .unwrap_or((passed + failed) as u64) as usize;
            let success = failed == 0;

            let mut failures = Vec::new();
            if let Some(tests) = v["tests"].as_array() {
                for t in tests {
                    if t["outcome"] == "failed" {
                        let name = t["nodeid"].as_str().unwrap_or("unknown").to_string();
                        let msg = t["call"]["longrepr"]
                            .as_str()
                            .unwrap_or("test failed")
                            .to_string();
                        let duration_secs = t["call"]["duration"].as_f64();
                        failures.push(TestFailure {
                            name,
                            message: msg,
                            duration_secs,
                        });
                    }
                }
            }

            BuildTestReport {
                success,
                total_tests: total,
                passed_tests: passed,
                failed_tests: failed,
                failures,
                diagnostics: Vec::new(),
                raw_snippet: String::new(),
            }
        } else {
            // Fallback for non-JSON or plain output
            BuildTestReport {
                success: !report_json.contains("FAILED"),
                total_tests: 0,
                passed_tests: 0,
                failed_tests: if report_json.contains("FAILED") { 1 } else { 0 },
                failures: Vec::new(),
                diagnostics: Vec::new(),
                raw_snippet: report_json.chars().take(500).collect(),
            }
        }
    }

    /// Synthesize remediation intent from diagnostic findings.
    pub fn synthesize_remediation_intent(diagnostics: &[Diagnostic]) -> Option<String> {
        let error_diag = diagnostics.iter().find(|d| d.level == "error")?;

        let location = match (&error_diag.file_path, error_diag.line_number) {
            (Some(f), Some(l)) => format!(" in {f}:{l}"),
            (Some(f), None) => format!(" in {f}"),
            _ => String::new(),
        };

        let code_str = match &error_diag.code {
            Some(c) => format!(" [{c}]"),
            None => String::new(),
        };

        Some(format!(
            "Fix compiler error{code_str}{location}: {}",
            error_diag.message
        ))
    }

    /// Synthesize an executable execution Plan from compiler diagnostics.
    pub fn synthesize_plan_from_diagnostics(
        &self,
        diagnostics: &[Diagnostic],
        db: Option<&rusqlite::Connection>,
    ) -> Option<crate::plan::Plan> {
        use crate::plan::{Plan, PlanStatus, PlanStep, RollbackStep};
        use crate::policy::PolicyEngine;
        use capability_layer::CapabilityKind;

        let intent = Self::synthesize_remediation_intent(diagnostics)?;
        let mut steps = Vec::new();
        let mut rollback_steps = Vec::new();

        let mut step_num = 1;
        for diag in diagnostics {
            if diag.level != "error" {
                continue;
            }
            if let Some(file_path) = &diag.file_path {
                let target_path = if Path::new(file_path).is_absolute() {
                    PathBuf::from(file_path)
                } else {
                    self.workspace_root.join(file_path)
                };

                let step_id = format!("step-{}", step_num);
                let desc = format!(
                    "Apply compiler diagnostic fix for {} ({})",
                    file_path, diag.message
                );

                let (previous_content, is_new_file) = if target_path.exists() {
                    match std::fs::read_to_string(&target_path) {
                        Ok(c) => (Some(c), false),
                        Err(_) => (None, false),
                    }
                } else {
                    (None, true)
                };

                let new_content = match (&previous_content, &diag.suggested_replacement) {
                    (Some(prev), Some(rep)) => {
                        let mut lines: Vec<&str> = prev.lines().collect();
                        if let Some(line_num) = diag.line_number
                            && line_num > 0
                            && line_num <= lines.len()
                        {
                            lines[line_num - 1] = rep;
                            let mut joined = lines.join("\n");
                            joined.push('\n');
                            joined
                        } else {
                            rep.clone()
                        }
                    }
                    (None, Some(rep)) => rep.clone(),
                    _ => {
                        // Skip diagnostics without machine-applicable suggested replacements
                        continue;
                    }
                };

                let cap = CapabilityKind::FileWrite {
                    path: target_path.to_string_lossy().to_string(),
                    content: new_content,
                    append: false,
                };

                let blast_radius = PolicyEngine::evaluate_capability(&cap, db);

                steps.push(PlanStep {
                    step_id: step_id.clone(),
                    description: desc,
                    capability: cap,
                    blast_radius,
                    timeout_secs: 30,
                });

                if is_new_file {
                    rollback_steps.push(RollbackStep {
                        step_id: format!("rollback-{}", step_id),
                        description: format!("Delete newly created file {}", file_path),
                        capability: CapabilityKind::FileDelete {
                            path: target_path.to_string_lossy().to_string(),
                        },
                        triggers: vec![step_id],
                    });
                } else if let Some(prev) = previous_content {
                    rollback_steps.push(RollbackStep {
                        step_id: format!("rollback-{}", step_id),
                        description: format!("Restore original content of {}", file_path),
                        capability: CapabilityKind::FileWrite {
                            path: target_path.to_string_lossy().to_string(),
                            content: prev,
                            append: false,
                        },
                        triggers: vec![step_id],
                    });
                }

                step_num += 1;
            }
        }

        if steps.is_empty() {
            return None;
        }

        let caps: Vec<CapabilityKind> = steps.iter().map(|s| s.capability.clone()).collect();
        let aggregate_blast_radius = PolicyEngine::evaluate_plan_aggregate(&caps, db);

        let plan_id = format!(
            "plan-fix-{}",
            &chrono::Utc::now()
                .timestamp_nanos_opt()
                .unwrap_or(0)
                .to_string()[..8]
        );

        Some(Plan {
            id: plan_id,
            intent,
            summary: format!(
                "Automated compiler diagnostic remediation with {} step(s)",
                steps.len()
            ),
            steps,
            rollback_steps,
            aggregate_blast_radius,
            created_at: chrono::Utc::now().to_rfc3339(),
            status: PlanStatus::Draft,
        })
    }

    /// Run cargo check or clippy with JSON message format on the workspace.
    pub fn run_cargo_check(&self) -> Result<BuildTestReport, String> {
        let output = std::process::Command::new("cargo")
            .arg("check")
            .arg("--message-format=json")
            .current_dir(&self.workspace_root)
            .output()
            .map_err(|e| format!("Failed to invoke cargo check: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let diagnostics = Self::parse_cargo_json(&stdout);
        let has_errors = diagnostics.iter().any(|d| d.level == "error");
        let success = output.status.success() && !has_errors;

        Ok(BuildTestReport {
            success,
            total_tests: 0,
            passed_tests: 0,
            failed_tests: if success { 0 } else { 1 },
            failures: Vec::new(),
            diagnostics,
            raw_snippet: if success {
                String::new()
            } else {
                stderr.chars().take(1000).collect()
            },
        })
    }

    /// Run cargo test and collect test results and diagnostics.
    pub fn run_cargo_test(&self) -> Result<BuildTestReport, String> {
        let output = std::process::Command::new("cargo")
            .arg("test")
            .arg("--")
            .arg("--nocapture")
            .current_dir(&self.workspace_root)
            .output()
            .map_err(|e| format!("Failed to invoke cargo test: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let success = output.status.success();

        let mut failures = Vec::new();
        for line in stdout.lines() {
            if line.starts_with("test ") && line.ends_with(" ... FAILED") {
                let name = line
                    .trim_start_matches("test ")
                    .trim_end_matches(" ... FAILED")
                    .to_string();
                failures.push(TestFailure {
                    name,
                    message: "Test execution failed".into(),
                    duration_secs: None,
                });
            }
        }

        let failed_count = failures.len();

        Ok(BuildTestReport {
            success,
            total_tests: failed_count,
            passed_tests: 0,
            failed_tests: failed_count,
            failures,
            diagnostics: Vec::new(),
            raw_snippet: if success {
                String::new()
            } else {
                format!("{stdout}\n{stderr}").chars().take(1000).collect()
            },
        })
    }

    /// Run pytest and parse output report if available.
    pub fn run_pytest(&self) -> Result<BuildTestReport, String> {
        let output = std::process::Command::new("pytest")
            .arg("--json-report")
            .arg("--json-report-file=.report.json")
            .current_dir(&self.workspace_root)
            .output()
            .map_err(|e| format!("Failed to invoke pytest: {e}"))?;

        let report_file = self.workspace_root.join(".report.json");
        if report_file.exists()
            && let Ok(content) = std::fs::read_to_string(&report_file)
        {
            let _ = std::fs::remove_file(&report_file);
            return Ok(Self::parse_pytest_json(&content));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_pytest_json(&stdout))
    }

    /// Execute automated remediation loop: test -> diagnose -> plan -> review -> implement -> verify.
    pub fn execute_remediation_loop(
        &self,
        implementer: &crate::implementer::Implementer,
        reviewer: &crate::reviewer::Reviewer,
        db: Option<&rusqlite::Connection>,
        max_iterations: usize,
    ) -> Result<RemediationResult, String> {
        let initial_report = self.run_cargo_check()?;
        if initial_report.success {
            return Ok(RemediationResult {
                resolved: true,
                iterations: 0,
                initial_report: initial_report.clone(),
                final_report: initial_report,
                plan_history: Vec::new(),
            });
        }

        let mut current_report = initial_report.clone();
        let mut plan_history = Vec::new();
        let mut iterations = 0;

        while iterations < max_iterations && !current_report.success {
            iterations += 1;
            let plan = self
                .synthesize_plan_from_diagnostics(&current_report.diagnostics, db)
                .ok_or_else(|| "Could not synthesize plan from diagnostics".to_string())?;

            let review = reviewer.review_plan(&plan);
            if review.verdict == crate::reviewer::ReviewVerdict::Rejected {
                return Err(format!(
                    "Reviewer rejected remediation plan: {}",
                    review.summary
                ));
            }

            let exec_report = implementer.execute_plan(&plan);
            let succeeded = exec_report.final_status == crate::plan::PlanStatus::Completed;
            plan_history.push(exec_report);

            if !succeeded {
                return Err("Plan execution failed during remediation".into());
            }

            current_report = self.run_cargo_check()?;
        }

        let resolved = current_report.success;
        Ok(RemediationResult {
            resolved,
            iterations,
            initial_report,
            final_report: current_report,
            plan_history,
        })
    }

    /// Localize the root cause definition across workspace crates for a given diagnostic.
    pub fn localize_diagnostic_root(&self, diag: &Diagnostic) -> Option<CrossCrateRootCause> {
        let primary_span = diag.spans.iter().find(|s| s.is_primary)?;
        let call_site_file = if Path::new(&primary_span.file_path).is_absolute() {
            PathBuf::from(&primary_span.file_path)
        } else {
            self.workspace_root.join(&primary_span.file_path)
        };
        let call_site_line = primary_span.line_start;

        // Check if there is a secondary span pointing to an upstream definition (e.g. note: method defined here)
        let secondary_span = diag.spans.iter().find(|s| !s.is_primary);

        if let Some(sec) = secondary_span {
            let def_file = if Path::new(&sec.file_path).is_absolute() {
                PathBuf::from(&sec.file_path)
            } else {
                self.workspace_root.join(&sec.file_path)
            };

            let sym_name = self.symbol_graph.as_ref().and_then(|g| {
                g.find_in_file(&def_file, sec.line_start)
                    .map(|s| s.name.clone())
            });

            let upstream_crate = self
                .symbol_graph
                .as_ref()
                .and_then(|g| {
                    g.find_in_file(&def_file, sec.line_start)
                        .map(|s| s.crate_name.clone())
                })
                .unwrap_or_else(|| "upstream".to_string());

            return Some(CrossCrateRootCause {
                call_site_file,
                call_site_line,
                definition_file: def_file,
                definition_line: sec.line_start,
                symbol_name: sym_name,
                upstream_crate: upstream_crate.clone(),
                diagnostic_code: diag.code.clone(),
                explanation: format!(
                    "Call site at {}:{} references definition at {}:{} in crate '{}'",
                    primary_span.file_path,
                    call_site_line,
                    sec.file_path,
                    sec.line_start,
                    upstream_crate
                ),
            });
        }

        // If no secondary span in diagnostic, attempt symbol lookup via WorkspaceSymbolGraph
        if let Some(graph) = &self.symbol_graph {
            for sym in &graph.symbols {
                if diag.message.contains(&sym.name) {
                    return Some(CrossCrateRootCause {
                        call_site_file,
                        call_site_line,
                        definition_file: sym.file_path.clone(),
                        definition_line: sym.line_start,
                        symbol_name: Some(sym.name.clone()),
                        upstream_crate: sym.crate_name.clone(),
                        diagnostic_code: diag.code.clone(),
                        explanation: format!(
                            "Diagnostic matches indexed symbol '{}' ({:?}) defined at {}:{} in crate '{}'",
                            sym.name,
                            sym.kind,
                            sym.file_path.display(),
                            sym.line_start,
                            sym.crate_name
                        ),
                    });
                }
            }
        }

        None
    }

    /// Synthesize a coordinated multi-crate Plan addressing both call-site and definition-site repairs.
    pub fn synthesize_cross_crate_plan(
        &self,
        diagnostics: &[Diagnostic],
        db: Option<&rusqlite::Connection>,
    ) -> Option<crate::plan::Plan> {
        use crate::plan::{Plan, PlanStatus, PlanStep, RollbackStep};
        use crate::policy::PolicyEngine;
        use capability_layer::CapabilityKind;

        let intent = Self::synthesize_remediation_intent(diagnostics)?;
        let mut steps = Vec::new();
        let mut rollback_steps = Vec::new();
        let mut modified_files = std::collections::HashSet::new();

        let mut step_num = 1;
        for diag in diagnostics {
            if diag.level != "error" {
                continue;
            }

            for span in &diag.spans {
                if span.file_path.is_empty() {
                    continue;
                }

                let target_path = if Path::new(&span.file_path).is_absolute() {
                    PathBuf::from(&span.file_path)
                } else {
                    self.workspace_root.join(&span.file_path)
                };

                let step_id = format!("step-{}", step_num);
                let desc = format!(
                    "Apply cross-crate fix for {} (span {}:{})",
                    span.file_path, span.line_start, span.column_start
                );

                let (previous_content, is_new_file) = if target_path.exists() {
                    match std::fs::read_to_string(&target_path) {
                        Ok(c) => (Some(c), false),
                        Err(_) => (None, false),
                    }
                } else {
                    (None, true)
                };

                let new_content = match (&previous_content, &span.suggested_replacement) {
                    (Some(prev), Some(rep)) => {
                        let mut lines: Vec<&str> = prev.lines().collect();
                        if span.line_start > 0 && span.line_start <= lines.len() {
                            lines[span.line_start - 1] = rep;
                            let mut joined = lines.join("\n");
                            joined.push('\n');
                            joined
                        } else {
                            rep.clone()
                        }
                    }
                    (None, Some(rep)) => rep.clone(),
                    _ => {
                        // Skip spans without machine-applicable suggested replacements
                        continue;
                    }
                };

                let cap = CapabilityKind::FileWrite {
                    path: target_path.to_string_lossy().to_string(),
                    content: new_content,
                    append: false,
                };

                let blast_radius = PolicyEngine::evaluate_capability(&cap, db);

                steps.push(PlanStep {
                    step_id: step_id.clone(),
                    description: desc,
                    capability: cap,
                    blast_radius,
                    timeout_secs: 30,
                });

                if is_new_file {
                    rollback_steps.push(RollbackStep {
                        step_id: format!("rollback-{}", step_id),
                        description: format!("Delete newly created file {}", span.file_path),
                        capability: CapabilityKind::FileDelete {
                            path: target_path.to_string_lossy().to_string(),
                        },
                        triggers: vec![step_id],
                    });
                } else if let Some(prev) = previous_content
                    && modified_files.insert(target_path.clone())
                {
                    rollback_steps.push(RollbackStep {
                        step_id: format!("rollback-{}", step_id),
                        description: format!("Restore original content of {}", span.file_path),
                        capability: CapabilityKind::FileWrite {
                            path: target_path.to_string_lossy().to_string(),
                            content: prev,
                            append: false,
                        },
                        triggers: vec![step_id],
                    });
                }

                step_num += 1;
            }
        }

        if steps.is_empty() {
            return None;
        }

        let caps: Vec<CapabilityKind> = steps.iter().map(|s| s.capability.clone()).collect();
        let aggregate_blast_radius = PolicyEngine::evaluate_plan_aggregate(&caps, db);

        let plan_id = format!(
            "plan-crosscrate-{}",
            &chrono::Utc::now()
                .timestamp_nanos_opt()
                .unwrap_or(0)
                .to_string()[..8]
        );

        Some(Plan {
            id: plan_id,
            intent,
            summary: format!(
                "Coordinated cross-crate remediation plan with {} step(s)",
                steps.len()
            ),
            steps,
            rollback_steps,
            aggregate_blast_radius,
            created_at: chrono::Utc::now().to_rfc3339(),
            status: PlanStatus::Draft,
        })
    }
}

/// Summary result of an automated remediation loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemediationResult {
    /// Whether all diagnostics/failures were resolved.
    pub resolved: bool,
    /// Number of repair iterations attempted.
    pub iterations: usize,
    /// The initial diagnostic report before repair.
    pub initial_report: BuildTestReport,
    /// The final diagnostic report after repair.
    pub final_report: BuildTestReport,
    /// History of executed plans and execution reports.
    pub plan_history: Vec<crate::plan::PlanExecutionReport>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cargo_json_compiler_message() {
        let sample_cargo_json = r#"
{"reason":"compiler-artifact","package_id":"sample 0.1.0"}
{"reason":"compiler-message","package_id":"sample 0.1.0","message":{"rendered":"error[E0308]: mismatched types\n --> src/main.rs:10:5\n","children":[],"code":{"code":"E0308","explanation":null},"level":"error","message":"mismatched types","spans":[{"byte_end":150,"byte_start":140,"column_end":15,"column_start":5,"file_name":"src/main.rs","is_primary":true,"line_end":10,"line_start":10,"suggested_replacement":"42","suggestion_applicability":"MachineApplicable","text":[{"highlight_end":15,"highlight_start":5,"text":"    \"wrong\""}]}]}}
{"reason":"build-finished","success":false}
"#;

        let diags = CodeEngine::parse_cargo_json(sample_cargo_json);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, "error");
        assert_eq!(diags[0].code.as_deref(), Some("E0308"));
        assert_eq!(diags[0].file_path.as_deref(), Some("src/main.rs"));
        assert_eq!(diags[0].line_number, Some(10));
        assert_eq!(diags[0].suggested_replacement.as_deref(), Some("42"));

        let intent = CodeEngine::synthesize_remediation_intent(&diags);
        assert!(intent.is_some());
        assert!(
            intent
                .unwrap()
                .contains("Fix compiler error [E0308] in src/main.rs:10: mismatched types")
        );
    }

    #[test]
    fn test_parse_pytest_json() {
        let sample_pytest_json = r#"{
            "summary": { "passed": 4, "failed": 1, "total": 5 },
            "tests": [
                {
                    "nodeid": "test_app.py::test_auth",
                    "outcome": "failed",
                    "call": { "longrepr": "AssertionError: 401 != 200", "duration": 0.05 }
                }
            ]
        }"#;

        let report = CodeEngine::parse_pytest_json(sample_pytest_json);
        assert!(!report.success);
        assert_eq!(report.passed_tests, 4);
        assert_eq!(report.failed_tests, 1);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].name, "test_app.py::test_auth");
    }

    #[test]
    fn test_synthesize_plan_from_diagnostics() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ai_engine_test_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let test_file = temp_dir.join("main.rs");
        std::fs::write(&test_file, "fn main() {\n    let x = \"wrong\";\n}\n").unwrap();

        let engine = CodeEngine::new(&temp_dir);
        let diags = vec![Diagnostic {
            level: "error".into(),
            code: Some("E0308".into()),
            message: "mismatched types".into(),
            file_path: Some("main.rs".into()),
            line_number: Some(2),
            column_number: Some(5),
            suggested_replacement: Some("    let x = 42;".into()),
            spans: vec![DiagnosticSpan {
                file_path: "main.rs".into(),
                line_start: 2,
                line_end: 2,
                column_start: 5,
                column_end: 15,
                is_primary: true,
                label: None,
                suggested_replacement: Some("    let x = 42;".into()),
            }],
            notes: Vec::new(),
        }];

        let plan = engine.synthesize_plan_from_diagnostics(&diags, None);
        assert!(plan.is_some());
        let plan = plan.unwrap();

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.rollback_steps.len(), 1);
        assert!(plan.intent.contains("mismatched types"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_code_engine_remediation_workflow() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ai_remediation_test_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let test_file = temp_dir.join("broken.rs");
        std::fs::write(&test_file, "let a = 1;\nlet b = \"hello\";\n").unwrap();

        let engine = CodeEngine::new(&temp_dir);
        let diags = vec![Diagnostic {
            level: "error".into(),
            code: Some("E0308".into()),
            message: "mismatched types".into(),
            file_path: Some("broken.rs".into()),
            line_number: Some(2),
            column_number: Some(1),
            suggested_replacement: Some("let b = 2;".into()),
            spans: vec![DiagnosticSpan {
                file_path: "broken.rs".into(),
                line_start: 2,
                line_end: 2,
                column_start: 1,
                column_end: 15,
                is_primary: true,
                label: None,
                suggested_replacement: Some("let b = 2;".into()),
            }],
            notes: Vec::new(),
        }];

        let plan = engine
            .synthesize_plan_from_diagnostics(&diags, None)
            .unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.rollback_steps.len(), 1);

        // Verification of step capability
        if let capability_layer::CapabilityKind::FileWrite { path, content, .. } =
            &plan.steps[0].capability
        {
            assert!(path.ends_with("broken.rs"));
            assert_eq!(content, "let a = 1;\nlet b = 2;\n");
        } else {
            panic!("Expected FileWrite capability");
        }

        // Verification of rollback capability
        if let capability_layer::CapabilityKind::FileWrite { path, content, .. } =
            &plan.rollback_steps[0].capability
        {
            assert!(path.ends_with("broken.rs"));
            assert_eq!(content, "let a = 1;\nlet b = \"hello\";\n");
        } else {
            panic!("Expected FileWrite rollback capability");
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cross_crate_diagnostic_localization() {
        let temp_dir = std::env::temp_dir().join(format!(
            "cross_crate_diag_test_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let crate_a = temp_dir.join("crate-a");
        let crate_b = temp_dir.join("crate-b");
        std::fs::create_dir_all(&crate_a).unwrap();
        std::fs::create_dir_all(&crate_b).unwrap();

        let lib_a = crate_a.join("lib.rs");
        let main_b = crate_b.join("main.rs");
        std::fs::write(&lib_a, "pub fn process_order(id: u64) -> bool { id > 0 }\n").unwrap();
        std::fs::write(&main_b, "fn run() { crate_a::process_order(1, 2); }\n").unwrap();

        let mut graph = WorkspaceSymbolGraph::new();
        graph.index_file("crate-a", &lib_a).unwrap();
        graph.index_file("crate-b", &main_b).unwrap();

        let engine = CodeEngine::new(&temp_dir).with_symbol_graph(graph);

        let diag = Diagnostic {
            level: "error".into(),
            code: Some("E0061".into()),
            message: "this function takes 1 argument but 2 arguments were supplied".into(),
            file_path: Some("crate-b/main.rs".into()),
            line_number: Some(1),
            column_number: Some(12),
            suggested_replacement: Some("crate_a::process_order(1);".into()),
            spans: vec![
                DiagnosticSpan {
                    file_path: "crate-b/main.rs".into(),
                    line_start: 1,
                    line_end: 1,
                    column_start: 12,
                    column_end: 38,
                    is_primary: true,
                    label: Some("supplied 2 arguments".into()),
                    suggested_replacement: Some("crate_a::process_order(1);".into()),
                },
                DiagnosticSpan {
                    file_path: "crate-a/lib.rs".into(),
                    line_start: 1,
                    line_end: 1,
                    column_start: 1,
                    column_end: 35,
                    is_primary: false,
                    label: Some("function defined here".into()),
                    suggested_replacement: None,
                },
            ],
            notes: vec!["note: function defined here".into()],
        };

        let root_cause = engine.localize_diagnostic_root(&diag);
        assert!(root_cause.is_some());
        let root = root_cause.unwrap();

        assert_eq!(root.call_site_file, temp_dir.join("crate-b/main.rs"));
        assert_eq!(root.call_site_line, 1);
        assert_eq!(root.definition_file, temp_dir.join("crate-a/lib.rs"));
        assert_eq!(root.definition_line, 1);
        assert_eq!(root.symbol_name.as_deref(), Some("process_order"));
        assert_eq!(root.upstream_crate, "crate-a");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_synthesize_cross_crate_multistep_plan() {
        let temp_dir = std::env::temp_dir().join(format!(
            "cross_crate_plan_test_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let crate_a = temp_dir.join("crate-a");
        let crate_b = temp_dir.join("crate-b");
        std::fs::create_dir_all(&crate_a).unwrap();
        std::fs::create_dir_all(&crate_b).unwrap();

        let lib_a = crate_a.join("lib.rs");
        let main_b = crate_b.join("main.rs");
        std::fs::write(&lib_a, "pub fn compute() -> i32 { 10 }\n").unwrap();
        std::fs::write(&main_b, "fn main() { let _ = crate_a::compute(); }\n").unwrap();

        let engine = CodeEngine::new(&temp_dir);

        let diags = vec![Diagnostic {
            level: "error".into(),
            code: Some("E0308".into()),
            message: "mismatched types across crates".into(),
            file_path: Some("crate-b/main.rs".into()),
            line_number: Some(1),
            column_number: Some(1),
            suggested_replacement: None,
            spans: vec![
                DiagnosticSpan {
                    file_path: "crate-a/lib.rs".into(),
                    line_start: 1,
                    line_end: 1,
                    column_start: 1,
                    column_end: 30,
                    is_primary: false,
                    label: Some("updated signature".into()),
                    suggested_replacement: Some("pub fn compute() -> i64 { 10 }\n".into()),
                },
                DiagnosticSpan {
                    file_path: "crate-b/main.rs".into(),
                    line_start: 1,
                    line_end: 1,
                    column_start: 1,
                    column_end: 35,
                    is_primary: true,
                    label: Some("consumer site".into()),
                    suggested_replacement: Some(
                        "fn main() { let _: i64 = crate_a::compute(); }\n".into(),
                    ),
                },
            ],
            notes: Vec::new(),
        }];

        let plan = engine.synthesize_cross_crate_plan(&diags, None);
        assert!(plan.is_some());
        let plan = plan.unwrap();

        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.rollback_steps.len(), 2);

        let paths: Vec<String> = plan
            .steps
            .iter()
            .map(|s| {
                if let capability_layer::CapabilityKind::FileWrite { path, .. } = &s.capability {
                    path.clone()
                } else {
                    String::new()
                }
            })
            .collect();

        assert!(paths.iter().any(|p| p.ends_with("crate-a/lib.rs")));
        assert!(paths.iter().any(|p| p.ends_with("crate-b/main.rs")));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
