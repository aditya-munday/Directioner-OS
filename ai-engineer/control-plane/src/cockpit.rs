//! Directioner-OS System Cockpit & Real-Time Incident Dashboard.
//!
//! Provides operational visibility into systemd unit states, package graphs,
//! capability-layer audit streams, hardware/file killswitch status, and
//! autonomous incident remediation plans.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use capability_layer::killswitch::KillSwitch;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::plan::Plan;

/// System health and systemd unit summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemHealthSummary {
    /// Total number of installed/tracked packages.
    pub total_packages: i64,
    /// Total number of system units.
    pub total_units: i64,
    /// Number of failed or degraded units.
    pub failed_unit_count: usize,
    /// List of failed unit names.
    pub failed_units: Vec<String>,
}

/// Compact entry from capability daemon audit log stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditStreamEntry {
    /// Sequential monotonic audit ID.
    pub seq: u64,
    /// RFC3339 timestamp.
    pub timestamp: String,
    /// Caller identifier (e.g. ai-engineer, cli, subagent).
    pub caller: String,
    /// Capability kind / description.
    pub capability: String,
    /// Outcome status (e.g. success, denied, error).
    pub status: String,
    /// Plan or request correlation ID.
    pub correlation: String,
}

/// Comprehensive point-in-time snapshot of Directioner-OS operational state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CockpitSnapshot {
    /// RFC3339 snapshot capture timestamp.
    pub timestamp: String,
    /// True if the hardware/file killswitch is currently engaged (blocking all mutations).
    pub killswitch_engaged: bool,
    /// System health and unit status metrics.
    pub health: SystemHealthSummary,
    /// Recent capability dispatch audit entries (newest first).
    pub recent_audit: Vec<AuditStreamEntry>,
    /// Currently inspected or active remediation plan.
    pub active_plan: Option<Plan>,
}

/// Polling and telemetry aggregator for the operational cockpit.
pub struct CockpitState {
    db_path: PathBuf,
    audit_log_path: PathBuf,
    lock_file_path: PathBuf,
}

impl CockpitState {
    /// Create a new cockpit telemetry aggregator with specified system paths.
    pub fn new(
        db_path: impl Into<PathBuf>,
        audit_log_path: impl Into<PathBuf>,
        lock_file_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            db_path: db_path.into(),
            audit_log_path: audit_log_path.into(),
            lock_file_path: lock_file_path.into(),
        }
    }

    /// Poll current system status and produce a point-in-time snapshot.
    pub fn poll(&self, active_plan: Option<Plan>) -> CockpitSnapshot {
        let killswitch = KillSwitch::new(&self.lock_file_path);
        let killswitch_engaged = killswitch.is_engaged();

        let health = self.poll_health();
        let recent_audit = self.poll_audit_stream(10);

        CockpitSnapshot {
            timestamp: chrono::Utc::now().to_rfc3339(),
            killswitch_engaged,
            health,
            recent_audit,
            active_plan,
        }
    }

    fn poll_health(&self) -> SystemHealthSummary {
        if !self.db_path.exists() {
            return SystemHealthSummary {
                total_packages: 0,
                total_units: 0,
                failed_unit_count: 0,
                failed_units: Vec::new(),
            };
        }

        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => {
                return SystemHealthSummary {
                    total_packages: 0,
                    total_units: 0,
                    failed_unit_count: 0,
                    failed_units: Vec::new(),
                };
            }
        };

        let total_packages: i64 = conn
            .query_row("SELECT COUNT(*) FROM packages", [], |r| r.get(0))
            .unwrap_or(0);

        let total_units: i64 = conn
            .query_row("SELECT COUNT(*) FROM units", [], |r| r.get(0))
            .unwrap_or(0);

        let mut failed_units = Vec::new();
        if let Ok(mut stmt) = conn
            .prepare("SELECT name FROM units WHERE active_state = 'failed' OR sub_state = 'failed'")
            && let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0))
        {
            for unit in rows.flatten() {
                failed_units.push(unit);
            }
        }

        let failed_unit_count = failed_units.len();

        SystemHealthSummary {
            total_packages,
            total_units,
            failed_unit_count,
            failed_units,
        }
    }

    fn poll_audit_stream(&self, limit: usize) -> Vec<AuditStreamEntry> {
        if !self.audit_log_path.exists() {
            return Vec::new();
        }

        let file = match File::open(&self.audit_log_path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines().map_while(Result::ok) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                let seq = val.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
                let timestamp = val
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let caller = val
                    .get("caller")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let capability = val
                    .get("capability")
                    .and_then(|v| v.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let status = val
                    .get("response")
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let correlation = val
                    .get("correlation_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                entries.push(AuditStreamEntry {
                    seq,
                    timestamp,
                    caller,
                    capability,
                    status,
                    correlation,
                });
            }
        }

        // Return newest entries first
        entries.reverse();
        entries.truncate(limit);
        entries
    }
}

/// Formatter and ANSI terminal renderer for the operational cockpit.
pub struct CockpitRenderer;

