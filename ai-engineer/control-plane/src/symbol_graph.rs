//! Workspace Symbol Graph & AST Localization (Phase 2.2).
//!
//! Indexes declarations (structs, enums, traits, functions, methods, type aliases)
//! across all workspace crates, enabling `CodeEngine` to trace cross-crate compiler
//! diagnostics back to their root definitions and coordinate multi-crate repairs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The category of a Rust symbol definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// Struct declaration (`struct Foo`).
    Struct,
    /// Enum declaration (`enum Bar`).
    Enum,
    /// Trait declaration (`trait Baz`).
    Trait,
    /// Top-level or associated function (`fn qux()`).
    Function,
    /// Method declared within an `impl` block.
    Method,
    /// Type alias (`type X = Y`).
    TypeAlias,
    /// Constant or static (`const C: T = ...`).
    Const,
    /// Module declaration (`mod foo`).
    Module,
}

/// Metadata describing a symbol definition in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolDefinition {
    /// Simple identifier of the symbol (e.g. `CapabilityKind`).
    pub name: String,
    /// Name of the containing crate (e.g. `capability-layer`).
    pub crate_name: String,
    /// Relative or qualified module path (e.g. `capability_layer::handler`).
    pub module_path: String,
    /// Classification of the symbol.
    pub kind: SymbolKind,
    /// Absolute or workspace-relative path to the source file.
    pub file_path: PathBuf,
    /// Starting 1-indexed line number.
    pub line_start: usize,
    /// Ending 1-indexed line number.
    pub line_end: usize,
    /// The signature line or snippet.
    pub signature: Option<String>,
    /// Visibility qualifier (e.g. `pub`, `pub(crate)`, `private`).
    pub visibility: String,
}

/// Index of all symbols and declarations across workspace crates.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceSymbolGraph {
    /// All indexed symbol definitions.
    pub symbols: Vec<SymbolDefinition>,
    /// Map of crate name to its relative root path.
    pub crates: HashMap<String, PathBuf>,
}

impl WorkspaceSymbolGraph {
    /// Create an empty symbol graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a symbol graph by scanning a workspace root directory.
    pub fn from_workspace(workspace_root: &Path) -> Result<Self, String> {
        let mut graph = Self::new();
        graph.scan_workspace(workspace_root)?;
        Ok(graph)
    }

    /// Scan workspace directory tree for crates and source files.
    pub fn scan_workspace(&mut self, root: &Path) -> Result<(), String> {
        if !root.exists() {
            return Err(format!("Workspace root does not exist: {}", root.display()));
        }

        let mut source_files = Vec::new();
        self.collect_source_files(root, root, &mut source_files)?;

        for (crate_name, file_path) in source_files {
            self.index_file(&crate_name, &file_path)?;
        }

        Ok(())
    }

    fn collect_source_files(
        &mut self,
        current_dir: &Path,
        root: &Path,
        files: &mut Vec<(String, PathBuf)>,
    ) -> Result<(), String> {
        let entries = fs::read_dir(current_dir)
            .map_err(|e| format!("Failed to read dir {}: {e}", current_dir.display()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if dir_name == "target" || dir_name == ".git" || dir_name == "vm" {
                    continue;
                }

                // Check if this dir contains a Cargo.toml (a crate)
                let cargo_toml = path.join("Cargo.toml");
                if cargo_toml.exists() {
                    let crate_name = dir_name.to_string();
                    self.crates.insert(crate_name.clone(), path.clone());
                }

                self.collect_source_files(&path, root, files)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                // Determine containing crate name from path
                let crate_name = self.infer_crate_name(&path, root);
                files.push((crate_name, path));
            }
        }
        Ok(())
    }

    fn infer_crate_name(&self, file_path: &Path, root: &Path) -> String {
        for (name, crate_path) in &self.crates {
            if file_path.starts_with(crate_path) {
                return name.clone();
            }
        }
        if let Ok(rel) = file_path.strip_prefix(root)
            && let Some(first_comp) = rel.components().next()
        {
            return first_comp.as_os_str().to_string_lossy().to_string();
        }
        "root".to_string()
    }

