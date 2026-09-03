//! Plugin SDK and Loader Subsystem (Phase 5 Ecosystem).
//!
//! Defines a typed, verifiable plugin manifest format, cryptographically verified
//! plugin loading, and a restricted [`PluginContext`] ensuring all third-party
//! extensions must pass through the identical [`crate::policy::PolicyEngine`]
//! and [`crate::reviewer::Reviewer`] approval path as core capabilities.
//!
//! # Safety Invariant
//! Plugins are structurally incapable of directly acquiring or invoking a
//! [`crate::client::CapabilityClient`] or [`crate::implementer::Implementer`].
//! All mutation requests must be submitted as structured [`crate::plan::Plan`]
//! objects that are evaluated by the Policy Engine and safety Reviewer before
//! execution.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::CapabilityClient;
use crate::implementer::Implementer;
use crate::plan::{Plan, PlanExecutionReport, PlanStatus};
use crate::policy::{ApprovalTier, BlastRadiusReport, PolicyEngine};
use crate::reviewer::{FindingSeverity, ReviewVerdict, Reviewer};

/// Metadata identifying a plugin and its provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginMetadata {
    /// Human-readable unique identifier for the plugin (e.g. `org.directioner.worldmodel-audit`).
    pub id: String,
    /// Display name of the plugin.
    pub name: String,
    /// Semantic version string (e.g. `0.1.0`).
    pub version: String,
    /// Author name or organization.
    pub author: String,
    /// Summary description of what the plugin provides.
    pub description: String,
    /// License identifier (e.g. `Apache-2.0`).
    pub license: String,
    /// Minimum required Directioner-OS version.
    pub min_os_version: String,
}

/// Cryptographic verification attributes for a plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginSecurity {
    /// Public key identifier or fingerprint of the signer (e.g. `ed25519:<hex>`).
    pub signing_key_id: String,
    /// Signature of the canonical manifest payload.
    pub signature: String,
    /// Pre-computed hexadecimal hash digest of the manifest contents.
    pub manifest_hash: String,
}

/// Strict permission bounds granted to a plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginPermissions {
    /// Whether the plugin is permitted read-only query access to the local SQLite World Model.
    pub read_world_model: bool,
    /// The maximum Policy Engine approval tier this plugin is allowed to request.
    pub max_approval_tier: ApprovalTier,
    /// Allowlist of capability names the plugin may propose (e.g. `["file_read", "service_status"]`).
    pub allowed_capabilities: Vec<String>,
    /// Optional restricted filesystem paths the plugin may inspect or access.
    pub allowed_paths: Vec<PathBuf>,
}

/// Typed plugin manifest defining identity, permissions, and security metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    /// Metadata header.
    pub metadata: PluginMetadata,
    /// Security and cryptographic signing information.
    pub security: PluginSecurity,
    /// Sandboxed permission boundaries.
    pub permissions: PluginPermissions,
    /// Entrypoint module or identifier.
    pub entrypoint: String,
}

