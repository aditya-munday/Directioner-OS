//! Benchmark Scenario 3: Dependency Audit & Blast-Radius Evaluation.

use capability_layer::CapabilityKind;
use control_plane::policy::{ApprovalTier, PolicyEngine};
use world_model::model::{PackageData, PackageRow, ProvideEdge, RequireEdge};
use world_model::sync::sync_packages;

use crate::BenchmarkScenario;

/// Scenario 3: Evaluate blast-radius and approval tier for packages with deep reverse dependencies.
pub struct DependencyAuditScenario;

impl BenchmarkScenario for DependencyAuditScenario {
    fn name(&self) -> &'static str {
        "dependency_audit"
    }

    fn description(&self) -> &'static str {
        "Audit blast radius of package modification against live World Model dependency graph"
    }

    fn run(&self) -> (bool, String) {
        let temp = match tempfile::tempdir() {
            Ok(t) => t,
            Err(e) => return (false, format!("Failed to create tempdir: {e}")),
        };

        let db_path = temp.path().join("wm.sqlite");
        let conn = match world_model::db::open(&db_path) {
            Ok(c) => c,
            Err(e) => return (false, format!("Failed to open db: {e}")),
        };

        // Populate a synthetic world model with a core library and 25 dependent packages
        let mut packages = Vec::new();
        let mut provides = Vec::new();
        let mut requires = Vec::new();

        packages.push(PackageRow {
            name: "libcrypto3".into(),
            evr: "3.0.1".into(),
            arch: "x86_64".into(),
            summary: "Cryptography library".into(),
        });
        provides.push(ProvideEdge {
            pkg: "libcrypto3".into(),
            provide: "libcrypto.so.3()(64bit)".into(),
            version: String::new(),
        });
        provides.push(ProvideEdge {
            pkg: "libcrypto3".into(),
            provide: "libcrypto3".into(),
            version: String::new(),
        });

        for i in 0..25 {
            let app_name = format!("app-service-{i}");
            packages.push(PackageRow {
                name: app_name.clone(),
                evr: "1.0.0".into(),
                arch: "x86_64".into(),
                summary: format!("App service {i}"),
            });
            provides.push(ProvideEdge {
                pkg: app_name.clone(),
                provide: app_name.clone(),
                version: String::new(),
            });
            requires.push(RequireEdge {
                pkg: app_name,
                dep: "libcrypto.so.3()(64bit)".into(),
                flags: String::new(),
                version: String::new(),
            });
        }

        let pkg_data = PackageData {
            packages,
            provides,
            requires,
        };

        if let Err(e) = sync_packages(&conn, &pkg_data) {
            return (false, format!("Failed to sync packages: {e}"));
        }

        // Evaluate blast radius for modifying libcrypto3
        let cap = CapabilityKind::PackageInstall {
            package: "libcrypto3".into(),
            approved: false,
        };

        let report = PolicyEngine::evaluate_capability(&cap, Some(&conn));

        if report.score < 80 {
            return (
                false,
                format!("Expected high blast score (>=80), got {}", report.score),
            );
        }

        if report.tier != ApprovalTier::ExpertReview {
            return (
                false,
                format!("Expected ExpertReview tier, got {:?}", report.tier),
            );
        }

        let has_rev_dep_factor = report
            .factors
            .iter()
            .any(|f| f.contains("reverse dependencies"));

        if !has_rev_dep_factor {
            return (
                false,
                "Policy engine failed to cite reverse dependencies in blast factors".into(),
            );
        }

        (
            true,
            format!(
                "Correctly assigned score {} ({:?}) based on reverse dependencies",
                report.score, report.tier
            ),
        )
    }
}
