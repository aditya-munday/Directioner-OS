//! Integration tests for Benchmark Suite (Phase 1.6).

use benchmark_suite::BenchmarkRunner;

#[test]
fn test_all_benchmark_scenarios_pass() {
    let runner = BenchmarkRunner::default();
    let scorecard = runner.run_all();

    for res in &scorecard.results {
        assert!(
            res.passed,
            "Scenario '{}' failed: {}",
            res.name, res.details
        );
    }

    assert_eq!(scorecard.total_scenarios, 32);
    assert_eq!(scorecard.passed_scenarios, 32);
    assert_eq!(scorecard.known_gap_scenarios, 0);
    assert_eq!(scorecard.pass_rate_pct, 100.0);
    assert_eq!(scorecard.score, 32);
    assert_eq!(scorecard.max_score, 32);
}
