use directioner_os_mcp::McpHandler;
use serde_json::json;

#[test]
fn test_mcp_list_tools() {
    let handler = McpHandler::new();
    let tools = handler.list_tools();
    assert_eq!(tools.len(), 5);
    let names: Vec<String> = tools.into_iter().map(|t| t.name).collect();
    assert!(names.contains(&"os_observe".to_string()));
    assert!(names.contains(&"os_catalog_list".to_string()));
    assert!(names.contains(&"os_catalog_show".to_string()));
    assert!(names.contains(&"os_preview_action".to_string()));
    assert!(names.contains(&"os_dbus_tree".to_string()));
}

#[test]
fn test_mcp_call_observe() {
    let handler = McpHandler::new();
    let res = handler.call_tool("os_observe", &json!({})).unwrap();
    assert_eq!(res["system"]["os"], "Directioner OS");
}

#[test]
fn test_mcp_call_preview() {
    let handler = McpHandler::new();
    let res = handler
        .call_tool(
            "os_preview_action",
            &json!({
                "capability_id": "audio.volume.set",
                "params": { "level": 75 }
            }),
        )
        .unwrap();

    assert_eq!(res["capability_id"], "audio.volume.set");
    assert!(res["reversible"].as_bool().unwrap());
    assert_eq!(res["risk"], "low");
}

#[test]
fn test_mcp_call_unknown_tool() {
    let handler = McpHandler::new();
    let err = handler.call_tool("os_nonexistent", &json!({})).unwrap_err();
    assert!(err.contains("Unknown tool"));
}