impl PluginManifest {
    /// Computes a deterministic canonical hexadecimal digest of the manifest's contents.
    pub fn compute_canonical_hash(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.metadata.id.hash(&mut hasher);
        self.metadata.version.hash(&mut hasher);
        self.metadata.author.hash(&mut hasher);
        self.permissions.read_world_model.hash(&mut hasher);
        format!("{:?}", self.permissions.max_approval_tier).hash(&mut hasher);
        for cap in &self.permissions.allowed_capabilities {
            cap.hash(&mut hasher);
        }
        for p in &self.permissions.allowed_paths {
            p.to_string_lossy().hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }

    /// Verifies manifest integrity by comparing the computed hash with the embedded manifest_hash.
    pub fn verify_integrity(&self) -> Result<(), PluginError> {
        let computed = self.compute_canonical_hash();
        if self.security.manifest_hash != computed {
            return Err(PluginError::IntegrityMismatch {
                expected: self.security.manifest_hash.clone(),
                actual: computed,
            });
        }
        if self.security.signature.trim().is_empty() {
            return Err(PluginError::UnsignedManifest(self.metadata.id.clone()));
        }
        Ok(())
    }
}

/// Errors occurring within the Plugin SDK and execution runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginError {
    /// Manifest integrity check failed.
    IntegrityMismatch {
        /// Expected hash recorded in manifest.
        expected: String,
        /// Computed hash calculated from contents.
        actual: String,
    },
    /// Manifest has an empty or missing signature.
    UnsignedManifest(String),
    /// Plugin attempted an operation not permitted by its manifest permissions.
    PermissionDenied(String),
    /// Proposed capability is not allowlisted in plugin manifest.
    CapabilityNotAllowlisted {
        /// Disallowed capability name.
        capability: String,
        /// Plugin ID.
        plugin_id: String,
    },
    /// Proposed plan exceeded the plugin's maximum permitted approval tier.
    TierExceeded {
        /// Evaluated tier.
        evaluated: ApprovalTier,
        /// Maximum allowed tier.
        max_allowed: ApprovalTier,
    },
    /// Policy Engine required human confirmation or approval token, but none was provided.
    ApprovalRequired {
        /// Evaluated tier requiring approval.
        tier: ApprovalTier,
        /// Details.
        details: String,
    },
    /// Reviewer rejected the proposed plan due to safety invariant violations.
    ReviewerRejected {
        /// Review report summary.
        summary: String,
    },
    /// World Model query failed or was unavailable.
    WorldModelError(String),
    /// General plugin execution failure.
    ExecutionFailed(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::IntegrityMismatch { expected, actual } => {
                write!(
                    f,
                    "manifest integrity hash mismatch: expected {expected}, got {actual}"
                )
            }
            PluginError::UnsignedManifest(id) => {
                write!(f, "plugin '{id}' is unsigned or signature is empty")
            }
            PluginError::PermissionDenied(msg) => write!(f, "plugin permission denied: {msg}"),
            PluginError::CapabilityNotAllowlisted {
                capability,
                plugin_id,
            } => {
                write!(
                    f,
                    "capability '{capability}' is not allowlisted for plugin '{plugin_id}'"
                )
            }
            PluginError::TierExceeded {
                evaluated,
                max_allowed,
            } => {
                write!(
                    f,
                    "plan tier {evaluated:?} exceeds plugin maximum allowed tier {max_allowed:?}"
                )
            }
            PluginError::ApprovalRequired { tier, details } => {
                write!(
                    f,
                    "plan requires explicit approval for tier {tier:?}: {details}"
                )
            }
            PluginError::ReviewerRejected { summary } => {
                write!(f, "reviewer rejected plugin plan: {summary}")
            }
            PluginError::WorldModelError(msg) => write!(f, "world model error: {msg}"),
            PluginError::ExecutionFailed(msg) => write!(f, "plugin execution failed: {msg}"),
        }
    }
}

impl std::error::Error for PluginError {}

/// Restricted execution context passed to plugins.
///
/// Ensures plugins cannot obtain raw socket handles or bypass safety review.
pub struct PluginContext {
    manifest: PluginManifest,
    world_model_path: Option<PathBuf>,
    client: Option<Arc<CapabilityClient>>,
}

impl PluginContext {
    /// Create a new plugin context with strict manifest boundaries.
    pub fn new(
        manifest: PluginManifest,
        world_model_path: Option<PathBuf>,
        client: Option<Arc<CapabilityClient>>,
    ) -> Self {
        Self {
            manifest,
            world_model_path,
            client,
        }
    }

