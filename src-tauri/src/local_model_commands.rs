//! Tauri commands for local model management.
//!
//! These commands are always registered so the Settings UI can call them
//! regardless of compile-time feature flags. When `local-llm` is disabled,
//! they return safe stub responses.

use serde::Serialize;

/// Model info returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct LocalModelInfo {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub ram_required_gb: f32,
    pub description: String,
    pub downloaded: bool,
}

/// Tauri command: list available local models and their download status.
#[tauri::command]
pub fn get_local_models() -> Result<serde_json::Value, String> {
    #[cfg(feature = "local-llm")]
    {
        let models: Vec<LocalModelInfo> = crate::llm::model_registry::available_models()
            .iter()
            .map(|m| LocalModelInfo {
                id: m.id.to_string(),
                name: m.name.to_string(),
                size_bytes: m.size_bytes,
                ram_required_gb: m.ram_required_gb,
                description: m.description.to_string(),
                downloaded: crate::llm::model_manager::is_model_downloaded(m),
            })
            .collect();

        return Ok(serde_json::json!({
            "featureEnabled": true,
            "models": models,
        }));
    }

    #[cfg(not(feature = "local-llm"))]
    Ok(serde_json::json!({
        "featureEnabled": false,
        "models": [],
    }))
}

/// Tauri command: download a local model by ID.
///
/// Emits `model-download-progress` events during download.
#[tauri::command]
pub async fn download_local_model(
    #[allow(unused_variables)] app: tauri::AppHandle,
    #[allow(unused_variables)] model_id: String,
) -> Result<String, String> {
    #[cfg(feature = "local-llm")]
    {
        let model = crate::llm::model_registry::find_model(&model_id)
            .ok_or_else(|| format!("Unknown model: {}", model_id))?;

        let path = crate::llm::model_manager::download_model(model, &app).await?;

        // Verify hash if available (skip for now — hashes recorded on first download)
        // TODO: Add SHA-256 hashes to model_registry once verified

        return Ok(path.to_string_lossy().to_string());
    }

    #[cfg(not(feature = "local-llm"))]
    Err("Local LLM feature not enabled in this build".to_string())
}

/// Tauri command: delete a downloaded local model.
#[tauri::command]
pub fn delete_local_model(
    #[allow(unused_variables)] model_id: String,
) -> Result<(), String> {
    #[cfg(feature = "local-llm")]
    {
        let model = crate::llm::model_registry::find_model(&model_id)
            .ok_or_else(|| format!("Unknown model: {}", model_id))?;
        return crate::llm::model_manager::delete_model(model);
    }

    #[cfg(not(feature = "local-llm"))]
    Err("Local LLM feature not enabled in this build".to_string())
}
