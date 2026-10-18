// Directioner OS — Brand Identity Module (Domain 6, TODO 054/091/092)
// Single source of truth for brand constants at runtime.
// Reads from /etc/os-release (or build-time env vars in tests).
//
// Used by: dos-account --version, directioner-report, fastfetch hook,
//          About panel, unit startup messages, MOTD generator.

use std::collections::HashMap;
use std::fs;

/// Directioner OS brand identity, loaded from /etc/os-release.
#[derive(Debug, Clone)]
pub struct BrandIdentity {
    pub name: String,
    pub id: String,
    pub version: String,
    pub version_id: String,
    pub pretty_name: String,
    pub variant_id: String,
    pub build_id: String,
    pub image_id: String,
    pub home_url: String,
    pub support_url: String,
    pub bug_report_url: String,
}

impl BrandIdentity {
    /// Load from /etc/os-release (or TEST_OS_RELEASE env var for tests).
    pub fn load() -> Self {
        let path =
            std::env::var("TEST_OS_RELEASE").unwrap_or_else(|_| "/etc/os-release".to_string());
        Self::from_path(path)
    }

    /// Load brand identity from a specified file path.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Self {
        let content = fs::read_to_string(path).unwrap_or_default();
        Self::from_content(&content)
    }

    /// Construct brand identity directly from os-release format string.
    pub fn from_content(content: &str) -> Self {
        let map = parse_os_release(content);

        BrandIdentity {
            name: map
                .get("NAME")
                .cloned()
                .unwrap_or_else(|| "Directioner OS".to_string()),
            id: map
                .get("ID")
                .cloned()
                .unwrap_or_else(|| "directioner".to_string()),
            version: map
                .get("VERSION")
                .cloned()
                .unwrap_or_else(|| "42.0.0 (Genesis)".to_string()),
            version_id: map
                .get("VERSION_ID")
                .cloned()
                .unwrap_or_else(|| "42.0.0".to_string()),
            pretty_name: map
                .get("PRETTY_NAME")
                .cloned()
                .unwrap_or_else(|| "Directioner OS 42 (Genesis)".to_string()),
            variant_id: map
                .get("VARIANT_ID")
                .cloned()
                .unwrap_or_else(|| "server".to_string()),
            build_id: map
                .get("BUILD_ID")
                .cloned()
                .unwrap_or_else(|| "dev".to_string()),
            image_id: map
                .get("IMAGE_ID")
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
            home_url: map
                .get("HOME_URL")
                .cloned()
                .unwrap_or_else(|| "https://github.com/aditya-munday/Directioner-OS".to_string()),
            support_url: map.get("SUPPORT_URL").cloned().unwrap_or_else(|| {
                "https://github.com/aditya-munday/Directioner-OS/issues".to_string()
            }),
            bug_report_url: map.get("BUG_REPORT_URL").cloned().unwrap_or_else(|| {
                "https://github.com/aditya-munday/Directioner-OS/issues".to_string()
            }),
        }
    }

    /// Format a --version line for a given binary name and contract version.
    /// Format: "<name> <ver> (build <build_id>; contract v<contract>)\nDirectioner OS <version>"
    pub fn version_line(
        &self,
        binary_name: &str,
        binary_version: &str,
        contract_version: u32,
    ) -> String {
        format!(
            "{} {} (build {}; contract v{})\n{} {}",
            binary_name,
            binary_version,
            self.build_id,
            contract_version,
            self.name,
            self.version_id,
        )
    }

    /// One-line summary for MOTD / fastfetch context.
    pub fn motd_summary(&self) -> String {
        format!(
            "{} — Image: {} | Build: {} | Variant: {}",
            self.pretty_name, self.image_id, self.build_id, self.variant_id
        )
    }

    /// Assert the OS ID is "directioner" — panics if not.
    /// Allows CI to catch accidental Fedora identity slipping through.
    pub fn assert_is_directioner(&self) {
        assert_eq!(
            self.id, "directioner",
            "os-release ID must be 'directioner', got '{}'",
            self.id
        );
        assert!(
            !self.name.to_lowercase().contains("fedora"),
            "os-release NAME must not contain 'fedora', got '{}'",
            self.name
        );
        assert!(
            !self.pretty_name.to_lowercase().contains("fedora linux"),
            "os-release PRETTY_NAME must not say 'Fedora Linux', got '{}'",
            self.pretty_name
        );
    }
}

/// Parse /etc/os-release key=value format into a HashMap.
pub fn parse_os_release(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let val = val.trim_matches('"').trim_matches('\'');
            map.insert(key.to_string(), val.to_string());
        }
    }
    map
}

