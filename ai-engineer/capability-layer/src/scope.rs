//! Path scope enforcement for file capabilities.

use std::io::{self};
use std::path::{Component, Path, PathBuf};

/// Path sandbox enforcing filesystem isolation within an authorized root directory.
#[derive(Debug, Clone)]
pub struct ScopedPath {
    root: PathBuf,
}

impl ScopedPath {
    /// Creates a new ScopedPath sandbox rooted at the given directory.
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)?;
        let root = std::fs::canonicalize(root)?;
        Ok(Self { root })
    }

    /// Returns the canonicalized sandbox root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves and canonicalizes a path string, strictly ensuring it does not escape the sandbox root.
    pub fn resolve(&self, raw: &str) -> std::io::Result<PathBuf> {
        let raw_p = Path::new(raw);
        let candidate = if raw_p.is_absolute() {
            if raw_p.starts_with(&self.root) {
                raw_p.to_path_buf()
            } else {
                self.root.join(Self::safe_display(raw))
            }
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

    /// Sanitizes path components into a relative display path without root or parent traversals.
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
