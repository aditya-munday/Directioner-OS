use rustix::fd::OwnedFd;
use rustix::fs::{openat, openat2, Mode, OFlags, ResolveFlags};
use std::fs::File;
use std::io;
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

    pub fn open_scoped(
        &self,
        subpath: &str,
        oflags: OFlags,
        mode: Mode,
    ) -> io::Result<OwnedFd> {
        let clean_relative = Self::safe_display(subpath);
        let root_file = File::open(&self.root)?;

        let resolve = ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS;
        match openat2(&root_file, &clean_relative, oflags, mode, resolve) {
            Ok(fd) => Ok(fd),
            Err(rustix::io::Errno::NOSYS) => {
                let resolved = self.resolve(subpath)?;
                let flags_with_nofollow = oflags | OFlags::NOFOLLOW;
                openat(&root_file, &resolved, flags_with_nofollow, mode)
                    .map_err(io::Error::from)
            }
            Err(e) => Err(io::Error::from(e)),
        }
    }

    pub fn resolve(&self, raw: &str) -> std::io::Result<PathBuf> {
        let raw_p = Path::new(raw);
        for comp in raw_p.components() {
            if comp == Component::ParentDir {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("path escapes work scope via parent component: {raw}"),
                ));
            }
        }

        let candidate = if raw_p.is_absolute() {
            if raw_p.starts_with(&self.root) {
                raw_p.to_path_buf()
            } else {
                self.root.join(Self::safe_display(raw))
            }
        } else {
            self.root.join(raw)
        };

        if let Ok(meta) = candidate.symlink_metadata() {
            if meta.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("symlink resolution prohibited in scope: {raw}"),
                ));
            }
        }

        let abs = match std::fs::canonicalize(&candidate) {
            Ok(p) => p,
            Err(_) => {
                if let Some(parent) = candidate.parent() {
                    let parent_abs = std::fs::canonicalize(parent).map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("parent directory does not exist: {raw}"),
                        )
                    })?;
                    if !parent_abs.starts_with(&self.root) {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            format!("path escapes work scope: {raw}"),
                        ));
                    }
                    candidate
                } else {
                    candidate
                }
            }
        };

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
