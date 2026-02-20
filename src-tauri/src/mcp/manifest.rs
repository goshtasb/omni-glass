//! Plugin manifest parser and validator.
//!
//! Each plugin directory must contain an `omni-glass.plugin.json` file
//! describing the plugin's identity, runtime, entry point, and permissions.

use serde::Deserialize;
use std::path::Path;

/// The filename expected in every plugin directory.
pub const MANIFEST_FILENAME: &str = "omni-glass.plugin.json";

/// Parsed and validated plugin manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub runtime: Runtime,
    pub entry: String,
    #[serde(default)]
    pub permissions: Permissions,
}

/// Plugin runtime environment.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Node,
    Python,
    Binary,
}

/// Plugin permission declarations.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub clipboard: bool,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub filesystem: bool,
}

/// Load and validate a plugin manifest from a directory.
pub fn load_manifest(plugin_dir: &Path) -> Result<PluginManifest, String> {
    let manifest_path = plugin_dir.join(MANIFEST_FILENAME);
    if !manifest_path.exists() {
        return Err(format!(
            "No {} found in {}",
            MANIFEST_FILENAME,
            plugin_dir.display()
        ));
    }

    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read {}: {}", manifest_path.display(), e))?;

    let manifest: PluginManifest = serde_json::from_str(&raw)
        .map_err(|e| format!("Invalid manifest JSON in {}: {}", manifest_path.display(), e))?;

    validate(&manifest, plugin_dir)?;
    Ok(manifest)
}

/// Validate manifest fields.
fn validate(m: &PluginManifest, plugin_dir: &Path) -> Result<(), String> {
    // ID must be non-empty and look like reverse-domain
    if m.id.is_empty() || !m.id.contains('.') {
        return Err(format!(
            "Plugin id '{}' must be reverse-domain format (e.g. com.example.plugin)",
            m.id
        ));
    }

    // Name must be non-empty
    if m.name.trim().is_empty() {
        return Err("Plugin name must not be empty".to_string());
    }

    // Version must be non-empty
    if m.version.trim().is_empty() {
        return Err("Plugin version must not be empty".to_string());
    }

    // Entry must not contain path traversal
    if m.entry.contains("..") {
        return Err(format!(
            "Plugin entry '{}' must not contain path traversal (..)",
            m.entry
        ));
    }

    // Entry file must exist
    let entry_path = plugin_dir.join(&m.entry);
    if !entry_path.exists() {
        return Err(format!(
            "Plugin entry file '{}' not found in {}",
            m.entry,
            plugin_dir.display()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_plugin(dir: &Path, manifest_json: &str, entry: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(MANIFEST_FILENAME), manifest_json).unwrap();
        fs::write(dir.join(entry), "// test").unwrap();
    }

    #[test]
    fn valid_manifest_loads() {
        let dir = std::env::temp_dir().join("og-test-valid-manifest");
        let _ = fs::remove_dir_all(&dir);
        setup_test_plugin(
            &dir,
            r#"{
                "id": "com.example.test",
                "name": "Test Plugin",
                "version": "1.0.0",
                "runtime": "node",
                "entry": "index.js"
            }"#,
            "index.js",
        );
        let m = load_manifest(&dir).unwrap();
        assert_eq!(m.id, "com.example.test");
        assert_eq!(m.runtime, Runtime::Node);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_path_traversal_in_entry() {
        let dir = std::env::temp_dir().join("og-test-traversal");
        let _ = fs::remove_dir_all(&dir);
        setup_test_plugin(
            &dir,
            r#"{
                "id": "com.example.evil",
                "name": "Evil",
                "version": "1.0.0",
                "runtime": "node",
                "entry": "../../../etc/passwd"
            }"#,
            "index.js",
        );
        let err = load_manifest(&dir).unwrap_err();
        assert!(err.contains("path traversal"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_reverse_domain_id() {
        let dir = std::env::temp_dir().join("og-test-bad-id");
        let _ = fs::remove_dir_all(&dir);
        setup_test_plugin(
            &dir,
            r#"{
                "id": "no-dots",
                "name": "Bad ID",
                "version": "1.0.0",
                "runtime": "node",
                "entry": "index.js"
            }"#,
            "index.js",
        );
        let err = load_manifest(&dir).unwrap_err();
        assert!(err.contains("reverse-domain"));
        let _ = fs::remove_dir_all(&dir);
    }
}
