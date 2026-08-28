//! Blast-radius scoring & approval gate policy engine (Phase 1.5).
//!
//! Evaluates the risk profile of individual capabilities and entire multi-step
//! plans. Every action is assigned a quantitative blast-radius score (0–100)
//! and classified into one of four approval tiers:
//!
//! - [`ApprovalTier::Auto`]: Zero/negligible side-effect risk (reads, status, pings).
//! - [`ApprovalTier::Notify`]: Low risk, easily revertible actions in user-scoped space.
//! - [`ApprovalTier::Confirm`]: Medium risk (package installs, system config writes, service restarts).
//! - [`ApprovalTier::ExpertReview`]: High/destructive risk (core packages like `glibc`,
//!   protected system units like `ssh`/`systemd-udevd`, critical files like `/etc/sudoers`).

use capability_layer::CapabilityKind;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// The approval gate tier determined by the blast-radius score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalTier {
    /// Action has no side-effects or negligible risk; executes automatically without prompting.
    Auto,
    /// Low-risk action; recorded in the Work Log and user is notified, but no prompt required.
    Notify,
    /// Moderate-risk action; requires explicit user approval (`--approve` or interactive `y/N`).
    Confirm,
    /// Critical or destructive action; requires explicit expert acknowledgment and double confirmation.
    ExpertReview,
}

impl ApprovalTier {
    /// Convert a 0–100 blast radius score to its corresponding approval tier.
    pub fn from_score(score: u32) -> Self {
        match score {
            0..=19 => ApprovalTier::Auto,
            20..=39 => ApprovalTier::Notify,
            40..=79 => ApprovalTier::Confirm,
            _ => ApprovalTier::ExpertReview,
        }
    }

    /// Whether this tier requires interactive human approval before execution.
    pub fn requires_confirmation(&self) -> bool {
        matches!(self, ApprovalTier::Confirm | ApprovalTier::ExpertReview)
    }
}

/// Detailed evaluation breakdown of an action's blast radius.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlastRadiusReport {
    /// Overall composite score (0 to 100).
    pub score: u32,
    /// Approval tier assigned to this action.
    pub tier: ApprovalTier,
    /// Specific risk factors contributing to the score.
    pub factors: Vec<String>,
}

/// Core package names whose modification has wide system blast radius.
pub const CRITICAL_SYSTEM_PACKAGES: &[&str] = &[
    "glibc",
    "systemd",
    "kernel",
    "kernel-core",
    "openssl",
    "openssl-libs",
    "dnf5",
    "rpm",
    "selinux-policy",
    "polkit",
    "dbus-broker",
    "dbus",
];

/// Critical file path prefixes / patterns whose modification has wide system blast radius.
pub const CRITICAL_SYSTEM_PATHS: &[&str] = &[
    "/etc/sudoers",
    "/etc/sudoers.d",
    "/etc/shadow",
    "/etc/passwd",
    "/etc/fstab",
    "/etc/ssh",
    "/etc/pam.d",
    "/etc/systemd/system.conf",
    "/boot",
];

/// Evaluator that computes blast radius using policy rules and optional World Model queries.
pub struct PolicyEngine;

