//! Plugin configuration persistence.
//!
//! Each plugin can declare a `configuration` field in its manifest with
//! typed fields (string, number, boolean). User values are stored in
//! `~/.config/omni-glass/plugin-config/{plugin_id}.json`.
//!
//! This module handles loading, saving, and querying per-plugin config.

use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Directory where plugin configs are stored.
fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("omni-glass")
        .join("plugin-config")
}

/// Full path to a plugin's config file.
fn config_path(plugin_id: &str) -> PathBuf {
    config_dir().join(format!("{}.json", plugin_id))
}

/// Load all configuration values for a plugin.
///
/// Returns an empty map if the config file doesn't exist or is invalid.
pub fn load_config(plugin_id: &str) -> HashMap<String, Value> {
    let path = config_path(plugin_id);
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// Persist all configuration values for a plugin.
///
/// Creates the config directory if it doesn't exist.
pub fn save_config(
    plugin_id: &str,
    config: &HashMap<String, Value>,
) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;
    let path = config_path(plugin_id);
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    log::info!("[CONFIG] Saved config for plugin '{}'", plugin_id);
    Ok(())
}

/// Get a single configuration value for a plugin.
///
/// Returns `None` if the key doesn't exist or the config file is missing.
pub fn get_config_value(plugin_id: &str, key: &str) -> Option<Value> {
    load_config(plugin_id).get(key).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_config_returns_empty() {
        let config = load_config("com.example.nonexistent-plugin-test");
        assert!(config.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let id = "com.example.config-test-roundtrip";
        let mut config = HashMap::new();
        config.insert("repo".to_string(), Value::String("owner/repo".to_string()));
        config.insert("count".to_string(), Value::Number(42.into()));

        save_config(id, &config).unwrap();
        let loaded = load_config(id);
        assert_eq!(loaded.get("repo").unwrap().as_str().unwrap(), "owner/repo");
        assert_eq!(loaded.get("count").unwrap().as_i64().unwrap(), 42);

        // Cleanup
        let _ = std::fs::remove_file(config_path(id));
    }

    #[test]
    fn get_single_value() {
        let id = "com.example.config-test-single";
        let mut config = HashMap::new();
        config.insert("key1".to_string(), Value::String("val1".to_string()));
        save_config(id, &config).unwrap();

        assert_eq!(
            get_config_value(id, "key1").unwrap().as_str().unwrap(),
            "val1"
        );
        assert!(get_config_value(id, "missing").is_none());

        // Cleanup
        let _ = std::fs::remove_file(config_path(id));
    }
}
