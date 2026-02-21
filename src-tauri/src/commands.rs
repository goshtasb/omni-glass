//! Simple Tauri command handlers.
//!
//! These are thin wrappers that bridge frontend invoke() calls to Rust.
//! Each command does one thing: read state, write clipboard, close window, etc.
//!
//! Complex multi-step commands live in pipeline.rs instead.

use crate::capture::CaptureState;
use crate::llm;
use crate::mcp;
use crate::safety;
use tauri::Manager;

/// Tauri command: crop the stored screenshot to the given rectangle.
///
/// Called by the frontend overlay when the user releases the mouse.
/// Returns base64-encoded PNG of the cropped region.
#[tauri::command]
pub fn crop_region(
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

    let png_bytes = crate::capture::crop_to_png_bytes(screenshot, x, y, width, height)
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

/// Tauri command: get capture info (screenshot path + click timestamp).
///
/// Called by the overlay on load. This replaces the event-based approach
/// which raced — the event fired before JS was ready to listen.
#[tauri::command]
pub fn get_capture_info(
    state: tauri::State<'_, CaptureState>,
) -> Result<crate::capture::CaptureInfo, String> {
    let guard = state.capture_info.lock().map_err(|e| e.to_string())?;
    guard
        .clone()
        .ok_or("No capture info available".to_string())
}

/// Tauri command: get the OCR text from the last snip.
///
/// Used by action menu to copy text to clipboard.
#[tauri::command]
pub fn get_ocr_text(
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
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_text(&text)
        .map_err(|e| e.to_string())?;
    log::info!("[ACTION] Copied {} chars to clipboard", text.len());
    Ok(())
}

/// Tauri command: close the overlay and clean up capture state.
#[tauri::command]
pub fn close_overlay(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("overlay") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: close the action menu window.
#[tauri::command]
pub fn close_action_menu(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("action-menu") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: close the permission prompt window.
#[tauri::command]
pub fn close_permission_prompt(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("permission-prompt") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: get the current ActionMenu data.
///
/// Called by the action-menu window on load.
#[tauri::command]
pub fn get_action_menu(
    state: tauri::State<'_, llm::ActionMenuState>,
) -> Result<llm::ActionMenu, String> {
    let guard = state.menu.lock().map_err(|e| e.to_string())?;
    guard
        .clone()
        .ok_or("No action menu available".to_string())
}

/// Tauri command: run a confirmed shell command.
///
/// Only called after the user explicitly clicks "Run" in the confirmation
/// dialog. Runs the command via the default shell and returns its output.
#[tauri::command]
pub async fn run_confirmed_command(command: String) -> Result<String, String> {
    // Double-check safety before executing
    let check = safety::command_check::is_command_safe(&command);
    if !check.safe {
        return Err(format!(
            "Command blocked by safety layer: {}",
            check.reason.unwrap_or_else(|| "Unknown".to_string())
        ));
    }

    log::info!("[EXECUTE] Running confirmed command: {}", command);

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .map_err(|e| format!("Failed to run command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        log::info!("[EXECUTE] Command succeeded");
        Ok(stdout)
    } else {
        log::warn!("[EXECUTE] Command failed: {}", stderr);
        Err(format!("Command failed:\n{}", stderr))
    }
}

/// Tauri command: write file content to the user's Desktop.
///
/// Used for export_csv and other file-generating actions.
/// Validates filename, writes to Desktop directory.
#[tauri::command]
pub fn write_to_desktop(filename: String, content: String) -> Result<String, String> {
    if !safety::command_check::is_path_safe(&filename) {
        return Err("Unsafe filename".to_string());
    }

    let desktop = dirs::desktop_dir().ok_or("Could not find Desktop directory")?;
    let path = desktop.join(&filename);

    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    let full_path = path.to_string_lossy().to_string();
    log::info!("[EXECUTE] Wrote file: {}", full_path);
    Ok(full_path)
}

/// Tauri command: close the text launcher window.
#[tauri::command]
pub fn close_text_launcher(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("text-launcher") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: get display names of loaded plugins.
///
/// Used by the text launcher to show available tools in the placeholder.
#[tauri::command]
pub async fn get_plugin_names(
    registry: tauri::State<'_, mcp::ToolRegistry>,
) -> Result<Vec<String>, String> {
    let tools = registry.all_tools().await;
    let names: Vec<String> = tools.iter().map(|t| t.display_name.clone()).collect();
    Ok(names)
}

/// Tauri command: close the tray menu window.
#[tauri::command]
pub fn close_tray_menu(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("tray-menu") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tauri command: start the snip (capture) flow.
///
/// Called from the tray menu "Snip Screen" option. Delegates to the same
/// start_snip_mode function that was previously triggered by tray click.
#[tauri::command]
pub fn start_snip(app: tauri::AppHandle) -> Result<(), String> {
    let click_epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;
    crate::tray::start_snip_mode(&app, click_epoch_ms)
        .map_err(|e| e.to_string())
}

/// Tauri command: open the text launcher window.
///
/// Called from the tray menu "Type Command" option.
#[tauri::command]
pub fn open_text_launcher(app: tauri::AppHandle) -> Result<(), String> {
    crate::show_text_launcher(&app);
    Ok(())
}

/// Tauri command: write file to a user-chosen path (from save dialog).
///
/// The frontend shows a native save-file picker and passes the chosen path here.
#[tauri::command]
pub fn write_file_to_path(file_path: String, content: String) -> Result<String, String> {
    if !safety::command_check::is_path_safe(&file_path) {
        return Err("Unsafe file path".to_string());
    }

    std::fs::write(&file_path, &content)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    log::info!("[EXPORT] Wrote file: {}", file_path);
    Ok(file_path)
}
