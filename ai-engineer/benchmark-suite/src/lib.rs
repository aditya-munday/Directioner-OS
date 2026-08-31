//! Directioner-OS AI Engineer Benchmark Suite (Phase 1.6).
//!
//! Provides a standardized, scored test harness comprising 7 real-world system
//! engineering and safety scenarios to measure the reliability, determinism,
//! blast-radius compliance, and rollback efficacy of the AI Engineer subsystem.

pub mod scenarios;

use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Detailed outcome of executing a single benchmark scenario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioResult {
    /// Identifier name of the scenario.
    pub name: String,
    /// Human-readable description of the task.
    pub description: String,
    /// Whether the scenario met all pass criteria.
    pub passed: bool,
    /// Whether this scenario represents a recognized capability gap.
    pub is_known_gap: bool,
    /// Number of points scored for this scenario (0 or 1).
    pub score: u32,
    /// Maximum points available for this scenario.
    pub max_score: u32,
    /// Execution time in milliseconds.
    pub duration_ms: u64,
    /// Detailed diagnostic log or reason for failure.
    pub details: String,
}

/// Overall summary report from running the benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkScorecard {
    /// Total number of scenarios evaluated.
    pub total_scenarios: usize,
    /// Number of implemented scenarios passing.
    pub passed_scenarios: usize,
    /// Number of explicit tracked capability gap scenarios.
    pub known_gap_scenarios: usize,
    /// Total score achieved.
    pub score: u32,
    /// Maximum possible score.
    pub max_score: u32,
    /// Pass rate percentage across implemented scenarios (0.0 to 100.0).
    pub pass_rate_pct: f64,
    /// Total duration of all scenarios in milliseconds.
    pub total_duration_ms: u64,
    /// Results for each individual scenario.
    pub results: Vec<ScenarioResult>,
}

impl BenchmarkScorecard {
    /// Render the scorecard into a formatted terminal table.
    pub fn render_terminal(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();

        writeln!(
            &mut out,
            "╔══════════════════════════════════════════════════════════════════════════════════════════╗"
        )
        .unwrap();
        writeln!(
            &mut out,
            "║                  DIRECTIONER-OS AI ENGINEER BENCHMARK SUITE (PHASE 1.6)                  ║"
        )
        .unwrap();
        writeln!(
            &mut out,
            "╠══════════════════════════════════════════════════════════════════════════════════════════╣"
        )
        .unwrap();
        writeln!(
            &mut out,
            "║  Status: {}/{} Passing ({:.1}%) | {} Tracked Gap(s)  │  Duration: {:>5} ms                ║",
            self.passed_scenarios,
            self.total_scenarios - self.known_gap_scenarios,
            self.pass_rate_pct,
            self.known_gap_scenarios,
            self.total_duration_ms
        )
        .unwrap();
        writeln!(
            &mut out,
            "╠══════════════════════════════════════════════════════════════════════════════════════════╣"
        )
        .unwrap();
        writeln!(
            &mut out,
            "║  #  │ Scenario Name                    │ Status │ Score │ Time (ms) │ Notes              ║"
        )
        .unwrap();
        writeln!(
            &mut out,
            "╟─────┼──────────────────────────────────┼────────┼───────┼───────────┼────────────────────╢"
        )
        .unwrap();

        for (i, res) in self.results.iter().enumerate() {
            let status_str = if res.passed {
                " PASS "
            } else if res.is_known_gap {
                " GAP  "
            } else {
                " FAIL "
            };
            let notes_trimmed = if res.details.len() > 20 {
                format!("{}…", &res.details[..19])
            } else {
                res.details.clone()
            };
            writeln!(
                &mut out,
                "║ {:>2}  │ {:<32} │ {:<6} │ {}/{}   │ {:>9} │ {:<18} ║",
                i + 1,
                res.name,
                status_str,
                res.score,
                res.max_score,
                res.duration_ms,
                notes_trimmed
            )
            .unwrap();
        }

        writeln!(
            &mut out,
            "╚══════════════════════════════════════════════════════════════════════════════════════════╝"
        )
        .unwrap();
        out
    }
}

