//! Text launcher pipeline — executes typed commands.
//!
//! This is a separate pipeline from the snip pipeline (pipeline.rs).
//! The user types text in the launcher (Cmd+Shift+Space) instead of
//! snipping a screen region. The LLM decides: respond directly or
//! route to a tool (built-in or plugin).

use crate::llm;
use crate::mcp;
use serde::{Deserialize, Serialize};

use llm::prompts_text_command::{self, TEXT_COMMAND_MAX_TOKENS, TEXT_COMMAND_SYSTEM_PROMPT};
use llm::streaming;

/// Result returned to the text launcher frontend.
///
/// Includes the full structured result from ActionResult so the frontend
/// can auto-execute: run commands, open URLs, copy to clipboard, save files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextCommandResult {
    pub status: String,
    pub text: String,
    pub action_id: Option<String>,
    /// "text" | "command" | "clipboard" | "file"
    pub result_type: String,
    pub command: Option<String>,
    pub file_path: Option<String>,
    pub file_content: Option<String>,
    pub clipboard_content: Option<String>,
}

/// LLM routing decision — parsed from the LLM response.
#[derive(Debug, Deserialize)]
struct RouteDecision {
    #[serde(rename = "type")]
    decision_type: String, // "direct" | "tool"
    text: Option<String>,
    tool_id: Option<String>,
    input_text: Option<String>,
}

/// Tauri command: execute a typed text command.
///
/// Routes through LLM to decide: direct response or tool dispatch.
#[tauri::command]
pub async fn execute_text_command(
    text: String,
    registry: tauri::State<'_, mcp::ToolRegistry>,
) -> Result<TextCommandResult, String> {
    log::info!("[TEXT_CMD] Input: {} chars", text.len());

    // Get all available tools for the LLM prompt
    let all_tools = registry.all_tools().await;
    let tool_descriptions: Vec<String> = all_tools
        .iter()
        .map(|t| {
            let qname = mcp::registry::qualified_name(&t.plugin_id, &t.name);
            format!("- {} ({}): {}", t.display_name, qname, t.description)
        })
        .collect();
    let tools_prompt = tool_descriptions.join("\n");

    // Build the LLM request
    let user_message = prompts_text_command::build_text_command_message(&text, &tools_prompt);

    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "No API key configured".to_string())?;
    if api_key.is_empty() {
        return Err("No API key configured".to_string());
    }

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": llm::prompts::MODEL,
            "max_tokens": TEXT_COMMAND_MAX_TOKENS,
            "system": TEXT_COMMAND_SYSTEM_PROMPT,
            "messages": [{"role": "user", "content": user_message}]
        }))
        .send()
        .await
        .map_err(|e| format!("API call failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API error: {}", &body[..200.min(body.len())]));
    }

    let body = resp.text().await.map_err(|e| e.to_string())?;
    let response_text = extract_text(&body)?;
    eprintln!("[TEXT_CMD] Raw router response: {}", &response_text[..300.min(response_text.len())]);
    let json_text = streaming::strip_code_fences(&response_text);

    let decision: RouteDecision = serde_json::from_str(&json_text)
        .map_err(|e| format!("Failed to parse LLM routing decision: {}", e))?;

    eprintln!(
        "[TEXT_CMD] Router decision: type={}, tool_id={:?}, text={:?}",
        decision.decision_type,
        decision.tool_id,
        decision.text.as_deref().map(|t| &t[..80.min(t.len())])
    );

    match decision.decision_type.as_str() {
        "direct" => {
            log::info!("[TEXT_CMD] Direct response");
            Ok(TextCommandResult {
                status: "success".to_string(),
                text: decision.text.unwrap_or_default(),
                action_id: None,
                result_type: "text".to_string(),
                command: None,
                file_path: None,
                file_content: None,
                clipboard_content: None,
            })
        }
        "tool" => {
            let tool_id = decision.tool_id.unwrap_or_default();
            let input = decision.input_text.unwrap_or_else(|| text.clone());
            log::info!("[TEXT_CMD] Routing to tool: {}", tool_id);
            route_to_tool(&registry, &tool_id, &input).await
        }
        other => Err(format!("Unknown route type: {}", other)),
    }
}

/// Dispatch to a tool — built-in (LLM execute) or plugin (MCP).
async fn route_to_tool(
    registry: &mcp::ToolRegistry,
    tool_id: &str,
    input_text: &str,
) -> Result<TextCommandResult, String> {
    // Strip plugin prefix (e.g. "builtin::run_command" or "builtin:run_command" → "run_command")
    let bare_id = tool_id
        .rsplit_once("::")
        .map(|(_, name)| name)
        .unwrap_or_else(|| tool_id.rsplit_once(':').map(|(_, name)| name).unwrap_or(tool_id));
    eprintln!("[TEXT_CMD] Dispatching tool: raw={}, bare={}", tool_id, bare_id);

    let result = if registry.is_plugin_action(tool_id).await {
        // Plugin tool — use MCP dispatch with args bridge
        let resolved = registry.resolve_action(tool_id).await;
        let tool_meta = match &resolved {
            Some(qname) => registry.get_tool(qname).await,
            None => None,
        };
        mcp::execute_plugin_tool(
            registry,
            tool_id,
            input_text,
            tool_meta.as_ref().map(|t| t.description.as_str()),
            tool_meta.as_ref().and_then(|t| t.input_schema.as_ref()),
        )
        .await
    } else {
        // Built-in tool — use the execute pipeline
        llm::execute_action_anthropic(bare_id, input_text).await
    };

    Ok(TextCommandResult {
        status: result.status,
        text: result.result.text.unwrap_or_default(),
        action_id: Some(bare_id.to_string()),
        result_type: result.result.result_type,
        command: result.result.command,
        file_path: result.result.file_path,
        file_content: None, // File content handled via file_path
        clipboard_content: result.result.clipboard_content,
    })
}

/// Extract text content from an Anthropic Messages API response.
fn extract_text(body: &str) -> Result<String, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Invalid response: {}", e))?;
    let content = parsed
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or("No content in response")?;
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                return Ok(t.to_string());
            }
        }
    }
    Err("No text in response".to_string())
}
