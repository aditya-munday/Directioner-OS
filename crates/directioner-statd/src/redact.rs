use crate::tree::{SensitivityLevel, StateTree};
use std::collections::HashMap;

/// Redaction engine for sanitizing state tree nodes based on caller consent level (U069)
pub struct RedactionEngine;

impl RedactionEngine {
    /// Mask string containing potential personal identifiers
    pub fn sanitize_string(input: &str, consent_level: u8) -> String {
        if consent_level >= 2 {
            return input.to_string();
        }

        let mut output = input.to_string();

        // 1. Redact home directory paths (/home/<user>/... -> /home/<redacted>/...)
        if let Some(home_pos) = output.find("/home/") {
            let after_home = &output[home_pos + 6..];
            if let Some(slash_pos) = after_home.find('/') {
                let username = &after_home[..slash_pos];
                if !username.is_empty() {
                    output = output.replace(&format!("/home/{username}"), "/home/[REDACTED_USER]");
                }
            } else if !after_home.is_empty() {
                output = "/home/[REDACTED_USER]".to_string();
            }
        }

        // 2. Redact IP addresses (simple IPv4 pattern check)
        let words: Vec<String> = output
            .split_whitespace()
            .map(|s| {
                s.trim_matches(|c: char| !c.is_alphanumeric() && c != '.')
                    .to_string()
            })
            .collect();
        for cleaned in words {
            let parts: Vec<&str> = cleaned.split('.').collect();
            if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
                output = output.replace(&cleaned, "[REDACTED_IP]");
            }
        }

        output
    }

    /// Recursively sanitize JSON values
    pub fn sanitize_value(val: &serde_json::Value, consent_level: u8) -> serde_json::Value {
        match val {
            serde_json::Value::String(s) => {
                serde_json::Value::String(Self::sanitize_string(s, consent_level))
            }
            serde_json::Value::Array(arr) => serde_json::Value::Array(
                arr.iter()
                    .map(|v| Self::sanitize_value(v, consent_level))
                    .collect(),
            ),
            serde_json::Value::Object(map) => {
                let mut out = serde_json::Map::new();
                for (k, v) in map {
                    // Specific key redactions for level < 2
                    if consent_level < 2 {
                        let lower = k.to_lowercase();
                        if lower.contains("ssid") || lower.contains("bssid") {
                            out.insert(k.clone(), serde_json::json!("[REDACTED_SSID]"));
                            continue;
                        }
                        if lower.contains("mac") || lower.contains("hwaddr") {
                            out.insert(k.clone(), serde_json::json!("[REDACTED_MAC]"));
                            continue;
                        }
                        if lower.contains("ip") || lower.contains("address") {
                            out.insert(k.clone(), serde_json::json!("[REDACTED_IP]"));
                            continue;
                        }
                        if lower.contains("secret")
                            || lower.contains("key")
                            || lower.contains("token")
                            || lower.contains("password")
                        {
                            continue; // Omitted entirely
                        }
                    }
                    out.insert(k.clone(), Self::sanitize_value(v, consent_level));
                }
                serde_json::Value::Object(out)
            }
            other => other.clone(),
        }
    }

    /// Filter and redact an entire StateTree for a caller with `consent_level`
    pub fn filter_tree(tree: &StateTree, consent_level: u8) -> StateTree {
        let max_sensitivity = match consent_level {
            0 => SensitivityLevel::Public,
            1 => SensitivityLevel::DeviceState,
            _ => SensitivityLevel::Personal,
        };

        let mut filtered_nodes = HashMap::new();
        for (path, node) in &tree.nodes {
            // Level 3 (Secret) is NEVER exposed under any circumstances
            if node.sensitivity == SensitivityLevel::Secret {
                continue;
            }

            // Exclude nodes above caller's authorized sensitivity level
            if node.sensitivity > max_sensitivity {
                continue;
            }

            let sanitized_val = Self::sanitize_value(&node.value, consent_level);
            let mut sanitized_node = node.clone();
            sanitized_node.value = sanitized_val;
            filtered_nodes.insert(path.clone(), sanitized_node);
        }

        StateTree {
            version: tree.version,
            timestamp: tree.timestamp,
            nodes: filtered_nodes,
        }
    }
}
