//! LLM provider trait — common interface for all classification providers.
//!
//! Each provider implements this trait. The pipeline dispatches to the
//! active provider based on user configuration.

use serde::{Deserialize, Serialize};

/// Provider metadata exposed to the settings panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub env_key: String,
    pub cost_per_snip: String,
    pub speed_stars: u8,
    pub quality_stars: u8,
}

/// All known providers and their display info.
pub fn all_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "anthropic".to_string(),
            name: "Claude Haiku — Fast, ~$0.002/snip".to_string(),
            env_key: "ANTHROPIC_API_KEY".to_string(),
            cost_per_snip: "~$0.002".to_string(),
            speed_stars: 4,
            quality_stars: 5,
        },
        ProviderInfo {
            id: "gemini".to_string(),
            name: "Gemini Flash — Not yet benchmarked".to_string(),
            env_key: "GEMINI_API_KEY".to_string(),
            cost_per_snip: "Free tier / ~$0.0001".to_string(),
            speed_stars: 5,
            quality_stars: 4,
        },
        ProviderInfo {
            id: "local".to_string(),
            name: "Local (Qwen 2.5) — Offline, free, slower".to_string(),
            env_key: "".to_string(),
            cost_per_snip: "Free".to_string(),
            speed_stars: 2,
            quality_stars: 3,
        },
    ]
}

/// Check if a provider has an API key configured.
///
/// For the "local" provider, checks whether any model is downloaded.
pub fn is_provider_configured(provider_id: &str) -> bool {
    match provider_id {
        "anthropic" => env_key_set("ANTHROPIC_API_KEY"),
        "gemini" => env_key_set("GEMINI_API_KEY"),
        #[cfg(feature = "local-llm")]
        "local" => !super::model_manager::downloaded_model_ids().is_empty(),
        _ => false,
    }
}

fn env_key_set(key: &str) -> bool {
    std::env::var(key).map(|k| !k.is_empty()).unwrap_or(false)
}