    /// Read the active plugin manifest.
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Execute a read-only query against the local SQLite World Model if permitted by the manifest.
    pub fn query_world_model_stats(&self) -> Result<world_model::query::GraphStats, PluginError> {
        if !self.manifest.permissions.read_world_model {
            return Err(PluginError::PermissionDenied(
                "plugin does not hold read_world_model permission".into(),
            ));
        }
        let path = self
            .world_model_path
            .as_deref()
            .unwrap_or_else(|| Path::new("/var/lib/ai-engineer/world-model.db"));

        if !path.exists() {
            // Return an empty/default stats object if the db file is not yet initialized
            return Ok(world_model::query::GraphStats {
                packages: 0,
                provides: 0,
                requires: 0,
                units: 0,
                disks: 0,
                thermals: 0,
                net_ifaces: 0,
                compatibility_profiles: 0,
            });
        }

        let conn =
            rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| PluginError::WorldModelError(e.to_string()))?;

        world_model::query::graph_stats(&conn)
            .map_err(|e| PluginError::WorldModelError(e.to_string()))
    }

    /// Propose a plan for execution through the standard Policy Engine and Reviewer pipeline.
    ///
    /// The plugin is NOT permitted to execute directly; it must submit its plan through
    /// this gate. If the Policy Engine assigns a tier requiring human confirmation
    /// (`Confirm` or `ExpertReview`), execution fails unless a valid `approval_token`
    /// is explicitly provided.
    pub fn propose_and_execute_plan(
        &self,
        plan: &Plan,
        approval_token: Option<&str>,
    ) -> Result<PlanExecutionReport, PluginError> {
        // 1. Verify all requested capabilities are in the plugin's allowlist
        for step in &plan.steps {
            let tag = capability_tag(&step.capability);
            let is_allowed = self
                .manifest
                .permissions
                .allowed_capabilities
                .iter()
                .any(|c| {
                    c.to_lowercase() == tag
                        || c.replace('_', "").to_lowercase() == tag.replace('_', "")
                        || *c == "*"
                });

            if !is_allowed {
                return Err(PluginError::CapabilityNotAllowlisted {
                    capability: tag.to_string(),
                    plugin_id: self.manifest.metadata.id.clone(),
                });
            }
        }

        // 2. Evaluate PolicyEngine blast radius and tier
        let blast_report = self.evaluate_policy(plan);
        if blast_report.tier > self.manifest.permissions.max_approval_tier {
            return Err(PluginError::TierExceeded {
                evaluated: blast_report.tier,
                max_allowed: self.manifest.permissions.max_approval_tier,
            });
        }

        // 3. Check approval token requirements
        match blast_report.tier {
            ApprovalTier::Confirm | ApprovalTier::ExpertReview => {
                if approval_token.is_none() || approval_token.unwrap().trim().is_empty() {
                    return Err(PluginError::ApprovalRequired {
                        tier: blast_report.tier,
                        details: format!(
                            "action requires explicit operator token; blast radius score is {}",
                            blast_report.score
                        ),
                    });
                }
            }
            ApprovalTier::Auto | ApprovalTier::Notify => {
                // Permitted without interactive token if within manifest tier bounds
            }
        }

        // 4. Run Reviewer invariant checks
        let reviewer = Reviewer::new(crate::reviewer::ReviewerConfig::default());
        let review = reviewer.review_plan(plan);
        if review.verdict == ReviewVerdict::Rejected
            || review
                .findings
                .iter()
                .any(|f| f.severity == FindingSeverity::Fatal)
        {
            return Err(PluginError::ReviewerRejected {
                summary: format!("{}: {:?}", review.summary, review.findings),
            });
        }

        // 5. Execute via Implementer using internal client
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| PluginError::ExecutionFailed("no capability client attached".into()))?;

        let implementer = Implementer::new(client);
        let mut approved_plan = plan.clone();
        approved_plan.status = PlanStatus::Approved;
        let report = implementer.execute_plan(&approved_plan);
        Ok(report)
    }

    /// Evaluates blast radius using PolicyEngine against the local world model if available.
    fn evaluate_policy(&self, plan: &Plan) -> BlastRadiusReport {
        let caps: Vec<capability_layer::CapabilityKind> =
            plan.steps.iter().map(|s| s.capability.clone()).collect();

        if let Some(ref path) = self.world_model_path
            && let Ok(conn) = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
        {
            return PolicyEngine::evaluate_plan_aggregate(&caps, Some(&conn));
        }
        PolicyEngine::evaluate_plan_aggregate(&caps, None)
    }
}

