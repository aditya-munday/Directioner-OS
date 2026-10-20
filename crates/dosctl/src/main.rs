use directioner_actd::{ActRequest, ActResponse};
use directioner_statd::{StatRequest, StatResponse, StateTree};
use dos_catalog::{CatalogLoader, CatalogRegistry};
use dosctl::{DbusIntrospector, FabricClient};
use std::env;
use std::process::exit;

fn print_usage() {
    eprintln!("dosctl - Directioner OS Control Fabric CLI (U079)\n");
    eprintln!("Usage: dosctl <command> [arguments]\n");
    eprintln!("Commands:");
    eprintln!("  catalog list [dir]             List all capabilities in catalog");
    eprintln!("  catalog show <id> [dir]        Display detailed schema for capability");
    eprintln!("  observe [path]                 Query live state tree from statd daemon");
    eprintln!("  preview <id> [params_json]     Generate dry-run preview plan for action");
    eprintln!("  submit <id> [params_json]      Execute capability in user session");
    eprintln!("  undo <action_id>               Roll back an action using the undo journal");
    eprintln!("  undo-list                      List all active undoable actions");
    eprintln!("  dbus tree                      Introspect D-Bus services and interfaces (U085)");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        exit(1);
    }

    match args[1].as_str() {
        "catalog" => {
            if args.len() < 3 {
                eprintln!("Error: 'catalog' requires a subcommand ('list' or 'show')");
                exit(1);
            }
            let subcmd = &args[2];
            let dir = if args.len() >= 4 && subcmd == "list" {
                &args[3]
            } else if args.len() >= 5 && subcmd == "show" {
                &args[4]
            } else {
                "catalog"
            };

            let entries = match CatalogLoader::load_dir(dir) {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("Failed to load catalog from '{dir}': {err}");
                    exit(1);
                }
            };
            let registry = CatalogRegistry::from_entries(entries);

            if subcmd == "list" {
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
                println!("\nTotal registered: {}", registry.len());
            } else if subcmd == "show" {
                if args.len() < 4 {
                    eprintln!("Error: 'catalog show' requires a capability ID");
                    exit(1);
                }
                let target_id = &args[3];
                match registry.get(target_id) {
                    Some(entry) => {
                        println!("{}", serde_json::to_string_pretty(entry).unwrap());
                    }
                    None => {
                        eprintln!("Capability '{target_id}' not found");
                        exit(1);
                    }
                }
            } else {
                eprintln!("Unknown catalog subcommand: '{subcmd}'");
                exit(1);
            }
        }
        "observe" => {
            let req = if args.len() >= 3 {
                StatRequest::GetNode {
                    path: args[2].clone(),
                }
            } else {
                StatRequest::GetTree
            };

            match FabricClient::query_statd(&req).await {
                Ok(StatResponse::Success { data }) => {
                    if args.len() >= 3 {
                        println!("{}", serde_json::to_string_pretty(&data).unwrap());
                    } else if let Ok(tree) = serde_json::from_value::<StateTree>(data.clone()) {
                        println!("Directioner OS State Tree (v{})", tree.version);
                        println!("Snapshot time: {}\n", tree.timestamp);
                        println!(
                            "{:<32} {:<12} {:<12} VALUE",
                            "NODE PATH", "SENSITIVITY", "SOURCE"
                        );
                        println!("{}", "-".repeat(90));
                        let mut sorted_keys: Vec<&String> = tree.nodes.keys().collect();
                        sorted_keys.sort();
                        for k in sorted_keys {
                            let node = &tree.nodes[k];
                            println!(
                                "{:<32} {:<12?} {:<12} {}",
                                node.path, node.sensitivity, node.source, node.value
                            );
                        }
                    } else {
                        println!("{}", serde_json::to_string_pretty(&data).unwrap());
                    }
                }
                Ok(StatResponse::Error { code, message }) => {
                    eprintln!("statd error [{code}]: {message}");
                    exit(1);
                }
                Err(err) => {
                    eprintln!("Failed to connect to directioner-statd: {err}");
                    eprintln!("(Is directioner-statd.service running?)");
                    exit(1);
                }
            }
        }
        "preview" => {
            if args.len() < 3 {
                eprintln!("Error: 'preview' requires a capability ID");
                exit(1);
            }
            let cap_id = &args[2];
            let params = if args.len() >= 4 {
                serde_json::from_str(&args[3]).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            let req = ActRequest::Preview {
                capability_id: cap_id.clone(),
                params,
            };

            match FabricClient::request_actd(&req).await {
                Ok(ActResponse::Success { data }) => {
                    println!("=== Dry-Run Preview Plan (U074) ===");
                    println!("{}", serde_json::to_string_pretty(&data).unwrap());
                }
                Ok(ActResponse::Error { code, message }) => {
                    eprintln!("actd error [{code}]: {message}");
                    exit(1);
                }
                Err(err) => {
                    eprintln!("Failed to connect to directioner-actd: {err}");
                    exit(1);
                }
            }
        }
        "submit" => {
            if args.len() < 3 {
                eprintln!("Error: 'submit' requires a capability ID");
                exit(1);
            }
            let cap_id = &args[2];
            let params = if args.len() >= 4 {
                serde_json::from_str(&args[3]).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            let req = ActRequest::Execute {
                capability_id: cap_id.clone(),
                params,
            };

            match FabricClient::request_actd(&req).await {
                Ok(ActResponse::Success { data }) => {
                    println!("Action submitted successfully!");
                    println!("{}", serde_json::to_string_pretty(&data).unwrap());
                }
                Ok(ActResponse::Error { code, message }) => {
                    eprintln!("Execution rejected [{code}]: {message}");
                    exit(1);
                }
                Err(err) => {
                    eprintln!("Failed to connect to directioner-actd: {err}");
                    exit(1);
                }
            }
        }
        "undo" => {
            if args.len() < 3 {
                eprintln!("Error: 'undo' requires an action ID");
                exit(1);
            }
            let action_id = args[2].clone();
            let req = ActRequest::Undo { action_id };

            match FabricClient::request_actd(&req).await {
                Ok(ActResponse::Success { data }) => {
                    println!("Undo executed successfully!");
                    println!("{}", serde_json::to_string_pretty(&data).unwrap());
                }
                Ok(ActResponse::Error { code, message }) => {
                    eprintln!("Undo failed [{code}]: {message}");
                    exit(1);
                }
                Err(err) => {
                    eprintln!("Failed to connect to directioner-actd: {err}");
                    exit(1);
                }
            }
        }
        "undo-list" => {
            let req = ActRequest::ListUndo;
            match FabricClient::request_actd(&req).await {
                Ok(ActResponse::Success { data }) => {
                    println!("=== Active Undoable Actions (U075) ===");
                    println!("{}", serde_json::to_string_pretty(&data).unwrap());
                }
                Ok(ActResponse::Error { code, message }) => {
                    eprintln!("Error [{code}]: {message}");
                    exit(1);
                }
                Err(err) => {
                    eprintln!("Failed to connect to directioner-actd: {err}");
                    exit(1);
                }
            }
        }
        "dbus" => {
            if args.len() >= 3 && args[2] == "tree" {
                println!("{}", DbusIntrospector::render_tree());
            } else {
                eprintln!("Usage: dosctl dbus tree");
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
