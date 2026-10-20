use dos_catalog::{
    BackendKind, CatalogEntry, CatalogLinter, CatalogLoader, CatalogRegistry, EntryStatus,
    LintErrorKind, Scope, TierHint,
};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

fn sample_valid_entry(id: &str, domain: &str) -> CatalogEntry {
    CatalogEntry {
        id: id.into(),
        domain: domain.into(),
        summary: "Set system master audio volume level".into(),
        params: json!({
            "type": "object",
            "required": ["volume_percent"],
            "properties": {
                "volume_percent": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 150
                }
            }
        }),
        scope: Scope::User,
        tier_hint: TierHint::Tier2UserNonDestructive,
        preview: true,
        undo: true,
        backend: BackendKind::Dbus,
        consent_level: 1,
        status: EntryStatus::Real,
        help_text: "Changes output audio level between 0% and 150% via PipeWire/WirePlumber".into(),
        gui_route: Some("settings/sound".into()),
    }
}

#[test]
fn test_valid_entry_passes_lint() {
    let entry = sample_valid_entry("audio.volume.set", "audio");
    let violations = CatalogLinter::lint_entry(&entry);
    assert!(
        violations.is_empty(),
        "Expected no violations, got: {violations:?}"
    );
}

#[test]
fn test_lint_rejects_missing_preview_on_mutating() {
    let mut entry = sample_valid_entry("audio.volume.set", "audio");
    entry.preview = false;
    let violations = CatalogLinter::lint_entry(&entry);
    assert_eq!(violations.len(), 1);
    assert!(matches!(
        violations[0].kind,
        LintErrorKind::MissingPreviewOnMutating(_)
    ));
}

#[test]
fn test_lint_rejects_missing_undo_on_mutating() {
    let mut entry = sample_valid_entry("audio.volume.set", "audio");
    entry.undo = false;
    let violations = CatalogLinter::lint_entry(&entry);
    assert_eq!(violations.len(), 1);
    assert!(matches!(
        violations[0].kind,
        LintErrorKind::MissingUndoOnMutating(_)
    ));
}

#[test]
fn test_lint_rejects_invalid_id_format() {
    let entry = sample_valid_entry("volume_set", "audio");
    let violations = CatalogLinter::lint_entry(&entry);
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.kind, LintErrorKind::InvalidIdFormat(_)))
    );
}

#[test]
fn test_lint_rejects_domain_mismatch() {
    let entry = sample_valid_entry("network.wifi.connect", "audio");
    let violations = CatalogLinter::lint_entry(&entry);
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.kind, LintErrorKind::DomainMismatch { .. }))
    );
}

#[test]
fn test_lint_rejects_invalid_consent_level() {
    let mut entry = sample_valid_entry("audio.volume.set", "audio");
    entry.consent_level = 9;
    let violations = CatalogLinter::lint_entry(&entry);
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.kind, LintErrorKind::InvalidConsentLevel(9)))
    );
}

#[test]
fn test_lint_rejects_invalid_params_schema() {
    let mut entry = sample_valid_entry("audio.volume.set", "audio");
    entry.params = json!("not-an-object");
    let violations = CatalogLinter::lint_entry(&entry);
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.kind, LintErrorKind::InvalidParamsSchema(_)))
    );
}

#[test]
fn test_loader_and_registry_integration() {
    let temp = tempdir().unwrap();
    let file_path = temp.path().join("audio.toml");
    let toml_content = r#"
domain = "audio"

[[entries]]
id = "audio.volume.set"
domain = "audio"
summary = "Set output volume level"
params = { type = "object", properties = { volume_percent = { type = "integer" } } }
scope = "user"
tier_hint = "tier2_user_non_destructive"
preview = true
undo = true
backend = "dbus"
consent_level = 1
status = "real"
help_text = "Adjusts output volume level"
gui_route = "settings/sound"

[[entries]]
id = "audio.volume.get"
domain = "audio"
summary = "Read current output volume"
params = { type = "object" }
scope = "session"
tier_hint = "tier1_read"
preview = false
undo = false
backend = "dbus"
consent_level = 0
status = "real"
help_text = "Queries PipeWire for current master audio volume"
"#;
    fs::write(&file_path, toml_content).unwrap();

    let entries = CatalogLoader::load_dir(temp.path()).unwrap();
    assert_eq!(entries.len(), 2);

    let registry = CatalogRegistry::from_entries(entries);
    assert_eq!(registry.len(), 2);

    let get_entry = registry
        .get("audio.volume.get")
        .expect("Should find audio.volume.get");
    assert_eq!(get_entry.scope, Scope::Session);
    assert_eq!(get_entry.tier_hint, TierHint::Tier1Read);

    let by_domain = registry.by_domain("audio");
    assert_eq!(by_domain.len(), 2);
}

#[test]
fn test_loader_rejects_duplicate_id() {
    let temp = tempdir().unwrap();
    let toml1 = r#"
domain = "audio"
[[entries]]
id = "audio.volume.set"
domain = "audio"
summary = "Set output volume"
params = { type = "object" }
scope = "user"
tier_hint = "tier2_user_non_destructive"
preview = true
undo = true
backend = "dbus"
consent_level = 1
status = "real"
help_text = "Volume"
"#;
    let toml2 = r#"
domain = "audio_alt"
[[entries]]
id = "audio.volume.set"
domain = "audio_alt"
summary = "Duplicate ID"
params = { type = "object" }
scope = "user"
tier_hint = "tier2_user_non_destructive"
preview = true
undo = true
backend = "dbus"
consent_level = 1
status = "real"
help_text = "Duplicate"
"#;
    fs::write(temp.path().join("a.toml"), toml1).unwrap();
    fs::write(temp.path().join("b.toml"), toml2).unwrap();

    let res = CatalogLoader::load_dir(temp.path());
    assert!(res.is_err(), "Expected error on duplicate ID");
}
