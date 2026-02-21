//! Tauri commands for the plugin approval flow.
//!
//! These commands are called by the permission-prompt.ts frontend
//! to display pending plugin approvals and record user decisions.

use crate::mcp::approval;
use crate::mcp::loader::PendingApprovals;
use crate::mcp::manifest::PluginManifest;
use crate::mcp::registry::ToolRegistry;
use crate::mcp::sandbox::risk::{self, RiskLevel};
use serde::Serialize;

/// Plugin info sent to the permission prompt UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: crate::mcp::manifest::Permissions,
    pub risk_level: RiskLevel,
    pub is_update: bool, // true if PermissionsChanged (not NeedsApproval)
}

/// Get all plugins awaiting user approval.
#[tauri::command]
pub async fn get_pending_approvals(
    state: tauri::State<'_, PendingApprovals>,
) -> Result<Vec<PendingPlugin>, String> {
    let queue = state.queue.lock().await;
    Ok(queue
        .iter()
        .map(|(manifest, _dir, is_update)| PendingPlugin {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            permissions: manifest.permissions.clone(),
            risk_level: risk::calculate_risk(&manifest.permissions),
            is_update: *is_update,
        })
        .collect())
}

/// Record the user's approval or denial of a plugin.
///
/// If approved, the plugin is loaded immediately (sandboxed spawn,
/// initialize, discover tools, register). If denied, it's removed
/// from the pending queue and recorded in the approvals file.
#[tauri::command]
pub async fn approve_plugin(
    plugin_id: String,
    approved: bool,
    pending: tauri::State<'_, PendingApprovals>,
    registry: tauri::State<'_, ToolRegistry>,
) -> Result<(), String> {
    // Find and remove the plugin from the pending queue
    let entry = {
        let mut queue = pending.queue.lock().await;
        let idx = queue.iter().position(|(m, _, _)| m.id == plugin_id);
        match idx {
            Some(i) => Some(queue.remove(i)),
            None => None,
        }
    };

    let mut store = approval::load_approvals();

    if approved {
        if let Some((manifest, plugin_dir, _)) = entry {
            approval::record_approval(&mut store, &manifest);
            approval::save_approvals(&store)?;

            // Load the plugin now
            crate::mcp::loader::load_approved_plugin(&manifest, &plugin_dir, &registry).await?;
            log::info!("[APPROVAL] Plugin '{}' approved and loaded", plugin_id);
        }
    } else {
        approval::record_denial(&mut store, &plugin_id);
        approval::save_approvals(&store)?;
        log::info!("[APPROVAL] Plugin '{}' denied", plugin_id);
    }

    Ok(())
}

/// Serializable summary for the frontend.
impl From<&PluginManifest> for PendingPlugin {
    fn from(m: &PluginManifest) -> Self {
        PendingPlugin {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            permissions: m.permissions.clone(),
            risk_level: risk::calculate_risk(&m.permissions),
            is_update: false,
        }
    }
}
