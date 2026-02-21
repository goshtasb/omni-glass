//! Settings panel Tauri commands and provider resolution.
//!
//! Handles:
//! - Provider configuration (get/set active provider, save API keys)
//! - API key storage (OS keychain via keyring crate + env var)
//! - Provider connection testing
//! - OCR mode get/set
//! - Settings window lifecycle

use crate::llm;
use tauri::Manager;

// ── Provider resolution ──────────────────────────────────────────────

/// Determine which LLM provider to use.
///
/// Priority:
/// 1. LLM_PROVIDER env var (explicit override: "anthropic" or "gemini")
/// 2. First provider with an API key set (env var or keychain)
/// 3. "anthropic" as final default
pub fn resolve_provider() -> String {
    // Explicit override
    if let Ok(p) = std::env::var("LLM_PROVIDER") {
        let p = p.to_lowercase();
        if matches!(p.as_str(), "anthropic" | "gemini" | "local") {
            log::info!("[LLM] Provider override: {}", p);
            return p;
        }
    }

    // Auto-detect: first configured key wins
    if has_api_key("anthropic") {
        return "anthropic".to_string();
    }
    if has_api_key("gemini") {
        return "gemini".to_string();
    }

    // Default (will trigger fallback menu since no key is set)
    "anthropic".to_string()
}

/// Check if a provider has an API key available (env var or keychain).
/// If found in keychain but not in env, loads it into env for the provider to use.
/// For "local" provider, checks if any model is downloaded (no API key needed).
fn has_api_key(provider_id: &str) -> bool {
    #[cfg(feature = "local-llm")]
    if provider_id == "local" {
        return !llm::model_manager::downloaded_model_ids().is_empty();
    }
    let env_key = match provider_id {
        "anthropic" => "ANTHROPIC_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        _ => return false,
    };

    // Check env var first
    if std::env::var(env_key).map(|k| !k.is_empty()).unwrap_or(false) {
        return true;
    }

    // Check OS keychain
    if let Ok(entry) = keyring::Entry::new("omni-glass", provider_id) {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() {
                // Load into env so the provider functions can read it
                std::env::set_var(env_key, &key);
                log::info!("[SETTINGS] Loaded {} key from OS keychain", provider_id);
                return true;
            }
        }
    }

    false
}

// ── Tauri commands ───────────────────────────────────────────────────

/// Tauri command: get provider configuration for the settings panel.
#[tauri::command]
pub fn get_provider_config() -> Result<serde_json::Value, String> {
    let providers = llm::provider::all_providers();
    let active = resolve_provider();
    let configured: Vec<String> = providers
        .iter()
        .filter(|p| llm::provider::is_provider_configured(&p.id))
        .map(|p| p.id.clone())
        .collect();

    Ok(serde_json::json!({
        "activeProvider": active,
        "providers": providers,
        "configuredProviders": configured,
    }))
}

/// Tauri command: set the active LLM provider.
#[tauri::command]
pub fn set_active_provider(provider_id: String) -> Result<(), String> {
    std::env::set_var("LLM_PROVIDER", &provider_id);
    log::info!("[SETTINGS] Active provider set to: {}", provider_id);
    Ok(())
}

/// Tauri command: save an API key to the OS keychain.
#[tauri::command]
pub fn save_api_key(provider_id: String, api_key: String) -> Result<(), String> {
    // Save to OS keychain
    let entry = keyring::Entry::new("omni-glass", &provider_id)
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .set_password(&api_key)
        .map_err(|e| format!("Failed to save key: {}", e))?;

    // Also set as env var so the current session picks it up immediately
    let env_key = match provider_id.as_str() {
        "anthropic" => "ANTHROPIC_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        "local" => return Ok(()), // No API key needed for local provider
        _ => return Err(format!("Unknown provider: {}", provider_id)),
    };
    std::env::set_var(env_key, &api_key);

    log::info!("[SETTINGS] API key saved for provider: {}", provider_id);
    Ok(())
}

/// Tauri command: test a provider's API connection.
///
/// Sends a minimal request and checks for a valid response.
#[tauri::command]
pub async fn test_provider(provider_id: String) -> Result<bool, String> {
    // Local provider — no API to test, just check if a model is downloaded
    if provider_id == "local" {
        #[cfg(feature = "local-llm")]
        {
            let ok = !llm::model_manager::downloaded_model_ids().is_empty();
            log::info!("[SETTINGS] Test local — model downloaded: {}", ok);
            return Ok(ok);
        }
        #[cfg(not(feature = "local-llm"))]
        {
            return Ok(false);
        }
    }

    let (url, headers, body) = match provider_id.as_str() {
        "anthropic" => {
            let key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| "No ANTHROPIC_API_KEY set".to_string())?;
            (
                "https://api.anthropic.com/v1/messages".to_string(),
                vec![
                    ("x-api-key".to_string(), key),
                    ("anthropic-version".to_string(), "2023-06-01".to_string()),
                    ("content-type".to_string(), "application/json".to_string()),
                ],
                serde_json::json!({
                    "model": "claude-haiku-4-5-20251001",
                    "max_tokens": 50,
                    "messages": [{"role": "user", "content": "Reply with just: ok"}]
                }),
            )
        }
        "gemini" => {
            let key = std::env::var("GEMINI_API_KEY")
                .map_err(|_| "No GEMINI_API_KEY set".to_string())?;
            (
                format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key={}",
                    key
                ),
                vec![("content-type".to_string(), "application/json".to_string())],
                serde_json::json!({
                    "contents": [{"role": "user", "parts": [{"text": "Reply with just: ok"}]}],
                    "generationConfig": {"maxOutputTokens": 50}
                }),
            )
        }
        _ => return Err(format!("Unknown provider: {}", provider_id)),
    };

    let client = reqwest::Client::new();
    let mut req = client.post(&url);
    for (k, v) in headers {
        req = req.header(&k, &v);
    }
    let resp = req.json(&body).send().await.map_err(|e| e.to_string())?;

    let ok = resp.status().is_success();
    log::info!(
        "[SETTINGS] Test {} — status: {}",
        provider_id,
        resp.status()
    );
    Ok(ok)
}

/// Tauri command: close the settings window.
#[tauri::command]
pub fn close_settings(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: open the settings window.
///
/// Called from the tray context menu.
#[tauri::command]
pub fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_focus();
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Omni-Glass Settings")
    .inner_size(520.0, 500.0)
    .resizable(true)
    .build()
    .map_err(|e| format!("Failed to create settings window: {}", e))?;

    Ok(())
}

/// Tauri command: get the current OCR recognition mode.
#[tauri::command]
pub fn get_ocr_mode() -> String {
    std::env::var("OCR_MODE")
        .unwrap_or_else(|_| "fast".to_string())
        .to_lowercase()
}

/// Tauri command: set the OCR recognition mode.
#[tauri::command]
pub fn set_ocr_mode(mode: String) -> Result<(), String> {
    let mode = mode.to_lowercase();
    if mode != "fast" && mode != "accurate" {
        return Err(format!("Invalid OCR mode: {}. Use 'fast' or 'accurate'.", mode));
    }
    std::env::set_var("OCR_MODE", &mode);
    log::info!("[SETTINGS] OCR mode set to: {}", mode);
    Ok(())
}
