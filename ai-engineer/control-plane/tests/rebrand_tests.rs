// Directioner OS — Rebrand tests (Domain 6, TODOs 087, 088, 089, 092, 099)

use std::io::Write;
use std::process::Command;

use control_plane::brand_identity::{BrandIdentity, parse_os_release, render_os_release};

fn write_temp(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

// ── os-release rendering ──────────────────────────────────────────────────────

#[test]
fn rendered_os_release_id_is_directioner() {
    let s = render_os_release(
        "Directioner OS",
        "directioner",
        "42.0.0 (Genesis)",
        "42.0.0",
        "Directioner OS 42 (Genesis)",
        "server",
        "b1",
        "img1",
        "https://example.com",
        "https://example.com",
        "https://example.com",
    );
    let map = parse_os_release(&s);
    assert_eq!(map["ID"], "directioner");
}

#[test]
fn rendered_os_release_has_id_like_fedora() {
    let s = render_os_release(
        "Directioner OS",
        "directioner",
        "42.0.0 (Genesis)",
        "42.0.0",
        "Directioner OS 42 (Genesis)",
        "server",
        "b1",
        "img1",
        "https://example.com",
        "https://example.com",
        "https://example.com",
    );
    assert!(
        s.contains("ID_LIKE=fedora"),
        "ID_LIKE=fedora must be present for dnf compat"
    );
}

#[test]
fn rendered_os_release_name_not_fedora_linux() {
    let s = render_os_release(
        "Directioner OS",
        "directioner",
        "42.0.0 (Genesis)",
        "42.0.0",
        "Directioner OS 42 (Genesis)",
        "server",
        "b1",
        "img1",
        "https://example.com",
        "https://example.com",
        "https://example.com",
    );
    let map = parse_os_release(&s);
    assert_ne!(map.get("NAME").map(|s| s.as_str()), Some("Fedora Linux"));
    assert!(
        !map.get("PRETTY_NAME")
            .unwrap_or(&String::new())
            .contains("Fedora Linux")
    );
}

#[test]
fn rendered_os_release_has_required_fields() {
    let s = render_os_release(
        "Directioner OS",
        "directioner",
        "42.0.0 (Genesis)",
        "42.0.0",
        "Directioner OS 42 (Genesis)",
        "desktop",
        "build-42",
        "img-42",
        "https://github.com/aditya-munday/Directioner-OS",
        "https://github.com/aditya-munday/Directioner-OS/issues",
        "https://github.com/aditya-munday/Directioner-OS/issues",
    );
    let map = parse_os_release(&s);
    for field in &[
        "NAME",
        "ID",
        "VERSION_ID",
        "PRETTY_NAME",
        "VARIANT_ID",
        "BUILD_ID",
    ] {
        assert!(map.contains_key(*field), "Missing required field: {field}");
    }
}

// ── BrandIdentity from os-release ────────────────────────────────────────────

#[test]
fn brand_identity_loads_and_validates() {
    let content = concat!(
        "NAME=\"Directioner OS\"\n",
        "ID=directioner\n",
        "VERSION=\"42.0.0 (Genesis)\"\n",
        "VERSION_ID=42.0.0\n",
        "PRETTY_NAME=\"Directioner OS 42 (Genesis)\"\n",
        "VARIANT_ID=server\n",
        "BUILD_ID=test-build-99\n",
        "IMAGE_ID=test-img-99\n",
        "HOME_URL=\"https://github.com/aditya-munday/Directioner-OS\"\n",
        "SUPPORT_URL=\"https://github.com/aditya-munday/Directioner-OS/issues\"\n",
        "BUG_REPORT_URL=\"https://github.com/aditya-munday/Directioner-OS/issues\"\n",
    );
    let brand = BrandIdentity::from_content(content);
    brand.assert_is_directioner();
    assert_eq!(brand.build_id, "test-build-99");
    assert_eq!(brand.image_id, "test-img-99");
}

// ── --version format (TODO 099) ───────────────────────────────────────────────

#[test]
fn version_line_matches_golden_format() {
    let content = concat!(
        "NAME=\"Directioner OS\"\n",
        "ID=directioner\n",
        "VERSION_ID=42.0.0\n",
        "BUILD_ID=abc1234\n",
    );
    let brand = BrandIdentity::from_content(content);
    let line = brand.version_line("dos-account", "1.0.0", 1);

    let lines: Vec<&str> = line.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines[0].starts_with("dos-account 1.0.0 (build abc1234; contract v1)"),
        "First line format wrong: {}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("Directioner OS 42.0.0"),
        "Second line must be 'Directioner OS <version>': {}",
        lines[1]
    );
}

#[test]
fn version_line_varies_by_contract_version() {
    let content = concat!(
        "NAME=\"Directioner OS\"\n",
        "ID=directioner\n",
        "VERSION_ID=42.0.0\n",
        "BUILD_ID=xyz\n",
    );
    let brand = BrandIdentity::from_content(content);
    let v1 = brand.version_line("tool", "0.1.0", 1);
    let v2 = brand.version_line("tool", "0.1.0", 2);
    assert!(v1.contains("contract v1"));
    assert!(v2.contains("contract v2"));
    assert_ne!(v1, v2);
}

