//! Local LLM provider — CLASSIFY, EXECUTE, ARGS_BRIDGE, and TEXT_CMD
//! functions using the llama.cpp backend via llama-cpp-2.
//!
//! Each function mirrors the signature of its Anthropic counterpart.
//! NOTE: GBNF grammar-guided generation is disabled due to a crash bug
//! in llama-cpp-2 v0.1.135 (SIGABRT in llama_grammar_reject_candidates).
//! Instead, we generate freely and extract JSON with a robust fallback.

use super::execute::ActionResult;
use super::local_state::LocalLlmState;
use super::prompts_execute_local;
use super::prompts_local;
use super::streaming;
use super::types::{Action, ActionMenu, ActionMenuSkeleton};
use tauri::Emitter;

/// CLASSIFY: local LLM version.
///
/// Emits skeleton event, generates ActionMenu JSON via grammar-constrained
/// local inference, then emits the complete event.
pub async fn classify_local(
    app: &tauri::AppHandle,
    text: &str,
    has_table: bool,
    has_code: bool,
    confidence: f64,
    plugin_tools: &str,
    state: &LocalLlmState,
) -> ActionMenu {
    let start = std::time::Instant::now();

    // Emit skeleton immediately so the UI shows a loading state
    let _ = app.emit(
        "action-menu-skeleton",
        ActionMenuSkeleton {
            content_type: "unknown".to_string(),
            summary: "Analyzing with local model...".to_string(),
        },
    );

    let prompt = prompts_local::build_local_classify_prompt(
        text, confidence, has_table, has_code, plugin_tools,
    );

    // Grammar disabled — llama-cpp-2 v0.1.135 GBNF crashes (SIGABRT).
    // Generate freely and extract JSON from the output.
    let result = state
        .generate(&prompt, prompts_local::LOCAL_CLASSIFY_MAX_TOKENS, None)
        .await;

    let menu = match result {
        Ok(raw) => {
            match extract_and_parse::<ActionMenu>(&raw) {
                Ok(menu) => {
                    log::info!(
                        "[LOCAL_CLASSIFY] Success: {} actions in {}ms",
                        menu.actions.len(),
                        start.elapsed().as_millis()
                    );
                    menu
                }
                Err(e) => {
                    log::warn!("[LOCAL_CLASSIFY] Parse failed: {} — raw: {}", e, &raw[..200.min(raw.len())]);
                    fallback_menu(text)
                }
            }
        }
        Err(e) => {
            log::error!("[LOCAL_CLASSIFY] Generation failed: {}", e);
            fallback_menu(text)
        }
    };

    // Emit completion event
    let _ = app.emit("action-menu-complete", &menu);

    menu
}

/// EXECUTE: local LLM version.
///
/// Same interface as `execute_action_anthropic` but runs locally.
pub async fn execute_action_local(
    action_id: &str,
    extracted_text: &str,
    state: &LocalLlmState,
) -> ActionResult {
    let start = std::time::Instant::now();
    log::info!("[LOCAL_EXECUTE] Action: {}, text length: {}", action_id, extracted_text.len());

    let prompt = prompts_execute_local::build_local_execute_prompt(
        action_id, extracted_text, "macos",
    );

    // Grammar disabled — see module-level note.
    let result = state
        .generate(&prompt, prompts_execute_local::LOCAL_EXECUTE_MAX_TOKENS, None)
        .await;

    match result {
        Ok(raw) => {
            match extract_and_parse::<ActionResult>(&raw) {
                Ok(r) => {
                    log::info!(
                        "[LOCAL_EXECUTE] Success: status={} in {}ms",
                        r.status,
                        start.elapsed().as_millis()
                    );
                    r
                }
                Err(e) => {
                    // Local models often return flat JSON instead of nested ActionResult.
                    // Try to salvage fields from the flat structure.
                    log::warn!("[LOCAL_EXECUTE] Structured parse failed: {} — trying flat JSON", e);
                    salvage_flat_result(action_id, &raw)
                }
            }
        }
        Err(e) => {
            log::error!("[LOCAL_EXECUTE] Generation failed: {}", e);
            ActionResult::error(action_id, &format!("Local generation failed: {}", e))
        }
    }
}

