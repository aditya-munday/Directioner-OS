//! Directioner-OS AI Engineer Control Plane (Phase 1.2 / 1.3 / 1.5).
//!
//! Orchestrates the multi-agent engineering workflow:
//!
//! 1. [`planner::Planner`] decomposes high-level intent into an auditable [`plan::Plan`].
//! 2. [`policy::PolicyEngine`] evaluates blast radius and determines [`policy::ApprovalTier`].
//! 3. [`simulation::Simulator`] generates dry-run diffs and side-effect projections.
//! 4. [`implementer::Implementer`] dispatches steps over Unix socket to `ai-capabilityd`,
//!    handling rollback upon failure.
//! 5. [`verifier::Verifier`] independently confirms that post-execution system state
//!    matches intent.

pub mod client;
pub mod cockpit;
pub mod code_engine;
pub mod conversational;
pub mod draft_validator;
pub mod federation;
pub mod health_model;
pub mod hwcheck;
pub mod implementer;
pub mod incident_response;
pub mod invariants;
pub mod localization;
pub mod plan;
pub mod planner;
pub mod plugin;
pub mod policy;
pub mod remediation;
pub mod reviewer;
pub mod self_healing;
pub mod simulation;
pub mod symbol_graph;
pub mod unit_graph;
pub mod verifier;

pub use client::{CapabilityClient, ClientError};
pub use cockpit::{
    AuditStreamEntry, CockpitRenderer, CockpitSnapshot, CockpitState, SystemHealthSummary,
};
pub use code_engine::{BuildTestReport, CodeEngine, Diagnostic, TestFailure};
pub use conversational::{
    ChatMessage, ChatRole, CloudAnthropicBackend, CloudGeminiBackend, CloudProvider,
    ConversationSession, ConversationalConfig, ConversationalError, IntentParser, LlmBackend,
    LocalSocketBackend, MockBackend,
};
pub use draft_validator::{
    CandidateCapabilityDraft, CandidatePlanDraft, CandidateRollbackDraft, CandidateStepDraft,
    DriftError, MAX_DRAFT_PAYLOAD_BYTES, PlanDraftValidator, ValidationError,
};
pub use federation::{
    FederatedHealthSnapshot, FederatedPeerStatus, FederatedResourceSummary, FederationError,
    FederationPeerClient, FederationPublisher, SignedFederatedSnapshot,
};
pub use health_model::{
    HealthEvidence, HealthModel, HealthModelConfig, HealthStatus, SubsystemHealth,
    SystemHealthReport,
};
pub use implementer::Implementer;
pub use incident_response::{
    IncidentPostureReport, IncidentResponseAgent, IncidentResponseConfig, Tier1RecoveryObservation,
};
pub use plan::{
    Plan, PlanExecutionReport, PlanStatus, PlanStep, RollbackStep, StepExecutionResult,
};
pub use planner::{Planner, PlannerConfig};
pub use plugin::{
    PluginContext, PluginError, PluginExtension, PluginManifest, PluginMetadata, PluginPermissions,
    PluginRegistry, PluginSecurity, WorldModelAuditPlugin,
};
pub use policy::{ApprovalTier, BlastRadiusReport, PolicyEngine};
pub use remediation::{RemediationAgent, RemediationConfig, RemediationError};
pub use reviewer::{
    FindingSeverity, ReviewFinding, ReviewReport, ReviewVerdict, Reviewer, ReviewerConfig,
};
pub use self_healing::{
    EntityIncidentState, SelfHealingConfig, SelfHealingEngine, SelfHealingOutcome,
    SelfHealingReport,
};
pub use simulation::{PlanPreview, Simulator, StepPreview};
pub use symbol_graph::{SymbolDefinition, SymbolKind, WorkspaceSymbolGraph};
pub use verifier::{VerificationCheck, VerificationReport, Verifier};