    /// Index definitions inside a single Rust source file.
    pub fn index_file(&mut self, crate_name: &str, file_path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read {}: {e}", file_path.display()))?;

        let lines: Vec<&str> = content.lines().collect();
        let mut current_impl: Option<String> = None;

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
                continue;
            }

            // Track impl blocks: impl Foo { or impl Trait for Foo {
            if trimmed.starts_with("impl ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Some(for_idx) = parts.iter().position(|&p| p == "for") {
                        if for_idx + 1 < parts.len() {
                            current_impl =
                                Some(parts[for_idx + 1].trim_end_matches('{').trim().to_string());
                        }
                    } else {
                        current_impl = Some(parts[1].trim_end_matches('{').trim().to_string());
                    }
                }
                continue;
            }

            if trimmed == "}" {
                current_impl = None;
            }

            // Check for definitions
            if let Some(sym) = self.parse_symbol_from_line(
                trimmed,
                crate_name,
                file_path,
                line_num,
                current_impl.as_deref(),
            ) {
                self.symbols.push(sym);
            }
        }

        Ok(())
    }

    fn parse_symbol_from_line(
        &self,
        line: &str,
        crate_name: &str,
        file_path: &Path,
        line_num: usize,
        current_impl: Option<&str>,
    ) -> Option<SymbolDefinition> {
        let (vis, rest) = if let Some(stripped) = line.strip_prefix("pub(crate) ") {
            ("pub(crate)", stripped)
        } else if let Some(stripped) = line.strip_prefix("pub ") {
            ("pub", stripped)
        } else {
            ("private", line)
        };

        let rest_trimmed = rest.trim_start();

        // struct
        if let Some(stripped) = rest_trimmed.strip_prefix("struct ") {
            let name = stripped.split([' ', '<', '(', '{', ';']).next()?.trim();
            if !name.is_empty() {
                return Some(SymbolDefinition {
                    name: name.to_string(),
                    crate_name: crate_name.to_string(),
                    module_path: format!("{}::{}", crate_name.replace('-', "_"), name),
                    kind: SymbolKind::Struct,
                    file_path: file_path.to_path_buf(),
                    line_start: line_num,
                    line_end: line_num,
                    signature: Some(line.to_string()),
                    visibility: vis.to_string(),
                });
            }
        }

        // enum
        if let Some(stripped) = rest_trimmed.strip_prefix("enum ") {
            let name = stripped.split([' ', '<', '{']).next()?.trim();
            if !name.is_empty() {
                return Some(SymbolDefinition {
                    name: name.to_string(),
                    crate_name: crate_name.to_string(),
                    module_path: format!("{}::{}", crate_name.replace('-', "_"), name),
                    kind: SymbolKind::Enum,
                    file_path: file_path.to_path_buf(),
                    line_start: line_num,
                    line_end: line_num,
                    signature: Some(line.to_string()),
                    visibility: vis.to_string(),
                });
            }
        }

        // trait
        if let Some(stripped) = rest_trimmed.strip_prefix("trait ") {
            let name = stripped.split([' ', '<', ':', '{']).next()?.trim();
            if !name.is_empty() {
                return Some(SymbolDefinition {
                    name: name.to_string(),
                    crate_name: crate_name.to_string(),
                    module_path: format!("{}::{}", crate_name.replace('-', "_"), name),
                    kind: SymbolKind::Trait,
                    file_path: file_path.to_path_buf(),
                    line_start: line_num,
                    line_end: line_num,
                    signature: Some(line.to_string()),
                    visibility: vis.to_string(),
                });
            }
        }

        // type alias
        if let Some(stripped) = rest_trimmed.strip_prefix("type ") {
            let name = stripped.split([' ', '<', '=']).next()?.trim();
            if !name.is_empty() {
                return Some(SymbolDefinition {
                    name: name.to_string(),
                    crate_name: crate_name.to_string(),
                    module_path: format!("{}::{}", crate_name.replace('-', "_"), name),
                    kind: SymbolKind::TypeAlias,
                    file_path: file_path.to_path_buf(),
                    line_start: line_num,
                    line_end: line_num,
                    signature: Some(line.to_string()),
                    visibility: vis.to_string(),
                });
            }
        }

        // function / method
        if rest_trimmed.starts_with("fn ") || rest_trimmed.starts_with("async fn ") {
            let fn_part = if let Some(stripped) = rest_trimmed.strip_prefix("async fn ") {
                stripped
            } else if let Some(stripped) = rest_trimmed.strip_prefix("fn ") {
                stripped
            } else {
                rest_trimmed
            };
            let name = fn_part.split(['(', '<', ' ']).next()?.trim();
            if !name.is_empty() {
                let (kind, mod_path) = if let Some(parent) = current_impl {
                    (
                        SymbolKind::Method,
                        format!("{}::{}::{}", crate_name.replace('-', "_"), parent, name),
                    )
                } else {
                    (
                        SymbolKind::Function,
                        format!("{}::{}", crate_name.replace('-', "_"), name),
                    )
                };

                return Some(SymbolDefinition {
                    name: name.to_string(),
                    crate_name: crate_name.to_string(),
                    module_path: mod_path,
                    kind,
                    file_path: file_path.to_path_buf(),
                    line_start: line_num,
                    line_end: line_num,
                    signature: Some(line.to_string()),
                    visibility: vis.to_string(),
                });
            }
        }

        None
    }

    /// Find all symbol definitions matching a given simple identifier.
    pub fn find_by_name(&self, name: &str) -> Vec<&SymbolDefinition> {
        self.symbols.iter().filter(|s| s.name == name).collect()
    }

    /// Find a symbol definition by exact qualified path (e.g. `capability_layer::CapabilityKind`).
    pub fn find_by_qualified_path(&self, path: &str) -> Option<&SymbolDefinition> {
        self.symbols
            .iter()
            .find(|s| s.module_path == path || s.module_path.ends_with(path))
    }

    /// Find the symbol definition located at a specific file path and line number.
    pub fn find_in_file(&self, path: &Path, line: usize) -> Option<&SymbolDefinition> {
        self.symbols.iter().find(|s| {
            (s.file_path == path || s.file_path.ends_with(path))
                && s.line_start <= line
                && line <= s.line_end
        })
    }

    /// Find all symbols within a given crate name.
    pub fn find_by_crate(&self, crate_name: &str) -> Vec<&SymbolDefinition> {
        self.symbols
            .iter()
            .filter(|s| s.crate_name == crate_name)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_rust_file_symbols() {
        let temp_dir = std::env::temp_dir().join(format!(
            "symbol_graph_test_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let src_file = temp_dir.join("lib.rs");
        let code = r#"
pub struct UserConfig {
    pub name: String,
}

pub enum SystemStatus {
    Ready,
    Degraded,
}

pub trait EngineRunner {
    fn run(&self) -> bool;
}

pub fn top_level_helper(x: u32) -> u32 {
    x + 1
}

impl UserConfig {
    pub fn validate(&self) -> bool {
        !self.name.is_empty()
    }
}
"#;
        fs::write(&src_file, code).unwrap();

        let mut graph = WorkspaceSymbolGraph::new();
        graph.index_file("test-crate", &src_file).unwrap();

        let structs = graph.find_by_name("UserConfig");
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].kind, SymbolKind::Struct);
        assert_eq!(structs[0].visibility, "pub");

        let enums = graph.find_by_name("SystemStatus");
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].kind, SymbolKind::Enum);

        let traits = graph.find_by_name("EngineRunner");
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].kind, SymbolKind::Trait);

        let fns = graph.find_by_name("top_level_helper");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].kind, SymbolKind::Function);

        let methods = graph.find_by_name("validate");
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].kind, SymbolKind::Method);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
