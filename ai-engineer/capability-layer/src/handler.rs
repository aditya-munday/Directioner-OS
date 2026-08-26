//! Capability dispatch: validate, execute, audit.

use std::time::Instant;

use serde_json::{Value, json};

use crate::audit::{AuditEntry, AuditLog, AuditOutcome, now_rfc3339};
use crate::killswitch::KillSwitch;
use crate::scope::ScopedPath;
use crate::{
    CapError, CapabilityKind, CapabilityRequest, CapabilityResponse, MAX_READ_BYTES, Result,
};

#[derive(Debug, Clone)]
pub struct HandlerConfig {
    pub approval_required: bool,
    pub protected_units: Vec<String>,
    pub dry_run: bool,
}

impl Default for HandlerConfig {
    fn default() -> Self {
        Self {
            approval_required: true,
            protected_units: vec!["ssh.service".into(), "systemd-udevd.service".into()],
            dry_run: true,
        }
    }
}

pub struct Handler {
    pub scope: ScopedPath,
    pub audit: AuditLog,
    pub killswitch: KillSwitch,
    pub config: HandlerConfig,
}

impl Handler {
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
        }
    }

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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CapError::Internal(e.to_string()))?;
        }
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(!append)
            .append(append)
            .open(&path)
            .map_err(|e| CapError::Internal(e.to_string()))?;
        f.write_all(content.as_bytes())
            .map_err(|e| CapError::Internal(e.to_string()))?;
        f.sync_all()
            .map_err(|e| CapError::Internal(e.to_string()))?;
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
        if self.config.dry_run {
            return Ok(json!({
                "dry_run": true,
                "would_run": format!("dnf5 install -y {package}"),
                "package": package,
            }));
        }
        let dnf = which("dnf5").or_else(|| which("dnf"));
        match dnf {
            Some(bin) => {
                let out = std::process::Command::new(&bin)
                    .arg("install")
                    .arg("-y")
                    .arg(package)
                    .output()
                    .map_err(|e| CapError::Internal(e.to_string()))?;
                if out.status.success() {
                    Ok(json!({ "package": package, "status": "installed" }))
                } else {
                    Err(CapError::Internal(format!(
                        "dnf failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    )))
                }
            }
            None => Err(CapError::Internal(
                "package adapter unavailable: dnf5/dnf not found".into(),
            )),
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
        if self.config.dry_run {
            return Ok(json!({
                "dry_run": true,
                "would_run": format!("dnf5 remove -y {package}"),
                "package": package,
            }));
        }
        let dnf = which("dnf5").or_else(|| which("dnf"));
        match dnf {
            Some(bin) => {
                let out = std::process::Command::new(&bin)
                    .arg("remove")
                    .arg("-y")
                    .arg(package)
                    .output()
                    .map_err(|e| CapError::Internal(e.to_string()))?;
                if out.status.success() {
                    Ok(json!({ "package": package, "status": "removed" }))
                } else {
                    Err(CapError::Internal(format!(
                        "dnf remove failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    )))
                }
            }
            None => Err(CapError::Internal(
                "package adapter unavailable: dnf5/dnf not found".into(),
            )),
        }
    }

    fn cap_service_status(&self, unit: &str) -> std::result::Result<Value, CapError> {
        if let Some(sysctl) = which("systemctl") {
            let out = std::process::Command::new(&sysctl)
                .arg("is-active")
                .arg(unit)
                .output()
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
        if self.config.protected_units.iter().any(|u| u == unit) {
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
        if let Some(sysctl) = which("systemctl") {
            let out = std::process::Command::new(&sysctl)
                .arg("restart")
                .arg(unit)
                .output()
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
        if self.config.protected_units.iter().any(|u| u == unit) {
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
        if let Some(sysctl) = which("systemctl") {
            let out = std::process::Command::new(&sysctl)
                .arg("stop")
                .arg(unit)
                .output()
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
    }
}

fn which(bin: &str) -> Option<std::path::PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
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
        let mut h = handler(dir.path(), HandlerConfig::default());
        let (_, resp) = h
            .execute(&req(
                "p7",
                CapabilityKind::ServiceRestart {
                    unit: "ssh.service".into(),
                    approved: true,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));
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
                assert!(
                    result["would_run"]
                        .as_str()
                        .unwrap()
                        .contains("dnf5 remove -y nginx")
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn service_stop_respects_protected_list_and_approval() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = handler(dir.path(), HandlerConfig::default());

        // Protected unit denied
        let (_, resp) = h
            .execute(&req(
                "srv_stop_prot",
                CapabilityKind::ServiceStop {
                    unit: "ssh.service".into(),
                    approved: true,
                },
            ))
            .unwrap();
        assert!(matches!(resp, CapabilityResponse::Denied { .. }));

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
}
