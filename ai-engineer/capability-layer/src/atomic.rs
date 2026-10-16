//! Atomic file operations with crash consistency and symlink defense.
//!
//! Provides [`atomic_write`], which safely commits file content by writing to a temporary file
//! in the same directory, fsyncing the file, atomically renaming it into place, and fsyncing
//! the parent directory.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Fault-injection test hooks for simulating crash consistency and interrupted writes.
pub mod fault {
    use std::cell::Cell;

    thread_local! {
        static INJECT_FAULT_BEFORE_RENAME: Cell<bool> = const { Cell::new(false) };
    }

    /// Configures the fault hook to simulate a crash or interruption immediately before rename.
    pub fn set_fault_before_rename(enabled: bool) {
        INJECT_FAULT_BEFORE_RENAME.with(|f| f.set(enabled));
    }

    /// Returns whether the pre-rename fault is currently configured to fire for the current thread.
    pub fn should_fault_before_rename() -> bool {
        INJECT_FAULT_BEFORE_RENAME.with(|f| f.get())
    }
}

/// Atomically writes `content` to `path` using write-temp + fsync + rename semantics.
///
/// Steps:
/// 1. Reject writes if `path` is an existing symlink (O_NOFOLLOW defense).
/// 2. Ensure the parent directory exists.
/// 3. Create a unique temporary file in the same parent directory using `O_NOFOLLOW | O_EXCL`.
/// 4. Write data to the temporary file.
/// 5. Fsync the temporary file (`sync_all`) to flush data and metadata.
/// 6. (Optional test fault injection hook).
/// 7. Atomically rename the temporary file to `path`.
/// 8. Fsync the parent directory to guarantee the new directory entry is persisted.
/// 9. If any error occurs prior to rename, unlinks the temporary file to prevent leftover garbage.
pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    // 5.4 Keep O_NOFOLLOW symlink defense on all opens
    if let Ok(meta) = path.symlink_metadata() {
        if meta.file_type().is_symlink() {
            return Err(io::Error::from_raw_os_error(rustix::io::Errno::LOOP.raw_os_error()));
        }
    }

    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => {
            std::fs::create_dir_all(p)?;
            p
        }
        _ => Path::new("."),
    };

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    let pid = std::process::id();
    let count = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(".{filename}.tmp.{pid}.{count}");
    let temp_path = parent.join(temp_name);

    let write_res = (|| -> io::Result<()> {
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);

        let mut file = options.open(&temp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);

        if fault::should_fault_before_rename() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Crash-injection fault before rename",
            ));
        }

        // Atomic rename in the same filesystem directory
        std::fs::rename(&temp_path, path)?;

        // 5.3 Fsync the parent directory after rename
        if let Ok(parent_dir) = File::open(parent) {
            let _ = parent_dir.sync_all();
        }

        Ok(())
    })();

    if write_res.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }

    write_res
}
