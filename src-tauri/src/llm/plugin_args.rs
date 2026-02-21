//! LLM-to-tool-args bridge — generates structured JSON arguments for plugin tools.
//!
//! When a plugin tool has a non-trivial input schema (more than just `{text}`),
//! we need an LLM call to transform the raw OCR text into properly structured
//! arguments. For example, a GitHub Issues tool expects `{title, body, repo}`
//! — this module generates those from free-form screen text.

use crate::llm::streaming;

const ARGS_SYSTEM_PROMPT: &str = r#"You generate JSON arguments for a tool call. Given the tool's input schema and user-provided text, extract the relevant information and produce a JSON object that matches the schema exactly.

<rules>
1. Output ONLY valid JSON matching the input_schema. No other text.
2. Extract relevant information from the user text to fill schema fields.
3. ALL required fields must be present.
4. Use sensible defaults for optional fields when the text doesn't provide them.
5. Do NOT include fields not defined in the schema.
</rules>"#;

const ARGS_MAX_TOKENS: u32 = 512;

/// Generate structured arguments for a plugin tool call.
///
/// Uses the LLM to transform free-form OCR text into a JSON object
/// matching the tool's input schema. Falls back to `{text}` on failure.
pub async fn generate_plugin_args(
    tool_name: &str,
    tool_description: &str,
    input_schema: &serde_json::Value,
    extracted_text: &str,
) -> Result<serde_json::Value, String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "No API key configured".to_string())?;
    if api_key.is_empty() {
        return Err("No API key configured".to_string());
    }

    let schema_str = serde_json::to_string_pretty(input_schema).unwrap_or_default();
    let user_message = format!(
        "Tool: {}\nDescription: {}\n\nInput schema:\n{}\n\nUser text:\n{}",
        tool_name, tool_description, schema_str, extracted_text,
    );

    log::info!(
        "[ARGS_BRIDGE] Generating args for tool '{}' (schema has {} properties)",
        tool_name,
        input_schema
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|m| m.len())
            .unwrap_or(0)
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": super::prompts::MODEL,
            "max_tokens": ARGS_MAX_TOKENS,
            "system": ARGS_SYSTEM_PROMPT,
            "messages": [{"role": "user", "content": user_message}]
        }))
        .send()
        .await
        .map_err(|e| format!("Args bridge API call failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Args bridge API error: {}",
            &body[..200.min(body.len())]
        ));
    }

    let body = resp.text().await.map_err(|e| e.to_string())?;
    let response_text = extract_text_content(&body)?;
    let json_text = streaming::strip_code_fences(&response_text);

    let args: serde_json::Value = serde_json::from_str(&json_text)
        .map_err(|e| format!("Failed to parse generated args: {}", e))?;

    validate_required_fields(&args, input_schema)?;

    log::info!("[ARGS_BRIDGE] Generated args for '{}': {}", tool_name, args);
    Ok(args)
}

/// Check if a tool's input schema is trivial (just `{text: string}` or empty).
///
/// Trivial schemas don't need an LLM call — we pass `{text: ocr_text}` directly.
pub fn is_trivial_schema(schema: &serde_json::Value) -> bool {
    let props = schema.get("properties").and_then(|p| p.as_object());
    match props {
        None => true,
        Some(map) => map.len() <= 1 && map.contains_key("text"),
    }
}

/// Extract text content from an Anthropic Messages API response body.
fn extract_text_content(body: &str) -> Result<String, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Invalid API response: {}", e))?;
    let content = parsed
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or("No content in response")?;
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                return Ok(text.to_string());
            }
        }
    }
    Err("No text content in response".to_string())
}

/// Validate that all required fields from the schema are present in the args.
fn validate_required_fields(
    args: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    let required = schema.get("required").and_then(|r| r.as_array());
    if let Some(required_fields) = required {
        let args_obj = args
            .as_object()
            .ok_or("Generated args must be a JSON object")?;
        for field in required_fields {
            if let Some(name) = field.as_str() {
                if !args_obj.contains_key(name) {
                    return Err(format!("Missing required field: {}", name));
                }
            }
        }
    }
    Ok(())
}
