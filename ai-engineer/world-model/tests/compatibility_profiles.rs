//! Integration tests for Proton/Wine application compatibility profiles in World Model.

use world_model::{db, model::CompatibilityProfile, query, sources, sync};

#[test]
fn test_compatibility_profiles_parsers_json_and_line_protocol() {
    let json_data = r#"[
        {
            "app_id": "steam:730",
            "app_title": "Counter-Strike 2",
            "runner_version": "Proton-9.0",
            "tier": "Platinum",
            "dxvk_version": "2.4",
            "vkd3d_version": null,
            "env_overrides": "[\"DXVK_HUD=fps\"]",
            "dll_overrides": null,
            "notes": "Native Vulkan backend recommended"
        },
        {
            "app_id": "standalone:affinity_photo",
            "app_title": "Affinity Photo 2",
            "runner_version": "GE-Proton8-26",
            "tier": "Gold",
            "dxvk_version": "2.3.1",
            "vkd3d_version": "2.13",
            "env_overrides": "[\"WINE_FULLSCREEN_FSR=1\"]",
            "dll_overrides": "{\"d3d11\": \"native,builtin\"}",
            "notes": "Requires custom winetricks font pack"
        }
    ]"#;

    let profiles = sources::parse_compatibility_profiles(json_data);
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].app_id, "steam:730");
    assert_eq!(profiles[0].tier, "Platinum");
    assert_eq!(profiles[1].app_title, "Affinity Photo 2");
    assert_eq!(profiles[1].runner_version, "GE-Proton8-26");

    let line_data = "\
# Application compatibility manifest
COMPAT|standalone:notepad_plus_plus|Notepad++|Wine-Staging-9.0|Platinum|||||Runs out-of-the-box
COMPAT|standalone:vegas_pro|VEGAS Pro 21|GE-Proton9-1|Silver|2.4|2.13|[\"WINE_LARGE_ADDRESS_AWARE=1\"]|{\"mfplat\": \"native\"}|Media Foundation workaround required
";
    let line_profiles = sources::parse_compatibility_profiles(line_data);
    assert_eq!(line_profiles.len(), 2);
    assert_eq!(line_profiles[0].app_id, "standalone:notepad_plus_plus");
    assert_eq!(line_profiles[0].tier, "Platinum");
    assert_eq!(line_profiles[1].app_title, "VEGAS Pro 21");
    assert_eq!(line_profiles[1].tier, "Silver");
    assert_eq!(line_profiles[1].dxvk_version.as_deref(), Some("2.4"));
}

#[test]
fn test_compatibility_profiles_sync_and_query_roundtrip() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let conn = db::open(tmp.path()).unwrap();

    let profiles = vec![
        CompatibilityProfile {
            app_id: "steam:1091500".to_string(),
            app_title: "Cyberpunk 2077".to_string(),
            runner_version: "Proton-9.0".to_string(),
            tier: "Platinum".to_string(),
            dxvk_version: None,
            vkd3d_version: Some("2.13".to_string()),
            env_overrides: Some("[\"VKD3D_CONFIG=dxr11\"]".to_string()),
            dll_overrides: None,
            notes: Some("DirectX 12 Ray Tracing enabled".to_string()),
        },
        CompatibilityProfile {
            app_id: "epic:unreal_editor".to_string(),
            app_title: "Unreal Editor 5.4".to_string(),
            runner_version: "Wine-GE-8-26".to_string(),
            tier: "Gold".to_string(),
            dxvk_version: Some("2.4".to_string()),
            vkd3d_version: Some("2.13".to_string()),
            env_overrides: None,
            dll_overrides: Some("{\"dxgi\": \"native\"}".to_string()),
            notes: Some("Runs smoothly with Vulkan shader cache prewarmed".to_string()),
        },
    ];

    let count = sync::sync_compatibility_profiles(&conn, &profiles).unwrap();
    assert_eq!(count, 2);

    let found = query::compatibility_profile(&conn, "steam:1091500").unwrap();
    assert!(found.is_some());
    let cp = found.unwrap();
    assert_eq!(cp.app_title, "Cyberpunk 2077");
    assert_eq!(cp.tier, "Platinum");
    assert_eq!(cp.vkd3d_version.as_deref(), Some("2.13"));

    let missing = query::compatibility_profile(&conn, "steam:unknown_app").unwrap();
    assert!(missing.is_none());

    let all = query::all_compatibility_profiles(&conn).unwrap();
    assert_eq!(all.len(), 2);
    // Ordered by app_title: Cyberpunk 2077, Unreal Editor 5.4
    assert_eq!(all[0].app_title, "Cyberpunk 2077");
    assert_eq!(all[1].app_title, "Unreal Editor 5.4");

    let stats = query::graph_stats(&conn).unwrap();
    assert_eq!(stats.compatibility_profiles, 2);
}
