//! System tray setup and click handler.
//!
//! The tray icon is the primary entry point for Omni-Glass.
//! Clicking it triggers the screen capture flow.

use tauri::{
    image::Image as TauriImage,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

/// Sets up the system tray icon with a click handler.
///
/// Left-click: triggers screen capture (snip mode).
/// Right-click: opens context menu with Quit option.
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let quit_item = MenuItemBuilder::with_id("quit", "Quit Omni-Glass").build(app)?;
    let menu = MenuBuilder::new(app).item(&quit_item).build()?;

    // Decode the PNG icon to RGBA for Tauri's Image type
    let icon_bytes = include_bytes!("../icons/32x32.png");
    let icon_img = image::load_from_memory(icon_bytes)
        .map_err(|e| format!("Failed to decode tray icon: {}", e))?;
    let rgba = icon_img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let tray_icon = TauriImage::new_owned(rgba.into_raw(), w, h);

    let _tray = TrayIconBuilder::new()
        .icon(tray_icon)
        .tooltip("Omni-Glass — Click to snip")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray_icon, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                ..
            } = event
            {
                let click_epoch_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as f64;
                log::info!(
                    "[LATENCY] tray_click_epoch_ms={:.1}",
                    click_epoch_ms
                );
                let app = tray_icon.app_handle();
                if let Err(e) = start_snip_mode(app, click_epoch_ms) {
                    log::error!("Failed to start snip mode: {}", e);
                }
            }
        })
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                log::info!("Quit requested from tray menu");
                app.exit(0);
            }
        })
        .build(app)?;

    Ok(())
}

/// Initiates snip mode: captures the screen, then opens the overlay window.
///
/// The screenshot is saved to a temp BMP file and loaded by the webview
/// via Tauri's asset protocol. This eliminates image encoding from the
/// critical path — the `image` crate's PNG/JPEG encoders are too slow
/// in debug mode (2.6-3.2s for a Retina screenshot).
fn start_snip_mode(
    app: &AppHandle,
    click_epoch_ms: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::capture::{self, CaptureState};

    let start = std::time::Instant::now();

    // Guard: if the overlay window already exists, close it first.
    // The user clicked again because they want a fresh snip.
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

    // Step 2: Write raw RGBA pixels to temp BMP file.
    // BMP = headers + raw pixels, no compression. ~32MB for Retina but
    // it's a local file write, not a network transfer. This replaces
    // PNG (2.6s) and JPEG (3.2s) encoding that was killing latency.
    // PERF: Replace BMP temp file with shared memory buffer. 32MB disk
    // write is acceptable for spike, not for production.
    let temp_path = std::env::temp_dir().join("omni-glass-capture.bmp");
    screenshot
        .save(&temp_path)
        .map_err(|e| format!("BMP save failed: {}", e))?;

    let save_us = start.elapsed().as_micros() - capture_us;
    let file_size = std::fs::metadata(&temp_path)
        .map(|m| m.len())
        .unwrap_or(0);
    log::info!(
        "[LATENCY] bmp_save={:.2}ms ({} bytes, {})",
        save_us as f64 / 1000.0,
        file_size,
        temp_path.display()
    );

    // Step 3: Store the screenshot for later cropping
    let state = app.state::<CaptureState>();
    *state.screenshot.lock().unwrap() = Some(screenshot);

    // Step 4: Create the overlay window
    let overlay_window = tauri::WebviewWindowBuilder::new(
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

    // Step 5: Send temp file path + click timestamp to the overlay.
    // The frontend uses convertFileSrc() to load via Tauri's asset protocol.
    let payload = serde_json::json!({
        "imagePath": temp_path.to_string_lossy(),
        "clickEpochMs": click_epoch_ms,
    });
    overlay_window.emit("screenshot-ready", &payload)?;

    let total_us = start.elapsed().as_micros();
    log::info!(
        "[LATENCY] rust_total={:.2}ms (capture={:.2} + save={:.2} + window={:.2} + emit={:.2})",
        total_us as f64 / 1000.0,
        capture_us as f64 / 1000.0,
        save_us as f64 / 1000.0,
        window_us as f64 / 1000.0,
        (total_us - capture_us - save_us - window_us) as f64 / 1000.0,
    );

    Ok(())
}
