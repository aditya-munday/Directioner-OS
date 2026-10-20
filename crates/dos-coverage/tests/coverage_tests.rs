use dos_catalog::{BackendKind, CatalogEntry, CatalogRegistry, EntryStatus, Scope, TierHint};
use dos_coverage::CoverageEvaluator;

#[test]
fn test_coverage_evaluation_and_gate() {
    let mut reg = CatalogRegistry::new();
    reg.register(CatalogEntry {
        id: "network.wifi.connect".into(),
        domain: "network".into(),
        summary: "Connect wifi".into(),
        params: serde_json::json!({}),
        scope: Scope::User,
        tier_hint: TierHint::Tier2UserNonDestructive,
        preview: true,
        undo: true,
        backend: BackendKind::Dbus,
        consent_level: 1,
        status: EntryStatus::Real,
        help_text: "Connect to wifi network".into(),
        gui_route: None,
    });

    let report = CoverageEvaluator::evaluate(&reg);
    assert_eq!(report.total_catalog_entries, 1);
    assert!(report.total_dbus_services > 0);
    assert!(report.overall_coverage_percent > 0.0);

    let net_area = report.areas.iter().find(|a| a.area == "network").unwrap();
    assert_eq!(net_area.in_catalog, 1);
}
