//! Plugin approval state management.
//!
//! Tracks which plugins the user has approved or denied. Stores decisions
//! in a JSON file at `~/.config/omni-glass/plugin-approvals.json` (macOS:
//! `~/Library/Application Support/omni-glass/plugin-approvals.json`).
//!
//! Re-prompts the user when a plugin's permissions change (detected by
//! comparing SHA-256 hashes of the serialized permissions).

use crate::mcp::manifest::{Permissions, PluginManifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const APPROVALS_FILE: &str = "plugin-approvals.json";

/// Approval decision for a plugin.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalStatus {
    Approved,
    Denied,
    NeedsApproval,
    PermissionsChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApprovalStore {
    #[serde(default)]
    pub approved: HashMap<String, ApprovalRecord>,
    #[serde(default)]
    pub denied: HashMap<String, DenialRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub version: String,
    pub permissions_hash: String,
    pub approved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenialRecord {
    pub denied_at: String,
}

/// Path to the approvals JSON file.
fn approvals_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|c| c.join("omni-glass").join(APPROVALS_FILE))
}

/// Load the approval store from disk. Returns empty store if file doesn't exist.
pub fn load_approvals() -> ApprovalStore {
    let path = match approvals_path() {
        Some(p) => p,
        None => return ApprovalStore::default(),
    };
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => ApprovalStore::default(),
    }
}

/// Save the approval store to disk.
pub fn save_approvals(store: &ApprovalStore) -> Result<(), String> {
    let path = approvals_path().ok_or("Could not determine config directory")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let json = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize approvals: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Check whether a plugin is approved, denied, or needs a prompt.
pub fn check_approval(store: &ApprovalStore, manifest: &PluginManifest) -> ApprovalStatus {
    // Check denied first
    if store.denied.contains_key(&manifest.id) {
        return ApprovalStatus::Denied;
    }

    // Check approved
    if let Some(record) = store.approved.get(&manifest.id) {
        let current_hash = hash_permissions(&manifest.permissions);
        if record.permissions_hash == current_hash {
            return ApprovalStatus::Approved;
        }
        return ApprovalStatus::PermissionsChanged;
    }

    ApprovalStatus::NeedsApproval
}

/// Record an approval decision.
pub fn record_approval(store: &mut ApprovalStore, manifest: &PluginManifest) {
    store.denied.remove(&manifest.id);
    store.approved.insert(
        manifest.id.clone(),
        ApprovalRecord {
            version: manifest.version.clone(),
            permissions_hash: hash_permissions(&manifest.permissions),
            approved_at: chrono_now(),
        },
    );
}

/// Record a denial decision.
pub fn record_denial(store: &mut ApprovalStore, plugin_id: &str) {
    store.approved.remove(plugin_id);
    store.denied.insert(
        plugin_id.to_string(),
        DenialRecord {
            denied_at: chrono_now(),
        },
    );
}

/// SHA-256 hash of the serialized permissions. Deterministic because
/// Permissions is a struct (not HashMap), so field order is stable.
pub fn hash_permissions(permissions: &Permissions) -> String {
    let json = serde_json::to_string(permissions).unwrap_or_default();
    let hash = Sha256::digest(json.as_bytes());
    format!("sha256:{:x}", hash)
}

/// ISO 8601 timestamp (simplified — no chrono crate dependency).
fn chrono_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("unix:{}", dur.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest(perms: Permissions) -> PluginManifest {
        PluginManifest {
            id: "com.test.plugin".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            runtime: crate::mcp::manifest::Runtime::Node,
            entry: "index.js".to_string(),
            permissions: perms,
            configuration: None,
        }
    }

    #[test]
    fn new_plugin_needs_approval() {
        let store = ApprovalStore::default();
        let manifest = test_manifest(Permissions::default());
        assert_eq!(check_approval(&store, &manifest), ApprovalStatus::NeedsApproval);
    }

    #[test]
    fn approved_plugin_loads_silently() {
        let mut store = ApprovalStore::default();
        let manifest = test_manifest(Permissions::default());
        record_approval(&mut store, &manifest);
        assert_eq!(check_approval(&store, &manifest), ApprovalStatus::Approved);
    }

    #[test]
    fn permission_change_triggers_reprompt() {
        let mut store = ApprovalStore::default();
        let manifest = test_manifest(Permissions::default());
        record_approval(&mut store, &manifest);

        // Change permissions
        let new_manifest = test_manifest(Permissions {
            network: Some(vec!["evil.com".to_string()]),
            ..Default::default()
        });
        assert_eq!(
            check_approval(&store, &new_manifest),
            ApprovalStatus::PermissionsChanged
        );
    }

    #[test]
    fn denied_plugin_stays_denied() {
        let mut store = ApprovalStore::default();
        record_denial(&mut store, "com.test.plugin");
        let manifest = test_manifest(Permissions::default());
        assert_eq!(check_approval(&store, &manifest), ApprovalStatus::Denied);
    }

    #[test]
    fn hash_is_stable() {
        let perms = Permissions { clipboard: true, ..Default::default() };
        let h1 = hash_permissions(&perms);
        let h2 = hash_permissions(&perms);
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
    }
}
