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

/// Cache entry for a tracked source file in the incremental symbol graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCacheEntry {
    /// Crate name the file belongs to.
    pub crate_name: String,
    /// Last modified system time in epoch milliseconds.
    pub mtime_ms: u64,
    /// File size in bytes.
    pub file_size: u64,
    /// Symbols extracted from this file.
    pub symbols: Vec<SymbolDefinition>,
}

/// Incremental file-cached symbol graph for workspace intelligence.
///
/// Avoids full workspace re-scanning by caching parsed AST symbols per-file
/// and only re-indexing files whose modification timestamps or sizes have changed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IncrementalSymbolGraph {
    /// Workspace root directory.
    pub workspace_root: PathBuf,
    /// Per-file symbol and metadata cache.
    pub file_cache: HashMap<PathBuf, FileCacheEntry>,
    /// Map of crate name to its root directory.
    pub crates: HashMap<String, PathBuf>,
}

impl IncrementalSymbolGraph {
    /// Create a new incremental symbol graph for the given workspace root.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            file_cache: HashMap::new(),
            crates: HashMap::new(),
        }
    }

    /// Perform an incremental synchronization across the workspace.
    ///
    /// Discovers all `.rs` files and crates in the workspace root, detects
    /// removed files, and re-indexes only files whose modification timestamps
    /// or file sizes have changed since the last synchronization.
    /// Returns the number of files that were re-indexed.
    pub fn sync_incremental(&mut self) -> Result<usize, String> {
        let root = self.workspace_root.clone();
        if !root.exists() {
            return Err(format!("Workspace root does not exist: {}", root.display()));
        }

        // 1. Discover crates and files
        let mut graph_temp = WorkspaceSymbolGraph::new();
        let mut discovered_files = Vec::new();
        graph_temp.collect_source_files(&root, &root, &mut discovered_files)?;
        self.crates = graph_temp.crates;

        // 2. Remove files that no longer exist
        let discovered_set: std::collections::HashSet<PathBuf> =
            discovered_files.iter().map(|(_, p)| p.clone()).collect();
        self.file_cache.retain(|p, _| discovered_set.contains(p));

        // 3. Incrementally update modified/new files
        let mut reindexed_count = 0;
        for (crate_name, file_path) in discovered_files {
            let metadata = fs::metadata(&file_path)
                .map_err(|e| format!("Failed to read metadata for {}: {e}", file_path.display()))?;
            let mtime_ms = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let file_size = metadata.len();

            let needs_reindex = match self.file_cache.get(&file_path) {
                Some(entry) => {
                    entry.mtime_ms != mtime_ms
                        || entry.file_size != file_size
                        || entry.crate_name != crate_name
                }
                None => true,
            };

            if needs_reindex {
                let mut temp_graph = WorkspaceSymbolGraph::new();
                temp_graph.index_file(&crate_name, &file_path)?;
                let symbols = temp_graph.symbols;

                self.file_cache.insert(
                    file_path,
                    FileCacheEntry {
                        crate_name,
                        mtime_ms,
                        file_size,
                        symbols,
                    },
                );
                reindexed_count += 1;
            }
        }

        Ok(reindexed_count)
    }

    /// Explicitly re-index a single file in $O(1)$ time.
    pub fn update_file(&mut self, file_path: &Path) -> Result<bool, String> {
        if !file_path.exists() {
            self.file_cache.remove(file_path);
            return Ok(true);
        }

        let mut temp_graph = WorkspaceSymbolGraph::new();
        temp_graph.crates = self.crates.clone();
        let crate_name = temp_graph.infer_crate_name(file_path, &self.workspace_root);

        let metadata = fs::metadata(file_path)
            .map_err(|e| format!("Failed to read metadata for {}: {e}", file_path.display()))?;
        let mtime_ms = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let file_size = metadata.len();

        temp_graph.index_file(&crate_name, file_path)?;
        let symbols = temp_graph.symbols;

        self.file_cache.insert(
            file_path.to_path_buf(),
            FileCacheEntry {
                crate_name,
                mtime_ms,
                file_size,
                symbols,
            },
        );

        Ok(true)
    }

    /// Remove a file from the incremental cache upon deletion.
    pub fn remove_file(&mut self, file_path: &Path) {
        self.file_cache.remove(file_path);
    }

    /// Construct a consolidated `WorkspaceSymbolGraph` from the cached symbols.
    pub fn build_graph(&self) -> WorkspaceSymbolGraph {
        let mut symbols = Vec::new();
        for entry in self.file_cache.values() {
            symbols.extend(entry.symbols.clone());
        }
        WorkspaceSymbolGraph {
            symbols,
            crates: self.crates.clone(),
        }
    }

    /// Query symbols by name directly from the cache.
    pub fn find_by_name(&self, name: &str) -> Vec<&SymbolDefinition> {
        self.file_cache
            .values()
            .flat_map(|e| &e.symbols)
            .filter(|s| s.name == name)
            .collect()
    }

    /// Query symbols by qualified path directly from the cache.
    pub fn find_by_qualified_path(&self, path: &str) -> Option<&SymbolDefinition> {
        self.file_cache
            .values()
            .flat_map(|e| &e.symbols)
            .find(|s| s.module_path == path || s.module_path.ends_with(path))
    }

    /// Return count of (tracked_files, total_symbols).
    pub fn stats(&self) -> (usize, usize) {
        let total_symbols = self.file_cache.values().map(|e| e.symbols.len()).sum();
        (self.file_cache.len(), total_symbols)
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

    #[test]
    fn test_incremental_symbol_graph_sync_and_file_updates() {
        let temp_dir = std::env::temp_dir().join(format!(
            "inc_symbol_graph_test_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // Create crate Cargo.toml and src/lib.rs
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let src_dir = temp_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let lib_file = src_dir.join("lib.rs");
        fs::write(&lib_file, "pub struct InitialSymbol;\n").unwrap();

        let mut inc_graph = IncrementalSymbolGraph::new(&temp_dir);

        // 1. Initial sync scans and indexes 1 file
        let reindexed = inc_graph.sync_incremental().unwrap();
        assert_eq!(reindexed, 1);
        let (files, syms) = inc_graph.stats();
        assert_eq!(files, 1);
        assert_eq!(syms, 1);
        assert_eq!(inc_graph.find_by_name("InitialSymbol").len(), 1);

        // 2. Second sync on unmodified workspace re-indexes 0 files (instant cache hit)
        let reindexed_2 = inc_graph.sync_incremental().unwrap();
        assert_eq!(reindexed_2, 0);

        // 3. Add a second source file
        let helper_file = src_dir.join("helper.rs");
        fs::write(
            &helper_file,
            "pub fn compute_value() -> u64 { 42 }\npub enum Mode { Fast }\n",
        )
        .unwrap();

        let reindexed_3 = inc_graph.sync_incremental().unwrap();
        assert_eq!(reindexed_3, 1);
        let (files_3, syms_3) = inc_graph.stats();
        assert_eq!(files_3, 2);
        assert_eq!(syms_3, 3);
        assert_eq!(inc_graph.find_by_name("compute_value").len(), 1);
        assert_eq!(inc_graph.find_by_name("Mode").len(), 1);

        // 4. Update single file in-place
        fs::write(&helper_file, "pub fn compute_value() -> u64 { 100 }\n").unwrap();
        inc_graph.update_file(&helper_file).unwrap();
        assert_eq!(inc_graph.find_by_name("Mode").len(), 0);
        assert_eq!(inc_graph.find_by_name("compute_value").len(), 1);

        // 5. Delete file
        fs::remove_file(&helper_file).unwrap();
        let reindexed_4 = inc_graph.sync_incremental().unwrap();
        assert_eq!(reindexed_4, 0);
        let (files_4, syms_4) = inc_graph.stats();
        assert_eq!(files_4, 1);
        assert_eq!(syms_4, 1);
        assert_eq!(inc_graph.find_by_name("compute_value").len(), 0);

        // 6. Test build_graph() export
        let consolidated = inc_graph.build_graph();
        assert_eq!(consolidated.symbols.len(), 1);
        assert_eq!(consolidated.symbols[0].name, "InitialSymbol");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
