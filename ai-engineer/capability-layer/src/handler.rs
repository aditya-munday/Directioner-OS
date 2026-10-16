//! Capability dispatch: validate, execute, audit.

use std::time::Instant;

use serde_json::{Value, json};

use crate::audit::{AuditEntry, AuditLog, AuditOutcome, now_rfc3339};
use crate::killswitch::KillSwitch;
use crate::scope::ScopedPath;
use crate::{
    CapError, CapabilityKind, CapabilityRequest, CapabilityResponse, MAX_READ_BYTES, Result,
};

/// Configuration settings for the Capability [`Handler`].
#[derive(Debug, Clone)]
pub struct HandlerConfig {
    /// Whether explicit approval tokens are required for mutating capabilities.
    pub approval_required: bool,
    /// Protected systemd units that cannot be stopped or restarted.
    pub protected_units: Vec<String>,
    /// Whether package mutations operate in simulated dry-run mode.
    pub dry_run: bool,
}

impl Default for HandlerConfig {
    fn default() -> Self {
        Self {
            approval_required: true,
            // Protected units. Matching is normalised so that `ssh`, `ssh.service`, `sshd` and
            // `sshd.service` all match the same protected entry, because Fedora uses the
            // `sshd.service` OpenSSH unit name while Debian/Ubuntu use `ssh.service`.
            protected_units: vec![
                "ssh.service".into(),
                "sshd.service".into(),
                "systemd-udevd.service".into(),
                "dbus.service".into(),
                "firewalld.service".into(),
            ],
            dry_run: true,
        }
    }
}

/// Central dispatcher executing typed capability requests against system APIs.
pub struct Handler {
    /// Filesystem sandbox path resolver.
    pub scope: ScopedPath,
    /// Append-only audit journal.
    pub audit: AuditLog,
    /// Hardware/file killswitch controller.
    pub killswitch: KillSwitch,
    /// Active handler configuration.
    pub config: HandlerConfig,
    /// Optional directory to prioritize when resolving binary paths.
    custom_bin_dir: Option<std::path::PathBuf>,
}

impl Handler {
    /// Initializes a new Handler instance with scope, audit log, killswitch, and config.
    pub fn new(
        scope: ScopedPath,
        audit: AuditLog,
        killswitch: KillSwitch,
        config: HandlerConfig,
    ) -> Self {
        Self {
            scope,
            audit,
            killswitch,
            config,
            custom_bin_dir: None,
        }
    }

