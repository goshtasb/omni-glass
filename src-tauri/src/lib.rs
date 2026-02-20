//! Omni-Glass — Tauri application entry point.
//!
//! This is the app shell that wires together:
//! - System tray (tray.rs)
//! - Screen capture domain (capture/)
//! - OCR domain (ocr/) — Apple Vision via swift-bridge FFI
//! - LLM domain (llm/) — Claude API classification
//! - Tauri command handlers for frontend communication

mod capture;
mod llm;
mod ocr;
mod tray;

use capture::CaptureState;
use tauri::Manager;

/// Tauri command: crop the stored screenshot to the given rectangle.
///
/// Called by the frontend overlay when the user releases the mouse.
/// Returns base64-encoded PNG of the cropped region.
#[tauri::command]
fn crop_region(
    state: tauri::State<'_, CaptureState>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let start = std::time::Instant::now();

    let guard = state.screenshot.lock().map_err(|e| e.to_string())?;
    let screenshot = guard
        .as_ref()
        .ok_or("No screenshot available — capture first")?;

    let png_bytes = capture::crop_to_png_bytes(screenshot, x, y, width, height)
        .map_err(|e| e.to_string())?;

    let base64_png = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &png_bytes,
    );

    let crop_ms = start.elapsed().as_millis();
    log::info!(
        "Cropped region ({}x{} at {},{}) in {}ms — {} bytes",
        width, height, x, y, crop_ms, png_bytes.len()
    );

    Ok(base64_png)
}

