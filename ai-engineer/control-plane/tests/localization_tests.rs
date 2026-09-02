//! Unit and integration test suite for localization and message catalog resolution.

use control_plane::localization::MessageCatalog;

#[test]
fn test_default_en_us_catalog_key_coverage() {
    let catalog = MessageCatalog::default_en_us();
    assert_eq!(catalog.locale, "en-US");

    let required_keys = [
        "cli.banner.ai_engineer",
        "cli.banner.cockpit",
        "cli.banner.health",
        "cli.banner.hwcheck",
        "cli.banner.worklog",
        "alert.rollback_failure",
        "alert.action_required",
        "alert.killswitch_engaged",
        "alert.self_healing_disabled",
        "alert.remediation_blocked",
        "reviewer.verdict.approved",
        "reviewer.verdict.needs_review",
        "reviewer.verdict.rejected",
        "hwcheck.status.supported",
        "hwcheck.wifi.broadcom_warning",
        "hwcheck.wifi.realtek_warning",
        "hwcheck.fingerprint.tod_warning",
        "hwcheck.sleep.apst_warning",
        "conversational.shell.draft_notice",
        "conversational.shell.no_execution",
        "compat.profile.found",
        "compat.profile.missing",
    ];

    for key in required_keys {
        assert!(
            catalog.get(key).is_some(),
            "Missing translation key '{key}' in default en-US catalog"
        );
    }
}

#[test]
fn test_message_catalog_parameter_interpolation() {
    let catalog = MessageCatalog::default_en_us();

    // 1. Single parameter interpolation
    let msg1 = catalog.format("hwcheck.wifi.broadcom_warning", &[("slot", "0000:03:00.0")]);
    assert!(msg1.contains("Broadcom WiFi adapter [0000:03:00.0] requires broadcom-wl kmod."));

    // 2. Multiple parameter interpolation
    let msg2 = catalog.format(
        "compat.profile.found",
        &[
            ("title", "Cyberpunk 2077"),
            ("app_id", "steam:1091500"),
            ("tier", "Platinum"),
            ("runner", "Proton-9.0"),
            ("dxvk", "2.4"),
            ("vkd3d", "2.13"),
        ],
    );
    assert_eq!(
        msg2,
        "Cyberpunk 2077 (steam:1091500): tier=Platinum runner=Proton-9.0 dxvk=2.4 vkd3d=2.13"
    );

    // 3. Fallback on missing key
    let msg_missing = catalog.format("nonexistent.key.xyz", &[("foo", "bar")]);
    assert_eq!(msg_missing, "[missing translation: nonexistent.key.xyz]");
}

#[test]
fn test_message_catalog_json_roundtrip_and_custom_locale() {
    let original = MessageCatalog::default_en_us();
    let json_str = original.to_json();
    let restored = MessageCatalog::from_json(&json_str).expect("Failed to deserialize catalog");

    assert_eq!(restored.locale, "en-US");
    assert_eq!(restored.messages.len(), original.messages.len());

    // Custom Spanish locale
    let es_json = r#"{
        "locale": "es-ES",
        "messages": {
            "cli.banner.ai_engineer": "Línea de Control de IA de Directioner-OS",
            "alert.rollback_failure": "🚨 ALERTA CRÍTICA DEL SISTEMA: FALLO EN REVERSIÓN DETECTADO"
        }
    }"#;

    let es_catalog = MessageCatalog::from_json(es_json).expect("Failed to parse Spanish catalog");
    assert_eq!(es_catalog.locale, "es-ES");
    assert_eq!(
        es_catalog.get("cli.banner.ai_engineer"),
        Some("Línea de Control de IA de Directioner-OS")
    );
    assert_eq!(
        es_catalog.get("alert.rollback_failure"),
        Some("🚨 ALERTA CRÍTICA DEL SISTEMA: FALLO EN REVERSIÓN DETECTADO")
    );
}
