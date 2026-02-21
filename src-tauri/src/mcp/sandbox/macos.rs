//! macOS sandbox profile generator (sandbox-exec, deprecated but functional).
//!
//! Security model — "Broad System Allowlist":
//!   1. `(deny default)` — deny everything
//!   2. `(allow file-read* (subpath "/"))` — allow system-wide reads
//!   3. `(deny file-read* (subpath "/Users"))` — wall off ALL user data
//!   4. Re-allow ONLY: runtime prefix, plugin dir, plugin temp, and
//!      any manifest-declared paths the user approved
//!
//! This ensures ~/Desktop, ~/Projects/.env, ~/.pgpass, Chrome cookies,
//! etc. are mathematically inaccessible. Plugin stdout → LLM cloud API
//! = exfiltration path, so user files must be default-deny.

use crate::mcp::manifest::{PluginManifest, Runtime};
use std::path::{Path, PathBuf};

/// Paths for a runtime binary and its installation prefix.
pub struct RuntimePaths {
    pub binary: PathBuf,
    pub prefix: PathBuf,
}

/// Build a sandbox-exec `.sb` profile from manifest permissions.
/// Default-deny, broad system reads, wall off /Users, then selectively
/// re-allow runtime prefix, plugin dir, and declared paths.
pub fn generate_profile(
    manifest: &PluginManifest,
    plugin_dir: &Path,
) -> Result<String, String> {
    let runtime_paths = find_runtime_paths(&manifest.runtime)?;
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let home_str = home.to_string_lossy();
    let tmp_dir = format!("/private/tmp/omni-glass-{}", manifest.id);
    let plugin_dir_str = plugin_dir.to_string_lossy();

    let mut profile = String::new();

    // ── Layer 1: deny everything ──
    profile.push_str("(version 1)\n(deny default)\n\n");

    // ── Layer 2: broad system reads ──
    profile.push_str(";; System-wide reads (runtimes need hundreds of OS paths)\n");
    profile.push_str("(allow file-read* (subpath \"/\"))\n\n");

    // ── Layer 3: wall off ALL user data ──
    // Last-match-wins: this deny overrides the allow above for /Users
    profile.push_str(";; WALL OFF user data (LLM stdout = exfiltration vector)\n");
    profile.push_str("(deny file-read* (subpath \"/Users\"))\n\n");

    // ── Layer 4: re-allow specific paths within /Users ──
    profile.push_str(";; Re-allow: runtime prefix\n");
    let prefix_str = runtime_paths.prefix.to_string_lossy();
    if !prefix_str.is_empty() {
        profile.push_str(&format!(
            "(allow file-read* (subpath \"{}\"))\n\n", prefix_str
        ));
    }

    profile.push_str(";; Re-allow: plugin directory\n");
    profile.push_str(&format!(
        "(allow file-read* (subpath \"{}\"))\n\n", plugin_dir_str
    ));

    // ── Runtime binary exec ──
    let bin_str = runtime_paths.binary.to_string_lossy();
    profile.push_str(&format!(
        ";; Runtime binary\n(allow process-exec (literal \"{}\"))\n\n",
        bin_str
    ));

    // ── stdio writes (MCP transport) ──
    profile.push_str(";; stdio writes\n");
    for dev in &["/dev/stdout", "/dev/stderr", "/dev/null"] {
        profile.push_str(&format!("(allow file-write* (literal \"{}\"))\n", dev));
    }
    profile.push('\n');

    // ── Plugin temp directory (read + write) ──
    profile.push_str(&format!(
        ";; Plugin temp directory\n\
         (allow file-read* (subpath \"{}\"))\n\
         (allow file-write* (subpath \"{}\"))\n\n",
        tmp_dir, tmp_dir
    ));

    // ── sysctl (hw.ncpu, etc.) ──
    profile.push_str("(allow sysctl-read)\n\n");

    // ── Network (if declared) ──
    if let Some(ref domains) = manifest.permissions.network {
        if !domains.is_empty() {
            profile.push_str(
                ";; Network (coarse: domain filtering not possible)\n\
                 (allow network-outbound)\n(allow network-inbound)\n\
                 (allow network* (local ip \"localhost:*\"))\n\n"
            );
        }
    }

    // ── Declared filesystem paths (user-approved overrides) ──
    if let Some(ref fs_perms) = manifest.permissions.filesystem {
        profile.push_str(";; Declared filesystem access\n");
        for perm in fs_perms {
            let expanded = perm.path.replace("~", &home_str);
            match perm.access.as_str() {
                "write" | "read-write" => {
                    profile.push_str(&format!(
                        "(allow file-read* (subpath \"{}\"))\n\
                         (allow file-write* (subpath \"{}\"))\n",
                        expanded, expanded
                    ));
                }
                "read" => {
                    profile.push_str(&format!(
                        "(allow file-read* (subpath \"{}\"))\n", expanded
                    ));
                }
                _ => {}
            }
        }
        profile.push('\n');
    }

    // ── Shell (if declared) ──
    if let Some(ref shell) = manifest.permissions.shell {
        profile.push_str(";; Declared shell commands\n");
        profile.push_str("(allow process-fork)\n");
        profile.push_str("(allow process-exec (literal \"/bin/sh\"))\n");
        profile.push_str("(allow process-exec (literal \"/bin/bash\"))\n");
        profile.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n", tmp_dir
        ));
        for cmd in &shell.commands {
            if let Ok(cmd_path) = which::which(cmd) {
                profile.push_str(&format!(
                    "(allow process-exec (literal \"{}\"))\n",
                    cmd_path.to_string_lossy()
                ));
            }
        }
        profile.push('\n');
    }

    Ok(profile)
}