/// Tauri command: process a snip through the full pipeline (streaming).
///
/// crop → OCR → open skeleton menu → stream LLM classify → populate actions.
/// The action menu window opens immediately with a skeleton UI,
/// then fills in as the streaming response arrives (~300ms TTFT).
#[tauri::command]
async fn process_snip(
    app: tauri::AppHandle,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    menu_x: f64,
    menu_y: f64,
) -> Result<(), String> {
    let pipeline_start = std::time::Instant::now();

    // Stage 2a: Crop the stored screenshot
    let cropped = {
        let state = app.state::<CaptureState>();
        let guard = state.screenshot.lock().map_err(|e| e.to_string())?;
        let screenshot = guard
            .as_ref()
            .ok_or("No screenshot available — capture first")?;
        screenshot.crop_imm(x, y, width, height)
    };
    let crop_ms = pipeline_start.elapsed().as_millis();
    log::info!(
        "[CAPTURE] Bounding box received: {{x: {}, y: {}, w: {}, h: {}}}",
        x, y, width, height
    );
    log::info!("[CAPTURE] Region crop: {}ms", crop_ms);

    // Stage 2b: Encode crop to PNG bytes in memory — no disk I/O.
    let encode_start = std::time::Instant::now();
    let mut png_bytes = Vec::new();
    cropped
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("PNG encode failed: {}", e))?;
    let encode_ms = encode_start.elapsed().as_millis();
    log::info!("[CAPTURE] PNG encode: {}ms ({} bytes)", encode_ms, png_bytes.len());

    // Stage 2c: OCR — bytes passed directly, no temp file
    let ocr_start = std::time::Instant::now();
    let ocr_level = match std::env::var("OCR_MODE").unwrap_or_default().as_str() {
        "accurate" => ocr::RecognitionLevel::Accurate,
        _ => ocr::RecognitionLevel::Fast,
    };
    let ocr_result = ocr::recognize_text_from_bytes(png_bytes, ocr_level);
    let ocr_ms = ocr_start.elapsed().as_millis();
    log::info!("[OCR] Recognition level: {:?}", ocr_level);
    log::info!(
        "[OCR] Extracted {} chars in {}ms",
        ocr_result.char_count, ocr_ms
    );

    // Stage 2d: Content structure heuristics
    let has_table = ocr::heuristics::detect_table_structure(&ocr_result.text);
    let has_code = ocr::heuristics::detect_code_structure(&ocr_result.text);
    log::info!("[OCR] has_table_structure: {}", has_table);
    log::info!("[OCR] has_code_structure: {}", has_code);

    // Store OCR text early — action menu needs it for Copy Text (available in skeleton)
    let menu_state = app.state::<llm::ActionMenuState>();
    *menu_state.ocr_text.lock().unwrap() = Some(ocr_result.text.clone());

    // Stage 3a: Close overlay
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.destroy();
    }

    // Stage 3b: Open action menu window BEFORE LLM call.
    // Shows skeleton immediately — Copy Text is clickable, summary shimmer visible.
    let render_start = std::time::Instant::now();

    if let Some(existing) = app.get_webview_window("action-menu") {
        let _ = existing.destroy();
    }

    let _menu_window = tauri::WebviewWindowBuilder::new(
        &app,
        "action-menu",
        tauri::WebviewUrl::App("action-menu.html".into()),
    )
    .title("Omni-Glass Actions")
    .inner_size(300.0, 280.0)
    .position(menu_x, menu_y + 8.0)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .build()
    .map_err(|e| format!("Failed to create action menu window: {}", e))?;

    let render_ms = render_start.elapsed().as_millis();
    let local_ms = pipeline_start.elapsed().as_millis();
    log::info!("[RENDER] Skeleton menu window created in {}ms", render_ms);
    log::info!(
        "[PIPELINE] Local processing: {}ms (crop={} + encode={} + ocr={} + window={})",
        local_ms, crop_ms, encode_ms, ocr_ms, render_ms
    );

    // Stage 4: Stream LLM classify — emits events to the action menu window.
    // "action-menu-skeleton" at TTFT with contentType + summary
    // "action-menu-complete" when full ActionMenu JSON is parsed
    //
    // Provider selection: LLM_PROVIDER env var > first configured key
    let provider = resolve_provider();
    let action_menu = match provider.as_str() {
        "gemini" => {
            llm::classify_streaming_gemini(
                &app,
                &ocr_result.text,
                has_table,
                has_code,
                ocr_result.confidence,
            )
            .await
        }
        _ => {
            llm::classify_streaming(
                &app,
                &ocr_result.text,
                has_table,
                has_code,
                ocr_result.confidence,
            )
            .await
        }
    };

    // Store final ActionMenu in state (fallback for get_action_menu command)
    *menu_state.menu.lock().unwrap() = Some(action_menu);

    let total_ms = pipeline_start.elapsed().as_millis();
    log::info!(
        "[PIPELINE] Total (mouse-up to actions complete): {}ms",
        total_ms
    );
    log::info!(
        "[PIPELINE] Perceived latency (mouse-up to skeleton): ~{}ms + TTFT",
        local_ms
    );

    Ok(())
}

/// Tauri command: get capture info (screenshot path + click timestamp).
///
/// Called by the overlay on load. This replaces the event-based approach
/// which raced — the event fired before JS was ready to listen.
#[tauri::command]
fn get_capture_info(
    state: tauri::State<'_, CaptureState>,
) -> Result<capture::CaptureInfo, String> {
    let guard = state.capture_info.lock().map_err(|e| e.to_string())?;
    guard
        .clone()
        .ok_or("No capture info available".to_string())
}

/// Tauri command: get the OCR text from the last snip.
///
/// Used by action menu to copy text to clipboard.
#[tauri::command]
fn get_ocr_text(
    state: tauri::State<'_, llm::ActionMenuState>,
) -> Result<String, String> {
    let guard = state.ocr_text.lock().map_err(|e| e.to_string())?;
    guard
        .clone()
        .ok_or("No OCR text available".to_string())
}

/// Tauri command: copy text to the system clipboard.
///
/// Uses arboard for native clipboard access — works reliably
/// unlike navigator.clipboard in transparent webview windows.
#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_text(&text)
        .map_err(|e| e.to_string())?;
    log::info!("[ACTION] Copied {} chars to clipboard", text.len());
    Ok(())
}

