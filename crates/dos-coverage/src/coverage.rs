use dos_catalog::CatalogRegistry;
use dosctl::DbusIntrospector;
use serde::{Deserialize, Serialize};

/// Coverage report per capability area (U087)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaCoverage {
    /// Area or domain name
    pub area: String,
    /// Number of capabilities covered in catalog
    pub in_catalog: usize,
    /// Number of uncovered potential OS endpoints
    pub uncovered_surface: usize,
    /// Coverage percentage (0.0 to 100.0)
    pub coverage_percent: f64,
}

/// Overall OS Introspection Coverage Report (U087)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    /// Total registered capabilities in catalog
    pub total_catalog_entries: usize,
    /// Total introspected D-Bus service endpoints
    pub total_dbus_services: usize,
    /// Area breakdowns
    pub areas: Vec<AreaCoverage>,
    /// Global coverage percentage
    pub overall_coverage_percent: f64,
}

/// Evaluator measuring catalog coverage against system surface (U087)
pub struct CoverageEvaluator;

impl CoverageEvaluator {
    /// Generate a coverage report comparing catalog entries to introspected D-Bus nodes
    pub fn evaluate(registry: &CatalogRegistry) -> CoverageReport {
        let dbus_nodes = DbusIntrospector::inspect_system();
        let total_dbus = dbus_nodes.len();
        let total_catalog = registry.len();

        let domains = ["network", "audio", "power", "display", "apps", "system"];
        let mut areas = Vec::new();

        for domain in domains {
            let in_cat = registry.by_domain(domain).len();
            // Estimate matching OS endpoints for the domain
            let related_dbus = dbus_nodes
                .iter()
                .filter(|n| n.service.to_lowercase().contains(domain))
                .count()
                .max(1);

            let percent = ((in_cat as f64) / ((in_cat + related_dbus) as f64)) * 100.0;

            areas.push(AreaCoverage {
                area: domain.to_string(),
                in_catalog: in_cat,
                uncovered_surface: related_dbus,
                coverage_percent: (percent * 10.0).round() / 10.0,
            });
        }

        let overall = if total_dbus > 0 {
            let cov = ((total_catalog as f64) / (total_catalog + total_dbus) as f64) * 100.0;
            (cov * 10.0).round() / 10.0
        } else {
            100.0
        };

        CoverageReport {
            total_catalog_entries: total_catalog,
            total_dbus_services: total_dbus,
            areas,
            overall_coverage_percent: overall,
        }
    }
}