/// ARGS_BRIDGE: local LLM version.
///
/// Generates structured plugin tool arguments from OCR text + schema.
pub async fn generate_plugin_args_local(
    tool_name: &str,
    tool_description: &str,
    input_schema: &serde_json::Value,
    extracted_text: &str,
    state: &LocalLlmState,
) -> Result<serde_json::Value, String> {
    let schema_str = serde_json::to_string_pretty(input_schema).unwrap_or_default();
    let prompt = prompts_execute_local::build_local_args_prompt(
        tool_name, tool_description, &schema_str, extracted_text,
    );

    // No strict grammar for args — schemas vary per plugin.
    // Rely on the model's JSON instruction following.
    let json_text = state.generate(&prompt, 512, None).await?;
    let clean = streaming::strip_code_fences(&json_text);

    let args: serde_json::Value = serde_json::from_str(&clean)
        .map_err(|e| format!("Local model args parse failed: {}", e))?;

    log::info!("[LOCAL_ARGS] Generated args for '{}'", tool_name);
    Ok(args)
}

/// TEXT_CMD: local LLM version.
///
/// Handles the text launcher routing decision locally.
/// Returns the raw JSON string for the caller to parse.
pub async fn execute_text_command_local(
    text: &str,
    tools_prompt: &str,
    state: &LocalLlmState,
) -> Result<String, String> {
    let prompt = prompts_execute_local::build_local_text_command_prompt(text, tools_prompt);
    // Grammar disabled — see module-level note.
    let raw = state.generate(&prompt, 512, None).await?;

    // Extract JSON from the output (model may add prose around it)
    let clean = extract_json_str(&raw);
    Ok(clean.to_string())
}

/// Extract JSON from model output: strip code fences, find the outermost
/// `{...}` block, parse it. Handles prose before/after the JSON object.
fn extract_and_parse<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, String> {
    let clean = streaming::strip_code_fences(raw);
    // Try direct parse first
    if let Ok(v) = serde_json::from_str::<T>(&clean) {
        return Ok(v);
    }
    // Try to find outermost { ... } in the text
    let json_str = extract_json_str(&clean);
    serde_json::from_str::<T>(json_str)
        .map_err(|e| format!("JSON extraction failed: {}", e))
}

/// Find the outermost `{...}` in a string (brace-balanced extraction).
fn extract_json_str(text: &str) -> &str {
    let text = text.trim();
    if let Some(start) = text.find('{') {
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape_next = false;
        for (i, ch) in text[start..].char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }
            match ch {
                '\\' if in_string => escape_next = true,
                '"' => in_string = !in_string,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return &text[start..start + i + 1];
                    }
                }
                _ => {}
            }
        }
    }
    text
}

