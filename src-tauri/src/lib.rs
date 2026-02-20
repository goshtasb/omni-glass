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
pub mod safety;
pub mod settings_commands;
mod tray;

use capture::CaptureState;
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
        .manage(CaptureState::new())
        .manage(llm::ActionMenuState::new())
        .manage(ToolRegistry::new())
        .invoke_handler(tauri::generate_handler![
            // Simple commands (commands.rs)
            commands::crop_region,
            commands::get_capture_info,
            commands::get_ocr_text,
            commands::copy_to_clipboard,
            commands::close_overlay,
            commands::close_action_menu,
            commands::get_action_menu,
            commands::run_confirmed_command,
            commands::write_to_desktop,
            commands::write_file_to_path,
            // Pipeline commands (pipeline.rs)
            pipeline::process_snip,
            pipeline::execute_action,
            // Settings commands (settings_commands.rs)
            settings_commands::get_provider_config,
            settings_commands::set_active_provider,
            settings_commands::save_api_key,
            settings_commands::test_provider,
            settings_commands::close_settings,
            settings_commands::open_settings,
            settings_commands::get_ocr_mode,
            settings_commands::set_ocr_mode,
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
                mcp::builtins::register_builtins(&registry).await;
                mcp::loader::load_plugins(&registry).await;
            });

            log::info!("System tray initialized — ready for snips");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error running Omni-Glass");
}
