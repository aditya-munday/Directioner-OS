//! Code Engineering & Structured Diagnostics Integration (Phase 2).
//!
//! Provides structured runners and analyzers for compiler outputs (`cargo build/test --message-format=json`,
//! `pytest --json-report`), enabling automated error localization, code graph traversal, and remediation.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A parsed compiler diagnostic or linter finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    /// Severity level ("error", "warning", "note", "help").
    pub level: String,
    /// Compiler error code (e.g. "E0308", "E0425"), if present.
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
}

impl CodeEngine {
    /// Create a new CodeEngine for the given workspace root.
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
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

                let primary_span = msg["spans"].as_array().and_then(|spans| {
                    spans
                        .iter()
                        .find(|s| s["is_primary"].as_bool().unwrap_or(false))
                });

                let file_path =
                    primary_span.and_then(|s| s["file_name"].as_str().map(|str| str.to_string()));
                let line_number =
                    primary_span.and_then(|s| s["line_start"].as_u64().map(|n| n as usize));
                let column_number =
                    primary_span.and_then(|s| s["column_start"].as_u64().map(|n| n as usize));
                let suggested_replacement = primary_span
                    .and_then(|s| s["suggested_replacement"].as_str().map(|s| s.to_string()));

                diagnostics.push(Diagnostic {
                    level,
                    code,
                    message,
                    file_path,
                    line_number,
                    column_number,
                    suggested_replacement,
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

                let new_content = if let (Some(prev), Some(rep), Some(line_num)) = (
                    &previous_content,
                    &diag.suggested_replacement,
                    diag.line_number,
                ) {
                    let mut lines: Vec<&str> = prev.lines().collect();
                    if line_num > 0 && line_num <= lines.len() {
                        lines[line_num - 1] = rep;
                        let mut joined = lines.join("\n");
                        joined.push('\n');
                        joined
                    } else {
                        rep.clone()
                    }
                } else if let Some(rep) = &diag.suggested_replacement {
                    rep.clone()
                } else {
                    format!("// TODO: Fix compiler diagnostic in {}\n", file_path)
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
}