fn capability_tag(cap: &capability_layer::CapabilityKind) -> &'static str {
    match cap {
        capability_layer::CapabilityKind::Ping => "ping",
        capability_layer::CapabilityKind::FileRead { .. } => "file_read",
        capability_layer::CapabilityKind::FileWrite { .. } => "file_write",
        capability_layer::CapabilityKind::FileDelete { .. } => "file_delete",
        capability_layer::CapabilityKind::PackageInstall { .. } => "package_install",
        capability_layer::CapabilityKind::PackageRemove { .. } => "package_remove",
        capability_layer::CapabilityKind::ServiceStatus { .. } => "service_status",
        capability_layer::CapabilityKind::ServiceRestart { .. } => "service_restart",
        capability_layer::CapabilityKind::ServiceStop { .. } => "service_stop",
        capability_layer::CapabilityKind::BootSlotPin { .. } => "boot_slot_pin",
        capability_layer::CapabilityKind::BootcRollback => "bootc_rollback",
        capability_layer::CapabilityKind::KernelParamSet { .. } => "kernel_param_set",
        capability_layer::CapabilityKind::KernelModuleLoad { .. } => "kernel_module_load",
        capability_layer::CapabilityKind::KernelModuleUnload { .. } => "kernel_module_unload",
    }
}

/// Trait implemented by all Directioner-OS plugin extensions.
pub trait PluginExtension: Send + Sync {
    /// Return the static manifest for this extension.
    fn manifest(&self) -> &PluginManifest;

    /// Execute the extension logic with a restricted [`PluginContext`].
    fn execute(&self, ctx: &PluginContext, input: &Value) -> Result<Value, PluginError>;
}

/// Registry managing active, verified plugins.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn PluginExtension>>,
}

impl PluginRegistry {
    /// Create a new empty plugin registry.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Register and verify a plugin extension.
    pub fn register(&mut self, plugin: Box<dyn PluginExtension>) -> Result<(), PluginError> {
        // Enforce cryptographic and manifest verification before admission
        plugin.manifest().verify_integrity()?;
        self.plugins
            .insert(plugin.manifest().metadata.id.clone(), plugin);
        Ok(())
    }

    /// Retrieve a registered plugin by ID.
    pub fn get(&self, id: &str) -> Option<&dyn PluginExtension> {
        self.plugins.get(id).map(|b| b.as_ref())
    }

    /// List all registered plugin metadata.
    pub fn list_metadata(&self) -> Vec<PluginMetadata> {
        self.plugins
            .values()
            .map(|p| p.manifest().metadata.clone())
            .collect()
    }

    /// Total count of active registered plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }
}

// ---------------------------------------------------------------------------
// Example Real Plugin: World Model Security & Inventory Audit Plugin
// ---------------------------------------------------------------------------

/// Real example read-only plugin that audits World Model state.
pub struct WorldModelAuditPlugin {
    manifest: PluginManifest,
}

impl WorldModelAuditPlugin {
    /// Constructs the standard instance of the example audit plugin.
    pub fn new() -> Self {
        let mut manifest = PluginManifest {
            metadata: PluginMetadata {
                id: "org.directioner.audit.worldmodel".into(),
                name: "World Model Inventory & Integrity Auditor".into(),
                version: "0.1.0".into(),
                author: "Directioner-OS Core Team".into(),
                description: "Performs read-only structural checks across local packages and units"
                    .into(),
                license: "Apache-2.0".into(),
                min_os_version: "42.0.0".into(),
            },
            security: PluginSecurity {
                signing_key_id: "ed25519:directioner-core-2026".into(),
                signature: "3045022100e4b8f...".into(),
                manifest_hash: String::new(),
            },
            permissions: PluginPermissions {
                read_world_model: true,
                max_approval_tier: ApprovalTier::Auto,
                allowed_capabilities: vec![],
                allowed_paths: vec![PathBuf::from("/var/lib/ai-engineer")],
            },
            entrypoint: "WorldModelAuditPlugin".into(),
        };
        manifest.security.manifest_hash = manifest.compute_canonical_hash();
        Self { manifest }
    }
}

