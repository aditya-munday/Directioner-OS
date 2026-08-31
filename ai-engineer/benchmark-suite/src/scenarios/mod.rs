//! Standard benchmark scenarios for Phases 1 to 3.

pub mod auditability;
pub mod auto_approve_policy_refusal;
pub mod blast_radius;
pub mod cockpit_telemetry;
pub mod conversational_scenarios;
pub mod dependency_audit;
pub mod killswitch;
pub mod multi_crate_repair;
pub mod multi_crate_rollback;
pub mod remediation;
pub mod rollback;
pub mod rollback_gaps;
pub mod webserver;

pub use auditability::AuditabilityScenario;
pub use auto_approve_policy_refusal::AutoApprovePolicyRefusalScenario;
pub use blast_radius::BlastRadiusScenario;
pub use cockpit_telemetry::CockpitTelemetryScenario;
pub use conversational_scenarios::{
    ConversationalAdversarialRejectionScenario, ConversationalCloudFailoverScenario,
    ConversationalIntentParsingScenario, ConversationalLocalFallbackScenario,
    ConversationalMultiTurnRefinementScenario, ConversationalSafetyBoundaryScenario,
    ConversationalStateDriftScenario,
};
pub use dependency_audit::DependencyAuditScenario;
pub use killswitch::KillswitchScenario;
pub use multi_crate_repair::MultiCrateRepairScenario;
pub use multi_crate_rollback::MultiCrateRollbackScenario;
pub use remediation::RemediationScenario;
pub use rollback::RollbackScenario;
pub use rollback_gaps::PackageServiceRollbackScenario;
pub use webserver::WebserverScenario;
