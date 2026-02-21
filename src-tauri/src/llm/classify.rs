//! Anthropic Claude CLASSIFY pipeline — streaming SSE.
//!
//! Streams the response and emits Tauri events as data becomes available:
//! - "action-menu-skeleton" at TTFT (~300ms) with contentType + summary
//! - "action-menu-complete" when the full ActionMenu JSON is parsed

use super::prompts::{self, CLASSIFY_SYSTEM_PROMPT, MAX_TOKENS, MODEL};
use super::streaming;
use super::types::{ActionMenu, ActionMenuSkeleton};
use tauri::Emitter;

/// Call Claude API with streaming to classify OCR text.
///
/// Emits Tauri events as the response streams:
/// - "action-menu-skeleton" when contentType + summary are available
/// - "action-menu-complete" when the full ActionMenu is parsed
///
/// Always returns a valid ActionMenu (fallback on any error).
pub async fn classify_streaming(
    app: &tauri::AppHandle,
    text: &str,
    has_table: bool,
    has_code: bool,
    confidence: f64,
    plugin_tools: &str,
) -> ActionMenu {
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) if !key.is_empty() => {
            eprintln!("[CLASSIFY] API key found ({} chars)", key.len());
            key
        }
        Ok(_) => {
            eprintln!("[CLASSIFY] ANTHROPIC_API_KEY is set but EMPTY");
            log::warn!("[LLM] No ANTHROPIC_API_KEY set — returning fallback actions");
            let menu = ActionMenu::fallback();
            let _ = app.emit("action-menu-complete", &menu);
            return menu;
        }
        Err(e) => {
            eprintln!("[CLASSIFY] ANTHROPIC_API_KEY not in env: {}", e);
            log::warn!("[LLM] No ANTHROPIC_API_KEY set — returning fallback actions");
            let menu = ActionMenu::fallback();
            let _ = app.emit("action-menu-complete", &menu);
            return menu;
        }
    };

    if text.trim().is_empty() {
        eprintln!("[CLASSIFY] OCR text is EMPTY — fallback");
        log::warn!("[LLM] Empty OCR text — returning fallback actions");
        let menu = ActionMenu::fallback();
        let _ = app.emit("action-menu-complete", &menu);
        return menu;
    }

    eprintln!("[CLASSIFY] OCR text: {} chars, starting API call...", text.len());

    let user_message = prompts::build_classify_message(text, confidence, has_table, has_code, plugin_tools);

    log::info!("[LLM] Provider: anthropic (streaming)");
    log::info!("[LLM] Model: {}", MODEL);

    let start = std::time::Instant::now();

    let client = reqwest::Client::new();
    let mut response = match client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": MODEL,
            "max_tokens": MAX_TOKENS,
            "stream": true,
            "system": CLASSIFY_SYSTEM_PROMPT,
            "messages": [
                {
                    "role": "user",
                    "content": user_message,
                }
            ]
        }))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            eprintln!("[CLASSIFY] HTTP request FAILED: {}", e);
            log::error!("[LLM] HTTP request failed: {}", e);
            let menu = ActionMenu::fallback();
            let _ = app.emit("action-menu-complete", &menu);
            return menu;
        }
    };

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        eprintln!("[CLASSIFY] API error {}: {}", status, body);
        log::error!("[LLM] API returned {}: {}", status, body);
        let menu = ActionMenu::fallback();
        let _ = app.emit("action-menu-complete", &menu);
        return menu;
    }

    eprintln!("[CLASSIFY] API returned 200, streaming...");

    let ttfb_ms = start.elapsed().as_millis();
    log::info!("[LLM] TTFB: {}ms", ttfb_ms);

    // Stream SSE events, accumulate text content
    let mut accumulated_text = String::new();
    let mut sse_buffer = String::new();
    let mut skeleton_emitted = false;
    let mut ttft_logged = false;
    let mut input_tokens: u64 = 0;

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                sse_buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events (separated by \n\n)
                let events = streaming::parse_sse_events(&mut sse_buffer);
                for (event_type, data) in events {
                    match event_type.as_str() {
                        "content_block_delta" => {
                            if let Some(text_delta) = extract_text_delta(&data) {
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
                        }
                        "message_start" => {
                            // Extract input token count
                            if let Ok(json) =
                                serde_json::from_str::<serde_json::Value>(&data)
                            {
                                if let Some(usage) = json
                                    .get("message")
                                    .and_then(|m| m.get("usage"))
                                {
                                    input_tokens =
                                        usage["input_tokens"].as_u64().unwrap_or(0);
                                    log::info!("[LLM] Input tokens: {}", input_tokens);
                                }
                            }
                        }
                        "message_delta" => {
                            // Extract output token count
                            if let Ok(json) =
                                serde_json::from_str::<serde_json::Value>(&data)
                            {
                                if let Some(usage) = json.get("usage") {
                                    let output_tokens =
                                        usage["output_tokens"].as_u64().unwrap_or(0);
                                    log::info!("[LLM] Output tokens: {}", output_tokens);
                                    // Haiku pricing: $0.80/M input, $4/M output
                                    let cost = (input_tokens as f64 * 0.80
                                        + output_tokens as f64 * 4.0)
                                        / 1_000_000.0;
                                    log::info!("[LLM] Estimated cost: ${:.6}", cost);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(None) => break, // Stream complete
            Err(e) => {
                log::error!("[LLM] Stream error: {}", e);
                break;
            }
        }
    }

    let api_ms = start.elapsed().as_millis();
    eprintln!("[CLASSIFY] Stream complete: {}ms, accumulated {} chars", api_ms, accumulated_text.len());

    // Parse the full accumulated text as ActionMenu
    let json_str = streaming::strip_code_fences(&accumulated_text);
    let menu = match serde_json::from_str::<ActionMenu>(&json_str) {
        Ok(menu) => {
            eprintln!("[CLASSIFY] SUCCESS — {} actions, type={}", menu.actions.len(), menu.content_type);
            log::info!("[LLM] Parsed {} actions, type={}", menu.actions.len(), menu.content_type);
            menu
        }
        Err(e) => {
            eprintln!("[CLASSIFY] JSON PARSE FAILED: {}", e);
            log::warn!("[LLM] Failed to parse ActionMenu: {} — raw: {}", e, &accumulated_text[..accumulated_text.len().min(200)]);
            ActionMenu::fallback()
        }
    };

    let _ = app.emit("action-menu-complete", &menu);
    menu
}

/// Extract the text delta from an Anthropic content_block_delta SSE data payload.
fn extract_text_delta(data: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;
    json["delta"]["text"].as_str().map(|s| s.to_string())
}

/// Non-streaming classify (used by integration tests, not the main pipeline).
pub async fn classify(
    text: &str,
    has_table: bool,
    has_code: bool,
    confidence: f64,
    plugin_tools: &str,
) -> ActionMenu {
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => return ActionMenu::fallback(),
    };
    if text.trim().is_empty() {
        return ActionMenu::fallback();
    }

    let user_message = prompts::build_classify_message(
        text, confidence, has_table, has_code, plugin_tools,
    );
    let start = std::time::Instant::now();

    let response = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": MODEL,
            "max_tokens": MAX_TOKENS,
            "system": CLASSIFY_SYSTEM_PROMPT,
            "messages": [{"role": "user", "content": user_message}]
        }))
        .send()
        .await;

    let response = match response {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            log::error!("[LLM] API returned {}", r.status());
            return ActionMenu::fallback();
        }
        Err(e) => {
            log::error!("[LLM] HTTP request failed: {}", e);
            return ActionMenu::fallback();
        }
    };

    let body: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(_) => return ActionMenu::fallback(),
    };

    log::info!("[LLM] API latency: {}ms", start.elapsed().as_millis());

    let text_content = match body["content"][0]["text"].as_str() {
        Some(t) => t,
        None => return ActionMenu::fallback(),
    };

    let json_str = streaming::strip_code_fences(text_content);
    serde_json::from_str::<ActionMenu>(&json_str).unwrap_or_else(|e| {
        log::warn!("[LLM] Failed to parse ActionMenu: {}", e);
        ActionMenu::fallback()
    })
}