/// Tauri command: close the overlay and clean up capture state.
#[tauri::command]
fn close_overlay(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("overlay") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: close the action menu window.
#[tauri::command]
fn close_action_menu(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("action-menu") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: get the current ActionMenu data.
///
/// Called by the action-menu window on load.
#[tauri::command]
fn get_action_menu(
    state: tauri::State<'_, llm::ActionMenuState>,
) -> Result<llm::ActionMenu, String> {
    let guard = state.menu.lock().map_err(|e| e.to_string())?;
    guard
        .clone()
        .ok_or("No action menu available".to_string())
}

// ── Settings panel commands ──────────────────────────────────────────

/// Tauri command: get provider configuration for the settings panel.
#[tauri::command]
fn get_provider_config() -> Result<serde_json::Value, String> {
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
fn set_active_provider(provider_id: String) -> Result<(), String> {
    // Store in env var for this session.
    // Persistent storage will use the settings file in a future iteration.
    std::env::set_var("LLM_PROVIDER", &provider_id);
    log::info!("[SETTINGS] Active provider set to: {}", provider_id);
    Ok(())
}

/// Tauri command: save an API key to the OS keychain.
#[tauri::command]
fn save_api_key(provider_id: String, api_key: String) -> Result<(), String> {
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
        _ => return Err(format!("Unknown provider: {}", provider_id)),
    };
    std::env::set_var(env_key, &api_key);

    log::info!("[SETTINGS] API key saved for provider: {}", provider_id);
    Ok(())
}

/// Tauri command: test a provider's API connection.
///
/// Sends a minimal classify request with hardcoded text and checks
/// for a valid JSON response.
#[tauri::command]
async fn test_provider(provider_id: String) -> Result<bool, String> {
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
fn close_settings(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: get the current OCR recognition mode.
#[tauri::command]
fn get_ocr_mode() -> String {
    std::env::var("OCR_MODE")
        .unwrap_or_else(|_| "fast".to_string())
        .to_lowercase()
}

/// Tauri command: set the OCR recognition mode.
#[tauri::command]
fn set_ocr_mode(mode: String) -> Result<(), String> {
    let mode = mode.to_lowercase();
    if mode != "fast" && mode != "accurate" {
        return Err(format!("Invalid OCR mode: {}. Use 'fast' or 'accurate'.", mode));
    }
    std::env::set_var("OCR_MODE", &mode);
    log::info!("[SETTINGS] OCR mode set to: {}", mode);
    Ok(())
}

/// Tauri command: open the settings window.
///
/// Called from the tray context menu.
#[tauri::command]
fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    // If settings window already exists, focus it
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

/// Determine which LLM provider to use.
///
/// Priority:
/// 1. LLM_PROVIDER env var (explicit override: "anthropic" or "gemini")
/// 2. First provider with an API key set (env var or keychain)
/// 3. "anthropic" as final default
fn resolve_provider() -> String {
    // Explicit override
    if let Ok(p) = std::env::var("LLM_PROVIDER") {
        let p = p.to_lowercase();
        if p == "gemini" || p == "anthropic" {
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
fn has_api_key(provider_id: &str) -> bool {
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

/// Entry point — called by Tauri runtime.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(CaptureState::new())
        .manage(llm::ActionMenuState::new())
        .invoke_handler(tauri::generate_handler![
            crop_region,
            process_snip,
            get_capture_info,
            close_overlay,
            close_action_menu,
            get_action_menu,
            get_ocr_text,
            copy_to_clipboard,
            get_provider_config,
            set_active_provider,
            save_api_key,
            test_provider,
            close_settings,
            open_settings,
            get_ocr_mode,
            set_ocr_mode,
        ])
        .setup(|app| {
            log::info!("Omni-Glass starting up");

            // Warm up Vision Framework to avoid cold-start penalty on first snip
            let warm_start = std::time::Instant::now();
            ocr::warm_up();
            log::info!(
                "[OCR] Vision warm-up complete in {}ms",
                warm_start.elapsed().as_millis()
            );

            tray::setup_tray(app.handle())?;

            log::info!("System tray initialized — ready for snips");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error running Omni-Glass");
}