impl PolicyEngine {
    /// Evaluate the blast radius of a single capability action.
    pub fn evaluate_capability(cap: &CapabilityKind, db: Option<&Connection>) -> BlastRadiusReport {
        let mut score: u32 = 0;
        let mut factors = Vec::new();

        match cap {
            CapabilityKind::Ping => {
                score = 0;
                factors.push("Ping is a zero-risk liveness probe".into());
            }
            CapabilityKind::ServiceStatus { unit } => {
                score = 5;
                factors.push(format!(
                    "Reading status of unit '{unit}' has no side effects"
                ));
            }
            CapabilityKind::FileRead { path } => {
                score = 5;
                factors.push(format!("Reading file '{path}' has no side effects"));
            }
            CapabilityKind::FileWrite {
                path,
                content,
                append,
            } => {
                let is_critical_path = CRITICAL_SYSTEM_PATHS
                    .iter()
                    .any(|crit| path.starts_with(crit));

                if is_critical_path {
                    score += 85;
                    factors.push(format!(
                        "Writing to critical system path '{path}' affects system security/boot"
                    ));
                } else if path.starts_with("/etc") {
                    score += 50;
                    factors.push(format!("Writing configuration inside '/etc': '{path}'"));
                } else if path.starts_with("/var") || path.starts_with("/usr") {
                    score += 45;
                    factors.push(format!("Writing system path '{path}'"));
                } else {
                    // User / tmp space
                    score += 25;
                    factors.push(format!("Writing file in workspace/user scope: '{path}'"));
                }

                if *append {
                    factors.push("Append mode preserves existing file contents".into());
                } else if content.is_empty() {
                    score += 15;
                    factors.push("Truncating file to zero length (deletion-equivalent)".into());
                } else {
                    score += 5;
                    factors.push(format!(
                        "Overwriting file (content length {} bytes)",
                        content.len()
                    ));
                }
            }
            CapabilityKind::FileDelete { path } => {
                let is_critical_path = CRITICAL_SYSTEM_PATHS
                    .iter()
                    .any(|crit| path.starts_with(crit));

                if is_critical_path {
                    score += 95;
                    factors.push(format!(
                        "Deleting critical system file '{path}' may render system unbootable or insecure"
                    ));
                } else if path.starts_with("/etc") {
                    score += 70;
                    factors.push(format!(
                        "Deleting configuration file inside '/etc': '{path}'"
                    ));
                } else if path.starts_with("/var") || path.starts_with("/usr") {
                    score += 65;
                    factors.push(format!("Deleting system file/directory '{path}'"));
                } else {
                    // User / workspace space
                    score += 35;
                    factors.push(format!("Deleting file in workspace/user scope: '{path}'"));
                }
            }
            CapabilityKind::PackageInstall { package, .. } => {
                let pkg_lower = package.to_lowercase();
                let is_critical = CRITICAL_SYSTEM_PACKAGES
                    .iter()
                    .any(|crit| pkg_lower == *crit || pkg_lower.starts_with(&format!("{crit}-")));

                if is_critical {
                    score += 85;
                    factors.push(format!(
                        "Package '{package}' is a core system component; modification carries high system blast radius"
                    ));
                } else {
                    score += 45;
                    factors.push(format!("Installing/updating package '{package}'"));
                }

                // Compensating rollback capability note
                factors.push("Compensating package.remove rollback capability is available".into());

                // Check world-model reverse dependency impact (fail-closed if unavailable)
                match db {
                    Some(conn) => match world_model::query::reverse_deps(conn, package) {
                        Ok(rev_deps) => {
                            let count = rev_deps.len();
                            if count > 20 {
                                score += 40;
                                factors.push(format!(
                                    "Package has {count} reverse dependencies in World Model (critical dependency blast radius)"
                                ));
                            } else if count > 5 {
                                score += 20;
                                factors.push(format!(
                                    "Package has {count} reverse dependencies in World Model (moderate dependency blast radius)"
                                ));
                            } else if count > 0 {
                                score += (count as u32).min(10);
                                factors.push(format!(
                                    "Package has {count} reverse dependencies in World Model"
                                ));
                            }
                        }
                        Err(e) => {
                            // Fail closed: query failed -> escalate to at least Confirm
                            score = score.max(50);
                            factors.push(format!(
                                "World Model dependency query failed ({e}); failing closed with unknown-risk penalty"
                            ));
                        }
                    },
                    None => {
                        // Fail closed: database connection offline -> escalate to at least Confirm
                        score = score.max(50);
                        factors.push(
                            "World Model database offline/unavailable; failing closed with unknown dependency risk (minimum Confirm tier)".into(),
                        );
                    }
                }
            }
            CapabilityKind::PackageRemove { package, .. } => {
                let pkg_lower = package.to_lowercase();
                let is_critical = CRITICAL_SYSTEM_PACKAGES
                    .iter()
                    .any(|crit| pkg_lower == *crit || pkg_lower.starts_with(&format!("{crit}-")));

                if is_critical {
                    score += 95;
                    factors.push(format!(
                        "Package '{package}' is a core system component; removal carries destructive system blast radius"
                    ));
                } else {
                    score += 55;
                    factors.push(format!("Removing package '{package}'"));
                }

                // Check world-model reverse dependencies that would break if removed
                match db {
                    Some(conn) => match world_model::query::reverse_deps(conn, package) {
                        Ok(rev_deps) => {
                            let count = rev_deps.len();
                            if count > 0 {
                                score += (count as u32 * 5).min(40);
                                factors.push(format!(
                                    "Removing package breaks {count} dependent packages in World Model"
                                ));
                            }
                        }
                        Err(e) => {
                            score = score.max(60);
                            factors.push(format!(
                                "World Model reverse dependency query failed ({e}); failing closed"
                            ));
                        }
                    },
                    None => {
                        score = score.max(60);
                        factors.push(
                            "World Model database offline; failing closed on package removal risk"
                                .into(),
                        );
                    }
                }
            }
            CapabilityKind::ServiceRestart { unit, .. } => {
                let protected = [
                    "ssh.service",
                    "systemd-udevd.service",
                    "dbus.service",
                    "firewalld.service",
                ];
                if protected.iter().any(|p| unit == p) {
                    score += 90;
                    factors.push(format!(
                        "Unit '{unit}' is a protected/critical system daemon; restart may disconnect or destabilize machine"
                    ));
                } else {
                    score += 50;
                    factors.push(format!("Restarting systemd service unit '{unit}'"));
                }

                // Check unit state from world-model if available
                match db {
                    Some(conn) => {
                        if let Ok(Some(u_info)) = world_model::query::unit_info(conn, unit) {
                            factors.push(format!(
                                "Current unit state in World Model: active='{}', substate='{}'",
                                u_info.active_state, u_info.sub_state
                            ));
                        }
                    }
                    None => {
                        factors.push(
                            "World Model offline; unit runtime state unverified prior to restart"
                                .into(),
                        );
                    }
                }
            }
            CapabilityKind::ServiceStop { unit, .. } => {
                let protected = [
                    "ssh.service",
                    "systemd-udevd.service",
                    "dbus.service",
                    "firewalld.service",
                ];
                if protected.iter().any(|p| unit == p) {
                    score += 95;
                    factors.push(format!(
                        "Unit '{unit}' is a protected/critical system daemon; stopping it will destabilize the machine"
                    ));
                } else {
                    score += 50;
                    factors.push(format!("Stopping systemd service unit '{unit}'"));
                }
            }
        }

        // Clamp composite score to 0..=100
        let final_score = score.min(100);
        let tier = ApprovalTier::from_score(final_score);

        BlastRadiusReport {
            score: final_score,
            tier,
            factors,
        }
    }

