//! Plugin loader — scans plugin directory, spawns MCP servers, discovers tools.
//!
//! Called once at startup from lib.rs `.setup()`. Each plugin is loaded
//! independently: a failure in one plugin does not block the others.

use crate::mcp::client::McpServer;
use crate::mcp::manifest::{self, PluginManifest, Runtime};
use crate::mcp::registry::ToolRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Default plugin directory: ~/.config/omni-glass/plugins/
fn plugins_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|c| c.join("omni-glass").join("plugins"))
}

/// Load all plugins from the plugins directory.
///
/// For each valid plugin subdirectory:
/// 1. Parse manifest
/// 2. Spawn MCP server process
/// 3. Run initialize handshake
/// 4. Discover tools via tools/list
/// 5. Register tools in the ToolRegistry
///
/// Failures are logged and skipped — never fatal to the app.
pub async fn load_plugins(registry: &ToolRegistry) {
    let dir = match plugins_dir() {
        Some(d) => d,
        None => {
            log::warn!("[MCP] Could not determine config directory, skipping plugins");
            return;
        }
    };

    if !dir.exists() {
        log::info!("[MCP] No plugins directory at {}, skipping", dir.display());
        return;
    }

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("[MCP] Failed to read plugins dir {}: {}", dir.display(), e);
            return;
        }
    };

    let mut loaded = 0u32;
    let mut total_tools = 0u32;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        match load_single_plugin(&path, registry).await {
            Ok(tool_count) => {
                loaded += 1;
                total_tools += tool_count;
            }
            Err(e) => {
                log::warn!(
                    "[MCP] Failed to load plugin '{}': {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    e
                );
            }
        }
    }

    log::info!(
        "[MCP] {} plugins loaded, {} tools discovered",
        loaded,
        total_tools
    );
}

/// Load a single plugin from its directory.
async fn load_single_plugin(plugin_dir: &Path, registry: &ToolRegistry) -> Result<u32, String> {
    // 1. Parse manifest
    let manifest = manifest::load_manifest(plugin_dir)?;
    log::info!(
        "[MCP] Loading plugin '{}' v{} ({})",
        manifest.name,
        manifest.version,
        manifest.id
    );

    // 2. Determine spawn command
    let (command, args) = resolve_command(&manifest, plugin_dir)?;

    // 3. Spawn the MCP server process
    let env = build_env(&manifest);
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let mut server = McpServer::spawn(&manifest.id, &command, &args_refs, env)?;

    // 4. Initialize handshake
    server.initialize().await?;

    // 5. Discover tools
    let tools = server.list_tools().await?;
    let tool_count = tools.len() as u32;

    // 6. Register tools and store server
    registry.register_plugin_tools(&manifest.id, tools).await;
    registry.add_server(manifest.id.clone(), server).await;

    Ok(tool_count)
}

/// Determine the command + args to spawn based on runtime and entry point.
fn resolve_command(manifest: &PluginManifest, plugin_dir: &Path) -> Result<(String, Vec<String>), String> {
    let entry_path = plugin_dir.join(&manifest.entry);
    let entry_str = entry_path.to_string_lossy().to_string();

    match manifest.runtime {
        Runtime::Node => Ok(("node".to_string(), vec![entry_str])),
        Runtime::Python => Ok(("python3".to_string(), vec![entry_str])),
        Runtime::Binary => {
            // Make sure binary is executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&entry_path) {
                    let mode = meta.permissions().mode();
                    if mode & 0o111 == 0 {
                        return Err(format!(
                            "Binary entry '{}' is not executable",
                            manifest.entry
                        ));
                    }
                }
            }
            Ok((entry_str, vec![]))
        }
    }
}

/// Build environment variables for the plugin process.
fn build_env(manifest: &PluginManifest) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("OMNI_GLASS_PLUGIN_ID".to_string(), manifest.id.clone());
    env
}
