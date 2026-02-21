//! System tray setup and click handler.
//!
//! The tray icon is the primary entry point for Omni-Glass.
//! Left/right-click opens a native menu with Snip Screen, Type Command,
//! Settings, and Quit.

use tauri::{
    image::Image as TauriImage,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

/// Sets up the system tray icon with a native menu.
///
/// Both left-click and right-click open the same menu:
///   - Snip Screen  → capture flow
///   - Type Command → text launcher
///   - Settings...  → settings window
///   - Quit         → exit
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let snip_item = MenuItemBuilder::with_id("snip", "Snip Screen").build(app)?;
    let type_item = MenuItemBuilder::with_id("type_command", "Type Command").build(app)?;
    let settings_item = MenuItemBuilder::with_id("settings", "Settings...").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit Omni-Glass").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&snip_item)
        .item(&type_item)
        .separator()
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()?;

    // Decode the PNG icon to RGBA for Tauri's Image type
    let icon_bytes = include_bytes!("../icons/32x32.png");
    let icon_img = image::load_from_memory(icon_bytes)
        .map_err(|e| format!("Failed to decode tray icon: {}", e))?;
    let rgba = icon_img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let tray_icon = TauriImage::new_owned(rgba.into_raw(), w, h);

    let _tray = TrayIconBuilder::new()
        .icon(tray_icon)
        .tooltip("Omni-Glass")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "snip" => {
                    log::info!("[TRAY] Snip Screen selected");
                    let click_epoch_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as f64;
                    if let Err(e) = start_snip_mode(app, click_epoch_ms) {
                        log::error!("Failed to start snip mode: {}", e);
                    }
                }
                "type_command" => {
                    log::info!("[TRAY] Type Command selected");
                    crate::show_text_launcher(app);
                }
                "settings" => {
                    log::info!("[TRAY] Settings selected");
                    if let Some(window) = app.get_webview_window("settings") {
                        let _ = window.set_focus();
                    } else {
                        let _ = tauri::WebviewWindowBuilder::new(
                            app,
                            "settings",
                            tauri::WebviewUrl::App("settings.html".into()),
                        )
                        .title("Omni-Glass Settings")
                        .inner_size(520.0, 500.0)
                        .resizable(true)
                        .build();
                    }
                }
                "quit" => {
                    log::info!("[TRAY] Quit selected");
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

/// Initiates snip mode: captures the screen, then opens the overlay window.
///
/// The screenshot is saved to a temp PNG file and loaded by the webview
/// via Tauri's asset protocol.
pub fn start_snip_mode(
    app: &AppHandle,
    click_epoch_ms: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::capture::{self, CaptureState};

    let start = std::time::Instant::now();

    // Guard: if the overlay window already exists, close it first.
    if let Some(existing) = app.get_webview_window("overlay") {
        log::info!("Closing existing overlay window before starting new snip");
        let _ = existing.destroy();
    }

    // Step 1: Capture the full screen
    let screenshot = capture::capture_primary_monitor()
        .map_err(|e| format!("Screen capture failed: {}", e))?;

    let capture_us = start.elapsed().as_micros();
    log::info!(
        "[LATENCY] xcap_capture={:.2}ms",
        capture_us as f64 / 1000.0
    );

    // Step 2: Save screenshot to temp PNG file for overlay display.
    let temp_path = std::env::temp_dir().join("omni-glass-capture.png");
    screenshot
        .save(&temp_path)
        .map_err(|e| format!("PNG save failed: {}", e))?;
    // Canonicalize to resolve /var → /private/var symlink on macOS.
    let temp_path = std::fs::canonicalize(&temp_path)
        .unwrap_or(temp_path);

    let save_us = start.elapsed().as_micros() - capture_us;
    let file_size = std::fs::metadata(&temp_path)
        .map(|m| m.len())
        .unwrap_or(0);
    log::info!(
        "[LATENCY] png_save={:.2}ms ({} bytes, {})",
        save_us as f64 / 1000.0,
        file_size,
        temp_path.display()
    );

    // Step 3: Store the screenshot + capture info for the overlay to fetch.
    let state = app.state::<CaptureState>();
    *state.screenshot.lock().unwrap() = Some(screenshot);
    *state.capture_info.lock().unwrap() = Some(capture::CaptureInfo {
        image_path: temp_path.to_string_lossy().to_string(),
        click_epoch_ms,
    });

    // Step 4: Create the overlay window.
    let _overlay_window = tauri::WebviewWindowBuilder::new(
        app,
        "overlay",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .fullscreen(true)
    .transparent(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .title("Omni-Glass Overlay")
    .build()?;

    let window_us = start.elapsed().as_micros() - capture_us - save_us;
    log::info!(
        "[LATENCY] window_create={:.2}ms",
        window_us as f64 / 1000.0
    );

    let total_us = start.elapsed().as_micros();
    log::info!(
        "[LATENCY] rust_total={:.2}ms (capture={:.2} + save={:.2} + window={:.2})",
        total_us as f64 / 1000.0,
        capture_us as f64 / 1000.0,
        save_us as f64 / 1000.0,
        window_us as f64 / 1000.0,
    );

    Ok(())
}
