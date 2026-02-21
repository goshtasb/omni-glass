//! Core snip-to-action pipeline commands.
//!
//! These are the multi-step orchestration commands:
//! - process_snip: crop → OCR → open skeleton menu → stream LLM classify
//! - execute_action: OCR text + chosen action → LLM execute → ActionResult

use crate::capture::CaptureState;
use crate::llm;
use crate::mcp;
use crate::ocr;
use crate::settings_commands::resolve_provider;
use tauri::Manager;

/// Tauri command: process a snip through the full pipeline (streaming).
///
/// crop → OCR → open skeleton menu → stream LLM classify → populate actions.
/// The action menu window opens immediately with a skeleton UI,
/// then fills in as the streaming response arrives (~300ms TTFT).
#[tauri::command]
pub async fn process_snip(
    app: tauri::AppHandle,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    menu_x: f64,
    menu_y: f64,
) -> Result<(), String> {
    let pipeline_start = std::time::Instant::now();

    // Write diagnostics to Desktop for debugging — appends each stage.
    let diag_path = dirs::desktop_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("omni-glass-debug.log");
    fn diag_write(path: &std::path::Path, msg: &str) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{}", msg);
        }
    }
    // Clear old log and start fresh
    let _ = std::fs::write(&diag_path, "");
    diag_write(&diag_path, &format!("=== SNIP: {}x{} at ({},{}) ===", width, height, x, y));

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
    diag_write(&diag_path, &format!("crop: {}ms", crop_ms));
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
    let png_bytes_for_reocr = png_bytes.clone();
    let ocr_result = ocr::recognize_text_from_bytes(png_bytes, ocr_level);
    let ocr_ms = ocr_start.elapsed().as_millis();
    diag_write(&diag_path, &format!("ocr: {} chars in {}ms, confidence={:.2}", ocr_result.char_count, ocr_ms, ocr_result.confidence));
    if ocr_result.char_count == 0 {
        diag_write(&diag_path, "WARNING: OCR returned ZERO characters!");
    } else {
        diag_write(&diag_path, &format!("ocr_preview: {:?}", &ocr_result.text[..ocr_result.text.len().min(200)]));
    }
    eprintln!("[PIPELINE] OCR: {} chars in {}ms, confidence={:.2}", ocr_result.char_count, ocr_ms, ocr_result.confidence);
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
    // Clear previous menu so the poll doesn't render stale data from a prior snip.
    let menu_state = app.state::<llm::ActionMenuState>();
    *menu_state.menu.lock().unwrap() = None;
    *menu_state.ocr_text.lock().unwrap() = Some(ocr_result.text.clone());
    *menu_state.crop_png.lock().unwrap() = Some(png_bytes_for_reocr);

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
    .resizable(true)
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
    // Get plugin tool descriptions so the LLM knows about installed plugins.
    let registry = app.state::<mcp::ToolRegistry>();
    let all_tools = registry.all_tools().await;
    let plugin_count = all_tools.iter().filter(|t| t.plugin_id != "builtin").count();
    let plugin_tools = registry.tools_for_prompt().await;
    diag_write(&diag_path, &format!("registry: {} total tools, {} plugin tools", all_tools.len(), plugin_count));

    let provider = resolve_provider();
    diag_write(&diag_path, &format!("provider: {}", provider));
    diag_write(&diag_path, &format!("ANTHROPIC_API_KEY present: {}", std::env::var("ANTHROPIC_API_KEY").map(|k| !k.is_empty()).unwrap_or(false)));
    diag_write(&diag_path, &format!("LLM_PROVIDER env: {:?}", std::env::var("LLM_PROVIDER").ok()));
    if !plugin_tools.is_empty() {
        diag_write(&diag_path, &format!("plugin_tools_for_prompt:\n{}", plugin_tools.trim()));
    } else {
        diag_write(&diag_path, "plugin_tools_for_prompt: EMPTY (no plugins or not loaded yet)");
    }
    eprintln!("[PIPELINE] LLM provider: {}", provider);
    let action_menu = match provider.as_str() {
        "gemini" => {
            llm::classify_streaming_gemini(
                &app,
                &ocr_result.text,
                has_table,
                has_code,
                ocr_result.confidence,
                &plugin_tools,
            )
            .await
        }
        #[cfg(feature = "local-llm")]
        "local" => {
            let local_state = app.state::<llm::local_state::LocalLlmState>();
            llm::local::classify_local(
                &app,
                &ocr_result.text,
                has_table,
                has_code,
                ocr_result.confidence,
                &plugin_tools,
                &local_state,
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
                &plugin_tools,
            )
            .await
        }
    };

    // Log classify result to diagnostics
    diag_write(&diag_path, &format!("classify_result: content_type={}, summary={}", action_menu.content_type, action_menu.summary));
    diag_write(&diag_path, &format!("actions: {}", action_menu.actions.len()));
    for a in &action_menu.actions {
        diag_write(&diag_path, &format!("  #{} {} ({})", a.priority, a.label, a.id));
    }
    let diag_ms = pipeline_start.elapsed().as_millis();
    diag_write(&diag_path, &format!("total_pipeline: {}ms", diag_ms));
    eprintln!("[PIPELINE] Diagnostics written to {}", diag_path.display());

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

