//! Windows sandbox stub — environment filtering only.
//!
//! Full AppContainer implementation is planned for Phase 3.
//! For now, plugins run with filtered environment variables but
//! without OS-level process sandboxing.

use std::collections::HashMap;
use std::path::Path;
use tokio::process::Child;

/// Spawn a plugin process with environment filtering only (no OS sandbox).
///
/// Logs a warning that full Windows sandboxing is not yet implemented.
pub fn spawn_unsandboxed(
    command: &str,
    args: &[&str],
    plugin_dir: &Path,
    env: &HashMap<String, String>,
) -> Result<Child, String> {
    log::warn!(
        "[SANDBOX] Windows AppContainer not yet implemented. \
         Plugin running with environment filtering only."
    );

    use std::process::Stdio;
    tokio::process::Command::new(command)
        .args(args)
        .current_dir(plugin_dir)
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn plugin: {}", e))
}
