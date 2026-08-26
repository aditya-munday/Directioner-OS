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
}
