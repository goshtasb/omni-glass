//! EXECUTE pipeline — performs a specific action on snipped text.
//!
//! This is the second LLM call in the product flow:
//! 1. CLASSIFY (Week 2): OCR text → ActionMenu (what can the user do?)
//! 2. EXECUTE (Week 4): OCR text + chosen action → ActionResult (do it)
//!
//! Unlike CLASSIFY, EXECUTE does NOT stream progressively to the UI.
//! The user already clicked a button and expects a brief wait.

use crate::safety;
use serde::{Deserialize, Serialize};

use super::prompts_execute::{self, EXECUTE_MAX_TOKENS, EXECUTE_SYSTEM_PROMPT};
use super::streaming;

// ── Types ──────────────────────────────────────────────────────────

/// The result of an EXECUTE action, returned by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub status: String, // "success" | "error" | "needs_confirmation"
    pub action_id: String,
    pub result: ActionResultBody,
    pub metadata: Option<ActionResultMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResultBody {
    #[serde(rename = "type")]
    pub result_type: String, // "text" | "file" | "command" | "clipboard"
    pub text: Option<String>,
    pub file_path: Option<String>,
    pub command: Option<String>,
    pub clipboard_content: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResultMetadata {
    pub tokens_used: Option<u32>,
    pub processing_note: Option<String>,
}

impl ActionResult {
    /// Fallback result when the LLM fails or returns invalid JSON.
    pub fn error(action_id: &str, message: &str) -> Self {
        Self {
            status: "error".to_string(),
            action_id: action_id.to_string(),
            result: ActionResultBody {
                result_type: "text".to_string(),
                text: Some(message.to_string()),
                file_path: None,
                command: None,
                clipboard_content: None,
                mime_type: None,
            },
            metadata: None,
        }
    }
}

// ── Pipeline ───────────────────────────────────────────────────────

/// Execute an action using the Anthropic provider.
///
/// Steps:
/// 1. Pre-flight: redact sensitive data
/// 2. Build action-specific user message
/// 3. Call Claude (non-streaming — accumulate full response)
/// 4. Parse ActionResult JSON
/// 5. Post-flight: validate command safety
pub async fn execute_action_anthropic(
    action_id: &str,
    extracted_text: &str,
) -> ActionResult {
    let start = std::time::Instant::now();

    // 1. Pre-flight: redact sensitive data before sending to cloud
    let redaction = safety::redact::redact_sensitive_data(extracted_text);
    let clean_text = &redaction.cleaned_text;

    // 2. Build the action-specific user message
    let user_message = prompts_execute::build_execute_message(action_id, clean_text, "macos");
    log::info!("[EXECUTE] Action: {}, text length: {}", action_id, clean_text.len());

    // 3. Call Claude API (non-streaming, accumulate full response)
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.is_empty() => {
            eprintln!("[EXECUTE] API key found ({} chars)", k.len());
            k
        }
        _ => {
            eprintln!("[EXECUTE] ERROR: No API key");
            return ActionResult::error(action_id, "No API key configured. Add your Anthropic API key in Settings.");
        }
    };

    let model = super::prompts::MODEL;
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "max_tokens": EXECUTE_MAX_TOKENS,
            "system": EXECUTE_SYSTEM_PROMPT,
            "messages": [{"role": "user", "content": user_message}]
        }))
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[EXECUTE] HTTP request FAILED: {}", e);
            log::error!("[EXECUTE] API request failed: {}", e);
            return ActionResult::error(action_id, &format!("API request failed: {}", e));
        }
    };

    let status_code = resp.status();
    let body = match resp.text().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[EXECUTE] Failed to read response body: {}", e);
            return ActionResult::error(action_id, &format!("Failed to read response: {}", e));
        }
    };

    if !status_code.is_success() {
        eprintln!("[EXECUTE] API error {}: {}", status_code, &body[..500.min(body.len())]);
        log::error!("[EXECUTE] API returned {}: {}", status_code, &body[..200.min(body.len())]);
        return ActionResult::error(action_id, &format!("API error ({})", status_code));
    }

    eprintln!("[EXECUTE] API returned 200, {} bytes", body.len());

    let llm_ms = start.elapsed().as_millis();
    log::info!("[EXECUTE] LLM response in {}ms", llm_ms);

    // 4. Extract text content from Anthropic response
    let response_text = extract_anthropic_text(&body);
    let response_text = match response_text {
        Some(t) => t,
        None => {
            log::error!("[EXECUTE] Could not extract text from response");
            return ActionResult::error(action_id, "Could not parse LLM response");
        }
    };

    // Strip markdown code fences if present
    let json_text = streaming::strip_code_fences(&response_text);

    // 5. Parse as ActionResult — try full parse, then repair truncated JSON
    eprintln!("[EXECUTE] JSON to parse: {}", &json_text[..500.min(json_text.len())]);
    let result = match serde_json::from_str::<ActionResult>(&json_text) {
        Ok(r) => {
            eprintln!("[EXECUTE] SUCCESS: status={}, type={}", r.status, r.result.result_type);
            r
        }
        Err(e) => {
            eprintln!("[EXECUTE] JSON parse failed, attempting repair: {}", e);
            // Try to salvage truncated JSON by extracting the "text" field
            match salvage_text_from_json(&json_text, action_id) {
                Some(r) => {
                    eprintln!("[EXECUTE] SALVAGED: status={}, type={}", r.status, r.result.result_type);
                    r
                }
                None => {
                    eprintln!("[EXECUTE] SALVAGE FAILED — raw: {}", &json_text[..500.min(json_text.len())]);
                    log::error!("[EXECUTE] JSON parse failed: {} — raw: {}", e, &json_text[..200.min(json_text.len())]);
                    return ActionResult::error(action_id, &format!("Failed to parse action result: {}", e));
                }
            }
        }
    };

    log::info!(
        "[EXECUTE] Result: status={}, type={}",
        result.status,
        result.result.result_type
    );

    // 6. Post-flight: command safety check
    if result.result.result_type == "command" {
        if let Some(ref cmd) = result.result.command {
            let check = safety::command_check::is_command_safe(cmd);
            if !check.safe {
                let reason = check.reason.unwrap_or_else(|| "Unknown safety concern".to_string());
                log::warn!("[EXECUTE] Command blocked by safety layer: {}", reason);
                return ActionResult::error(
                    action_id,
                    &format!("Command blocked: {}", reason),
                );
            }
        }
    }

    // 7. Post-flight: file path safety check
    if result.result.result_type == "file" {
        if let Some(ref path) = result.result.file_path {
            if !safety::command_check::is_path_safe(path) {
                return ActionResult::error(
                    action_id,
                    "File path contains unsafe traversal characters",
                );
            }
        }
    }

    result
}

