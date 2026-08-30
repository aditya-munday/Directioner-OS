//! Dry-run and simulation engine (Phase 1.3).
//!
//! Generates unified line diffs for file modifications, command projections
//! for package and service actions, and a transparent preview of all planned
//! side effects before anything is committed to disk.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use capability_layer::CapabilityKind;
use serde::{Deserialize, Serialize};

use crate::plan::{Plan, PlanStep};
use crate::policy::BlastRadiusReport;

/// Simulated preview of a single plan step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepPreview {
    /// Step identifier.
    pub step_id: String,
    /// Step description.
    pub description: String,
    /// Capability kind name (e.g. "file_write", "package_install").
    pub action_type: String,
    /// Textual unified diff (for file writes) or command projection.
    pub projection: String,
    /// Blast radius report for this step.
    pub blast_radius: BlastRadiusReport,
}

/// Complete plan simulation preview presented for user review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanPreview {
    /// Plan identifier.
    pub plan_id: String,
    /// High-level user intent.
    pub intent: String,
    /// Detailed summary of changes.
    pub summary: String,
    /// Aggregate blast radius.
    pub aggregate_blast_radius: BlastRadiusReport,
    /// Previews of each forward step.
    pub step_previews: Vec<StepPreview>,
    /// Number of rollback steps available if execution fails.
    pub rollback_step_count: usize,
}

impl PlanPreview {
    /// Render the plan preview into a formatted, human-readable terminal string.
    pub fn render_terminal(&self) -> String {
        let mut out = String::new();
        writeln!(
            &mut out,
            "══════════════════════════════════════════════════════════════════════"
        )
        .unwrap();
        writeln!(&mut out, "  DIRECTIONER-OS AI ENGINEER — PLAN PREVIEW").unwrap();
        writeln!(&mut out, "  Plan ID: {}", self.plan_id).unwrap();
        writeln!(&mut out, "  Intent:  \"{}\"", self.intent).unwrap();
        writeln!(
            &mut out,
            "  Tier:    {:?} (Blast Score: {}/100)",
            self.aggregate_blast_radius.tier, self.aggregate_blast_radius.score
        )
        .unwrap();
        writeln!(
            &mut out,
            "══════════════════════════════════════════════════════════════════════"
        )
        .unwrap();
        writeln!(&mut out, "\n[Summary]\n{}", self.summary).unwrap();
        writeln!(&mut out, "\n[Risk Factors]").unwrap();
        for factor in &self.aggregate_blast_radius.factors {
            writeln!(&mut out, "  • {factor}").unwrap();
        }

        writeln!(
            &mut out,
            "\n[Planned Actions ({})]",
            self.step_previews.len()
        )
        .unwrap();
        for (i, step) in self.step_previews.iter().enumerate() {
            writeln!(
                &mut out,
                "\n── Step {}: {} [{}] ──",
                i + 1,
                step.description,
                step.action_type
            )
            .unwrap();
            writeln!(
                &mut out,
                "   Risk Score: {}/100 ({:?})",
                step.blast_radius.score, step.blast_radius.tier
            )
            .unwrap();
            writeln!(
                &mut out,
                "   Preview:\n{}",
                indent_lines(&step.projection, "     ")
            )
            .unwrap();
        }

        writeln!(&mut out, "\n[Safety Scaffolding]").unwrap();
        writeln!(
            &mut out,
            "  • Compensating rollback steps registered: {}",
            self.rollback_step_count
        )
        .unwrap();
        writeln!(
            &mut out,
            "  • Audit logging: every capability call will be committed to append-only Work Log"
        )
        .unwrap();
        writeln!(
            &mut out,
            "  • Approval gate: {}",
            if self.aggregate_blast_radius.tier.requires_confirmation() {
                "CONFIRMATION REQUIRED before execution"
            } else {
                "Automatic clearance granted by policy"
            }
        )
        .unwrap();
        writeln!(
            &mut out,
            "══════════════════════════════════════════════════════════════════════"
        )
        .unwrap();
        out
    }
}

/// The simulator generates step and plan previews.
pub struct Simulator;

impl Simulator {
    /// Generate a full preview for a plan.
    pub fn simulate_plan(plan: &Plan, work_root: Option<&Path>) -> PlanPreview {
        let step_previews = plan
            .steps
            .iter()
            .map(|step| Self::simulate_step(step, work_root))
            .collect();

        PlanPreview {
            plan_id: plan.id.clone(),
            intent: plan.intent.clone(),
            summary: plan.summary.clone(),
            aggregate_blast_radius: plan.aggregate_blast_radius.clone(),
            step_previews,
            rollback_step_count: plan.rollback_steps.len(),
        }
    }