    /// Aggregate the blast radius score across a collection of capability actions.
    pub fn evaluate_plan_aggregate(
        caps: &[CapabilityKind],
        db: Option<&Connection>,
    ) -> BlastRadiusReport {
        if caps.is_empty() {
            return BlastRadiusReport {
                score: 0,
                tier: ApprovalTier::Auto,
                factors: vec!["Empty plan has zero blast radius".into()],
            };
        }

        let mut max_score = 0;
        let mut all_factors = Vec::new();

        for (idx, cap) in caps.iter().enumerate() {
            let report = Self::evaluate_capability(cap, db);
            if report.score > max_score {
                max_score = report.score;
            }
            for factor in report.factors {
                all_factors.push(format!("Step {}: {}", idx + 1, factor));
            }
        }

        // If plan has multiple destructive/modifying steps, apply a modest multiplicity penalty
        let modifying_steps = caps
            .iter()
            .filter(|c| {
                !matches!(
                    c,
                    CapabilityKind::Ping
                        | CapabilityKind::FileRead { .. }
                        | CapabilityKind::ServiceStatus { .. }
                )
            })
            .count();
        if modifying_steps > 3 && max_score < 95 {
            max_score = (max_score + 10).min(100);
            all_factors.push(format!(
                "Plan contains {modifying_steps} state-modifying actions (+10 multi-step factor)"
            ));
        }

        // Cross-crate dispersion penalty: calculate unique crate roots modified
        let mut unique_crates = std::collections::HashSet::new();
        for cap in caps {
            if (matches!(cap, CapabilityKind::FileWrite { .. })
                || matches!(cap, CapabilityKind::FileDelete { .. })
                || matches!(cap, CapabilityKind::FileRead { .. }))
                && let Some(path) = match cap {
                    CapabilityKind::FileWrite { path, .. }
                    | CapabilityKind::FileDelete { path }
                    | CapabilityKind::FileRead { path } => Some(path),
                    _ => None,
                }
                && let Some(crate_name) = extract_crate_from_path(path)
            {
                unique_crates.insert(crate_name);
            }
        }

        if unique_crates.len() >= 3 {
            let mut sorted: Vec<String> = unique_crates.into_iter().collect();
            sorted.sort();
            max_score = (max_score + 25).min(100);
            all_factors.push(format!(
                "Plan spans {} distinct workspace crates ({}) (+25 cross-crate dispersion factor)",
                sorted.len(),
                sorted.join(", ")
            ));
        } else if unique_crates.len() == 2 {
            let mut sorted: Vec<String> = unique_crates.into_iter().collect();
            sorted.sort();
            max_score = (max_score + 15).min(100);
            all_factors.push(format!(
                "Plan spans 2 distinct workspace crates ({}) (+15 cross-crate dispersion factor)",
                sorted.join(", ")
            ));
        }

        let tier = ApprovalTier::from_score(max_score);
        BlastRadiusReport {
            score: max_score,
            tier,
            factors: all_factors,
        }
    }
}

