use dos_catalog::{CatalogLinter, CatalogLoader, CatalogRegistry};
use std::env;
use std::process::exit;

fn print_usage() {
    eprintln!("Usage: dos-catalog <command> [catalog_dir]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  list [dir]   Load and list all registered catalog entries");
    eprintln!("  lint [dir]   Validate catalog definitions against invariants (U064)");
    eprintln!("  show <id> [dir] Inspect a specific capability by ID");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        exit(1);
    }

    let command = &args[1];
    let default_dir = "catalog";
    let catalog_dir = if args.len() >= 3 {
        &args[2]
    } else {
        default_dir
    };

    match command.as_str() {
        "list" => {
            let entries = match CatalogLoader::load_dir(catalog_dir) {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("FATAL: Failed to load catalog from '{catalog_dir}': {err}");
                    exit(1);
                }
            };
            let registry = CatalogRegistry::from_entries(entries);
            println!(
                "{:<32} {:<10} {:<24} {:<8} {:<8} SUMMARY",
                "ID", "SCOPE", "TIER", "PREVIEW", "UNDO"
            );
            println!("{}", "-".repeat(110));
            for entry in registry.all_entries() {
                println!(
                    "{:<32} {:<10?} {:<24?} {:<8} {:<8} {}",
                    entry.id,
                    entry.scope,
                    entry.tier_hint,
                    if entry.preview { "yes" } else { "no" },
                    if entry.undo { "yes" } else { "no" },
                    entry.summary
                );
            }
            println!("\nTotal capabilities registered: {}", registry.len());
        }
        "lint" => {
            println!("==> Linting Directioner OS Catalog at '{catalog_dir}'...");
            let entries = match CatalogLoader::load_dir(catalog_dir) {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("FATAL: Failed to load catalog files: {err}");
                    exit(1);
                }
            };

            let violations = CatalogLinter::lint_all(&entries);
            if violations.is_empty() {
                println!(
                    "OK: All {} capabilities passed catalog lint.",
                    entries.len()
                );
            } else {
                eprintln!(
                    "FAIL: Found {} catalog lint violation(s):",
                    violations.len()
                );
                for v in &violations {
                    eprintln!("  - {v}");
                }
                exit(1);
            }
        }
        "show" => {
            if args.len() < 3 {
                eprintln!("Error: 'show' requires a capability ID");
                exit(1);
            }
            let target_id = &args[2];
            let dir = if args.len() >= 4 {
                &args[3]
            } else {
                default_dir
            };
            let entries = match CatalogLoader::load_dir(dir) {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("FATAL: {err}");
                    exit(1);
                }
            };
            let registry = CatalogRegistry::from_entries(entries);
            match registry.get(target_id) {
                Some(entry) => {
                    let json = serde_json::to_string_pretty(entry).unwrap();
                    println!("{json}");
                }
                None => {
                    eprintln!("Capability '{target_id}' not found in catalog");
                    exit(1);
                }
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
