//! Integration tests for Benchmark Suite (Phase 1.6).

use benchmark_suite::BenchmarkRunner;

#[test]
fn test_all_benchmark_scenarios_pass() {
    let runner = BenchmarkRunner::default();
    let scorecard = runner.run_all();

    assert_eq!(scorecard.total_scenarios, 8);
    assert_eq!(scorecard.passed_scenarios, 7);
    assert_eq!(scorecard.known_gap_scenarios, 1);
    assert_eq!(scorecard.pass_rate_pct, 100.0);
    assert_eq!(scorecard.score, 7);
    assert_eq!(scorecard.max_score, 7);

    for res in &scorecard.results {
        if res.is_known_gap {
            assert!(
                !res.passed,
                "Known gap scenario '{}' unexpectedly passed: {}",
                res.name, res.details
            );
        } else {
            assert!(
                res.passed,
                "Scenario '{}' failed: {}",
                res.name, res.details
            );
        }
    }
}