/// Helper function to extract the crate name from a source file path.
pub fn extract_crate_from_path(path: &str) -> Option<String> {
    let p = std::path::Path::new(path);
    let comps: Vec<&str> = p
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    for (idx, &comp) in comps.iter().enumerate() {
        if (comp == "src" || comp == "tests") && idx > 0 {
            return Some(comps[idx - 1].to_string());
        }
    }

    if p.file_name().and_then(|f| f.to_str()) == Some("Cargo.toml") && comps.len() >= 2 {
        return Some(comps[comps.len() - 2].to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping_and_reads_are_auto_tier() {
        let ping_rep = PolicyEngine::evaluate_capability(&CapabilityKind::Ping, None);
        assert_eq!(ping_rep.tier, ApprovalTier::Auto);
        assert_eq!(ping_rep.score, 0);

        let read_rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::FileRead {
                path: "/etc/nginx/nginx.conf".into(),
            },
            None,
        );
        assert_eq!(read_rep.tier, ApprovalTier::Auto);
        assert!(!read_rep.tier.requires_confirmation());
    }

    #[test]
    fn test_user_file_write_is_notify() {
        let rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::FileWrite {
                path: "/home/user/app.conf".into(),
                content: "port=8080".into(),
                append: false,
            },
            None,
        );
        assert_eq!(rep.tier, ApprovalTier::Notify);
    }

    #[test]
    fn test_etc_write_and_service_restart_require_confirm() {
        let rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::FileWrite {
                path: "/etc/nginx/conf.d/app.conf".into(),
                content: "server { listen 8080; }".into(),
                append: false,
            },
            None,
        );
        assert_eq!(rep.tier, ApprovalTier::Confirm);
        assert!(rep.tier.requires_confirmation());

        let svc_rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::ServiceRestart {
                unit: "nginx.service".into(),
                approved: false,
            },
            None,
        );
        assert_eq!(svc_rep.tier, ApprovalTier::Confirm);
    }

    #[test]
    fn test_critical_package_and_protected_unit_are_expert_review() {
        let pkg_rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::PackageInstall {
                package: "glibc".into(),
                approved: false,
            },
            None,
        );
        assert_eq!(pkg_rep.tier, ApprovalTier::ExpertReview);
        assert!(pkg_rep.score >= 80);

        let sudoers_rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::FileWrite {
                path: "/etc/sudoers.d/custom".into(),
                content: "user ALL=(ALL) NOPASSWD: ALL".into(),
                append: false,
            },
            None,
        );
        assert_eq!(sudoers_rep.tier, ApprovalTier::ExpertReview);

        let ssh_rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::ServiceRestart {
                unit: "ssh.service".into(),
                approved: false,
            },
            None,
        );
        assert_eq!(ssh_rep.tier, ApprovalTier::ExpertReview);
    }

    #[test]
    fn test_package_install_fails_closed_when_db_is_none() {
        let pkg_rep = PolicyEngine::evaluate_capability(
            &CapabilityKind::PackageInstall {
                package: "ripgrep".into(),
                approved: false,
            },
            None, // World Model DB is offline / unavailable
        );

        // Must fail closed with minimum Confirm tier and >= 50 risk score
        assert_eq!(pkg_rep.tier, ApprovalTier::Confirm);
        assert!(pkg_rep.score >= 50);
        assert!(
            pkg_rep
                .factors
                .iter()
                .any(|f| f.contains("World Model database offline/unavailable"))
        );
    }

    #[test]
    fn test_cross_crate_dispersion_scoring() {
        // 1. Single-crate 2-file plan (modifying 2 files in crate-a)
        let single_crate_caps = vec![
            CapabilityKind::FileWrite {
                path: "/workspace/crate-a/src/mod1.rs".into(),
                content: "a".into(),
                append: false,
            },
            CapabilityKind::FileWrite {
                path: "/workspace/crate-a/src/lib.rs".into(),
                content: "b".into(),
                append: false,
            },
        ];
        let rep1 = PolicyEngine::evaluate_plan_aggregate(&single_crate_caps, None);
        // Base score 30 (25 user scope + 5 write) + 0 crate penalty = 30 (Notify tier)
        assert_eq!(rep1.score, 30);
        assert_eq!(rep1.tier, ApprovalTier::Notify);
        assert!(!rep1.tier.requires_confirmation());

        // 2. 2-crate plan (modifying crate-a and crate-b)
        let two_crate_caps = vec![
            CapabilityKind::FileWrite {
                path: "/workspace/crate-a/src/lib.rs".into(),
                content: "a".into(),
                append: false,
            },
            CapabilityKind::FileWrite {
                path: "/workspace/crate-b/src/main.rs".into(),
                content: "b".into(),
                append: false,
            },
        ];
        let rep2 = PolicyEngine::evaluate_plan_aggregate(&two_crate_caps, None);
        // Base score 30 + 15 (2 crates) = 45 (Escalated to Confirm tier!)
        assert_eq!(rep2.score, 45);
        assert_eq!(rep2.tier, ApprovalTier::Confirm);
        assert!(rep2.tier.requires_confirmation());

        // 3. 3-crate plan (modifying crate-a, crate-b, crate-c)
        let three_crate_caps = vec![
            CapabilityKind::FileWrite {
                path: "/workspace/crate-a/src/lib.rs".into(),
                content: "a".into(),
                append: false,
            },
            CapabilityKind::FileWrite {
                path: "/workspace/crate-b/src/lib.rs".into(),
                content: "b".into(),
                append: false,
            },
            CapabilityKind::FileWrite {
                path: "/workspace/crate-c/src/lib.rs".into(),
                content: "c".into(),
                append: false,
            },
        ];
        let rep3 = PolicyEngine::evaluate_plan_aggregate(&three_crate_caps, None);
        // Base score 30 + 25 (3 crates) = 55 (Confirm tier)
        assert_eq!(rep3.score, 55);
        assert_eq!(rep3.tier, ApprovalTier::Confirm);
        assert!(rep3.tier.requires_confirmation());

        // 4. 3-crate 5-file plan (modifying 5 files across 3 crates)
        let three_crate_5_files = vec![
            CapabilityKind::FileWrite {
                path: "/workspace/crate-a/src/mod1.rs".into(),
                content: "a".into(),
                append: false,
            },
            CapabilityKind::FileWrite {
                path: "/workspace/crate-a/src/lib.rs".into(),
                content: "b".into(),
                append: false,
            },
            CapabilityKind::FileWrite {
                path: "/workspace/crate-b/src/lib.rs".into(),
                content: "c".into(),
                append: false,
            },
            CapabilityKind::FileWrite {
                path: "/workspace/crate-c/src/mod1.rs".into(),
                content: "d".into(),
                append: false,
            },
            CapabilityKind::FileWrite {
                path: "/workspace/crate-c/src/lib.rs".into(),
                content: "e".into(),
                append: false,
            },
        ];
        let rep4 = PolicyEngine::evaluate_plan_aggregate(&three_crate_5_files, None);
        // Base score 30 + 10 (>3 steps) + 25 (3 crates) = 65 (Confirm tier)
        assert_eq!(rep4.score, 65);
        assert_eq!(rep4.tier, ApprovalTier::Confirm);
    }
}
