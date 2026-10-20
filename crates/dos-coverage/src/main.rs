use dos_catalog::{CatalogLoader, CatalogRegistry};
use dos_coverage::CoverageEvaluator;
use std::env;
use std::process::exit;

fn print_usage() {
    eprintln!("dos-coverage - Directioner OS Capability Coverage Tool (U087, U088)\n");
    eprintln!("Usage: dos-coverage <command> [catalog_dir] [options]\n");
    eprintln!("Commands:");
    eprintln!("  report [dir]                 Generate coverage report across domains");
    eprintln!(
        "  gate [dir] <min_percent>     CI gate failing if coverage falls below threshold (U088)"
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        exit(1);
    }

    let command = &args[1];
    let catalog_dir = if args.len() >= 3 { &args[2] } else { "catalog" };

    let entries = match CatalogLoader::load_dir(catalog_dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("Failed to load catalog from '{catalog_dir}': {err}");
            exit(1);
        }
    };
    let registry = CatalogRegistry::from_entries(entries);

    match command.as_str() {
        "report" => {
            let report = CoverageEvaluator::evaluate(&registry);
            println!("=== Directioner OS Capability Coverage Matrix (U087) ===\n");
            println!(
                "Total Catalog Capabilities: {}",
                report.total_catalog_entries
            );
            println!(
                "Total Introspected D-Bus Services: {}\n",
                report.total_dbus_services
            );
            println!(
                "{:<16} {:<12} {:<14} COVERAGE",
                "AREA", "IN-CATALOG", "UNCOVERED"
            );
            println!("{}", "-".repeat(55));
            for area in &report.areas {
                println!(
                    "{:<16} {:<12} {:<14} {:>5.1}%",
                    area.area, area.in_catalog, area.uncovered_surface, area.coverage_percent
                );
            }
            println!("{}", "-".repeat(55));
            println!(
                "Overall System Coverage: {:.1}%\n",
                report.overall_coverage_percent
            );
        }
        "gate" => {
            let threshold: f64 = if args.len() >= 4 {
                args[3].parse().unwrap_or(20.0)
            } else {
                20.0
            };

            let report = CoverageEvaluator::evaluate(&registry);
            println!(
                "==> Evaluating CI Coverage Gate: {:.1}% vs minimum {:.1}%",
                report.overall_coverage_percent, threshold
            );

            if report.overall_coverage_percent >= threshold {
                println!(
                    "PASS: Coverage gate satisfied ({:.1}% >= {:.1}%)",
                    report.overall_coverage_percent, threshold
                );
            } else {
                eprintln!(
                    "FAIL: Coverage regression! Current {:.1}% is below required {:.1}% (U088)",
                    report.overall_coverage_percent, threshold
                );
                exit(1);
            }
        }
        "help" | "--help" | "-h" => {
            print_usage();
        }
        other => {
            eprintln!("Unknown command: '{other}'");
            print_usage();
            exit(1);
        }
    }
}
