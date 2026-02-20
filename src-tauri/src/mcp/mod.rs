//! MCP (Model Context Protocol) plugin system.
//!
//! This module implements an MCP client that communicates with plugin
//! processes over JSON-RPC 2.0 / NDJSON stdio. It provides:
//!
//! - **types**: MCP protocol types (JSON-RPC framing, tool definitions)
//! - **client**: McpServer — spawn child process, handshake, call tools
//! - **manifest**: Parse and validate `omni-glass.plugin.json` files
//! - **registry**: ToolRegistry — central store for built-in + plugin tools
//! - **loader**: Scan plugins directory, spawn servers, discover tools
//! - **builtins**: Register the 6 built-in actions as internal tools

pub mod builtins;
pub mod client;
pub mod loader;
pub mod manifest;
pub mod registry;
pub mod types;

pub use registry::ToolRegistry;

use crate::llm::execute::{ActionResult, ActionResultBody};

/// Execute a plugin tool call, converting the MCP result to our ActionResult type.
///
/// Called by the pipeline when the action belongs to a plugin (not builtin).
pub async fn execute_plugin_tool(
    registry: &ToolRegistry,
    action_id: &str,
    input_text: &str,
) -> ActionResult {
    let arguments = serde_json::json!({ "text": input_text });

    match registry.call_plugin_tool(action_id, arguments).await {
        Ok(result) => {
            let text = result.text();
            if result.is_error {
                ActionResult::error(action_id, &format!("Plugin error: {}", text))
            } else {
                ActionResult {
                    status: "success".to_string(),
                    action_id: action_id.to_string(),
                    result: ActionResultBody {
                        result_type: "text".to_string(),
                        text: Some(text),
                        file_path: None,
                        command: None,
                        clipboard_content: None,
                        mime_type: None,
                    },
                    metadata: None,
                }
            }
        }
        Err(e) => ActionResult::error(action_id, &format!("Failed to call plugin tool: {}", e)),
    }
}
