//! Benchmark Scenario 5: Package / Service Inverse Rollback Capabilities (Phase 2).

use control_plane::planner::{Planner, PlannerConfig};

use crate::BenchmarkScenario;

/// Scenario testing package/service rollback capabilities.
pub struct PackageServiceRollbackScenario;

impl BenchmarkScenario for PackageServiceRollbackScenario {
    fn name(&self) -> &'static str {
        "package_service_compensating_rollback"
    }

    fn description(&self) -> &'static str {
        "Verify package.remove / service.stop inverse rollback coverage (Phase 2)"
    }

    fn is_known_gap(&self) -> bool {
        false
    }

    fn run(&self) -> (bool, String) {
        let planner = Planner::new(PlannerConfig::default());

        // 1. Package install rollback assertion
        let plan_pkg = planner.create_plan("install htop", None).unwrap();
        let has_package_rollback = plan_pkg.rollback_steps.iter().any(|rb| {
            matches!(
                rb.capability,
                capability_layer::CapabilityKind::PackageRemove { .. }
            )
        });

        // 2. Service restart rollback assertion
        let plan_svc = planner
            .create_plan("fix nginx.service restart", None)
            .unwrap();
        let has_service_rollback = plan_svc.rollback_steps.iter().any(|rb| {
            matches!(
                rb.capability,
                capability_layer::CapabilityKind::ServiceStop { .. }
            )
        });

        if !has_package_rollback {
            (
                false,
                "Missing PackageRemove compensating rollback step for package install".into(),
            )
        } else if !has_service_rollback {
            (
                false,
                "Missing ServiceStop compensating rollback step for service restart".into(),
            )
        } else {
            (
                true,
                "Verified: PackageRemove and ServiceStop compensating rollbacks generated successfully".into(),
            )
        }
    }
}
