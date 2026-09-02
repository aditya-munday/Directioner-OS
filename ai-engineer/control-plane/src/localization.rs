//! Localization and user-facing message catalog scaffolding for Directioner-OS.
//!
//! Provides structured string extraction, parameter interpolation, and multi-locale catalog
//! resolution for CLI tooling, diagnostic summaries, and operator alerts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Strongly-typed string message catalog for a specific locale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCatalog {
    /// BCP-47 locale code (e.g., "en-US", "es-ES", "de-DE", "ja-JP").
    pub locale: String,
    /// Key-value message translation table.
    pub messages: HashMap<String, String>,
}

impl Default for MessageCatalog {
    fn default() -> Self {
        Self::default_en_us()
    }
}

impl MessageCatalog {
    /// Returns the built-in default English (US) message catalog.
    pub fn default_en_us() -> Self {
        let mut messages = HashMap::new();

        // General CLI banners and headers
        messages.insert(
            "cli.banner.ai_engineer".into(),
            "Directioner-OS AI Engineer CLI (Phase 1, 2, 3 & 4 Control Plane)".into(),
        );
        messages.insert(
            "cli.banner.cockpit".into(),
            "Directioner-OS Operational Cockpit (Phase 3 Dashboard)".into(),
        );
        messages.insert(
            "cli.banner.health".into(),
            "Directioner-OS Health Model Evaluator (Phase 3.0.2)".into(),
        );
        messages.insert(
            "cli.banner.hwcheck".into(),
            "DIRECTIONER-OS HARDWARE COMPATIBILITY DIAGNOSTIC".into(),
        );
        messages.insert(
            "cli.banner.worklog".into(),
            "Directioner-OS Capability Audit Work Log".into(),
        );

        // Operational alerts
        messages.insert(
            "alert.rollback_failure".into(),
            "🚨 CRITICAL SYSTEM ALERT: COMPENSATING ROLLBACK FAILURE DETECTED".into(),
        );
        messages.insert(
            "alert.action_required".into(),
            "ACTION REQUIRED: System state may be dirty or inconsistent. Inspect immediately."
                .into(),
        );
        messages.insert(
            "alert.killswitch_engaged".into(),
            "KillSwitch engaged: Evaluation and dispatches halted.".into(),
        );
        messages.insert(
            "alert.self_healing_disabled".into(),
            "Self-healing disabled by configuration (enabled = false in {config_path}).".into(),
        );
        messages.insert(
            "alert.remediation_blocked".into(),
            "Remediation plan BLOCKED by Reviewer Agent: {reason}".into(),
        );

        // Reviewer verdicts & findings
        messages.insert(
            "reviewer.verdict.approved".into(),
            "✔ Plan Approved: No safety violations detected.".into(),
        );
        messages.insert("reviewer.verdict.needs_review".into(), "▲ Plan Requires Operator Confirmation: Blast radius score {score}/100 exceeds automatic threshold.".into());
        messages.insert(
            "reviewer.verdict.rejected".into(),
            "✘ Plan Rejected: Fatal safety invariant violation.".into(),
        );

        // Hardware diagnostics
        messages.insert(
            "hwcheck.status.supported".into(),
            "All hardware subsystems match known-good upstream Fedora bootc configurations.".into(),
        );
        messages.insert("hwcheck.wifi.broadcom_warning".into(), "Broadcom WiFi adapter [{slot}] requires broadcom-wl kmod. Layer into Containerfile via RPM Fusion.".into());
        messages.insert("hwcheck.wifi.realtek_warning".into(), "Realtek RTL8821CE [{slot}] requires out-of-tree akmod. Consider replacing with Intel AX210 M.2 card.".into());
        messages.insert("hwcheck.fingerprint.tod_warning".into(), "Goodix sensor [{usb_id}] requires proprietary libfprint-tod binary engine. Install via vendor container layer.".into());
        messages.insert("hwcheck.sleep.apst_warning".into(), "NVMe APST deep latency issue detected under s2idle. Add nvme_core.default_ps_max_latency_us=0 to kernel boot arguments.".into());

        // Conversational shell
        messages.insert("conversational.shell.draft_notice".into(), "Directioner-OS Conversational Operator Shell (Phase 4.0.1 Draft-Only Proposal Preview)".into());
        messages.insert(
            "conversational.shell.no_execution".into(),
            "Plans are generated as drafts. Direct execution is not wired in Phase 4.0.1.".into(),
        );

        // Application compatibility
        messages.insert(
            "compat.profile.found".into(),
            "{title} ({app_id}): tier={tier} runner={runner} dxvk={dxvk} vkd3d={vkd3d}".into(),
        );
        messages.insert(
            "compat.profile.missing".into(),
            "No compatibility profile registered for application ID: {app_id}".into(),
        );

        Self {
            locale: "en-US".into(),
            messages,
        }
    }

    /// Loads a message catalog from a JSON string.
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        serde_json::from_str::<Self>(json_str)
            .map_err(|e| format!("Failed to parse message catalog JSON: {e}"))
    }

    /// Serializes the message catalog to pretty-printed JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Looks up a raw translation string by message key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.messages.get(key).map(|s| s.as_str())
    }

    /// Formats a translated string with named parameter interpolation `{param}`.
    pub fn format(&self, key: &str, params: &[(&str, &str)]) -> String {
        if let Some(template) = self.get(key) {
            let mut formatted = template.to_string();
            for (param, val) in params {
                let placeholder = format!("{{{param}}}");
                formatted = formatted.replace(&placeholder, val);
            }
            formatted
        } else {
            // Fallback: return key and serialized params
            format!("[missing translation: {key}]")
        }
    }
}