/// Try to salvage a usable ActionResult from truncated/malformed JSON.
///
/// When max_tokens cuts off the response, the JSON is incomplete. We try:
/// 1. Parse as serde_json::Value to extract whatever fields are present
/// 2. Pull out the "text" or "command" field and build a valid ActionResult
fn salvage_text_from_json(raw: &str, action_id: &str) -> Option<ActionResult> {
    // Try parsing as a loose Value — serde won't help with truly truncated JSON,
    // so we manually extract key fields with string searching.
    let text = extract_json_string_field(raw, "text")?;
    let result_type = extract_json_string_field(raw, "type").unwrap_or_else(|| "text".to_string());
    let status = extract_json_string_field(raw, "status").unwrap_or_else(|| "success".to_string());
    let command = extract_json_string_field(raw, "command");

    Some(ActionResult {
        status,
        action_id: action_id.to_string(),
        result: ActionResultBody {
            result_type,
            text: Some(text),
            file_path: None,
            command,
            clipboard_content: None,
            mime_type: None,
        },
        metadata: None,
    })
}

/// Extract a string value for a given JSON key from potentially malformed JSON.
/// Searches for `"key"  :  "value"` patterns, skipping occurrences where
/// `"key"` appears as a value (not followed by `:`).
fn extract_json_string_field(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let mut search_from = 0;

    // Find the pattern as a KEY (followed by `:`) not as a VALUE
    loop {
        let key_pos = json[search_from..].find(&pattern)?;
        let abs_pos = search_from + key_pos;
        let after_key = json[abs_pos + pattern.len()..].trim_start();
        if after_key.starts_with(':') {
            // This is a key — extract the value
            let after_colon = after_key[1..].trim_start();
            if !after_colon.starts_with('"') {
                return None;
            }
            let content = &after_colon[1..];
            let mut end = 0;
            let bytes = content.as_bytes();
            while end < bytes.len() {
                if bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\') {
                    return Some(content[..end].replace("\\\"", "\"").replace("\\n", "\n"));
                }
                end += 1;
            }
            // No closing quote — return what we have (truncated)
            return Some(content.replace("\\\"", "\"").replace("\\n", "\n"));
        }
        // Not a key — skip and continue searching
        search_from = abs_pos + pattern.len();
    }
}

/// Extract the text content from an Anthropic Messages API response.
fn extract_anthropic_text(body: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    let content = parsed.get("content")?.as_array()?;
    for block in content {
        if block.get("type")?.as_str()? == "text" {
            return block.get("text")?.as_str().map(|s| s.to_string());
        }
    }
    None
}
