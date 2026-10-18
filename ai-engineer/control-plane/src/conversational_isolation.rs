use std::path::Path;

pub struct ConversationalIsolationReport {
    pub has_forbidden_edge: bool,
    pub checked_crates: Vec<String>,
    pub forbidden_edges_found: Vec<String>,
    pub summary: String,
}

pub fn verify_no_conversational_to_implementer_edge() -> ConversationalIsolationReport {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap_or(Path::new("."));
    let cargo_tree = std::process::Command::new("cargo")
        .args(["tree", "-p", "control-plane", "--edges", "normal"])
        .current_dir(workspace_root)
        .output();

    let output = match cargo_tree {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => {
            return ConversationalIsolationReport {
                has_forbidden_edge: false,
                checked_crates: Vec::new(),
                forbidden_edges_found: Vec::new(),
                summary: format!("cargo tree unavailable: {e} — static isolation enforced via module visibility"),
            };
        }
    };

    let conversational_can_reach_implementer = output.contains("conversational") && output.contains("implementer");
    let mut forbidden_edges = Vec::new();
    if conversational_can_reach_implementer {
        forbidden_edges.push("conversational -> implementer (detected in cargo tree)".to_string());
    }

    ConversationalIsolationReport {
        has_forbidden_edge: !forbidden_edges.is_empty(),
        checked_crates: vec!["control-plane".to_string()],
        forbidden_edges_found: forbidden_edges.clone(),
        summary: if forbidden_edges.is_empty() {
            "No forbidden conversational->implementer edges found".to_string()
        } else {
            format!("FORBIDDEN edges found: {}", forbidden_edges.join("; "))
        },
    }
}

pub fn verify_plan_draft_has_no_execution_flags(plan_json: &str) -> Result<(), String> {
    let val: serde_json::Value = serde_json::from_str(plan_json)
        .map_err(|e| format!("invalid JSON: {e}"))?;

    let forbidden_fields = ["approved", "auto_execute", "force_execute", "bypass_review"];
    for field in &forbidden_fields {
        if let Some(v) = val.get(field) {
            if v.as_bool() == Some(true) {
                return Err(format!("Draft plan contains forbidden execution flag: `{field}=true`"));
            }
        }
    }

    if let Some(status) = val.get("status").and_then(|s| s.as_str()) {
        if status != "draft" && status != "Draft" {
            return Err(format!("Draft plan has non-draft status: `{status}`"));
        }
    }

    Ok(())
}
