//! Directioner-OS AI Engineer Benchmark CLI (Phase 1.6).
//!
//! Executes the standard benchmark suite of 7 scored system engineering scenarios
//! and outputs human-readable scorecards or JSON metrics.

use std::env;
use std::process::exit;

use benchmark_suite::BenchmarkRunner;

fn main() {
    let args: Vec<String> = env::args().collect();
    let json_output = args.iter().any(|a| a == "--json");

    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            r#"Directioner-OS AI Engineer Benchmark Suite (Phase 1.6)

USAGE:
    ai-benchmark [OPTIONS]

OPTIONS:
    --json       Output detailed results in machine-readable JSON format
    -h, --help   Print this help text
"#
        );
        exit(0);
    }

    let runner = BenchmarkRunner::default();
    let scorecard = runner.run_all();

    if json_output {
        println!("{}", serde_json::to_string_pretty(&scorecard).unwrap());
    } else {
        println!("{}", scorecard.render_terminal());
    }

    let unexpected_failures =
        scorecard.total_scenarios - scorecard.passed_scenarios - scorecard.known_gap_scenarios;
    if unexpected_failures > 0 {
        exit(1);
    }
}
