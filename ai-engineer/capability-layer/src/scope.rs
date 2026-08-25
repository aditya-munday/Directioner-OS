//! Path scope enforcement for file capabilities.

use std::io::{self};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ScopedPath {
    root: PathBuf,
}

impl ScopedPath {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)?;
        let root = std::fs::canonicalize(root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, raw: &str) -> std::io::Result<PathBuf> {
        let candidate = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            self.root.join(raw)
        };
        let abs = std::fs::canonicalize(&candidate).map_err(|_| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("path does not exist: {raw}"),
            )
        })?;
        if !abs.starts_with(&self.root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("path escapes the work scope: {raw}"),
            ));
        }
        Ok(abs)
    }

    pub fn safe_display(raw: &str) -> PathBuf {
        Path::new(raw)
            .components()
            .map(|c| match c {
                Component::Normal(part) => Path::new(part).to_path_buf(),
                _ => PathBuf::new(),
            })
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    }
}
