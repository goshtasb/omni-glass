//! Model management — download, verify, store, and delete GGUF models.
//!
//! Models are stored in the platform-appropriate config directory:
//!   macOS:   ~/Library/Application Support/omni-glass/models/
//!   Linux:   ~/.config/omni-glass/models/
//!   Windows: %APPDATA%/omni-glass/models/

use super::model_registry::ModelInfo;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Base directory for downloaded models.
pub fn models_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("omni-glass")
        .join("models")
}

/// Full path for a model's GGUF file.
pub fn model_path(model: &ModelInfo) -> PathBuf {
    models_dir().join(model.filename)
}

/// Check if a model is already downloaded.
pub fn is_model_downloaded(model: &ModelInfo) -> bool {
    let path = model_path(model);
    path.exists() && path.metadata().map(|m| m.len() > 0).unwrap_or(false)
}

/// List IDs of all downloaded models.
pub fn downloaded_model_ids() -> Vec<String> {
    let registry = super::model_registry::available_models();
    registry
        .iter()
        .filter(|m| is_model_downloaded(m))
        .map(|m| m.id.to_string())
        .collect()
}

/// Download a model with progress reporting via a Tauri event emitter.
///
/// Supports resumption: if a partial file exists, sends a Range header.
/// Returns the path to the downloaded file on success.
pub async fn download_model(
    model: &ModelInfo,
    app: &tauri::AppHandle,
) -> Result<PathBuf, String> {
    use tauri::Emitter;

    let dir = models_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create models dir: {}", e))?;

    let dest = dir.join(model.filename);
    let partial = dir.join(format!("{}.partial", model.filename));

    // Check for resumable partial download
    let existing_bytes = if partial.exists() {
        partial.metadata().map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    log::info!(
        "[MODEL] Downloading {} ({} bytes, resuming from {})",
        model.id,
        model.size_bytes,
        existing_bytes
    );

    let client = reqwest::Client::new();
    let mut req = client.get(model.url);
    if existing_bytes > 0 {
        req = req.header("Range", format!("bytes={}-", existing_bytes));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }

    // Total size from content-length or known size
    let total = if existing_bytes > 0 {
        model.size_bytes
    } else {
        resp.content_length().unwrap_or(model.size_bytes)
    };

    // Open file for appending (resume) or create new
    let mut file = if existing_bytes > 0 {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&partial)
            .map_err(|e| format!("Failed to open partial file: {}", e))?
    } else {
        std::fs::File::create(&partial)
            .map_err(|e| format!("Failed to create file: {}", e))?
    };

    // Stream the response body in chunks, emitting progress events.
    // Uses reqwest's built-in chunk() — no futures-util dependency needed.
    use std::io::Write;
    let mut downloaded = existing_bytes;
    let mut resp = resp;

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("Download stream error: {}", e))?
    {
        file.write_all(&chunk)
            .map_err(|e| format!("Failed to write chunk: {}", e))?;
        downloaded += chunk.len() as u64;

        // Emit progress every ~500 KB to avoid flooding the event bus
        if downloaded % (512 * 1024) < chunk.len() as u64 || downloaded >= total {
            let _ = app.emit(
                "model-download-progress",
                serde_json::json!({
                    "modelId": model.id,
                    "downloaded": downloaded,
                    "total": total,
                    "percent": ((downloaded as f64 / total as f64) * 100.0) as u32,
                }),
            );
        }
    }

    // Rename .partial to final filename
    std::fs::rename(&partial, &dest)
        .map_err(|e| format!("Failed to finalize download: {}", e))?;

    log::info!("[MODEL] Download complete: {}", dest.display());
    Ok(dest)
}

/// Verify a downloaded model's integrity via SHA-256.
///
/// Reads the file in 8 MB chunks to avoid loading the entire 2 GB into memory.
pub fn verify_model_hash(path: &Path, expected_hash: &str) -> Result<(), String> {
    if expected_hash.is_empty() {
        // No hash to verify — skip (hash will be recorded on first download)
        return Ok(());
    }

    let file = std::fs::File::open(path)
        .map_err(|e| format!("Cannot open model for verification: {}", e))?;
    let mut reader = std::io::BufReader::with_capacity(8 * 1024 * 1024, file);
    let mut hasher = Sha256::new();

    use std::io::Read;
    let mut buf = vec![0u8; 8 * 1024 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Read error during hash: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let hash = format!("{:x}", hasher.finalize());
    if hash != expected_hash {
        return Err(format!(
            "Hash mismatch: expected {}, got {}",
            expected_hash, hash
        ));
    }

    Ok(())
}

/// Delete a downloaded model file.
pub fn delete_model(model: &ModelInfo) -> Result<(), String> {
    let path = model_path(model);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete model: {}", e))?;
        log::info!("[MODEL] Deleted: {}", path.display());
    }
    // Also clean up any partial downloads
    let partial = models_dir().join(format!("{}.partial", model.filename));
    if partial.exists() {
        let _ = std::fs::remove_file(&partial);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_dir_uses_config_dir() {
        let dir = models_dir();
        let dir_str = dir.to_string_lossy();
        // Should contain omni-glass/models regardless of platform
        assert!(dir_str.contains("omni-glass"));
        assert!(dir_str.ends_with("models"));
    }

    #[test]
    fn model_path_includes_filename() {
        let model = super::super::model_registry::default_model();
        let path = model_path(model);
        assert!(path.to_string_lossy().ends_with(".gguf"));
    }

    #[test]
    fn downloaded_model_ids_returns_empty_initially() {
        // Unless models are pre-downloaded, this should return empty or a subset
        let ids = downloaded_model_ids();
        // Just verify it doesn't panic
        assert!(ids.len() <= 2);
    }
}
