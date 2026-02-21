//! Omni-Glass — Tauri application entry point.
//!
//! This is the app shell that wires together all domains and commands.
//! No business logic lives here — only module declarations, plugin
//! registration, state management, and the command registry.
//!
//! Commands are split across:
//!   - commands.rs           — simple one-step commands (crop, close, clipboard, file I/O)
//!   - pipeline.rs           — multi-step orchestration (process_snip, execute_action)
//!   - settings_commands.rs  — settings panel + provider resolution

mod capture;
mod commands;
pub mod llm;
pub mod mcp;
mod ocr;
mod pipeline;
mod pipeline_text;
pub mod safety;
pub mod settings_commands;
mod tray;

use capture::CaptureState;
use mcp::loader::PendingApprovals;
use mcp::ToolRegistry;
use tauri::Manager;

/// Entry point — called by Tauri runtime.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env.local → .env from project root.
    // Uses CARGO_MANIFEST_DIR (compile-time path to src-tauri/) to reliably
    // find the project root regardless of the binary's working directory.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.parent().unwrap_or(manifest_dir);

    'env_load: for env_file in [".env.local", ".env"] {
        let path = project_root.join(env_file);
        if path.exists() {
            match dotenvy::from_path(&path) {
                Ok(_) => eprintln!("[STARTUP] Loaded {}", path.display()),
                Err(e) => eprintln!("[STARTUP] Failed to load {}: {}", path.display(), e),
            }
            break 'env_load;
        }
    }

    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        // Global shortcut plugin — available for opt-in hotkey binding.
        // Not registered by default; tray menu is the primary entry point.
        // .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(CaptureState::new())
        .manage(llm::ActionMenuState::new())
        .manage(ToolRegistry::new())
        .manage(PendingApprovals::new())
        .invoke_handler(tauri::generate_handler![
            // Simple commands (commands.rs)
            commands::crop_region,
            commands::get_capture_info,
            commands::get_ocr_text,
            commands::copy_to_clipboard,
            commands::close_overlay,
            commands::close_action_menu,
            commands::close_permission_prompt,
            commands::get_action_menu,
            commands::run_confirmed_command,
            commands::write_to_desktop,
            commands::write_file_to_path,
            commands::close_text_launcher,
            commands::close_tray_menu,
            commands::start_snip,
            commands::open_text_launcher,
            commands::get_plugin_names,
            // Pipeline commands (pipeline.rs / pipeline_text.rs)
            pipeline::process_snip,
            pipeline::execute_action,
            pipeline_text::execute_text_command,
            // Settings commands (settings_commands.rs)
            settings_commands::get_provider_config,
            settings_commands::set_active_provider,
            settings_commands::save_api_key,
            settings_commands::test_provider,
            settings_commands::close_settings,
            settings_commands::open_settings,
            settings_commands::get_ocr_mode,
            settings_commands::set_ocr_mode,
            // MCP approval commands (approval_commands.rs)
            mcp::approval_commands::get_pending_approvals,
            mcp::approval_commands::approve_plugin,
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

            // Load MCP plugins asynchronously (non-blocking).
            // Register built-in tools first, then scan for external plugins.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let registry = handle.state::<ToolRegistry>();
                let pending = handle.state::<PendingApprovals>();
                mcp::builtins::register_builtins(&registry).await;
                mcp::loader::load_plugins(&registry, &pending).await;

                // If any plugins are queued for approval, open the prompt window
                let has_pending = !pending.queue.lock().await.is_empty();
                if has_pending {
                    log::info!("[MCP] Opening permission prompt for pending plugins");
                    let _ = tauri::WebviewWindowBuilder::new(
                        &handle,
                        "permission-prompt",
                        tauri::WebviewUrl::App("permission-prompt.html".into()),
                    )
                    .title("Plugin Permissions")
                    .inner_size(460.0, 380.0)
                    .resizable(false)
                    .center()
                    .build();
                }
            });

            log::info!("System tray initialized — ready for snips");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error running Omni-Glass");
}

/// Open (or focus) the text launcher window.
pub(crate) fn show_text_launcher(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("text-launcher") {
        let _ = window.set_focus();
        return;
    }
    match tauri::WebviewWindowBuilder::new(
        app,
        "text-launcher",
        tauri::WebviewUrl::App("text-launcher.html".into()),
    )
    .title("Omni-Glass")
    .inner_size(600.0, 72.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .center()
    .build()
    {
        Ok(_) => log::info!("[TEXT_LAUNCHER] Window opened"),
        Err(e) => log::error!("[TEXT_LAUNCHER] Failed to open: {}", e),
    }
}