/// Salvage an ActionResult from flat JSON that the local model often produces.
///
/// Local models frequently return `{"status":"needs_confirmation","command":"ls -la"}`
/// instead of the nested `{"status":"...","actionId":"...","result":{"type":"command",...}}`.
/// This function extracts known fields and builds a proper ActionResult.
fn salvage_flat_result(action_id: &str, raw: &str) -> ActionResult {
    let clean = streaming::strip_code_fences(raw);
    let json_str = extract_json_str(&clean);

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
        let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("success");
        let command = v.get("command").and_then(|s| s.as_str());
        let text = v.get("text").and_then(|s| s.as_str());
        let result_type = v.get("type").and_then(|s| s.as_str());

        // If there's a command field, treat as a command result
        if let Some(cmd) = command {
            log::info!("[LOCAL_EXECUTE] Salvaged command result: {}", cmd);
            return ActionResult {
                status: status.to_string(),
                action_id: action_id.to_string(),
                result: super::execute::ActionResultBody {
                    result_type: "command".to_string(),
                    text: text.map(|t| t.to_string()),
                    file_path: None,
                    command: Some(cmd.to_string()),
                    clipboard_content: None,
                    mime_type: None,
                },
                metadata: None,
            };
        }

        // If there's a type hint, use it
        if let Some(rt) = result_type {
            log::info!("[LOCAL_EXECUTE] Salvaged {} result", rt);
            return ActionResult {
                status: status.to_string(),
                action_id: action_id.to_string(),
                result: super::execute::ActionResultBody {
                    result_type: rt.to_string(),
                    text: text.map(|t| t.to_string()),
                    file_path: v.get("filePath").and_then(|s| s.as_str()).map(|s| s.to_string()),
                    command: None,
                    clipboard_content: v.get("clipboardContent").and_then(|s| s.as_str()).map(|s| s.to_string()),
                    mime_type: None,
                },
                metadata: None,
            };
        }

        // Has text content — check if it contains a shell command we can extract
        if let Some(t) = text {
            // If the action expects a command, try to extract one from the explanation
            if is_command_action(action_id) {
                if let Some(cmd) = extract_command_from_text(t) {
                    log::info!("[LOCAL_EXECUTE] Extracted command from text: {}", cmd);
                    return ActionResult {
                        status: "needs_confirmation".to_string(),
                        action_id: action_id.to_string(),
                        result: super::execute::ActionResultBody {
                            result_type: "command".to_string(),
                            text: Some(t.to_string()),
                            file_path: None,
                            command: Some(cmd),
                            clipboard_content: None,
                            mime_type: None,
                        },
                        metadata: None,
                    };
                }
            }
            return ActionResult::text(action_id, t);
        }
    }

    // Final fallback for non-JSON: try to extract a command from raw prose
    if is_command_action(action_id) {
        if let Some(cmd) = extract_command_from_text(raw) {
            log::info!("[LOCAL_EXECUTE] Extracted command from raw text: {}", cmd);
            return ActionResult {
                status: "needs_confirmation".to_string(),
                action_id: action_id.to_string(),
                result: super::execute::ActionResultBody {
                    result_type: "command".to_string(),
                    text: None,
                    file_path: None,
                    command: Some(cmd),
                    clipboard_content: None,
                    mime_type: None,
                },
                metadata: None,
            };
        }
    }

    ActionResult::text(action_id, raw)
}

/// Check if an action ID suggests a command should be returned.
fn is_command_action(action_id: &str) -> bool {
    action_id.contains("command")
        || action_id.contains("run")
        || action_id.contains("fix")
        || action_id == "suggest_fix"
}

/// Try to extract a shell command from explanation text.
///
/// Local models often write "Run `sysctl -n hw.memsize`" or put commands in
/// backtick-fenced blocks instead of the JSON "command" field. Extract it.
fn extract_command_from_text(text: &str) -> Option<String> {
    // Try backtick-fenced code block first: ```...\ncommand\n```
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        // Skip optional language tag on the same line
        let after = if let Some(nl) = after.find('\n') {
            &after[nl + 1..]
        } else {
            after
        };
        if let Some(end) = after.find("```") {
            let cmd = after[..end].trim();
            if !cmd.is_empty() && cmd.lines().count() <= 3 {
                return Some(cmd.to_string());
            }
        }
    }

    // Try inline backtick: `command here`
    if let Some(start) = text.find('`') {
        let after = &text[start + 1..];
        if let Some(end) = after.find('`') {
            let cmd = after[..end].trim();
            if !cmd.is_empty() && !cmd.contains(' ') || cmd.split_whitespace().count() <= 8 {
                return Some(cmd.to_string());
            }
        }
    }

    None
}

/// Fallback ActionMenu when local model fails or returns invalid JSON.
fn fallback_menu(text: &str) -> ActionMenu {
    let summary = if text.len() > 40 {
        format!("{}...", &text[..40])
    } else {
        text.to_string()
    };

    ActionMenu {
        content_type: "unknown".to_string(),
        confidence: 0.0,
        summary,
        detected_language: None,
        actions: vec![
            Action {
                id: "copy_text".to_string(),
                label: "Copy Text".to_string(),
                icon: "clipboard".to_string(),
                priority: 1,
                description: "Copy extracted text to clipboard".to_string(),
                requires_execution: false,
            },
            Action {
                id: "explain".to_string(),
                label: "Explain This".to_string(),
                icon: "lightbulb".to_string(),
                priority: 2,
                description: "Explain this content".to_string(),
                requires_execution: true,
            },
            Action {
                id: "search_web".to_string(),
                label: "Search Web".to_string(),
                icon: "search".to_string(),
                priority: 3,
                description: "Search for this text online".to_string(),
                requires_execution: false,
            },
        ],
    }
}
