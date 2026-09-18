//! Kill-switch (Phase 0.4): one action that halts all AI Engineer activity.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const ENGAGED: &str = "ENGAGED\n";

/// State of the hardware/file killswitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillSwitchState {
    /// Killswitch is active; all mutations are hard-halted.
    Engaged,
    /// Killswitch is inactive; normal operation permitted.
    Disengaged,
}

use parking_lot::Mutex;

static ACTIVE_PGIDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// KillSwitch controller managing lockfile state on disk.
#[derive(Debug, Clone)]
pub struct KillSwitch {
    lock_file: PathBuf,
}

impl KillSwitch {
    /// Registers an active child process group ID.
    pub fn register_process_group(pgid: u32) {
        let mut list = ACTIVE_PGIDS.lock();
        if !list.contains(&pgid) {
            list.push(pgid);
        }
    }

    /// Unregisters an active child process group ID upon completion.
    pub fn unregister_process_group(pgid: u32) {
        let mut list = ACTIVE_PGIDS.lock();
        list.retain(|&id| id != pgid);
    }

    /// Terminates all currently registered child process groups via `killpg(SIGKILL)`.
    pub fn terminate_active_processes() {
        let pgs = {
            let mut list = ACTIVE_PGIDS.lock();
            std::mem::take(&mut *list)
        };
        for pgid in pgs {
            if let Some(pid) = rustix::process::Pid::from_raw(pgid as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
        }
    }

    /// Creates a new KillSwitch instance referencing the target lockfile path.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            lock_file: path.as_ref().to_path_buf(),
        }
    }

    /// Loads a KillSwitch instance from the specified path.
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(path))
    }

    /// Engages the killswitch by atomically writing the lockfile and terminating active subprocesses.
    pub fn engage(&mut self) -> io::Result<()> {
        if let Some(parent) = self.lock_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.lock_file)?;
        f.write_all(ENGAGED.as_bytes())?;
        f.sync_all()?;

        Self::terminate_active_processes();
        Ok(())
    }

    /// Disengages the killswitch by removing the lockfile.
    pub fn release(&mut self) -> io::Result<()> {
        if self.lock_file.exists() {
            std::fs::remove_file(&self.lock_file)?;
        }
        Ok(())
    }

    /// Returns `true` if the lockfile currently exists with `ENGAGED` content.
    pub fn is_engaged(&self) -> bool {
        match std::fs::read_to_string(&self.lock_file) {
            Ok(content) => content.trim() == "ENGAGED",
            Err(_) => false,
        }
    }

    /// Returns the current [`KillSwitchState`].
    pub fn state(&self) -> KillSwitchState {
        if self.is_engaged() {
            KillSwitchState::Engaged
        } else {
            KillSwitchState::Disengaged
        }
    }

    /// Returns the lockfile path.
    pub fn path(&self) -> &Path {
        &self.lock_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engage_persists_and_release_clears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock");
        let mut ks = KillSwitch::new(&path);
        assert!(!ks.is_engaged());
        ks.engage().unwrap();
        assert!(ks.is_engaged());
        assert_eq!(ks.state(), KillSwitchState::Engaged);
        let reloaded = KillSwitch::load(&path).unwrap();
        assert!(reloaded.is_engaged());
        ks.release().unwrap();
        assert!(!ks.is_engaged());
    }

    #[test]
    fn missing_lock_is_disengaged() {
        let dir = tempfile::tempdir().unwrap();
        let ks = KillSwitch::load(dir.path().join("none.lock")).unwrap();
        assert!(!ks.is_engaged());
    }

    #[test]
    fn independent_instance_sees_external_engage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock");
        // Simulates the daemon holding one instance while the `ai-killswitch`
        // CLI (a different process) writes the lock file.
        let daemon = KillSwitch::new(&path);
        assert!(!daemon.is_engaged());

        let mut cli = KillSwitch::new(&path);
        cli.engage().unwrap();

        // The daemon must observe the engage without any reload call —
        // the lock file is the source of truth, read on demand.
        assert!(daemon.is_engaged());
        assert_eq!(daemon.state(), KillSwitchState::Engaged);

        cli.release().unwrap();
        assert!(!daemon.is_engaged());
        assert_eq!(daemon.state(), KillSwitchState::Disengaged);
    }
}