/// Generate the content for /etc/os-release from brand parameters.
/// Used by directioner-release package build and tests.
#[allow(clippy::too_many_arguments)]
pub fn render_os_release(
    name: &str,
    id: &str,
    version: &str,
    version_id: &str,
    pretty_name: &str,
    variant_id: &str,
    build_id: &str,
    image_id: &str,
    home_url: &str,
    support_url: &str,
    bug_report_url: &str,
) -> String {
    format!(
        r#"NAME="{name}"
VERSION="{version}"
ID={id}
ID_LIKE=fedora
VERSION_ID={version_id}
PLATFORM_ID="platform:dos42"
PRETTY_NAME="{pretty_name}"
ANSI_COLOR="0;34"
LOGO=directioner-logo
CPE_NAME="cpe:/o:directioneros:directioner:{version_id}"
DEFAULT_HOSTNAME="directioner"
HOME_URL="{home_url}"
DOCUMENTATION_URL="{home_url}#readme"
SUPPORT_URL="{support_url}"
BUG_REPORT_URL="{bug_report_url}"
VARIANT_ID={variant_id}
BUILD_ID={build_id}
IMAGE_ID={image_id}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn _write_temp_os_release(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        let _ = f.write_all(content.as_bytes());
        f
    }

    #[test]
    fn test_parse_os_release_basic() {
        let content = r#"
NAME="Directioner OS"
ID=directioner
VERSION_ID=42.0.0
PRETTY_NAME="Directioner OS 42 (Genesis)"
BUILD_ID=abc123
"#;
        let map = parse_os_release(content);
        assert_eq!(map["NAME"], "Directioner OS");
        assert_eq!(map["ID"], "directioner");
        assert_eq!(map["VERSION_ID"], "42.0.0");
    }

    #[test]
    fn test_brand_identity_loads() {
        let content = r#"NAME="Directioner OS"
ID=directioner
VERSION="42.0.0 (Genesis)"
VERSION_ID=42.0.0
PRETTY_NAME="Directioner OS 42 (Genesis)"
VARIANT_ID=server
BUILD_ID=test-build
IMAGE_ID=test-image
HOME_URL="https://github.com/aditya-munday/Directioner-OS"
SUPPORT_URL="https://github.com/aditya-munday/Directioner-OS/issues"
BUG_REPORT_URL="https://github.com/aditya-munday/Directioner-OS/issues"
"#;
        let brand = BrandIdentity::from_content(content);
        brand.assert_is_directioner();
        assert_eq!(brand.variant_id, "server");
        assert_eq!(brand.build_id, "test-build");
    }

    #[test]
    fn test_version_line_format() {
        let content = r#"NAME="Directioner OS"
ID=directioner
VERSION_ID=42.0.0
BUILD_ID=abc123
"#;
        let brand = BrandIdentity::from_content(content);
        let vline = brand.version_line("dos-account", "1.0.0", 1);
        assert!(vline.starts_with("dos-account 1.0.0 (build abc123; contract v1)"));
        assert!(vline.contains("Directioner OS 42.0.0"));
    }

    #[test]
    fn test_render_os_release_no_fedora_linux() {
        let rendered = render_os_release(
            "Directioner OS",
            "directioner",
            "42.0.0 (Genesis)",
            "42.0.0",
            "Directioner OS 42 (Genesis)",
            "server",
            "build123",
            "img456",
            "https://github.com/aditya-munday/Directioner-OS",
            "https://github.com/aditya-munday/Directioner-OS/issues",
            "https://github.com/aditya-munday/Directioner-OS/issues",
        );
        // NAME must not say "Fedora Linux"
        assert!(
            !rendered.contains("Fedora Linux"),
            "os-release must not say 'Fedora Linux'"
        );
        // ID must be directioner
        assert!(rendered.contains("ID=directioner"));
        // ID_LIKE=fedora IS expected (technical compat field)
        assert!(rendered.contains("ID_LIKE=fedora"));
        // PRETTY_NAME must not say "Fedora"
        let pretty = rendered
            .lines()
            .find(|l| l.starts_with("PRETTY_NAME="))
            .unwrap_or("");
        assert!(
            !pretty.to_lowercase().contains("fedora linux"),
            "PRETTY_NAME must not say 'Fedora Linux'"
        );
    }

    #[test]
    #[should_panic(expected = "ID must be 'directioner'")]
    fn test_assert_is_directioner_fails_on_fedora() {
        let mut brand = BrandIdentity::load();
        brand.id = "fedora".to_string();
        brand.assert_is_directioner();
    }
}
