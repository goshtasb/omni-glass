//! Plugin loader — scans plugin directory, spawns sandboxed MCP servers.
//!
//! Called once at startup from lib.rs `.setup()`. Each plugin is checked
//! against the approval store before loading. Unapproved or permission-
//! changed plugins are queued for user approval via the permission prompt.

use crate::mcp::approval::{self, ApprovalStatus};
use crate::mcp::client::McpServer;
use crate::mcp::manifest::{self, PluginManifest, Runtime};
use crate::mcp::registry::ToolRegistry;
use crate::mcp::sandbox::env_filter;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

/// Plugins awaiting user approval. Managed as Tauri state.
pub struct PendingApprovals {
    pub queue: Mutex<Vec<(PluginManifest, PathBuf, bool)>>, // (manifest, dir, is_update)
}

impl PendingApprovals {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
        }
    }
}

/// Default plugin directory: ~/.config/omni-glass/plugins/
fn plugins_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|c| c.join("omni-glass").join("plugins"))
}

/// Load all plugins from the plugins directory.
///
/// For each valid plugin subdirectory:
/// 1. Parse manifest
/// 2. Check approval status
/// 3. Approved → spawn sandboxed, initialize, discover, register
/// 4. Denied → skip silently
/// 5. NeedsApproval / PermissionsChanged → queue for user prompt
///
/// Failures are logged and skipped — never fatal to the app.
pub async fn load_plugins(registry: &ToolRegistry, pending: &PendingApprovals) {
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

    let store = approval::load_approvals();
    let mut loaded = 0u32;
    let mut total_tools = 0u32;
    let mut queued = 0u32;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Parse manifest first
        let manifest = match manifest::load_manifest(&path) {
            Ok(m) => m,
            Err(e) => {
                log::warn!(
                    "[MCP] Failed to load manifest for '{}': {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    e
                );
                continue;
            }
        };

        // Check approval status
        match approval::check_approval(&store, &manifest) {
            ApprovalStatus::Approved => {
                match load_approved_plugin(&manifest, &path, registry).await {
                    Ok(tool_count) => {
                        loaded += 1;
                        total_tools += tool_count;
                    }
                    Err(e) => {
                        log::warn!("[MCP] Failed to load plugin '{}': {}", manifest.id, e);
                    }
                }
            }
            ApprovalStatus::Denied => {
                log::info!("[MCP] Plugin '{}' is denied, skipping", manifest.id);
            }
            ApprovalStatus::NeedsApproval => {
                log::info!("[MCP] Plugin '{}' needs approval, queuing", manifest.id);
                pending.queue.lock().await.push((manifest, path, false));
                queued += 1;
            }
            ApprovalStatus::PermissionsChanged => {
                log::info!(
                    "[MCP] Plugin '{}' permissions changed, queuing for re-approval",
                    manifest.id
                );
                pending.queue.lock().await.push((manifest, path, true));
                queued += 1;
            }
        }
    }

    log::info!(
        "[MCP] {} plugins loaded, {} tools discovered, {} awaiting approval",
        loaded,
        total_tools,
        queued
    );
}

/// Load a single approved plugin: env filter → sandbox → initialize → register.
///
/// Public because it's called from both the startup loader and the
/// `approve_plugin` command after the user grants permission.
pub async fn load_approved_plugin(
    manifest: &PluginManifest,
    plugin_dir: &Path,
    registry: &ToolRegistry,
) -> Result<u32, String> {
    log::info!(
        "[MCP] Loading plugin '{}' v{} ({})",
        manifest.name,
        manifest.version,
        manifest.id
    );

    // 1. Filter environment variables (all platforms)
    let env = env_filter::filter_environment(&manifest.permissions, &manifest.id);

    // 2. Determine spawn command
    let (command, args) = resolve_command(manifest, plugin_dir)?;
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    // 3. Spawn — sandboxed on macOS, filtered-env-only on other platforms
    let mut server = spawn_plugin(
        &manifest.id,
        manifest,
        plugin_dir,
        &command,
        &args_refs,
        env,
    )?;

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

/// Spawn a plugin process with platform-appropriate sandboxing.
fn spawn_plugin(
    plugin_id: &str,
    manifest: &PluginManifest,
    plugin_dir: &Path,
    command: &str,
    args: &[&str],
    env: std::collections::HashMap<String, String>,
) -> Result<McpServer, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::mcp::sandbox::macos;
        match macos::generate_profile(manifest, plugin_dir) {
            Ok(profile) => {
                let profile_path = macos::write_profile(plugin_id, &profile)?;
                log::info!("[SANDBOX] Profile written for '{}': {}", plugin_id, profile_path.display());
                return McpServer::spawn_sandboxed(
                    plugin_id, command, args, env, &profile_path, plugin_dir,
                );
            }
            Err(e) => {
                log::warn!(
                    "[SANDBOX] Failed to generate profile for '{}': {} — loading without sandbox",
                    plugin_id,
                    e
                );
            }
        }
    }

    // Fallback: spawn with filtered environment only (all platforms)
    McpServer::spawn(plugin_id, command, args, env, Some(plugin_dir))
}

/// Determine the command + args to spawn based on runtime and entry point.
fn resolve_command(
    manifest: &PluginManifest,
    plugin_dir: &Path,
) -> Result<(String, Vec<String>), String> {
    let entry_path = plugin_dir.join(&manifest.entry);
    let entry_str = entry_path.to_string_lossy().to_string();

    match manifest.runtime {
        Runtime::Node => Ok(("node".to_string(), vec![entry_str])),
        Runtime::Python => Ok(("python3".to_string(), vec![entry_str])),
        Runtime::Binary => {
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
