//! Integration test for the CLASSIFY pipeline.
//!
//! Tests that the non-streaming classify function returns a valid
//! ActionMenu (not fallback) when given real OCR text and a real API key.
//!
//! Loads the API key from .env.local using dotenvy — same as the app.

use omni_glass_lib::llm::classify;
use omni_glass_lib::llm::types::ActionMenu;

fn load_env() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.parent().unwrap_or(manifest_dir);
    let env_path = project_root.join(".env.local");
    eprintln!("[TEST] Loading env from: {}", env_path.display());
    if env_path.exists() {
        dotenvy::from_path(&env_path).expect("Failed to load .env.local");
        eprintln!("[TEST] Loaded .env.local");
    } else {
        eprintln!("[TEST] .env.local NOT FOUND at {}", env_path.display());
    }
    let key_present = std::env::var("ANTHROPIC_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false);
    eprintln!("[TEST] ANTHROPIC_API_KEY present: {}", key_present);
}

#[tokio::test]
async fn test_classify_returns_real_actions() {
    load_env();

    let key_present = std::env::var("ANTHROPIC_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false);
    if !key_present {
        eprintln!("SKIP: No ANTHROPIC_API_KEY");
        return;
    }

    let ocr_text = r#"Traceback (most recent call last):
  File "/usr/local/lib/python3.11/site-packages/app.py", line 42, in main
    import pandas
ModuleNotFoundError: No module named 'pandas'"#;

    eprintln!("[TEST] Calling classify with {} chars...", ocr_text.len());
    let start = std::time::Instant::now();
    let menu = classify(ocr_text, false, false, 0.95, "").await;
    let latency = start.elapsed().as_millis();

    eprintln!("[TEST] Classify returned in {}ms", latency);
    eprintln!("[TEST] content_type: {}", menu.content_type);
    eprintln!("[TEST] summary: {}", menu.summary);
    eprintln!("[TEST] actions: {}", menu.actions.len());
    for a in &menu.actions {
        eprintln!("[TEST]   #{} {} ({})", a.priority, a.label, a.id);
    }

    // The critical assertion: NOT the fallback
    assert_ne!(
        menu.summary, "Could not analyze content",
        "CLASSIFY returned FALLBACK — the API call failed!"
    );
    assert_ne!(
        menu.content_type, "unknown",
        "CLASSIFY returned unknown content type — likely fallback"
    );
    assert!(
        menu.actions.len() >= 3,
        "Expected at least 3 actions, got {}",
        menu.actions.len()
    );
    // Should detect this as an error
    assert_eq!(
        menu.content_type, "error",
        "Expected content_type=error for a Python traceback"
    );
}

#[tokio::test]
async fn test_classify_empty_text_returns_fallback() {
    load_env();

    let menu = classify("", false, false, 0.0, "").await;
    assert_eq!(menu.summary, "Could not analyze content");
    assert_eq!(menu.content_type, "unknown");
}