/// A benchmark scenario to evaluate.
pub trait BenchmarkScenario {
    /// Identifier name.
    fn name(&self) -> &'static str;
    /// Description of the scenario.
    fn description(&self) -> &'static str;
    /// Whether this scenario represents a recognized capability gap.
    fn is_known_gap(&self) -> bool {
        false
    }
    /// Maximum score points for this scenario.
    fn max_score(&self) -> u32 {
        if self.is_known_gap() { 0 } else { 1 }
    }
    /// Execute the scenario and return pass/fail with diagnostic details.
    fn run(&self) -> (bool, String);
}

/// Benchmark suite runner.
pub struct BenchmarkRunner {
    scenarios: Vec<Box<dyn BenchmarkScenario>>,
}

impl Default for BenchmarkRunner {
    fn default() -> Self {
        Self::standard_suite()
    }
}

impl BenchmarkRunner {
    /// Create a runner with the standard benchmark suite (11 core scenarios + 1 tracked gap).
    pub fn standard_suite() -> Self {
        let scenarios: Vec<Box<dyn BenchmarkScenario>> = vec![
            Box::new(scenarios::WebserverScenario),
            Box::new(scenarios::RemediationScenario),
            Box::new(scenarios::DependencyAuditScenario),
            Box::new(scenarios::RollbackScenario),
            Box::new(scenarios::PackageServiceRollbackScenario),
            Box::new(scenarios::BlastRadiusScenario),
            Box::new(scenarios::KillswitchScenario),
            Box::new(scenarios::AuditabilityScenario),
            Box::new(scenarios::MultiCrateRepairScenario),
            Box::new(scenarios::MultiCrateRollbackScenario),
            Box::new(scenarios::AutoApprovePolicyRefusalScenario),
            Box::new(scenarios::CockpitTelemetryScenario),
            Box::new(scenarios::ConversationalIntentParsingScenario),
            Box::new(scenarios::ConversationalMultiTurnRefinementScenario),
            Box::new(scenarios::ConversationalCloudFailoverScenario),
            Box::new(scenarios::ConversationalLocalFallbackScenario),
            Box::new(scenarios::ConversationalAdversarialRejectionScenario),
            Box::new(scenarios::ConversationalStateDriftScenario),
            Box::new(scenarios::ConversationalSafetyBoundaryScenario),
        ];

        Self { scenarios }
    }

    /// Execute all scenarios and compile a consolidated scorecard.
    pub fn run_all(&self) -> BenchmarkScorecard {
        let total_start = Instant::now();
        let mut results = Vec::new();
        let mut passed_count = 0;
        let mut gap_count = 0;
        let mut total_score = 0;
        let mut max_possible_score = 0;

        for scenario in &self.scenarios {
            let start = Instant::now();
            let (passed, details) = scenario.run();
            let duration_ms = start.elapsed().as_millis() as u64;

            let is_gap = scenario.is_known_gap();
            if is_gap {
                gap_count += 1;
            }

            let score = if passed { scenario.max_score() } else { 0 };
            if passed {
                passed_count += 1;
            }
            total_score += score;
            max_possible_score += scenario.max_score();

            results.push(ScenarioResult {
                name: scenario.name().to_string(),
                description: scenario.description().to_string(),
                passed,
                is_known_gap: is_gap,
                score,
                max_score: scenario.max_score(),
                duration_ms,
                details,
            });
        }

        let total_duration_ms = total_start.elapsed().as_millis() as u64;
        let total_scenarios = self.scenarios.len();
        let implemented_scenarios = total_scenarios - gap_count;
        let pass_rate_pct = if implemented_scenarios > 0 {
            (passed_count as f64 / implemented_scenarios as f64) * 100.0
        } else {
            0.0
        };

        BenchmarkScorecard {
            total_scenarios,
            passed_scenarios: passed_count,
            known_gap_scenarios: gap_count,
            score: total_score,
            max_score: max_possible_score,
            pass_rate_pct,
            total_duration_ms,
            results,
        }
    }
}