// ── brand.toml validator (TODO 092) ───────────────────────────────────────────

#[test]
fn brand_toml_validator_passes_on_valid_file() {
    let validator = repo_root().join("scripts/validate_brand_toml.py");
    let brand_toml = repo_root().join("brand.toml");

    if !validator.exists() || !brand_toml.exists() {
        eprintln!("Skipping brand.toml validator test — files not found");
        return;
    }

    let output = Command::new("python3")
        .arg(&validator)
        .arg("--file")
        .arg(&brand_toml)
        .output()
        .expect("Failed to run brand.toml validator");

    assert!(
        output.status.success(),
        "brand.toml validation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn brand_toml_validator_rejects_bad_channel() {
    // Write a bad brand.toml to a temp file (avoid hex literal issues by using concat!)
    let bad_toml = concat!(
        "[brand]\n",
        "name = \"Directioner OS\"\n",
        "short_name = \"Directioner\"\n",
        "id = \"directioner\"\n",
        "vendor = \"Directioner OS Project\"\n",
        "tagline = \"The AI-Native Operating System\"\n",
        "\n",
        "[version]\n",
        "major = 42\n",
        "minor = 0\n",
        "patch = 0\n",
        "channel = \"nightly-invalid\"\n",
        "version_string = \"42.0.0\"\n",
        "fedora_base_version = \"42\"\n",
        "\n",
        "[urls]\n",
        "homepage = \"https://example.com\"\n",
        "documentation = \"https://example.com\"\n",
        "support = \"https://example.com\"\n",
        "bug_report = \"https://example.com\"\n",
        "\n",
        "[theme]\n",
        "primary_color = \"#00D2FF\"\n",
        "accent_color = \"#3A7BD5\"\n",
        "background_dark = \"#0A0E17\"\n",
        "background_light = \"#F4F7FB\"\n",
    );
    let f = write_temp(bad_toml);
    let validator = repo_root().join("scripts/validate_brand_toml.py");

    if !validator.exists() {
        eprintln!("Skipping — validator script not found");
        return;
    }

    let output = Command::new("python3")
        .arg(&validator)
        .arg("--file")
        .arg(f.path())
        .output()
        .expect("Failed to run validator");

    assert!(
        !output.status.success(),
        "Validator should REJECT invalid channel 'nightly-invalid'"
    );
}

// ── token generator determinism (TODO 078) ───────────────────────────────────

#[test]
fn token_generator_is_deterministic() {
    let token_gen = repo_root().join("scripts/generate_brand_tokens.py");

    if !token_gen.exists() {
        eprintln!("Skipping — generate_brand_tokens.py not found");
        return;
    }

    let out1 = Command::new("python3")
        .arg(&token_gen)
        .current_dir(repo_root())
        .output()
        .expect("Token generator run 1 failed");
    assert!(
        out1.status.success(),
        "Token generator failed (run 1):\n{}",
        String::from_utf8_lossy(&out1.stderr)
    );

    let out2 = Command::new("python3")
        .arg(&token_gen)
        .arg("--check")
        .current_dir(repo_root())
        .output()
        .expect("Token generator --check failed");
    assert!(
        out2.status.success(),
        "Token generator --check failed (not deterministic):\n{}",
        String::from_utf8_lossy(&out2.stderr)
    );
}

// ── rebrand lint (TODO 087) ───────────────────────────────────────────────────

#[test]
fn rebrand_lint_passes_on_source_tree() {
    let lint = repo_root().join("scripts/rebrand_lint.py");
    if !lint.exists() {
        eprintln!("Skipping — rebrand_lint.py not found");
        return;
    }

    let output = Command::new("python3")
        .arg(&lint)
        .arg("--root")
        .arg(repo_root())
        .arg("--counts-only")
        .output()
        .expect("rebrand_lint.py failed to run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let count: u32 = stdout
        .lines()
        .find(|l| l.starts_with("violations:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);

    assert_eq!(
        count, 0,
        "rebrand_lint found {count} unallowlisted 'Fedora' strings in source tree"
    );
}

#[test]
fn rebrand_lint_detects_planted_string() {
    let lint = repo_root().join("scripts/rebrand_lint.py");
    if !lint.exists() {
        eprintln!("Skipping — rebrand_lint.py not found");
        return;
    }

    let tmpdir = tempfile::tempdir().unwrap();
    let planted = tmpdir.path().join("planted_test.txt");
    std::fs::write(&planted, "This OS is powered by Fedora\n").unwrap();

    let output = Command::new("python3")
        .arg(&lint)
        .arg("--root")
        .arg(tmpdir.path())
        .arg("--counts-only")
        .output()
        .expect("rebrand_lint.py failed to run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let count: u32 = stdout
        .lines()
        .find(|l| l.starts_with("violations:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);

    assert!(
        count >= 1,
        "rebrand_lint must detect planted 'Fedora' string (got {count})"
    );
    assert!(
        !output.status.success(),
        "rebrand_lint must exit non-zero on violations"
    );
}
