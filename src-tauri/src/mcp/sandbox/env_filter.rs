//! Environment variable filtering for plugin processes.
//!
//! Prevents plugins from reading API keys and secrets they haven't declared.
//! This is the most important security boundary for v1 — it works on all
//! platforms and provides meaningful protection even without OS-level sandboxing.

use crate::mcp::manifest::Permissions;
use std::collections::HashMap;

/// Essential vars every runtime needs to function.
const ESSENTIAL_VARS: &[&str] = &[
    "PATH", "HOME", "USER", "LANG", "TERM", "SHELL",
    "NODE_PATH",    // Node.js module resolution
    "PYTHONPATH",   // Python module resolution
];

/// Filter the process environment for a plugin, passing only safe variables.
///
/// 1. Always includes essential runtime vars (PATH, HOME, etc.)
/// 2. Always sets OMNI_GLASS_PLUGIN_ID and a plugin-specific TMPDIR
/// 3. Includes only env vars explicitly declared in permissions.environment
/// 4. NEVER passes API keys, tokens, or secrets unless explicitly declared
pub fn filter_environment(
    permissions: &Permissions,
    plugin_id: &str,
) -> HashMap<String, String> {
    let mut filtered = HashMap::new();

    // Essential runtime vars from the current process environment
    for key in ESSENTIAL_VARS {
        if let Ok(val) = std::env::var(key) {
            filtered.insert(key.to_string(), val);
        }
    }

    // Plugin identity
    filtered.insert("OMNI_GLASS_PLUGIN_ID".to_string(), plugin_id.to_string());

    // Plugin-specific temp directory (isolates temp files per plugin)
    filtered.insert(
        "TMPDIR".to_string(),
        format!("/tmp/omni-glass-{}", plugin_id),
    );

    // Only pass through env vars the plugin explicitly declared
    if let Some(ref declared_vars) = permissions.environment {
        for var_name in declared_vars {
            if let Ok(val) = std::env::var(var_name) {
                filtered.insert(var_name.clone(), val);
            }
        }
    }

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_essential_vars() {
        let perms = Permissions::default();
        let filtered = filter_environment(&perms, "com.test.plugin");
        // PATH should always be present (it's in the real env)
        assert!(filtered.contains_key("PATH"));
        assert_eq!(filtered["OMNI_GLASS_PLUGIN_ID"], "com.test.plugin");
    }

    #[test]
    fn strips_undeclared_api_keys() {
        // Set a test env var (will be cleaned up at process exit)
        std::env::set_var("TEST_SECRET_KEY_OG", "sk-secret-12345");
        let perms = Permissions::default(); // no environment declared
        let filtered = filter_environment(&perms, "com.test.plugin");
        assert!(!filtered.contains_key("TEST_SECRET_KEY_OG"));
        std::env::remove_var("TEST_SECRET_KEY_OG");
    }

    #[test]
    fn includes_declared_vars() {
        std::env::set_var("JIRA_TOKEN_OG_TEST", "jira-123");
        let perms = Permissions {
            environment: Some(vec!["JIRA_TOKEN_OG_TEST".to_string()]),
            ..Default::default()
        };
        let filtered = filter_environment(&perms, "com.test.plugin");
        assert_eq!(filtered["JIRA_TOKEN_OG_TEST"], "jira-123");
        std::env::remove_var("JIRA_TOKEN_OG_TEST");
    }

    #[test]
    fn overrides_tmpdir() {
        let perms = Permissions::default();
        let filtered = filter_environment(&perms, "com.test.plugin");
        assert_eq!(filtered["TMPDIR"], "/tmp/omni-glass-com.test.plugin");
    }
}