impl CockpitRenderer {
    /// Render a rich, styled terminal interface representing the snapshot.
    pub fn render_terminal(snapshot: &CockpitSnapshot) -> String {
        let mut out = String::new();

        out.push_str(
            "╔══════════════════════════════════════════════════════════════════════════════╗\n",
        );
        out.push_str(
            "║                   DIRECTIONER-OS — OPERATIONAL COCKPIT                       ║\n",
        );
        out.push_str(
            "╚══════════════════════════════════════════════════════════════════════════════╝\n",
        );

        // Top Status Bar
        let ks_badge = if snapshot.killswitch_engaged {
            "🔴 [KILLSWITCH ENGAGED - ALL DISPATCH BLOCKED]"
        } else {
            "🟢 [KILLSWITCH DISARMED - SYSTEM ACTIVE]"
        };

        out.push_str(&format!("  Status:    {ks_badge}\n"));
        out.push_str(&format!("  Timestamp: {}\n", snapshot.timestamp));
        out.push_str(
            "────────────────────────────────────────────────────────────────────────────────\n",
        );

        // Health Panel
        out.push_str("  SYSTEM HEALTH & TELEMETRY:\n");
        out.push_str(&format!(
            "    • Packages Tracked: {}\n",
            snapshot.health.total_packages
        ));
        out.push_str(&format!(
            "    • Systemd Units:    {} total\n",
            snapshot.health.total_units
        ));

        if snapshot.health.failed_unit_count == 0 {
            out.push_str("    • Degraded Units:   ✔ 0 (All services healthy)\n");
        } else {
            out.push_str(&format!(
                "    • Degraded Units:   ✘ {} FAILED ({})\n",
                snapshot.health.failed_unit_count,
                snapshot.health.failed_units.join(", ")
            ));
        }

        out.push_str(
            "────────────────────────────────────────────────────────────────────────────────\n",
        );

        // Active Plan Panel (if any)
        if let Some(plan) = &snapshot.active_plan {
            out.push_str("  ACTIVE REMEDIATION PLAN:\n");
            out.push_str(&format!("    • Plan ID:     {}\n", plan.id));
            out.push_str(&format!("    • Intent:      \"{}\"\n", plan.intent));
            out.push_str(&format!(
                "    • Safety Tier: {:?} (Blast Radius Score: {}/100)\n",
                plan.aggregate_blast_radius.tier, plan.aggregate_blast_radius.score
            ));
            out.push_str(&format!(
                "    • Steps:       {} forward, {} rollback\n",
                plan.steps.len(),
                plan.rollback_steps.len()
            ));
            out.push_str("────────────────────────────────────────────────────────────────────────────────\n");
        }

        // Live Audit Stream Panel
        out.push_str("  REAL-TIME CAPABILITY DISPATCH AUDIT STREAM (Newest First):\n");
        if snapshot.recent_audit.is_empty() {
            out.push_str("    (No recent capability dispatches recorded)\n");
        } else {
            for entry in &snapshot.recent_audit {
                let status_icon = match entry.status.as_str() {
                    "Success" | "success" => "✔",
                    "Denied" | "denied" => "✘ [DENIED]",
                    _ => "▲",
                };
                out.push_str(&format!(
                    "    #{:<4} {:<8} [{:<14}] {:<10} ({})\n",
                    entry.seq, status_icon, entry.capability, entry.caller, entry.correlation
                ));
            }
        }

        out.push_str(
            "════════════════════════════════════════════════════════════════════════════════\n",
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cockpit_state_polling() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("worldmodel.db");
        let audit_path = temp.path().join("audit.jsonl");
        let lock_path = temp.path().join("kill.lock");

        // Initialize SQLite DB
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE packages (id INTEGER PRIMARY KEY, name TEXT, epoch INTEGER, version TEXT, release TEXT, arch TEXT, summary TEXT, updated_at TEXT);
            CREATE TABLE units (id INTEGER PRIMARY KEY, name TEXT UNIQUE, load_state TEXT, active_state TEXT, sub_state TEXT, unit_file_state TEXT, description TEXT, updated_at TEXT);
            INSERT INTO packages (name, epoch, version, release, arch, summary, updated_at) VALUES ('nginx', 1, '1.24', '1', 'x86_64', 'Web server', '2026-08-27T00:00:00Z');
            INSERT INTO units (name, load_state, active_state, sub_state, unit_file_state, description, updated_at) VALUES ('nginx.service', 'loaded', 'failed', 'failed', 'enabled', 'Nginx Web Server', '2026-08-27T00:00:00Z');
            "#,
        ).unwrap();

        // Write mock audit log
        let audit_entry = serde_json::json!({
            "seq": 1,
            "timestamp": "2026-08-27T00:00:00Z",
            "caller": "ai-engineer",
            "capability": { "kind": "ServiceRestart" },
            "response": { "type": "Success" },
            "correlation_id": "plan-123:step-1"
        });
        std::fs::write(
            &audit_path,
            format!("{}\n", serde_json::to_string(&audit_entry).unwrap()),
        )
        .unwrap();

        let state = CockpitState::new(&db_path, &audit_path, &lock_path);
        let snapshot = state.poll(None);

        assert!(!snapshot.killswitch_engaged);
        assert_eq!(snapshot.health.total_packages, 1);
        assert_eq!(snapshot.health.total_units, 1);
        assert_eq!(snapshot.health.failed_unit_count, 1);
        assert_eq!(snapshot.health.failed_units, vec!["nginx.service"]);
        assert_eq!(snapshot.recent_audit.len(), 1);
        assert_eq!(snapshot.recent_audit[0].caller, "ai-engineer");

        let rendered = CockpitRenderer::render_terminal(&snapshot);
        assert!(rendered.contains("DIRECTIONER-OS — OPERATIONAL COCKPIT"));
        assert!(rendered.contains("KILLSWITCH DISARMED"));
        assert!(rendered.contains("nginx.service"));
    }
}
