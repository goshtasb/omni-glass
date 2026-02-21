//! Built-in actions registered as internal tools in the ToolRegistry.
//!
//! These tools bypass MCP stdio — they call existing Rust functions directly.
//! They're registered at startup so the ToolRegistry knows about all available
//! actions (both built-in and plugin) for prompt injection and dispatch.

use crate::mcp::registry::{RegisteredTool, ToolRegistry};

/// Register all built-in actions as tools in the registry.
///
/// Called once at startup. These tools use `plugin_id: "builtin"` and
/// are dispatched directly in the pipeline, not via MCP stdio.
pub async fn register_builtins(registry: &ToolRegistry) {
    let builtins = vec![
        RegisteredTool {
            plugin_id: "builtin".to_string(),
            name: "copy_text".to_string(),
            display_name: "Copy Text".to_string(),
            description: "Copy the extracted text to the clipboard".to_string(),
            input_schema: None,
        },
        RegisteredTool {
            plugin_id: "builtin".to_string(),
            name: "search_web".to_string(),
            display_name: "Search Web".to_string(),
            description: "Search the web for the extracted text".to_string(),
            input_schema: None,
        },
        RegisteredTool {
            plugin_id: "builtin".to_string(),
            name: "explain_error".to_string(),
            display_name: "Explain Error".to_string(),
            description: "Explain what this error means and why it occurred".to_string(),
            input_schema: None,
        },
        RegisteredTool {
            plugin_id: "builtin".to_string(),
            name: "suggest_fix".to_string(),
            display_name: "Suggest Fix".to_string(),
            description: "Analyze the error and suggest a fix (code or command)".to_string(),
            input_schema: None,
        },
        RegisteredTool {
            plugin_id: "builtin".to_string(),
            name: "export_csv".to_string(),
            display_name: "Export CSV".to_string(),
            description: "Extract tabular data and export as CSV file".to_string(),
            input_schema: None,
        },
        RegisteredTool {
            plugin_id: "builtin".to_string(),
            name: "explain".to_string(),
            display_name: "Explain This".to_string(),
            description: "Explain this content clearly and concisely".to_string(),
            input_schema: None,
        },
    ];

    for tool in builtins {
        registry.register_builtin(tool).await;
    }

    log::info!("[MCP] 6 built-in tools registered");
}
