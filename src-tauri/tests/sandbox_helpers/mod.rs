//! Shared test helpers for sandbox escape tests.

use omni_glass_lib::mcp::manifest::{Permissions, PluginManifest, Runtime};
use std::path::PathBuf;
use std::process::Command;

/// Create a test plugin directory with a minimal index.js entry.
pub fn setup_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("og-sandbox-test-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.js"), "// entry\n").unwrap();
    dir
}

/// Create a manifest with given permissions.
pub fn test_manifest(id: &str, perms: Permissions) -> PluginManifest {
    PluginManifest {
        id: id.to_string(),
        name: "Sandbox Test".to_string(),
        version: "1.0.0".to_string(),
        description: String::new(),
        runtime: Runtime::Node,
        entry: "index.js".to_string(),
        permissions: perms,
        configuration: None,
    }
}

/// Check if Node.js is available.
pub fn node_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

/// Run a Node.js script under sandbox-exec with the given profile.
/// CWD is set to the script's parent dir (the plugin directory) because
/// the sandbox walls off /Users — Node.js calls getcwd() at startup.
/// Returns (exit_code, stdout, stderr).
#[cfg(target_os = "macos")]
pub fn run_sandboxed(
    profile_path: &std::path::Path,
    script_path: &std::path::Path,
    env: std::collections::HashMap<String, String>,
) -> (i32, String, String) {
    let cwd = script_path.parent().unwrap_or(std::path::Path::new("/tmp"));
    let output = Command::new("sandbox-exec")
        .args(["-f", &profile_path.to_string_lossy()])
        .arg("node")
        .arg(script_path)
        .current_dir(cwd)
        .envs(env)
        .output()
        .expect("Failed to run sandbox-exec");

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (code, stdout, stderr)
}
