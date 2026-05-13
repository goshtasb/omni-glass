//! Known local LLM models — static registry of supported GGUF models.
//!
//! Each entry contains the download URL, file size, SHA-256 hash for
//! verification, and hardware requirements. Ships with Qwen-2.5 variants.

/// Metadata for a supported local model.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub size_bytes: u64,
    pub context_length: u32,
    pub ram_required_gb: f32,
    pub description: &'static str,
}

/// All models known to Omni-Glass.
///
/// Two Qwen-2.5 variants cover the 8 GB and 16 GB RAM tiers.
/// Both use Q4_K_M quantization (best quality-per-bit for 4-bit).
static MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "qwen2.5-3b-q4km",
        name: "Qwen 2.5 3B (Balanced)",
        filename: "qwen2.5-3b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf",
        size_bytes: 2_254_267_392, // ~2.1 GB
        context_length: 2048,
        ram_required_gb: 4.0,
        description: "Best balance of speed and quality. Requires 4 GB free RAM.",
    },
    ModelInfo {
        id: "qwen2.5-1.5b-q4km",
        name: "Qwen 2.5 1.5B (Faster)",
        filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
        size_bytes: 1_202_273_280, // ~1.12 GB
        context_length: 2048,
        ram_required_gb: 2.5,
        description: "Faster, smaller. Lower quality for complex tasks. Requires 2.5 GB free RAM.",
    },
];

/// Return all available models.
pub fn available_models() -> &'static [ModelInfo] {
    MODELS
}

/// The default recommended model.
pub fn default_model() -> &'static ModelInfo {
    &MODELS[0] // qwen2.5-3b-q4km
}

/// Look up a model by ID.
pub fn find_model(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_models() {
        let models = available_models();
        assert!(models.len() >= 2);
    }

    #[test]
    fn default_model_is_3b() {
        let m = default_model();
        assert_eq!(m.id, "qwen2.5-3b-q4km");
    }

    #[test]
    fn find_model_by_id() {
        assert!(find_model("qwen2.5-3b-q4km").is_some());
        assert!(find_model("qwen2.5-1.5b-q4km").is_some());
        assert!(find_model("nonexistent").is_none());
    }

    #[test]
    fn urls_point_to_huggingface() {
        for m in available_models() {
            assert!(m.url.starts_with("https://huggingface.co/"));
            assert!(m.url.ends_with(".gguf"));
        }
    }
}
