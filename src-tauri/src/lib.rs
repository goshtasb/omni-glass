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

    // Stage 2c: OCR via Apple Vision FFI — bytes passed directly, no temp file
    let ocr_start = std::time::Instant::now();
    let ocr_result = ocr::recognize_text_from_bytes(
        png_bytes,
        ocr::RecognitionLevel::Fast,
    );
    let ocr_ms = ocr_start.elapsed().as_millis();
    log::info!("[OCR] Recognition level: fast");
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
    let action_menu = llm::classify_streaming(
        &app,
        &ocr_result.text,
        has_table,
        has_code,
        ocr_result.confidence,
    )
    .await;

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