/// Tauri command: execute an action on the stored OCR text.
///
/// Called by the action menu when the user clicks an action that
/// requires LLM execution (explain_error, suggest_fix, export_csv, etc.).
/// Returns an ActionResult JSON to the frontend.
#[tauri::command]
pub async fn execute_action(
    app: tauri::AppHandle,
    state: tauri::State<'_, llm::ActionMenuState>,
    registry: tauri::State<'_, mcp::ToolRegistry>,
    action_id: String,
) -> Result<llm::ActionResult, String> {
    let fast_text = {
        let guard = state.ocr_text.lock().map_err(|e| e.to_string())?;
        guard
            .clone()
            .ok_or("No OCR text available — snip first".to_string())?
    };

    // Check if this action belongs to a plugin (non-builtin MCP tool).
    // If so, route to the plugin's MCP server with LLM-generated args.
    if registry.is_plugin_action(&action_id).await {
        log::info!("[EXECUTE] Routing to plugin: {}", action_id);
        let resolved = registry.resolve_action(&action_id).await;
        let tool_meta = match &resolved {
            Some(qname) => registry.get_tool(qname).await,
            None => None,
        };
        let result = mcp::execute_plugin_tool(
            &registry,
            &action_id,
            &fast_text,
            tool_meta.as_ref().map(|t| t.description.as_str()),
            tool_meta.as_ref().and_then(|t| t.input_schema.as_ref()),
            Some(&app),
        )
        .await;
        return Ok(result);
    }

    // For code-fix actions, re-OCR with .accurate for higher fidelity text.
    // The classify step used .fast (~30ms) which is good enough for action detection,
    // but code fixes need every bracket and quote to be correct.
    let needs_accurate = matches!(
        action_id.as_str(),
        "suggest_fix" | "fix_error" | "fix_syntax" | "fix_code" | "format_code"
    );
    let ocr_text = if needs_accurate {
        let crop_png = {
            let guard = state.crop_png.lock().map_err(|e| e.to_string())?;
            guard.clone()
        };
        match crop_png {
            Some(png_bytes) => {
                let start = std::time::Instant::now();
                let result = ocr::recognize_text_from_bytes(
                    png_bytes,
                    ocr::RecognitionLevel::Accurate,
                );
                let ms = start.elapsed().as_millis();
                eprintln!(
                    "[EXECUTE] Re-OCR (.accurate): {} chars in {}ms (was {} chars with .fast)",
                    result.char_count, ms, fast_text.len()
                );
                result.text
            }
            None => {
                eprintln!("[EXECUTE] No crop PNG available, using .fast OCR text");
                fast_text
            }
        }
    } else {
        fast_text
    };

    log::info!("[EXECUTE] Starting action: {}", action_id);
    let provider = resolve_provider();
    let result = match provider.as_str() {
        #[cfg(feature = "local-llm")]
        "local" => {
            let local_state = app.state::<llm::local_state::LocalLlmState>();
            llm::local::execute_action_local(&action_id, &ocr_text, &local_state).await
        }
        _ => llm::execute_action_anthropic(&action_id, &ocr_text).await,
    };
    log::info!(
        "[EXECUTE] Complete: status={}, type={}",
        result.status,
        result.result.result_type
    );

    Ok(result)
}