    /// Overrides binary resolution directory (useful for testing mock commands).
    pub fn with_custom_bin_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.custom_bin_dir = Some(dir.into());
        self
    }

    /// Returns `true` when `unit` matches the protected-unit list, using normalised
    /// names so that `ssh`, `sshd`, `ssh.service` and `sshd.service` are all treated the
    /// same — Fedora names that unit `sshd.service`, Debian/Ubuntu `ssh.service`.
    pub fn is_protected(&self, unit: &str) -> bool {
        let u = unit.trim();
        let u_base = u.strip_suffix(".service").unwrap_or(u);
        let is_u_ssh = u_base == "ssh" || u_base == "sshd";
        self.config.protected_units.iter().any(|p| {
            let p_trim = p.trim();
            if p_trim == u {
                return true;
            }
            let p_base = p_trim.strip_suffix(".service").unwrap_or(p_trim);
            if p_base == u_base {
                return true;
            }
            let is_p_ssh = p_base == "ssh" || p_base == "sshd";
            is_p_ssh && is_u_ssh
        })
    }

    fn which(&self, bin: &str) -> Option<std::path::PathBuf> {
        if let Some(ref dir) = self.custom_bin_dir {
            let candidate = dir.join(bin);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        which(bin)
    }

    /// Executes an external command within a dedicated process group, registering its PGID
    /// with the killswitch and actively polling for killswitch activation every 10ms.
    /// If the killswitch is engaged, the process group is immediately terminated via `SIGKILL`.
    pub fn run_command(
        &self,
        cmd: &mut std::process::Command,
    ) -> std::io::Result<std::process::Output> {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;
        let pgid = child.id();
        KillSwitch::register_process_group(pgid);

        let mut stdout_handle = child.stdout.take().map(|mut out| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = std::io::Read::read_to_end(&mut out, &mut buf);
                buf
            })
        });
        let mut stderr_handle = child.stderr.take().map(|mut err| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = std::io::Read::read_to_end(&mut err, &mut buf);
                buf
            })
        });

        loop {
            match child.try_wait()? {
                Some(status) => {
                    KillSwitch::unregister_process_group(pgid);
                    let stdout = stdout_handle
                        .take()
                        .and_then(|h| h.join().ok())
                        .unwrap_or_default();
                    let stderr = stderr_handle
                        .take()
                        .and_then(|h| h.join().ok())
                        .unwrap_or_default();
                    return Ok(std::process::Output {
                        status,
                        stdout,
                        stderr,
                    });
                }
                None => {
                    if self.killswitch.is_engaged() {
                        if let Some(pid) = rustix::process::Pid::from_raw(pgid as i32) {
                            let _ = rustix::process::kill_process_group(
                                pid,
                                rustix::process::Signal::KILL,
                            );
                        }
                        KillSwitch::unregister_process_group(pgid);
                        let _ = child.wait();
                        drop(stdout_handle);
                        drop(stderr_handle);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "subprocess terminated by killswitch",
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }

    /// Validates, audits, and executes a capability request, returning the sequence number and response.
    pub fn execute(&mut self, req: &CapabilityRequest) -> Result<(u64, CapabilityResponse)> {
        let started = Instant::now();
        let capability = capability_name(&req.capability);
        let args = redacted_args(&req.capability);

        if self.killswitch.is_engaged() {
            let seq = self.audit_write(
                req,
                capability,
                args,
                AuditOutcome::Denied {
                    reason: "kill-switch engaged".into(),
                },
                started,
            )?;
            return Ok((
                seq,
                CapabilityResponse::Denied {
                    request_id: req.id.clone(),
                    reason: "kill-switch engaged: ai-capabilityd is halted".into(),
                    audit_seq: seq,
                },
            ));
        }

        let r = match &req.capability {
            CapabilityKind::Ping => self.cap_ping(),
            CapabilityKind::FileRead { path } => self.cap_file_read(path),
            CapabilityKind::FileWrite {
                path,
                content,
                append,
            } => self.cap_file_write(path, content, *append),
            CapabilityKind::FileDelete { path } => self.cap_file_delete(path),
            CapabilityKind::PackageInstall { package, approved } => {
                self.cap_package_install(package, *approved)
            }
            CapabilityKind::PackageRemove { package, approved } => {
                self.cap_package_remove(package, *approved)
            }
            CapabilityKind::ServiceStatus { unit } => self.cap_service_status(unit),
            CapabilityKind::ServiceRestart { unit, approved } => {
                self.cap_service_restart(unit, *approved)
            }
            CapabilityKind::ServiceStop { unit, approved } => {
                self.cap_service_stop(unit, *approved)
            }
            CapabilityKind::BootSlotPin { slot, approved } => {
                self.cap_boot_slot_pin(*slot, *approved)
            }
            CapabilityKind::KernelParamSet {
                key,
                value,
                persistent,
                approved,
            } => self.cap_kernel_param_set(key, value, *persistent, *approved),
            CapabilityKind::KernelModuleLoad {
                module,
                args,
                approved,
            } => self.cap_kernel_module_load(module, args, *approved),
            CapabilityKind::KernelModuleUnload { module, approved } => {
                self.cap_kernel_module_unload(module, *approved)
            }
            CapabilityKind::BootcRollback => self.cap_bootc_rollback(),
        };

        let outcome = match &r {
            Ok(v) => AuditOutcome::Success { result: v.clone() },
            Err(CapError::Denied(reason)) => AuditOutcome::Denied {
                reason: reason.clone(),
            },
            Err(CapError::Internal(message)) => AuditOutcome::Error {
                message: message.clone(),
            },
        };

        let seq = self.audit_write(req, capability, args, outcome, started)?;

        let response = match r {
            Ok(result) => CapabilityResponse::Success {
                request_id: req.id.clone(),
                result,
                audit_seq: seq,
            },
            Err(CapError::Denied(reason)) => CapabilityResponse::Denied {
                request_id: req.id.clone(),
                reason,
                audit_seq: seq,
            },
            Err(CapError::Internal(message)) => CapabilityResponse::Error {
                request_id: req.id.clone(),
                message,
                audit_seq: seq,
            },
        };

        Ok((seq, response))
    }

    fn cap_ping(&self) -> std::result::Result<Value, CapError> {
        Ok(json!({ "pong": true, "dry_run": self.config.dry_run }))
    }

    fn cap_file_read(&self, raw: &str) -> std::result::Result<Value, CapError> {
        let path = self
            .scope
            .resolve(raw)
            .map_err(|e| CapError::Denied(e.to_string()))?;
        let meta = std::fs::metadata(&path).map_err(|e| CapError::Internal(e.to_string()))?;
        if !meta.is_file() {
            return Err(CapError::Denied(format!("not a regular file: {raw}")));
        }
        use std::io::Read;
        let mut buf = Vec::new();
        let mut f = std::fs::File::open(&path).map_err(|e| CapError::Internal(e.to_string()))?;
        let taken = (&mut f)
            .take(MAX_READ_BYTES as u64)
            .read_to_end(&mut buf)
            .map_err(|e| CapError::Internal(e.to_string()))?;
        let truncated = taken == MAX_READ_BYTES;
        let content = String::from_utf8_lossy(&buf).into_owned();
        Ok(json!({
            "path": path.display().to_string(),
            "bytes": taken,
            "truncated": truncated,
            "content": content,
        }))
    }

    fn cap_file_write(
        &self,
        raw: &str,
        content: &str,
        append: bool,
    ) -> std::result::Result<Value, CapError> {
        let raw_path = std::path::Path::new(raw);
        if raw_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(CapError::Denied(format!(
                "path contains parent traversal: {raw}"
            )));
        }
        let path = if raw_path.is_absolute() && raw_path.starts_with(self.scope.root()) {
            raw_path.to_path_buf()
        } else {
            let candidate = crate::scope::ScopedPath::safe_display(raw);
            self.scope.root().join(candidate)
        };
        if !path.starts_with(self.scope.root()) {
            return Err(CapError::Denied(format!("path escapes work scope: {raw}")));
        }
        // Verify that no component from the scoped root to the target is a symlink
        let mut check_path = self.scope.root().to_path_buf();
        if let Ok(rel) = path.strip_prefix(self.scope.root()) {
            for comp in rel.components() {
                check_path.push(comp);
                if let Ok(meta) = std::fs::symlink_metadata(&check_path)
                    && meta.file_type().is_symlink()
                {
                    return Err(CapError::Denied(format!(
                        "symlinks not permitted in scope: {raw}"
                    )));
                }
            }
        }
        if !append {
            crate::atomic::atomic_write(&path, content.as_bytes()).map_err(|e| {
                if e.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error()) {
                    CapError::Denied(format!("symlinks not permitted in scope: {raw}"))
                } else {
                    CapError::Internal(e.to_string())
                }
            })?;
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CapError::Internal(e.to_string()))?;
            }
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut options = std::fs::OpenOptions::new();
            options
                .create(true)
                .write(true)
                .truncate(false)
                .append(true)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
            let mut f = options
                .open(&path)
                .map_err(|e| CapError::Internal(e.to_string()))?;
            f.write_all(content.as_bytes())
                .map_err(|e| CapError::Internal(e.to_string()))?;
            f.sync_all()
                .map_err(|e| CapError::Internal(e.to_string()))?;
        }
        Ok(json!({
            "path": path.display().to_string(),
            "bytes": content.len(),
            "append": append,
        }))
    }

    fn cap_file_delete(&self, raw: &str) -> std::result::Result<Value, CapError> {
        let raw_path = std::path::Path::new(raw);
        if raw_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(CapError::Denied(format!(
                "path contains parent traversal: {raw}"
            )));
        }
        let path = if raw_path.is_absolute() && raw_path.starts_with(self.scope.root()) {
            raw_path.to_path_buf()
        } else {
            let candidate = crate::scope::ScopedPath::safe_display(raw);
            self.scope.root().join(candidate)
        };
        if !path.starts_with(self.scope.root()) {
            return Err(CapError::Denied(format!("path escapes work scope: {raw}")));
        }
        if !path.exists() {
            return Ok(json!({
                "path": path.display().to_string(),
                "deleted": false,
                "note": "file did not exist",
            }));
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| CapError::Internal(e.to_string()))?;
        } else {
            std::fs::remove_file(&path).map_err(|e| CapError::Internal(e.to_string()))?;
        }
        Ok(json!({
            "path": path.display().to_string(),
            "deleted": true,
        }))
    }

    fn cap_package_install(
        &self,
        package: &str,
        approved: bool,
    ) -> std::result::Result<Value, CapError> {
        if self.config.approval_required && !approved {
            return Err(CapError::Denied(format!(
                "approval gate not cleared (approved=false) for package.install {package}"
            )));
        }
        let ostree = which("rpm-ostree");
        let dnf = which("dnf5").or_else(|| which("dnf"));

        if self.config.dry_run {
            let cmd = if ostree.is_some() || dnf.is_none() {
                format!("rpm-ostree install -y {package}")
            } else {
                format!("dnf5 install -y {package}")
            };
            let deferred = cmd.starts_with("rpm-ostree");
            return Ok(json!({
                "dry_run": true,
                "would_run": cmd,
                "package": package,
                "deployment_deferred": deferred,
            }));
        }

        // 1. Fast binary check (e.g. /usr/bin/htop, /usr/bin/nvim, /usr/bin/tmux)
        if which(package).is_some() {
            return Ok(json!({
                "package": package,
                "status": "noop_already_installed",
                "adapter": if ostree.is_some() { "rpm-ostree" } else { "rpm" },
                "deployment_deferred": false
            }));
        }

        // 2. Fast rpm database query (rpm -q <package>)
        if let Some(rpm) = self.which("rpm") {
            let is_installed = self
                .run_command(std::process::Command::new(&rpm).arg("-q").arg(package))
                .map(|c| c.status.success())
                .unwrap_or(false);
            if is_installed {
                return Ok(json!({
                    "package": package,
                    "status": "noop_already_installed",
                    "adapter": if ostree.is_some() { "rpm-ostree" } else { "rpm" },
                    "deployment_deferred": false
                }));
            }
        }

        if let Some(bin) = ostree {
            let out = self
                .run_command(
                    std::process::Command::new(&bin)
                        .arg("install")
                        .arg("-y")
                        .arg(package),
                )
                .map_err(|e| CapError::Internal(e.to_string()))?;
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let combined = format!("{stderr} {stdout}").to_lowercase();
            if out.status.success() {
                // Attempt live layering without reboot if supported by system
                let live_applied = self
                    .run_command(std::process::Command::new(&bin).arg("apply-live"))
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                Ok(json!({
                    "package": package,
                    "status": if live_applied { "live_applied" } else { "staged_pending_reboot" },
                    "adapter": "rpm-ostree",
                    "deployment_deferred": !live_applied
                }))
            } else if combined.contains("already in the base")
                || combined.contains("already in base")
                || combined.contains("already provided")
                || combined.contains("already requested")
                || combined.contains("already installed")
                || combined.contains("is already")
                || combined.contains("failed to update rpmdb")
                || combined.contains("writing rpmdb")
            {
                Ok(json!({
                    "package": package,
                    "status": "noop_already_installed",
                    "adapter": "rpm-ostree",
                    "deployment_deferred": false
                }))
            } else {
                Err(CapError::Internal(format!(
                    "rpm-ostree install failed: {stderr}"
                )))
            }
        } else if let Some(bin) = dnf {
            let out = self
                .run_command(
                    std::process::Command::new(&bin)
                        .arg("install")
                        .arg("-y")
                        .arg(package),
                )
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if out.status.success() {
                Ok(json!({
                    "package": package,
                    "status": "installed",
                    "adapter": "dnf"
                }))
            } else {
                Err(CapError::Internal(format!(
                    "dnf failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else {
            Err(CapError::Internal(
                "package adapter unavailable: rpm-ostree / dnf5 not found".into(),
            ))
        }
    }

    fn cap_package_remove(
        &self,
        package: &str,
        approved: bool,
    ) -> std::result::Result<Value, CapError> {
        if self.config.approval_required && !approved {
            return Err(CapError::Denied(format!(
                "approval gate not cleared (approved=false) for package.remove {package}"
            )));
        }

        let ostree = self.which("rpm-ostree");
        let dnf = self.which("dnf5").or_else(|| self.which("dnf"));

        if self.config.dry_run {
            let cmd = if ostree.is_some() || dnf.is_none() {
                format!("rpm-ostree uninstall -y {package}")
            } else {
                format!("dnf5 remove -y {package}")
            };
            let deferred = cmd.starts_with("rpm-ostree");
            return Ok(json!({
                "dry_run": true,
                "would_run": cmd,
                "package": package,
                "deployment_deferred": deferred,
            }));
        }

        // On ostree/bootc, base packages baked into the image cannot be layered-uninstalled
        if ostree.is_some() {
            let is_layered = self
                .which("rpm-ostree")
                .and_then(|bin| {
                    self.run_command(std::process::Command::new(bin).arg("status"))
                        .ok()
                })
                .map(|o| {
                    let s = String::from_utf8_lossy(&o.stdout);
                    s.contains(&format!("LayeredPackages: {package}"))
                        || s.contains(&format!(" {package} "))
                })
                .unwrap_or(false);

            if !is_layered {
                return Ok(json!({
                    "package": package,
                    "status": "noop_already_absent",
                    "adapter": "rpm-ostree",
                    "deployment_deferred": false
                }));
            }
        }

        if let Some(bin) = ostree {
            let out = self
                .run_command(
                    std::process::Command::new(&bin)
                        .arg("uninstall")
                        .arg("-y")
                        .arg(package),
                )
                .map_err(|e| CapError::Internal(e.to_string()))?;
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let combined = format!("{stderr} {stdout}").to_lowercase();
            if out.status.success() {
                // Attempt dynamic live unlayering without reboot
                let live_unlayered = self
                    .run_command(std::process::Command::new(&bin).arg("apply-live"))
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                Ok(json!({
                    "package": package,
                    "status": if live_unlayered { "live_unlayered" } else { "staged_pending_reboot" },
                    "adapter": "rpm-ostree",
                    "deployment_deferred": !live_unlayered
                }))
            } else if combined.contains("is not currently layered")
                || combined.contains("not currently layered")
                || combined.contains("is not currently requested")
                || combined.contains("not currently requested")
                || combined.contains("not currently installed")
                || combined.contains("not installed")
                || combined.contains("already absent")
                || combined.contains("failed to update rpmdb")
                || combined.contains("writing rpmdb")
            {
                Ok(json!({
                    "package": package,
                    "status": "noop_already_absent",
                    "adapter": "rpm-ostree",
                    "deployment_deferred": false
                }))
            } else {
                Err(CapError::Internal(format!(
                    "rpm-ostree uninstall failed: {stderr}"
                )))
            }
        } else if let Some(bin) = dnf {
            let out = self
                .run_command(
                    std::process::Command::new(&bin)
                        .arg("remove")
                        .arg("-y")
                        .arg(package),
                )
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if out.status.success() {
                Ok(json!({
                    "package": package,
                    "status": "removed",
                    "adapter": "dnf"
                }))
            } else {
                Err(CapError::Internal(format!(
                    "dnf remove failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else {
            Err(CapError::Internal(
                "package adapter unavailable: rpm-ostree / dnf5 not found".into(),
            ))
        }
    }

    fn cap_service_status(&self, unit: &str) -> std::result::Result<Value, CapError> {
        if let Some(sysctl) = self.which("systemctl") {
            let out = self
                .run_command(
                    std::process::Command::new(&sysctl)
                        .arg("is-active")
                        .arg(unit),
                )
                .map_err(|e| CapError::Internal(e.to_string()))?;
            let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Ok(json!({ "unit": unit, "state": state }))
        } else {
            Ok(json!({
                "unit": unit,
                "state": "unknown",
                "adapter": "simulated",
                "note": "systemctl not present in this environment",
            }))
        }
    }

    fn cap_service_restart(
        &self,
        unit: &str,
        approved: bool,
    ) -> std::result::Result<Value, CapError> {
        if self.is_protected(unit) {
            return Err(CapError::Denied(format!(
                "unit is on the protected/do-not-touch list: {unit}"
            )));
        }
        if self.config.approval_required && !approved {
            return Err(CapError::Denied(format!(
                "approval gate not cleared (approved=false) for service.restart {unit}"
            )));
        }
        if self.config.dry_run {
            return Ok(json!({
                "unit": unit,
                "dry_run": true,
                "would_run": format!("systemctl restart {unit}"),
            }));
        }
        if let Some(sysctl) = self.which("systemctl") {
            let out = self
                .run_command(std::process::Command::new(&sysctl).arg("restart").arg(unit))
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if out.status.success() {
                Ok(json!({ "unit": unit, "restarted": true }))
            } else {
                Err(CapError::Internal(format!(
                    "systemctl restart failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else {
            Ok(json!({
                "unit": unit,
                "restarted": true,
                "adapter": "simulated",
            }))
        }
    }

    fn cap_service_stop(&self, unit: &str, approved: bool) -> std::result::Result<Value, CapError> {
        if self.is_protected(unit) {
            return Err(CapError::Denied(format!(
                "unit is on the protected/do-not-touch list: {unit}"
            )));
        }
        if self.config.approval_required && !approved {
            return Err(CapError::Denied(format!(
                "approval gate not cleared (approved=false) for service.stop {unit}"
            )));
        }
        if self.config.dry_run {
            return Ok(json!({
                "unit": unit,
                "dry_run": true,
                "would_run": format!("systemctl stop {unit}"),
            }));
        }
        if let Some(sysctl) = self.which("systemctl") {
            let out = self
                .run_command(std::process::Command::new(&sysctl).arg("stop").arg(unit))
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if out.status.success() {
                Ok(json!({ "unit": unit, "stopped": true }))
            } else {
                Err(CapError::Internal(format!(
                    "systemctl stop failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else {
            Ok(json!({
                "unit": unit,
                "stopped": true,
                "adapter": "simulated",
            }))
        }
    }

    fn cap_boot_slot_pin(&self, slot: u32, approved: bool) -> Result<Value> {
        if !approved {
            return Err(CapError::Denied(
                "boot slot pin unapproved by policy".into(),
            ));
        }
        if let Some(ostree) = self.which("ostree") {
            let mut out = self
                .run_command(std::process::Command::new(&ostree).args([
                    "admin",
                    "pin",
                    &slot.to_string(),
                ]))
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("Cannot pin staged deployment")
                    || stderr.contains("staged deployment")
                {
                    // Discard unbooted staged deployment leftover from prior tests so deployment can be pinned
                    if let Some(rpm_ostree) = self.which("rpm-ostree") {
                        let _ = self.run_command(
                            std::process::Command::new(&rpm_ostree).args(["cleanup", "-p"]),
                        );
                        let _ =
                            self.run_command(std::process::Command::new(&rpm_ostree).arg("cancel"));
                    }
                    out = self
                        .run_command(std::process::Command::new(&ostree).args([
                            "admin",
                            "pin",
                            &slot.to_string(),
                        ]))
                        .map_err(|e| CapError::Internal(e.to_string()))?;
                }
            }
            if out.status.success() {
                Ok(json!({ "slot": slot, "pinned": true }))
            } else {
                Err(CapError::Internal(format!(
                    "ostree admin pin failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else {
            Ok(json!({
                "slot": slot,
                "pinned": true,
                "adapter": "simulated",
            }))
        }
    }

    fn cap_bootc_rollback(&self) -> Result<Value> {
        if let Some(bootc) = self.which("bootc") {
            let out = self
                .run_command(std::process::Command::new(&bootc).arg("rollback"))
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if out.status.success() {
                Ok(json!({ "rollback_invoked": true, "mechanism": "bootc" }))
            } else if let Some(ostree) = self.which("ostree") {
                let out2 = self
                    .run_command(std::process::Command::new(&ostree).args(["admin", "switch", "1"]))
                    .map_err(|e| CapError::Internal(e.to_string()))?;
                if out2.status.success() {
                    Ok(json!({ "rollback_invoked": true, "mechanism": "ostree switch" }))
                } else {
                    Err(CapError::Internal(format!(
                        "bootc rollback and ostree switch both failed: {}",
                        String::from_utf8_lossy(&out2.stderr)
                    )))
                }
            } else {
                Err(CapError::Internal(format!(
                    "bootc rollback failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else if let Some(ostree) = self.which("ostree") {
            let out = self
                .run_command(std::process::Command::new(&ostree).args(["admin", "switch", "1"]))
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if out.status.success() {
                Ok(json!({ "rollback_invoked": true, "mechanism": "ostree switch" }))
            } else {
                Err(CapError::Internal(format!(
                    "ostree admin switch failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else {
            Ok(json!({
                "rollback_invoked": true,
                "adapter": "simulated",
            }))
        }
    }

    fn cap_kernel_param_set(
        &self,
        key: &str,
        value: &str,
        persistent: bool,
        approved: bool,
    ) -> Result<Value> {
        if self.config.approval_required && !approved {
            return Err(CapError::Denied(format!(
                "approval gate not cleared (approved=false) for kernel.param_set {key}"
            )));
        }

        // Validate key syntax
        let is_valid_key = !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
            && key.contains('.')
            && !key.starts_with('.')
            && !key.ends_with('.');
        if !is_valid_key {
            return Err(CapError::Denied(format!(
                "invalid kernel parameter key format: '{key}'"
            )));
        }

        // Check prohibited security keys
        if crate::PROHIBITED_KERNEL_PARAMS
            .iter()
            .any(|p| key == *p || key.starts_with(&format!("{p}.")))
        {
            return Err(CapError::Denied(format!(
                "kernel parameter '{key}' is on the prohibited security-critical denylist (cannot be modified)"
            )));
        }

        // Specific defense against core_pattern pipes
        if key == "kernel.core_pattern" && (value.starts_with('|') || value.contains('/')) {
            return Err(CapError::Denied(
                "kernel.core_pattern pipe or binary path redirect is prohibited".into(),
            ));
        }

        if self.config.dry_run {
            return Ok(json!({
                "dry_run": true,
                "key": key,
                "value": value,
                "persistent": persistent,
                "would_run": format!("sysctl -w {key}={value}"),
            }));
        }

        // Apply runtime sysctl
        if let Some(sysctl) = self.which("sysctl") {
            let out = self
                .run_command(
                    std::process::Command::new(&sysctl)
                        .arg("-w")
                        .arg(format!("{key}={value}")),
                )
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if !out.status.success() {
                return Err(CapError::Internal(format!(
                    "sysctl -w {key}={value} failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )));
            }
        } else {
            // Direct /proc/sys write fallback if sysctl binary not in PATH
            let proc_path = std::path::PathBuf::from("/proc/sys").join(key.replace('.', "/"));
            if proc_path.exists() {
                std::fs::write(&proc_path, value).map_err(|e| {
                    CapError::Internal(format!(
                        "failed writing to /proc/sys/{}: {e}",
                        key.replace('.', "/")
                    ))
                })?;
            }
        }

        // Handle persistence
        if persistent {
            let sysctl_dir = std::path::Path::new("/etc/sysctl.d");
            if sysctl_dir.exists() {
                let conf_path = sysctl_dir.join("99-directioner-tuning.conf");
                let line = format!("{key} = {value}\n");
                let existing = std::fs::read_to_string(&conf_path).unwrap_or_default();
                let new_content = format!("{existing}{line}");
                let _ = crate::atomic::atomic_write(&conf_path, new_content.as_bytes());
            }
        }

        Ok(json!({
            "key": key,
            "value": value,
            "applied": true,
            "persistent": persistent
        }))
    }

    fn cap_kernel_module_load(
        &self,
        module: &str,
        args: &[String],
        approved: bool,
    ) -> Result<Value> {
        if self.config.approval_required && !approved {
            return Err(CapError::Denied(format!(
                "approval gate not cleared (approved=false) for module.load {module}"
            )));
        }

        if !crate::ALLOWED_KERNEL_MODULES.contains(&module) {
            return Err(CapError::Denied(format!(
                "module '{module}' is not in the approved kernel module allowlist"
            )));
        }

        if self.config.dry_run {
            return Ok(json!({
                "dry_run": true,
                "module": module,
                "args": args,
                "would_run": format!("modprobe {} {}", module, args.join(" ")),
            }));
        }

        if let Some(modprobe) = self.which("modprobe") {
            let mut cmd = std::process::Command::new(&modprobe);
            cmd.arg(module);
            for a in args {
                cmd.arg(a);
            }
            let out = self
                .run_command(&mut cmd)
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if out.status.success() {
                Ok(json!({ "module": module, "loaded": true }))
            } else {
                Err(CapError::Internal(format!(
                    "modprobe {module} failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else {
            Ok(json!({
                "module": module,
                "loaded": true,
                "adapter": "simulated"
            }))
        }
    }

    fn cap_kernel_module_unload(&self, module: &str, approved: bool) -> Result<Value> {
        if self.config.approval_required && !approved {
            return Err(CapError::Denied(format!(
                "approval gate not cleared (approved=false) for module.unload {module}"
            )));
        }

        if !crate::ALLOWED_KERNEL_MODULES.contains(&module) {
            return Err(CapError::Denied(format!(
                "module '{module}' is not in the approved kernel module allowlist"
            )));
        }

        if self.config.dry_run {
            return Ok(json!({
                "dry_run": true,
                "module": module,
                "action": "unload",
                "would_run": format!("modprobe -r {module}"),
            }));
        }

        // 1. Refcount check via /proc/modules
        if let Ok(proc_modules) = std::fs::read_to_string("/proc/modules") {
            for line in proc_modules.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(name) = parts.first()
                    && *name == module
                {
                    if let Some(refcount_str) = parts.get(2)
                        && let Ok(refcount) = refcount_str.parse::<u32>()
                        && refcount > 0
                    {
                        return Err(CapError::Denied(format!(
                            "module '{module}' is in use (refcount={refcount})"
                        )));
                    }
                    break;
                }
            }
        }

        // 2. Sysfs refcnt check
        let sys_refcnt = format!("/sys/module/{module}/refcnt");
        if let Ok(refcnt_str) = std::fs::read_to_string(&sys_refcnt)
            && let Ok(refcnt) = refcnt_str.trim().parse::<u32>()
            && refcnt > 0
        {
            return Err(CapError::Denied(format!(
                "module '{module}' is in use (refcnt={refcnt})"
            )));
        }

        // 3. Sysfs holders check (dependent modules)
        let sys_holders = format!("/sys/module/{module}/holders");
        if let Ok(entries) = std::fs::read_dir(&sys_holders) {
            let holders: Vec<String> = entries
                .filter_map(|e| e.ok().map(|d| d.file_name().to_string_lossy().into_owned()))
                .collect();
            if !holders.is_empty() {
                return Err(CapError::Denied(format!(
                    "module '{module}' is in use (held by: {})",
                    holders.join(", ")
                )));
            }
        }

        // 4. Active network interface check (e.g. dummy0, veth, etc.)
        if let Ok(net_entries) = std::fs::read_dir("/sys/class/net") {
            for entry in net_entries.flatten() {
                let iface_name = entry.file_name().to_string_lossy().into_owned();
                if iface_name.starts_with(module) {
                    let flags_path = entry.path().join("flags");
                    if let Ok(flags_raw) = std::fs::read_to_string(flags_path) {
                        let flags_str = flags_raw.trim().trim_start_matches("0x");
                        if let Ok(flags_val) = u32::from_str_radix(flags_str, 16)
                            && (flags_val & 0x1) != 0
                        {
                            return Err(CapError::Denied(format!(
                                "module '{module}' is in use (interface {iface_name} is UP)"
                            )));
                        }
                    }
                }
            }
        }

        if let Some(modprobe) = self.which("modprobe") {
            let out = self
                .run_command(std::process::Command::new(&modprobe).arg("-r").arg(module))
                .map_err(|e| CapError::Internal(e.to_string()))?;
            if out.status.success() {
                Ok(json!({ "module": module, "unloaded": true }))
            } else {
                Err(CapError::Internal(format!(
                    "modprobe -r {module} failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                )))
            }
        } else {
            Ok(json!({
                "module": module,
                "unloaded": true,
                "adapter": "simulated"
            }))
        }
    }

    fn audit_write(
        &mut self,
        req: &CapabilityRequest,
        capability: String,
        args: Value,
        outcome: AuditOutcome,
        started: Instant,
    ) -> Result<u64> {
        let entry = AuditEntry {
            seq: 0,
            timestamp: now_rfc3339(),
            request_id: req.id.clone(),
            capability,
            args,
            requester: req.requester.clone(),
            token: req.token.clone(),
            is_rollback: req.is_rollback,
            outcome,
            duration_ms: started.elapsed().as_millis() as u64,
        };
        self.audit
            .append(entry)
            .map_err(|e| CapError::Internal(format!("audit append failed: {e}")))
    }
}

fn capability_name(cap: &CapabilityKind) -> String {
    match cap {
        CapabilityKind::Ping => "ping".into(),
        CapabilityKind::FileRead { .. } => "file.read".into(),
        CapabilityKind::FileWrite { .. } => "file.write".into(),
        CapabilityKind::FileDelete { .. } => "file.delete".into(),
        CapabilityKind::PackageInstall { .. } => "package.install".into(),
        CapabilityKind::PackageRemove { .. } => "package.remove".into(),
        CapabilityKind::ServiceStatus { .. } => "service.status".into(),
        CapabilityKind::ServiceRestart { .. } => "service.restart".into(),
        CapabilityKind::ServiceStop { .. } => "service.stop".into(),
        CapabilityKind::BootSlotPin { .. } => "boot_slot.pin".into(),
        CapabilityKind::KernelParamSet { .. } => "kernel.param_set".into(),
        CapabilityKind::KernelModuleLoad { .. } => "module.load".into(),
        CapabilityKind::KernelModuleUnload { .. } => "module.unload".into(),
        CapabilityKind::BootcRollback => "bootc.rollback".into(),
    }
}

fn redacted_args(cap: &CapabilityKind) -> Value {
    match cap {
        CapabilityKind::Ping => Value::Null,
        CapabilityKind::FileRead { path } => json!({ "path": path }),
        CapabilityKind::FileWrite { path, append, .. } => {
            json!({ "path": path, "append": append, "content_bytes_redacted": true })
        }
        CapabilityKind::FileDelete { path } => json!({ "path": path }),
        CapabilityKind::PackageInstall { package, approved } => {
            json!({ "package": package, "approved": approved })
        }
        CapabilityKind::PackageRemove { package, approved } => {
            json!({ "package": package, "approved": approved })
        }
        CapabilityKind::ServiceStatus { unit } => json!({ "unit": unit }),
        CapabilityKind::ServiceRestart { unit, approved } => {
            json!({ "unit": unit, "approved": approved })
        }
        CapabilityKind::ServiceStop { unit, approved } => {
            json!({ "unit": unit, "approved": approved })
        }
        CapabilityKind::BootSlotPin { slot, approved } => {
            json!({ "slot": slot, "approved": approved })
        }
        CapabilityKind::KernelParamSet {
            key,
            value,
            persistent,
            approved,
        } => json!({
            "key": key,
            "value": value,
            "persistent": persistent,
            "approved": approved
        }),
        CapabilityKind::KernelModuleLoad {
            module,
            args,
            approved,
        } => json!({
            "module": module,
            "args": args,
            "approved": approved
        }),
        CapabilityKind::KernelModuleUnload { module, approved } => {
            json!({ "module": module, "approved": approved })
        }
        CapabilityKind::BootcRollback => Value::Null,
    }
}

fn which(bin: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(bin);
    if p.is_absolute() && p.exists() {
        return Some(p.to_path_buf());
    }

    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(bin);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    let default_dirs = [
        "/usr/bin",
        "/usr/sbin",
        "/bin",
        "/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
    ];
    for dir in default_dirs {
        let candidate = std::path::Path::new(dir).join(bin);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::CapabilityKind;

    fn handler(dir: &std::path::Path, config: HandlerConfig) -> Handler {
        let scope = ScopedPath::new(dir).unwrap();
        let audit = AuditLog::open(dir.join("worklog.jsonl")).unwrap();
        let killswitch = KillSwitch::new(dir.join("kill.lock"));
        Handler::new(scope, audit, killswitch, config)
    }

    fn req(id: &str, cap: CapabilityKind) -> CapabilityRequest {
        CapabilityRequest {
            api_version: crate::API_VERSION,
            id: id.into(),
            requester: "test".into(),
            token: "test-token".into(),
            is_rollback: false,
            capability: cap,
        }
    }

    #[test]
    fn ping_succeeds_and_is_audited() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        let (seq, resp) = h.execute(&req("p1", CapabilityKind::Ping)).unwrap();
        assert_eq!(seq, 0);
        assert!(matches!(resp, CapabilityResponse::Success { .. }));
        let all = h.audit.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].capability, "ping");
        assert_eq!(all[0].request_id, "p1");
    }

    #[test]
    fn package_install_without_approval_is_denied() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        let (_, resp) = h
            .execute(&req(
                "p2",
                CapabilityKind::PackageInstall {
                    package: "nginx".into(),
                    approved: false,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));
        let all = h.audit.read_all().unwrap();
        assert!(matches!(all[0].outcome, AuditOutcome::Denied { .. }));
    }

    #[test]
    fn package_install_approved_dry_run_reports_projection() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        let (_, resp) = h
            .execute(&req(
                "p3",
                CapabilityKind::PackageInstall {
                    package: "nginx".into(),
                    approved: true,
                },
            ))
            .unwrap();
        match resp {
            CapabilityResponse::Success { result, .. } => {
                assert_eq!(result["dry_run"], true);
                assert!(result["would_run"].as_str().unwrap().contains("nginx"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn killswitch_denies_everything() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        h.killswitch.engage().unwrap();
        let (seq, resp) = h.execute(&req("p4", CapabilityKind::Ping)).unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));
        let all = h.audit.read_all().unwrap();
        assert!(
            all.iter()
                .any(|e| matches!(e.outcome, AuditOutcome::Denied { .. }))
        );
        assert_eq!(seq, 0);
    }

    #[test]
    fn file_write_then_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        h.execute(&req(
            "p5",
            CapabilityKind::FileWrite {
                path: "notes/hello.txt".into(),
                content: "hello world".into(),
                append: false,
            },
        ))
        .unwrap();
        let (_, resp) = h
            .execute(&req(
                "p6",
                CapabilityKind::FileRead {
                    path: "notes/hello.txt".into(),
                },
            ))
            .unwrap();
        match resp {
            CapabilityResponse::Success { result, .. } => {
                assert_eq!(result["content"], "hello world");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn service_restart_respects_protected_list() {
        let dir = tempfile::tempdir().unwrap();

        let units = [
            ("ssh.service", "Debian/Ubuntu ssh.service"),
            ("ssh", "bare ssh"),
            ("sshd.service", "Fedora sshd.service"),
            ("sshd", "bare sshd"),
            ("systemd-udevd.service", "systemd-udevd with suffix"),
            ("systemd-udevd", "bare systemd-udevd"),
            ("dbus.service", "dbus with suffix"),
            ("dbus", "bare dbus"),
            ("firewalld.service", "firewalld with suffix"),
            ("firewalld", "bare firewalld"),
        ];

        // Test in both dry-run mode and live (--exec) mode.
        for dry_run in [true, false] {
            let mode = if dry_run { "dry-run" } else { "live (--exec)" };
            let config = HandlerConfig {
                dry_run,
                ..HandlerConfig::default()
            };
            let mut h = handler(dir.path(), config);

            for (name, label) in &units {
                let (_, resp) = h
                    .execute(&req(
                        name,
                        CapabilityKind::ServiceRestart {
                            unit: (*name).into(),
                            approved: true,
                        },
                    ))
                    .unwrap();
                assert!(
                    matches!(resp, CapabilityResponse::Denied { .. }),
                    "{label} in {mode} mode should have been denied as a protected unit"
                );
            }
        }
    }

    #[test]
    fn path_escape_is_denied() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        let (_, resp) = h
            .execute(&req(
                "p8",
                CapabilityKind::FileWrite {
                    path: "../escape.txt".into(),
                    content: "x".into(),
                    append: false,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));
    }

    #[test]
    fn file_delete_removes_file_within_scope() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        h.execute(&req(
            "del1",
            CapabilityKind::FileWrite {
                path: "tmp/file.txt".into(),
                content: "to be deleted".into(),
                append: false,
            },
        ))
        .unwrap();

        assert!(dir.path().join("tmp/file.txt").exists());

        let (_, resp) = h
            .execute(&req(
                "del2",
                CapabilityKind::FileDelete {
                    path: "tmp/file.txt".into(),
                },
            ))
            .unwrap();

        match resp {
            CapabilityResponse::Success { result, .. } => {
                assert_eq!(result["deleted"], true);
            }
            other => panic!("unexpected response: {other:?}"),
        }
        assert!(!dir.path().join("tmp/file.txt").exists());
    }

    #[test]
    fn package_remove_approved_dry_run_reports_projection() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        let (_, resp) = h
            .execute(&req(
                "pkg_rem",
                CapabilityKind::PackageRemove {
                    package: "nginx".into(),
                    approved: true,
                },
            ))
            .unwrap();
        match resp {
            CapabilityResponse::Success { result, .. } => {
                assert_eq!(result["dry_run"], true);
                assert!(result["would_run"].as_str().unwrap().contains("nginx"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn service_stop_respects_protected_list_and_approval() {
        let dir = tempfile::tempdir().unwrap();

        let units = [
            ("ssh.service", "Debian/Ubuntu ssh.service"),
            ("ssh", "bare ssh"),
            ("sshd.service", "Fedora sshd.service"),
            ("sshd", "bare sshd"),
            ("systemd-udevd.service", "systemd-udevd with suffix"),
            ("systemd-udevd", "bare systemd-udevd"),
            ("dbus.service", "dbus with suffix"),
            ("dbus", "bare dbus"),
            ("firewalld.service", "firewalld with suffix"),
            ("firewalld", "bare firewalld"),
        ];

        // Test in both dry-run mode and live (--exec) mode.
        for dry_run in [true, false] {
            let mode = if dry_run { "dry-run" } else { "live (--exec)" };
            let config = HandlerConfig {
                dry_run,
                ..HandlerConfig::default()
            };
            let mut h = handler(dir.path(), config);

            for (name, label) in &units {
                let (_, resp) = h
                    .execute(&req(
                        name,
                        CapabilityKind::ServiceStop {
                            unit: (*name).into(),
                            approved: true,
                        },
                    ))
                    .unwrap();
                assert!(
                    matches!(resp, CapabilityResponse::Denied { .. }),
                    "{label} in {mode} mode should have been denied as a protected unit"
                );
            }
        }

        // Unapproved service stop of an unprotected unit is denied
        let mut h = handler(dir.path(), HandlerConfig::default());

        // Unapproved denied
        let (_, resp) = h
            .execute(&req(
                "srv_stop_unappr",
                CapabilityKind::ServiceStop {
                    unit: "custom.service".into(),
                    approved: false,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));

        // Approved success
        let (_, resp) = h
            .execute(&req(
                "srv_stop_ok",
                CapabilityKind::ServiceStop {
                    unit: "custom.service".into(),
                    approved: true,
                },
            ))
            .unwrap();
        match resp {
            CapabilityResponse::Success { result, .. } => {
                assert_eq!(result["dry_run"], true);
                assert!(
                    result["would_run"]
                        .as_str()
                        .unwrap()
                        .contains("systemctl stop custom.service")
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn boot_slot_pin_respects_approval() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());

        // Unapproved is denied
        let (_, resp) = h
            .execute(&req(
                "pin_unapp",
                CapabilityKind::BootSlotPin {
                    slot: 0,
                    approved: false,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));

        // Approved succeeds
        let (_, resp) = h
            .execute(&req(
                "pin_app",
                CapabilityKind::BootSlotPin {
                    slot: 0,
                    approved: true,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Success { .. }));
    }

    #[test]
    fn bootc_rollback_tier1_emergency_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());
        let (_, resp) = h
            .execute(&req("tier1_rb", CapabilityKind::BootcRollback))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Success { .. }));
    }

    #[test]
    fn kernel_param_set_approval_and_security_denylist() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());

        // Unapproved is denied
        let (_, resp) = h
            .execute(&req(
                "kp1",
                CapabilityKind::KernelParamSet {
                    key: "net.ipv4.ip_forward".into(),
                    value: "1".into(),
                    persistent: false,
                    approved: false,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));

        // Prohibited parameter is denied even if approved=true
        let (_, resp) = h
            .execute(&req(
                "kp2",
                CapabilityKind::KernelParamSet {
                    key: "kernel.core_pattern".into(),
                    value: "|/tmp/exploit".into(),
                    persistent: false,
                    approved: true,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));

        // Valid approved param succeeds in dry_run
        let (_, resp) = h
            .execute(&req(
                "kp3",
                CapabilityKind::KernelParamSet {
                    key: "net.ipv4.ip_forward".into(),
                    value: "1".into(),
                    persistent: false,
                    approved: true,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Success { .. }));
    }

    #[test]
    fn kernel_module_load_and_unload_allowlist_enforcement() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());

        // Unallowed module is denied even if approved=true
        let (_, resp) = h
            .execute(&req(
                "mod1",
                CapabilityKind::KernelModuleLoad {
                    module: "untrusted_rootkit".into(),
                    args: vec![],
                    approved: true,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));

        // Allowed module unapproved is denied
        let (_, resp) = h
            .execute(&req(
                "mod2",
                CapabilityKind::KernelModuleLoad {
                    module: "dummy".into(),
                    args: vec![],
                    approved: false,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));

        // Allowed module approved succeeds in dry_run
        let (_, resp) = h
            .execute(&req(
                "mod3",
                CapabilityKind::KernelModuleLoad {
                    module: "dummy".into(),
                    args: vec!["numdummies=2".into()],
                    approved: true,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Success { .. }));

        // Unload allowed module approved succeeds in dry_run
        let (_, resp) = h
            .execute(&req(
                "mod4",
                CapabilityKind::KernelModuleUnload {
                    module: "dummy".into(),
                    approved: true,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Success { .. }));
    }
}
