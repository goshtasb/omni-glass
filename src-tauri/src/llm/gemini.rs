//! Gemini Flash CLASSIFY pipeline — streaming SSE via Google AI API.
//!
//! Mirrors the Anthropic streaming implementation in classify.rs:
//! - "action-menu-skeleton" emitted when contentType + summary are parsed
//! - "action-menu-complete" emitted when full ActionMenu JSON is available
//!
//! Key differences from Anthropic:
//! - API key in URL query param, not header
//! - `responseMimeType: "application/json"` enforces valid JSON (no fence stripping)
//! - SSE events are `data: {...}` lines without `event:` prefix
//! - Text chunks in `candidates[0].content.parts[0].text`
//! - Token usage in `usageMetadata` of final chunk

use super::prompts::CLASSIFY_SYSTEM_PROMPT;
use super::streaming;
use super::types::{ActionMenu, ActionMenuSkeleton};
use tauri::Emitter;

pub const GEMINI_MODEL: &str = "gemini-2.0-flash";
pub const GEMINI_MAX_TOKENS: u32 = 512;

/// Gemini Flash pricing (as of Feb 2026):
/// Input:  $0.10 per 1M tokens (under 128k context)
/// Output: $0.40 per 1M tokens (under 128k context)
const INPUT_COST_PER_MILLION: f64 = 0.10;
const OUTPUT_COST_PER_MILLION: f64 = 0.40;

/// Stream a CLASSIFY request through Gemini Flash.
///
/// Same contract as `classify_streaming` in classify.rs:
/// - Emits "action-menu-skeleton" as soon as contentType + summary are available
/// - Emits "action-menu-complete" when the full JSON is parsed
/// - Always returns a valid ActionMenu (fallback on any error)
pub async fn classify_streaming_gemini(
    app: &tauri::AppHandle,
    text: &str,
    has_table: bool,
    has_code: bool,
    confidence: f64,
    plugin_tools: &str,
) -> ActionMenu {
    let api_key = match std::env::var("GEMINI_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => {
            log::warn!("[LLM] No GEMINI_API_KEY set — returning fallback actions");
            let menu = ActionMenu::fallback();
            let _ = app.emit("action-menu-complete", &menu);
            return menu;
        }
    };

    if text.trim().is_empty() {
        log::warn!("[LLM] Empty OCR text — returning fallback actions");
        let menu = ActionMenu::fallback();
        let _ = app.emit("action-menu-complete", &menu);
        return menu;
    }

    let user_message = super::prompts::build_classify_message(text, confidence, has_table, has_code, plugin_tools);

    log::info!("[LLM] Provider: gemini (streaming)");
    log::info!("[LLM] Model: {}", GEMINI_MODEL);

    let start = std::time::Instant::now();

    // Gemini streaming endpoint — API key in URL query param
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
        GEMINI_MODEL, api_key
    );

    let client = reqwest::Client::new();
    let mut response = match client
        .post(&url)
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        {
                            "text": user_message
                        }
                    ]
                }
            ],
            "systemInstruction": {
                "parts": [
                    {
                        "text": CLASSIFY_SYSTEM_PROMPT
                    }
                ]
            },
            "generationConfig": {
                "maxOutputTokens": GEMINI_MAX_TOKENS,
                "temperature": 0.1,
                "responseMimeType": "application/json"
            }
        }))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("[LLM] HTTP request failed: {}", e);
            let menu = ActionMenu::fallback();
            let _ = app.emit("action-menu-complete", &menu);
            return menu;
        }
    };

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        log::error!("[LLM] Gemini API returned {}: {}", status, body);
        let menu = ActionMenu::fallback();
        let _ = app.emit("action-menu-complete", &menu);
        return menu;
    }

    let ttfb_ms = start.elapsed().as_millis();
    log::info!("[LLM] TTFB: {}ms", ttfb_ms);

    // Stream SSE events, accumulate text content
    let mut accumulated_text = String::new();
    let mut sse_buffer = String::new();
    let mut skeleton_emitted = false;
    let mut ttft_logged = false;
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                sse_buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events (Gemini: data-only, no event: prefix)
                let events = streaming::parse_data_only_sse_events(&mut sse_buffer);
                for data in events {
                    // Extract text from candidates[0].content.parts[0].text
                    if let Some(text_delta) = extract_gemini_text(&data) {
                        if !ttft_logged && !text_delta.is_empty() {
                            log::info!(
                                "[LLM] TTFT: {}ms",
                                start.elapsed().as_millis()
                            );
                            ttft_logged = true;
                        }
                        accumulated_text.push_str(&text_delta);

                        // Try to extract skeleton from partial JSON
                        if !skeleton_emitted {
                            if let Some((ct, summary)) =
                                streaming::try_extract_skeleton(&accumulated_text)
                            {
                                let skeleton = ActionMenuSkeleton {
                                    content_type: ct,
                                    summary,
                                };
                                log::info!(
                                    "[LLM] Skeleton emitted at {}ms",
                                    start.elapsed().as_millis()
                                );
                                let _ = app.emit("action-menu-skeleton", &skeleton);
                                skeleton_emitted = true;
                            }
                        }
                    }

                    // Extract token usage from usageMetadata (present in final chunk)
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                        if let Some(usage) = json.get("usageMetadata") {
                            input_tokens = usage["promptTokenCount"].as_u64().unwrap_or(0);
                            output_tokens = usage["candidatesTokenCount"].as_u64().unwrap_or(0);
                        }
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                log::error!("[LLM] Stream error: {}", e);
                break;
            }
        }
    }

    let api_ms = start.elapsed().as_millis();
    log::info!("[LLM] Stream complete: {}ms", api_ms);

    // Log token usage and cost
    if input_tokens > 0 || output_tokens > 0 {
        log::info!("[LLM] Input tokens: {}", input_tokens);
        log::info!("[LLM] Output tokens: {}", output_tokens);
        let cost = (input_tokens as f64 * INPUT_COST_PER_MILLION
            + output_tokens as f64 * OUTPUT_COST_PER_MILLION)
            / 1_000_000.0;
        log::info!("[LLM] Estimated cost: ${:.6}", cost);
    }

    // Parse accumulated text as ActionMenu
    // Gemini with responseMimeType should return clean JSON — no fence stripping needed
    let json_str = accumulated_text.trim();
    let menu = match serde_json::from_str::<ActionMenu>(json_str) {
        Ok(menu) => {
            log::info!("[LLM] Parse result: success");
            log::info!("[LLM] Content type: {}", menu.content_type);
            log::info!("[LLM] Actions: {}", menu.actions.len());
            for action in &menu.actions {
                log::info!(
                    "[LLM]   #{} {} ({}): {}",
                    action.priority, action.label, action.id, action.description
                );
            }
            log::info!("[LLM] JSON enforcement: responseMimeType (no fence stripping)");
            menu
        }
        Err(e) => {
            log::warn!("[LLM] Failed to parse ActionMenu: {}", e);
            log::warn!("[LLM] Raw accumulated: {}", accumulated_text);
            log::info!("[LLM] Parse result: fallback");
            ActionMenu::fallback()
        }
    };

    let _ = app.emit("action-menu-complete", &menu);
    menu
}

/// Extract text content from a Gemini SSE data payload.
///
/// Gemini format: candidates[0].content.parts[0].text
fn extract_gemini_text(data: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;
    json.get("candidates")?
        .get(0)?
        .get("content")?
        .get("parts")?
        .get(0)?
        .get("text")?
        .as_str()
        .map(|s| s.to_string())
}
