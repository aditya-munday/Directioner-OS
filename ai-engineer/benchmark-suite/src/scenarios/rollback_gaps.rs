//! Benchmark Scenario 5: Package / Service Inverse Rollback Capability Gap (Tracked for Phase 2).

use control_plane::planner::{Planner, PlannerConfig};

use crate::BenchmarkScenario;

/// Scenario testing package/service rollback capabilities.
///
/// Explicitly documents that `package.remove` and `service.stop` capabilities
/// do not exist in the 6 baseline capabilities of Phase 0/1 and are tracked for Phase 2.
pub struct PackageServiceRollbackGapScenario;

impl BenchmarkScenario for PackageServiceRollbackGapScenario {
    fn name(&self) -> &'static str {
        "package_service_rollback_gap"
    }

    fn description(&self) -> &'static str {
        "Verify package.remove / service.stop inverse rollback coverage (Phase 2 capability gap)"
    }

    fn is_known_gap(&self) -> bool {
        true
    }

    fn run(&self) -> (bool, String) {
        let planner = Planner::new(PlannerConfig::default());
        let plan = planner.create_plan("install htop", None).unwrap();

        // Check if package.remove rollback exists
        let has_package_rollback = plan.rollback_steps.iter().any(|rb| {
            matches!(
                rb.description.to_lowercase().as_str(),
                d if d.contains("remove package") || d.contains("uninstall")
            )
        });

        if !has_package_rollback {
            (
                false,
                "GAP: package.remove & service.stop inverse primitives absent in Phase 1 (deferred to Phase 2)".into(),
            )
        } else {
            (
                true,
                "Package/service rollback capabilities are implemented".into(),
            )
        }
    }
}
