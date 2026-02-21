//! Integration tests for the MCP client + test plugin.
//!
//! These tests spawn the actual Node.js test plugin process and verify
//! the full JSON-RPC lifecycle: spawn → initialize → tools/list → tools/call.
//!
//! Requires: Node.js installed, test plugin at the platform config dir
//! (macOS: ~/Library/Application Support/omni-glass/plugins/com.omni-glass.test/)

use std::collections::HashMap;

/// Path to the test plugin's entry point.
fn test_plugin_entry() -> String {
    let config = dirs::config_dir().expect("No config dir");
    config
        .join("omni-glass/plugins/com.omni-glass.test/index.js")
        .to_string_lossy()
        .to_string()
}

/// Check if the test plugin exists.
fn plugin_available() -> bool {
    std::path::Path::new(&test_plugin_entry()).exists()
}

#[tokio::test]
async fn mcp_spawn_and_initialize() {
    if !plugin_available() {
        eprintln!("SKIP: test plugin not installed");
        return;
    }

    let mut server = omni_glass_lib::mcp::client::McpServer::spawn(
        "com.omni-glass.test",
        "node",
        &[&test_plugin_entry()],
        HashMap::new(),
        None,
    )
    .expect("Failed to spawn test plugin");

    let info = server.initialize().await.expect("Initialize failed");
    assert_eq!(info.name.as_deref(), Some("omni-glass-test-plugin"));
    assert_eq!(info.version.as_deref(), Some("0.1.0"));

    server.shutdown().await;
}

#[tokio::test]
async fn mcp_list_tools() {
    if !plugin_available() {
        eprintln!("SKIP: test plugin not installed");
        return;
    }

    let mut server = omni_glass_lib::mcp::client::McpServer::spawn(
        "com.omni-glass.test",
        "node",
        &[&test_plugin_entry()],
        HashMap::new(),
        None,
    )
    .expect("Failed to spawn");

    server.initialize().await.expect("Init failed");
    let tools = server.list_tools().await.expect("list_tools failed");

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo_text");
    assert!(tools[0].description.as_ref().unwrap().contains("Echo"));

    server.shutdown().await;
}

#[tokio::test]
async fn mcp_call_tool_echo() {
    if !plugin_available() {
        eprintln!("SKIP: test plugin not installed");
        return;
    }

    let mut server = omni_glass_lib::mcp::client::McpServer::spawn(
        "com.omni-glass.test",
        "node",
        &[&test_plugin_entry()],
        HashMap::new(),
        None,
    )
    .expect("Failed to spawn");

    server.initialize().await.expect("Init failed");

    let result = server
        .call_tool("echo_text", serde_json::json!({"text": "Hello, World!"}))
        .await
        .expect("call_tool failed");

    assert!(!result.is_error);
    assert_eq!(result.text(), "[Echo] Hello, World!");

    server.shutdown().await;
}

#[tokio::test]
async fn mcp_call_unknown_tool_returns_error() {
    if !plugin_available() {
        eprintln!("SKIP: test plugin not installed");
        return;
    }

    let mut server = omni_glass_lib::mcp::client::McpServer::spawn(
        "com.omni-glass.test",
        "node",
        &[&test_plugin_entry()],
        HashMap::new(),
        None,
    )
    .expect("Failed to spawn");

    server.initialize().await.expect("Init failed");

    let err = server
        .call_tool("nonexistent_tool", serde_json::json!({}))
        .await;

    assert!(err.is_err());
    assert!(err.unwrap_err().contains("Unknown tool"));

    server.shutdown().await;
}

#[tokio::test]
async fn registry_plugin_tool_dispatch() {
    if !plugin_available() {
        eprintln!("SKIP: test plugin not installed");
        return;
    }

    let registry = omni_glass_lib::mcp::ToolRegistry::new();

    // Spawn and initialize manually
    let mut server = omni_glass_lib::mcp::client::McpServer::spawn(
        "com.omni-glass.test",
        "node",
        &[&test_plugin_entry()],
        HashMap::new(),
        None,
    )
    .expect("Failed to spawn");

    server.initialize().await.expect("Init failed");
    let tools = server.list_tools().await.expect("list_tools failed");

    // Register tools and server in registry
    registry
        .register_plugin_tools("com.omni-glass.test", tools)
        .await;
    registry
        .add_server("com.omni-glass.test".to_string(), server)
        .await;

    // Verify the tool is findable
    assert!(registry.is_plugin_action("echo_text").await);
    assert!(!registry.is_plugin_action("copy_text").await); // not registered

    // Call through the registry
    let result = registry
        .call_plugin_tool("echo_text", serde_json::json!({"text": "Registry test"}))
        .await
        .expect("Registry call failed");

    assert_eq!(result.text(), "[Echo] Registry test");

    // Cleanup
    registry.shutdown_all().await;
}
