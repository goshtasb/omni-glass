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
//! - **sandbox**: OS-level process sandboxing (env filtering, macOS sandbox-exec)
//! - **approval**: Plugin approval state management (user consent)

pub mod approval;
pub mod approval_commands;
pub mod builtins;
pub mod client;
pub mod config_store;
pub mod loader;
pub mod manifest;
pub mod registry;
pub mod sandbox;
pub mod types;

pub use registry::ToolRegistry;

use crate::llm::execute::{ActionResult, ActionResultBody};
use crate::safety::{command_check, redact};

/// Execute a plugin tool call, converting the MCP result to our ActionResult type.
///
/// Called by the pipeline when the action belongs to a plugin (not builtin).
/// For non-trivial schemas, an LLM call generates structured arguments from
/// the OCR text. Safety checks are applied to plugin output before returning.
pub async fn execute_plugin_tool(
    registry: &ToolRegistry,
    action_id: &str,
    input_text: &str,
    tool_description: Option<&str>,
    input_schema: Option<&serde_json::Value>,
    #[allow(unused_variables)] app: Option<&tauri::AppHandle>,
) -> ActionResult {
    // Generate structured args for non-trivial schemas, fallback to {text} otherwise
    let arguments = match input_schema {
        Some(schema) if !crate::llm::plugin_args::is_trivial_schema(schema) => {
            let args_result = generate_args_for_provider(
                action_id,
                tool_description.unwrap_or(""),
                schema,
                input_text,
                app,
            )
            .await;
            match args_result {
                Ok(args) => args,
                Err(e) => {
                    log::warn!("[MCP] Args bridge failed, falling back to {{text}}: {}", e);
                    serde_json::json!({ "text": input_text })
                }
            }
        }
        _ => serde_json::json!({ "text": input_text }),
    };

    match registry.call_plugin_tool(action_id, arguments).await {
        Ok(result) => {
            let raw_text = result.text();
            if result.is_error {
                return ActionResult::error(action_id, &format!("Plugin error: {}", raw_text));
            }

            // Safety gate 1: block dangerous commands in plugin output
            let cmd_check = command_check::is_command_safe(&raw_text);
            if !cmd_check.safe {
                let reason = cmd_check.reason.unwrap_or_else(|| "blocked pattern".into());
                log::warn!(
                    "[SAFETY] Plugin '{}' output blocked: {}",
                    action_id,
                    reason
                );
                return ActionResult::error(
                    action_id,
                    &format!("Plugin output blocked by safety filter: {}", reason),
                );
            }

            // Safety gate 2: redact PII / secrets from plugin output
            let redaction = redact::redact_sensitive_data(&raw_text);
            if !redaction.redactions.is_empty() {
                let labels: Vec<&str> = redaction.redactions.iter()
                    .map(|r| r.label.as_str())
                    .collect();
                log::info!(
                    "[SAFETY] Redacted {} pattern(s) from plugin '{}' output: {:?}",
                    redaction.redactions.len(),
                    action_id,
                    labels
                );
            }

            ActionResult {
                status: "success".to_string(),
                action_id: action_id.to_string(),
                result: ActionResultBody {
                    result_type: "text".to_string(),
                    text: Some(redaction.cleaned_text),
                    file_path: None,
                    command: None,
                    clipboard_content: None,
                    mime_type: None,
                },
                metadata: None,
            }
        }
        Err(e) => ActionResult::error(action_id, &format!("Failed to call plugin tool: {}", e)),
    }
}

/// Route args generation to the active provider (Anthropic or local).
async fn generate_args_for_provider(
    action_id: &str,
    tool_description: &str,
    schema: &serde_json::Value,
    input_text: &str,
    #[allow(unused_variables)] app: Option<&tauri::AppHandle>,
) -> Result<serde_json::Value, String> {
    #[cfg(feature = "local-llm")]
    if let Some(app_handle) = app {
        let provider = crate::settings_commands::resolve_provider();
        if provider == "local" {
            use tauri::Manager;
            let state = app_handle.state::<crate::llm::local_state::LocalLlmState>();
            return crate::llm::local::generate_plugin_args_local(
                action_id,
                tool_description,
                schema,
                input_text,
                &state,
            )
            .await;
        }
    }

    crate::llm::plugin_args::generate_plugin_args(
        action_id,
        tool_description,
        schema,
        input_text,
    )
    .await
}
