use crate::schema::{CatalogEntry, DomainCatalogFile};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur when loading catalog files from disk
#[derive(Debug, Error)]
pub enum LoaderError {
    /// IO error reading catalog directory or file
    #[error("IO error accessing '{path}': {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// TOML parsing failure
    #[error("Failed to parse TOML in '{path}': {source}")]
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },
    /// Duplicate capability ID detected across catalog
    #[error("Duplicate capability id '{id}' found in '{path}'")]
    DuplicateId { id: String, path: PathBuf },
}

/// Catalog loader for scanning and loading capability domain definition files
pub struct CatalogLoader;

impl CatalogLoader {
    /// Load a single domain TOML file
    pub fn load_file(path: impl AsRef<Path>) -> Result<DomainCatalogFile, LoaderError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|e| LoaderError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let domain_file: DomainCatalogFile =
            toml::from_str(&content).map_err(|e| LoaderError::Toml {
                path: path.to_path_buf(),
                source: e,
            })?;

        Ok(domain_file)
    }

    /// Load all `.toml` domain files in a directory
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Vec<CatalogEntry>, LoaderError> {
        let dir = dir.as_ref();
        let mut entries = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        if !dir.exists() {
            return Ok(entries);
        }

        let read_dir = fs::read_dir(dir).map_err(|e| LoaderError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;

        let mut files: Vec<PathBuf> = read_dir
            .filter_map(|res| res.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
            .collect();

        // Sort files for deterministic loading order
        files.sort();

        for file_path in files {
            let domain_file = Self::load_file(&file_path)?;
            for entry in domain_file.entries {
                if !seen_ids.insert(entry.id.clone()) {
                    return Err(LoaderError::DuplicateId {
                        id: entry.id,
                        path: file_path,
                    });
                }
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}