    /// Simulate a single step and project its outcome.
    pub fn simulate_step(step: &PlanStep, work_root: Option<&Path>) -> StepPreview {
        let action_type = match &step.capability {
            CapabilityKind::Ping => "ping",
            CapabilityKind::FileRead { .. } => "file_read",
            CapabilityKind::FileWrite { .. } => "file_write",
            CapabilityKind::FileDelete { .. } => "file_delete",
            CapabilityKind::PackageInstall { .. } => "package_install",
            CapabilityKind::PackageRemove { .. } => "package_remove",
            CapabilityKind::ServiceStatus { .. } => "service_status",
            CapabilityKind::ServiceRestart { .. } => "service_restart",
            CapabilityKind::ServiceStop { .. } => "service_stop",
            CapabilityKind::BootSlotPin { .. } => "boot_slot_pin",
            CapabilityKind::KernelParamSet { .. } => "kernel_param_set",
            CapabilityKind::KernelModuleLoad { .. } => "kernel_module_load",
            CapabilityKind::KernelModuleUnload { .. } => "kernel_module_unload",
            CapabilityKind::BootcRollback => "bootc_rollback",
        }
        .to_string();

        let projection = match &step.capability {
            CapabilityKind::Ping => {
                "PROJECTION: ICMP/Daemon ping probe (no side-effects)".to_string()
            }
            CapabilityKind::FileRead { path } => {
                format!("PROJECTION: Read up to 1MB from path '{path}'")
            }
            CapabilityKind::FileWrite {
                path,
                content,
                append,
            } => {
                let actual_path = if let Some(root) = work_root {
                    let rel = path.trim_start_matches('/');
                    root.join(rel)
                } else {
                    Path::new(path).to_path_buf()
                };

                let old_content = fs::read_to_string(&actual_path).unwrap_or_default();
                let is_new = !actual_path.exists();

                if *append {
                    format!(
                        "PROJECTION: Append {} bytes to '{}':\n{}",
                        content.len(),
                        path,
                        format_diff_append(content)
                    )
                } else if is_new {
                    format!(
                        "PROJECTION: Create new file '{}' ({} bytes):\n{}",
                        path,
                        content.len(),
                        format_diff_new(content)
                    )
                } else {
                    format!(
                        "PROJECTION: Overwrite '{}' (unified diff):\n{}",
                        path,
                        generate_unified_diff(path, &old_content, content)
                    )
                }
            }
            CapabilityKind::FileDelete { path } => {
                format!("PROJECTION: Unlink / remove file '{path}'")
            }
            CapabilityKind::PackageInstall { package, approved } => {
                format!(
                    "PROJECTION: rpm-ostree install -y {package}\nExecution: Staged to pending ostree deployment (boot-deferred)\nDone When: Takes effect after next system reboot\nApproval Gate: {}",
                    if *approved {
                        "APPROVED"
                    } else {
                        "PENDING APPROVAL"
                    }
                )
            }
            CapabilityKind::PackageRemove { package, approved } => {
                format!(
                    "PROJECTION: rpm-ostree uninstall -y {package}\nExecution: Staged to pending ostree deployment (boot-deferred)\nDone When: Takes effect after next system reboot\nApproval Gate: {}",
                    if *approved {
                        "APPROVED"
                    } else {
                        "PENDING APPROVAL"
                    }
                )
            }
            CapabilityKind::ServiceStatus { unit } => {
                format!("PROJECTION: systemctl status {unit}")
            }
            CapabilityKind::ServiceRestart { unit, approved } => {
                format!(
                    "PROJECTION: systemctl restart {unit}\nApproval Gate: {}",
                    if *approved {
                        "APPROVED"
                    } else {
                        "PENDING APPROVAL"
                    }
                )
            }
            CapabilityKind::ServiceStop { unit, approved } => {
                format!(
                    "PROJECTION: systemctl stop {unit}\nApproval Gate: {}",
                    if *approved {
                        "APPROVED"
                    } else {
                        "PENDING APPROVAL"
                    }
                )
            }
            CapabilityKind::BootSlotPin { slot, approved } => {
                format!(
                    "PROJECTION: ostree admin pin {slot}\nApproval Gate: {}",
                    if *approved {
                        "APPROVED"
                    } else {
                        "PENDING APPROVAL"
                    }
                )
            }
            CapabilityKind::KernelParamSet {
                key,
                value,
                persistent,
                approved,
            } => {
                format!(
                    "PROJECTION: sysctl -w {key}={value}\nPersistence: {}\nApproval Gate: {}",
                    if *persistent {
                        "Staged to /etc/sysctl.d/99-directioner-tuning.conf (persistent)"
                    } else {
                        "Runtime-only (ephemeral)"
                    },
                    if *approved {
                        "APPROVED"
                    } else {
                        "PENDING APPROVAL"
                    }
                )
            }
            CapabilityKind::KernelModuleLoad {
                module,
                args,
                approved,
            } => {
                format!(
                    "PROJECTION: modprobe {module} {}\nApproval Gate: {}",
                    args.join(" "),
                    if *approved {
                        "APPROVED"
                    } else {
                        "PENDING APPROVAL"
                    }
                )
            }
            CapabilityKind::KernelModuleUnload { module, approved } => {
                format!(
                    "PROJECTION: modprobe -r {module}\nApproval Gate: {}",
                    if *approved {
                        "APPROVED"
                    } else {
                        "PENDING APPROVAL"
                    }
                )
            }
            CapabilityKind::BootcRollback => {
                "PROJECTION: Tier 1 emergency rollback to last known good deployment".to_string()
            }
        };

        StepPreview {
            step_id: step.step_id.clone(),
            description: step.description.clone(),
            action_type,
            projection,
            blast_radius: step.blast_radius.clone(),
        }
    }
}