/// Write a sandbox profile to a temp file and return its path.
pub fn write_profile(plugin_id: &str, profile: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(format!("/tmp/omni-glass-sandbox-{}.sb", plugin_id));
    std::fs::write(&path, profile)
        .map_err(|e| format!("Failed to write sandbox profile: {}", e))?;
    Ok(path)
}

/// Find the runtime binary and its installation prefix.
/// The prefix is the parent of `bin/` — e.g., for
/// `~/.nvm/versions/node/v24/bin/node`, prefix = `~/.nvm/versions/node/v24`.
pub fn find_runtime_paths(runtime: &Runtime) -> Result<RuntimePaths, String> {
    match runtime {
        Runtime::Node => {
            let binary = which::which("node")
                .map_err(|_| "Node.js not found in PATH".to_string())?;
            let prefix = binary.parent()
                .and_then(|bin| bin.parent())
                .unwrap_or(binary.parent().unwrap_or(Path::new("")))
                .to_path_buf();
            Ok(RuntimePaths { binary, prefix })
        }
        Runtime::Python => {
            let binary = which::which("python3")
                .or_else(|_| which::which("python"))
                .map_err(|_| "Python not found in PATH".to_string())?;
            let prefix = binary.parent()
                .and_then(|bin| bin.parent())
                .unwrap_or(binary.parent().unwrap_or(Path::new("")))
                .to_path_buf();
            Ok(RuntimePaths { binary, prefix })
        }
        Runtime::Binary => {
            Ok(RuntimePaths {
                binary: PathBuf::new(),
                prefix: PathBuf::new(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::manifest::{FsPerm, Permissions};

    fn test_manifest(perms: Permissions) -> PluginManifest {
        PluginManifest {
            id: "com.test.sandbox".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            runtime: Runtime::Node,
            entry: "index.js".to_string(),
            permissions: perms,
            configuration: None,
        }
    }

    #[test]
    fn profile_walls_off_users_directory() {
        let manifest = test_manifest(Permissions::default());
        let dir = std::env::temp_dir().join("og-sandbox-test");
        let _ = std::fs::create_dir_all(&dir);
        let profile = generate_profile(&manifest, &dir).unwrap();
        assert!(profile.contains("(deny default)"));
        assert!(profile.contains("(allow file-read* (subpath \"/\"))"));
        assert!(profile.contains("(deny file-read* (subpath \"/Users\"))"));
    }

    #[test]
    fn profile_re_allows_runtime_prefix() {
        let manifest = test_manifest(Permissions::default());
        let dir = std::env::temp_dir().join("og-sandbox-test");
        let _ = std::fs::create_dir_all(&dir);
        let profile = generate_profile(&manifest, &dir).unwrap();
        // Runtime prefix should appear after the /Users deny
        let users_deny_pos = profile.find("deny file-read* (subpath \"/Users\")").unwrap();
        let re_allow_pos = profile.find("Re-allow: runtime prefix").unwrap();
        assert!(re_allow_pos > users_deny_pos);
    }

    #[test]
    fn profile_includes_plugin_dir() {
        let manifest = test_manifest(Permissions::default());
        let dir = PathBuf::from("/tmp/og-test-plugin");
        let profile = generate_profile(&manifest, &dir).unwrap();
        assert!(profile.contains("/tmp/og-test-plugin"));
    }

    #[test]
    fn no_network_no_network_rule() {
        let manifest = test_manifest(Permissions::default());
        let dir = std::env::temp_dir();
        let profile = generate_profile(&manifest, &dir).unwrap();
        assert!(!profile.contains("network-outbound"));
    }

    #[test]
    fn with_network_allows_outbound() {
        let manifest = test_manifest(Permissions {
            network: Some(vec!["api.example.com".into()]),
            ..Default::default()
        });
        let dir = std::env::temp_dir();
        let profile = generate_profile(&manifest, &dir).unwrap();
        assert!(profile.contains("network-outbound"));
    }

    #[test]
    fn declared_fs_overrides_users_deny() {
        let manifest = test_manifest(Permissions {
            filesystem: Some(vec![
                FsPerm { path: "~/Documents".into(), access: "read".into() },
                FsPerm { path: "/tmp/test-write".into(), access: "write".into() },
            ]),
            ..Default::default()
        });
        let dir = std::env::temp_dir();
        let profile = generate_profile(&manifest, &dir).unwrap();
        let home = dirs::home_dir().unwrap();
        let home_str = home.to_string_lossy();
        // Read-only path gets re-allow after /Users deny
        assert!(profile.contains(&format!("{}/Documents", home_str)));
        // Write path gets both read and write
        assert!(profile.contains("file-write*"));
        assert!(profile.contains("/tmp/test-write"));
    }

    #[test]
    fn tilde_expanded_in_paths() {
        let manifest = test_manifest(Permissions {
            filesystem: Some(vec![FsPerm {
                path: "~/Documents".into(),
                access: "write".into(),
            }]),
            ..Default::default()
        });
        let dir = std::env::temp_dir();
        let profile = generate_profile(&manifest, &dir).unwrap();
        assert!(!profile.contains("\"~/"));
        let home = dirs::home_dir().unwrap();
        assert!(profile.contains(&format!("{}/Documents", home.to_string_lossy())));
    }
}
