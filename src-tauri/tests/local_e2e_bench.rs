//! E2E benchmark: Load model + CLASSIFY + EXECUTE using local LLM.
//!
//! Only runs when `--features local-llm` is enabled AND the model file exists.
//! Run with: cargo test --features local-llm --test local_e2e_bench -- --nocapture

#![cfg(feature = "local-llm")]

use omni_glass_lib::llm::local_state::LocalLlmState;
use omni_glass_lib::llm::model_manager;
use omni_glass_lib::llm::model_registry;
use omni_glass_lib::llm::prompts_execute_local;
use omni_glass_lib::llm::prompts_local;

fn model_exists() -> bool {
    let model = model_registry::default_model();
    model_manager::is_model_downloaded(model)
}

#[tokio::test]
async fn e2e_load_classify_execute() {
    if !model_exists() {
        eprintln!("SKIP: Model not downloaded. Download first via Settings UI.");
        return;
    }

    let model = model_registry::default_model();
    let model_path = model_manager::model_path(model);

    // Stage 1: Load model
    let load_start = std::time::Instant::now();
    let state = LocalLlmState::new();
    state.load(&model_path, model.id).await.expect("Model load failed");
    let load_ms = load_start.elapsed().as_millis();
    eprintln!("[BENCH] Model load: {}ms", load_ms);

    assert!(state.is_loaded().await, "Model should be loaded");

    // Stage 2: CLASSIFY — simulated error message (no grammar — disabled due to crash bug)
    let test_text = "TypeError: Cannot read properties of undefined (reading 'map')\n    at UserList (UserList.tsx:42:18)\n    at renderWithHooks (react-dom.development.js:16305)";

    let classify_prompt = prompts_local::build_local_classify_prompt(
        test_text, 0.92, false, true, "",
    );

    let classify_start = std::time::Instant::now();
    let classify_result = state
        .generate(&classify_prompt, prompts_local::LOCAL_CLASSIFY_MAX_TOKENS, None)
        .await;
    let classify_ms = classify_start.elapsed().as_millis();
    eprintln!("[BENCH] CLASSIFY: {}ms", classify_ms);

    let classify_ok = match &classify_result {
        Ok(json) => {
            eprintln!("[BENCH] CLASSIFY raw ({} chars): {}", json.len(), &json[..json.len().min(500)]);
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(json);
            match parsed {
                Ok(v) => {
                    eprintln!("[BENCH] CLASSIFY parsed OK — contentType: {:?}", v.get("contentType"));
                    let actions = v.get("actions").and_then(|a| a.as_array());
                    if let Some(actions) = actions {
                        eprintln!("[BENCH] CLASSIFY actions: {}", actions.len());
                        for a in actions {
                            eprintln!("  - {} ({})",
                                a.get("label").and_then(|l| l.as_str()).unwrap_or("?"),
                                a.get("id").and_then(|i| i.as_str()).unwrap_or("?"));
                        }
                    }
                    true
                }
                Err(e) => {
                    eprintln!("[BENCH] CLASSIFY parse error: {}", e);
                    false
                }
            }
        }
        Err(e) => {
            eprintln!("[BENCH] CLASSIFY error: {}", e);
            false
        }
    };
    assert!(classify_result.is_ok(), "CLASSIFY generation should succeed");

    // Stage 3: EXECUTE — explain error (no grammar)
    let execute_prompt = prompts_execute_local::build_local_execute_prompt(
        "explain_error", test_text, "macos",
    );

    let execute_start = std::time::Instant::now();
    let execute_result = state
        .generate(&execute_prompt, prompts_execute_local::LOCAL_EXECUTE_MAX_TOKENS, None)
        .await;
    let execute_ms = execute_start.elapsed().as_millis();
    eprintln!("[BENCH] EXECUTE: {}ms", execute_ms);

    let execute_ok = match &execute_result {
        Ok(text) => {
            eprintln!("[BENCH] EXECUTE raw ({} chars): {}", text.len(), &text[..text.len().min(500)]);
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(text);
            match parsed {
                Ok(v) => {
                    eprintln!("[BENCH] EXECUTE parsed as JSON — status: {:?}", v.get("status"));
                    true
                }
                Err(_) => {
                    eprintln!("[BENCH] EXECUTE returned prose (not JSON) — wrapping as text result");
                    // This is fine — the production code wraps prose as a text ActionResult
                    text.contains("TypeError") || text.contains("undefined") || text.contains("error") || text.len() > 50
                }
            }
        }
        Err(e) => {
            eprintln!("[BENCH] EXECUTE error: {}", e);
            false
        }
    };
    assert!(execute_result.is_ok(), "EXECUTE generation should succeed");

    // Stage 4: TEXT LAUNCHER — "What is 2+2?"
    let text_prompt = prompts_execute_local::build_local_text_command_prompt(
        "What is 2+2?",
        "- calculator: Does math calculations",
    );

    let text_start = std::time::Instant::now();
    let text_result = state.generate(&text_prompt, 256, None).await;
    let text_ms = text_start.elapsed().as_millis();
    eprintln!("[BENCH] TEXT_CMD: {}ms", text_ms);

    let text_ok = match &text_result {
        Ok(text) => {
            eprintln!("[BENCH] TEXT_CMD raw ({} chars): {}", text.len(), &text[..text.len().min(300)]);
            // Check if it contains "4" somewhere (the answer to 2+2)
            let has_answer = text.contains('4');
            eprintln!("[BENCH] TEXT_CMD contains answer '4': {}", has_answer);
            text.len() > 5
        }
        Err(e) => {
            eprintln!("[BENCH] TEXT_CMD error: {}", e);
            false
        }
    };
    assert!(text_result.is_ok(), "TEXT_CMD generation should succeed");

    // Summary
    let total_ms = load_start.elapsed().as_millis();
    eprintln!("\n=== BENCHMARK SUMMARY ===");
    eprintln!("Model:     {} ({})", model.name, model.id);
    eprintln!("Grammar:   DISABLED (llama-cpp-2 v0.1.135 crash bug)");
    eprintln!("Load:      {}ms", load_ms);
    eprintln!("CLASSIFY:  {}ms (valid JSON: {})", classify_ms, classify_ok);
    eprintln!("EXECUTE:   {}ms (useful output: {})", execute_ms, execute_ok);
    eprintln!("TEXT_CMD:  {}ms (useful output: {})", text_ms, text_ok);
    eprintln!("Pipeline:  {}ms (CLASSIFY + EXECUTE, excludes model load)", classify_ms + execute_ms);
    eprintln!("Total:     {}ms (all stages including model load)", total_ms);
    eprintln!("========================\n");
}
