//! LLM response types — ActionMenu and Action.
//!
//! These match the JSON schema from the LLM Integration PRD Section 6.
//! The LLM returns JSON that deserializes directly into these types.

use serde::{Deserialize, Serialize};

/// The action menu returned by the CLASSIFY pipeline.
///
/// Rendered as a popup near the snip location.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionMenu {
    pub content_type: String,
    pub confidence: f64,
    pub summary: String,
    pub detected_language: Option<String>,
    pub actions: Vec<Action>,
}

/// A single action the user can take on snipped content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub priority: u8,
    pub description: String,
    pub requires_execution: bool,
}

/// Partial menu data emitted during streaming, before the full ActionMenu is ready.
/// Sent to the frontend as soon as contentType + summary are parsed from the stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionMenuSkeleton {
    pub content_type: String,
    pub summary: String,
}

impl ActionMenu {
    /// Fallback menu for when the LLM call fails or returns invalid JSON.
    /// Always provides basic copy/explain/search actions.
    pub fn fallback() -> Self {
        Self {
            content_type: "unknown".to_string(),
            confidence: 0.0,
            summary: "Could not analyze content".to_string(),
            detected_language: None,
            actions: vec![
                Action {
                    id: "copy_text".to_string(),
                    label: "Copy Text".to_string(),
                    icon: "clipboard".to_string(),
                    priority: 1,
                    description: "Copy the extracted text to clipboard".to_string(),
                    requires_execution: false,
                },
                Action {
                    id: "explain".to_string(),
                    label: "Explain This".to_string(),
                    icon: "lightbulb".to_string(),
                    priority: 2,
                    description: "Explain what this content means".to_string(),
                    requires_execution: true,
                },
                Action {
                    id: "search_web".to_string(),
                    label: "Search Web".to_string(),
                    icon: "search".to_string(),
                    priority: 3,
                    description: "Search for this text online".to_string(),
                    requires_execution: false,
                },
            ],
        }
    }
}
