//! Test that the raw Anthropic streaming API works correctly.
//!
//! This bypasses Tauri completely and tests the streaming SSE parsing
//! with the real Anthropic API. If this passes but the GUI shows fallback,
//! the issue is event delivery timing, not the streaming itself.

use omni_glass_lib::llm::prompts::{self, CLASSIFY_SYSTEM_PROMPT, MAX_TOKENS, MODEL};
use omni_glass_lib::llm::streaming;
use omni_glass_lib::llm::types::ActionMenu;

fn load_env() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.parent().unwrap_or(manifest_dir);
    let env_path = project_root.join(".env.local");
    if env_path.exists() {
        dotenvy::from_path(&env_path).ok();
    }
}

#[tokio::test]
async fn test_streaming_classify_accumulates_text() {
    load_env();

    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => {
            eprintln!("SKIP: No ANTHROPIC_API_KEY");
            return;
        }
    };

    let ocr_text = r#"Traceback (most recent call last):
  File "/usr/local/lib/python3.11/site-packages/app.py", line 42, in main
    import pandas
ModuleNotFoundError: No module named 'pandas'"#;

    let user_message = prompts::build_classify_message(ocr_text, 0.95, false, false);

    eprintln!("[TEST] Sending streaming request to {} ...", MODEL);
    let start = std::time::Instant::now();

    let client = reqwest::Client::new();
    let mut response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": MODEL,
            "max_tokens": MAX_TOKENS,
            "stream": true,
            "system": CLASSIFY_SYSTEM_PROMPT,
            "messages": [{"role": "user", "content": user_message}]
        }))
        .send()
        .await
        .expect("HTTP request failed");

    let status = response.status();
    eprintln!("[TEST] HTTP status: {}", status);
    assert!(status.is_success(), "API returned error: {}", status);

    // Stream and accumulate
    let mut accumulated_text = String::new();
    let mut sse_buffer = String::new();
    let mut chunk_count = 0;

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                chunk_count += 1;
                let chunk_str = String::from_utf8_lossy(&chunk);
                sse_buffer.push_str(&chunk_str);

                let events = streaming::parse_sse_events(&mut sse_buffer);
                for (event_type, data) in &events {
                    if event_type == "content_block_delta" {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(text) = json["delta"]["text"].as_str() {
                                accumulated_text.push_str(text);
                            }
                        }
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                panic!("[TEST] Stream error: {}", e);
            }
        }
    }

    let elapsed = start.elapsed().as_millis();
    eprintln!("[TEST] Stream complete: {}ms, {} chunks, {} chars accumulated",
        elapsed, chunk_count, accumulated_text.len());
    eprintln!("[TEST] Accumulated text preview: {}",
        &accumulated_text[..accumulated_text.len().min(200)]);

    // Verify we got actual content
    assert!(
        !accumulated_text.is_empty(),
        "Streaming accumulated ZERO text — SSE parsing may be broken"
    );

    // Parse the JSON
    let json_str = streaming::strip_code_fences(&accumulated_text);
    let menu: ActionMenu = serde_json::from_str(&json_str)
        .unwrap_or_else(|e| panic!("JSON parse failed: {}. Raw: {}", e, accumulated_text));

    eprintln!("[TEST] Parsed: content_type={}, summary={}, actions={}",
        menu.content_type, menu.summary, menu.actions.len());

    assert_ne!(menu.content_type, "unknown");
    assert_ne!(menu.summary, "Could not analyze content");
    assert!(menu.actions.len() >= 3);
}