/// Simple line-by-line diff formatter.
fn generate_unified_diff(path: &str, old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut out = String::new();
    writeln!(&mut out, "--- a/{path}").unwrap();
    writeln!(&mut out, "+++ b/{path}").unwrap();

    // Naive line diff for preview purposes
    let max_len = old_lines.len().max(new_lines.len());
    for i in 0..max_len {
        match (old_lines.get(i), new_lines.get(i)) {
            (Some(o), Some(n)) if o == n => {
                writeln!(&mut out, "  {o}").unwrap();
            }
            (Some(o), Some(n)) => {
                writeln!(&mut out, "- {o}").unwrap();
                writeln!(&mut out, "+ {n}").unwrap();
            }
            (Some(o), None) => {
                writeln!(&mut out, "- {o}").unwrap();
            }
            (None, Some(n)) => {
                writeln!(&mut out, "+ {n}").unwrap();
            }
            (None, None) => {}
        }
    }

    if out.lines().count() <= 2 {
        writeln!(&mut out, "  (no text change)").unwrap();
    }
    out
}

fn format_diff_new(content: &str) -> String {
    let mut out = String::new();
    for line in content.lines() {
        writeln!(&mut out, "+ {line}").unwrap();
    }
    if content.is_empty() {
        writeln!(&mut out, "+ (empty file)").unwrap();
    }
    out
}

fn format_diff_append(content: &str) -> String {
    let mut out = String::new();
    writeln!(&mut out, "--- [existing content preserved] ---").unwrap();
    for line in content.lines() {
        writeln!(&mut out, ">>+ {line}").unwrap();
    }
    out
}

fn indent_lines(text: &str, indent: &str) -> String {
    text.lines()
        .map(|l| format!("{indent}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PolicyEngine;

    #[test]
    fn test_file_write_simulation_new_and_diff() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("app.conf");
        fs::write(&file_path, "port=8080\nworkers=2\n").unwrap();

        let step = PlanStep {
            step_id: "step-1".into(),
            description: "Update worker count".into(),
            capability: CapabilityKind::FileWrite {
                path: file_path.to_string_lossy().to_string(),
                content: "port=8080\nworkers=4\n".into(),
                append: false,
            },
            blast_radius: PolicyEngine::evaluate_capability(
                &CapabilityKind::FileWrite {
                    path: file_path.to_string_lossy().to_string(),
                    content: "port=8080\nworkers=4\n".into(),
                    append: false,
                },
                None,
            ),
            timeout_secs: 30,
        };

        let preview = Simulator::simulate_step(&step, None);
        assert_eq!(preview.action_type, "file_write");
        assert!(preview.projection.contains("- workers=2"));
        assert!(preview.projection.contains("+ workers=4"));
    }

    #[test]
    fn test_plan_preview_render_terminal() {
        let plan = Plan {
            id: "plan-test-123".into(),
            intent: "Setup web server".into(),
            summary: "Installs nginx and configures port".into(),
            steps: vec![PlanStep {
                step_id: "step-1".into(),
                description: "Install nginx".into(),
                capability: CapabilityKind::PackageInstall {
                    package: "nginx".into(),
                    approved: true,
                },
                blast_radius: PolicyEngine::evaluate_capability(
                    &CapabilityKind::PackageInstall {
                        package: "nginx".into(),
                        approved: true,
                    },
                    None,
                ),
                timeout_secs: 30,
            }],
            rollback_steps: vec![],
            aggregate_blast_radius: PolicyEngine::evaluate_plan_aggregate(
                &[CapabilityKind::PackageInstall {
                    package: "nginx".into(),
                    approved: true,
                }],
                None,
            ),
            created_at: "2026-08-26T00:00:00Z".into(),
            status: crate::plan::PlanStatus::Draft,
        };

        let preview = Simulator::simulate_plan(&plan, None);
        let rendered = preview.render_terminal();
        assert!(rendered.contains("DIRECTIONER-OS AI ENGINEER — PLAN PREVIEW"));
        assert!(rendered.contains("rpm-ostree install -y nginx"));
    }
}
