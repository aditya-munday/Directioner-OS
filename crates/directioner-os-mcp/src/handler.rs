use crate::protocol::McpTool;
use directioner_actd::PreviewEngine;
use dos_catalog::{CatalogLoader, CatalogRegistry};
use dosctl::DbusIntrospector;

/// Core request handler for the Directioner OS MCP server (U082, U083)
pub struct McpHandler {
    registry: CatalogRegistry,
}

impl Default for McpHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl McpHandler {
    /// Initialize handler with loaded catalog
    pub fn new() -> Self {
        let entries = CatalogLoader::load_dir("catalog").unwrap_or_default();
        Self {
            registry: CatalogRegistry::from_entries(entries),
        }
    }

    /// List all exposed MCP tools
    pub fn list_tools(&self) -> Vec<McpTool> {
        vec![
            McpTool {
                name: "os_observe".into(),
                description: "Read system state telemetry from Directioner OS (read-only)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Optional specific state node path, e.g. 'system.os.info'"
                        }
                    }
                }),
            },
            McpTool {
                name: "os_catalog_list".into(),
                description: "List all capabilities registered in the Directioner OS Capability Catalog".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "domain": {
                            "type": "string",
                            "description": "Optional filter by domain (e.g. 'network', 'audio', 'power')"
                        }
                    }
                }),
            },
            McpTool {
                name: "os_catalog_show".into(),
                description: "Inspect the schema and parameter specification of a specific capability".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Canonical capability ID, e.g. 'audio.volume.set'"
                        }
                    }
                }),
            },
            McpTool {
                name: "os_preview_action".into(),
                description: "Generate a dry-run preview plan for a capability without modifying state (U074)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "required": ["capability_id", "params"],
                    "properties": {
                        "capability_id": {
                            "type": "string",
                            "description": "Target capability ID"
                        },
                        "params": {
                            "type": "object",
                            "description": "Proposed action parameters"
                        }
                    }
                }),
            },
            McpTool {
                name: "os_dbus_tree".into(),
                description: "Inspect system D-Bus services and interfaces for discovering uncataloged capabilities (U085)".into(),
                input_schema: serde_json::json!({
                    "type": "object"
                }),
            },
        ]
    }

    /// Execute a tool call
    pub fn call_tool(
        &self,
        name: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match name {
            "os_observe" => {
                // Return public system facts
                Ok(serde_json::json!({
                    "system": {
                        "os": "Directioner OS",
                        "version": "42",
                        "status": "active"
                    }
                }))
            }
            "os_catalog_list" => {
                let domain = params.get("domain").and_then(|d| d.as_str());
                let entries = match domain {
                    Some(d) => self.registry.by_domain(d),
                    None => self.registry.all_entries(),
                };
                let list: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "id": e.id,
                            "domain": e.domain,
                            "summary": e.summary,
                            "scope": e.scope,
                            "tier_hint": e.tier_hint,
                            "preview": e.preview,
                            "undo": e.undo,
                        })
                    })
                    .collect();
                Ok(serde_json::json!({ "capabilities": list }))
            }
            "os_catalog_show" => {
                let id = params
                    .get("id")
                    .and_then(|i| i.as_str())
                    .ok_or_else(|| "Missing required parameter 'id'".to_string())?;

                match self.registry.get(id) {
                    Some(entry) => Ok(serde_json::to_value(entry).unwrap()),
                    None => Err(format!("Capability '{id}' not found in catalog")),
                }
            }
            "os_preview_action" => {
                let cap_id = params
                    .get("capability_id")
                    .and_then(|i| i.as_str())
                    .ok_or_else(|| "Missing required parameter 'capability_id'".to_string())?;

                let action_params = params
                    .get("params")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));

                let plan = PreviewEngine::generate_preview(cap_id, &action_params, None);
                Ok(serde_json::to_value(plan).unwrap())
            }
            "os_dbus_tree" => {
                let tree_text = DbusIntrospector::render_tree();
                Ok(serde_json::json!({ "tree": tree_text }))
            }
            other => Err(format!("Unknown tool: '{other}'")),
        }
    }
}