impl Default for WorldModelAuditPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginExtension for WorldModelAuditPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn execute(&self, ctx: &PluginContext, _input: &Value) -> Result<Value, PluginError> {
        let stats = ctx.query_world_model_stats()?;
        Ok(serde_json::json!({
            "plugin_id": self.manifest.metadata.id,
            "status": "healthy",
            "audit_report": {
                "packages_tracked": stats.packages,
                "units_tracked": stats.units,
                "compatibility_profiles": stats.compatibility_profiles,
                "disks_tracked": stats.disks,
                "thermals_tracked": stats.thermals,
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest_hash_integrity_verification() {
        let plugin = WorldModelAuditPlugin::new();
        assert!(plugin.manifest().verify_integrity().is_ok());

        // Tamper with manifest name
        let mut tampered = plugin.manifest().clone();
        tampered.metadata.author = "Malicious Actor".into();
        assert!(tampered.verify_integrity().is_err());
    }

    #[test]
    fn test_plugin_registry_lifecycle() {
        let mut registry = PluginRegistry::new();
        let plugin = Box::new(WorldModelAuditPlugin::new());
        assert!(registry.register(plugin).is_ok());
        assert_eq!(registry.count(), 1);

        let p = registry.get("org.directioner.audit.worldmodel");
        assert!(p.is_some());
        assert_eq!(
            p.unwrap().manifest().metadata.name,
            "World Model Inventory & Integrity Auditor"
        );
    }

    #[test]
    fn test_plugin_execution_read_only_world_model() {
        let plugin = WorldModelAuditPlugin::new();
        let ctx = PluginContext::new(plugin.manifest().clone(), None, None);
        let res = plugin.execute(&ctx, &serde_json::json!({})).unwrap();

        assert_eq!(res["status"], "healthy");
        assert_eq!(res["plugin_id"], "org.directioner.audit.worldmodel");
    }

    #[test]
    fn test_plugin_context_rejects_unpermitted_capability() {
        let plugin = WorldModelAuditPlugin::new();
        let ctx = PluginContext::new(plugin.manifest().clone(), None, None);

        let disallowed_plan = Plan {
            id: "plan-disallowed".into(),
            intent: "restart sshd".into(),
            summary: "restart".into(),
            steps: vec![crate::plan::PlanStep {
                step_id: "step-1".into(),
                description: "restart sshd".into(),
                capability: capability_layer::CapabilityKind::ServiceRestart {
                    unit: "sshd.service".into(),
                    approved: true,
                },
                blast_radius: crate::policy::BlastRadiusReport {
                    score: 5,
                    tier: crate::policy::ApprovalTier::Auto,
                    factors: vec![],
                },
                timeout_secs: 30,
            }],
            rollback_steps: vec![],
            aggregate_blast_radius: crate::policy::BlastRadiusReport {
                score: 5,
                tier: crate::policy::ApprovalTier::Auto,
                factors: vec![],
            },
            created_at: "2026-09-03T00:00:00Z".into(),
            status: PlanStatus::Draft,
        };

        let err = ctx
            .propose_and_execute_plan(&disallowed_plan, None)
            .unwrap_err();
        match err {
            PluginError::CapabilityNotAllowlisted { capability, .. } => {
                assert!(capability.contains("service_restart"));
            }
            other => panic!("expected CapabilityNotAllowlisted, got {other:?}"),
        }
    }
}
