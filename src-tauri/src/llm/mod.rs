//! LLM domain — multi-provider classification pipeline.
//!
//! Public API for the Brain layer of Omni-Glass.
//! External code should only use the functions exported here.
//!
//! Providers:
//!   - Anthropic Claude Haiku (classify.rs)
//!   - Google Gemini Flash (gemini.rs)
//!
//! Shared:
//!   - streaming.rs — SSE parsing + partial JSON extraction
//!   - provider.rs  — provider metadata + configuration checks

mod classify;
pub mod execute;
mod gemini;
pub mod provider;
pub mod prompts;
mod prompts_execute;
pub mod streaming;
pub mod types;

pub use classify::{classify, classify_streaming};
pub use execute::{execute_action_anthropic, ActionResult};
pub use gemini::classify_streaming_gemini;
pub use types::{ActionMenu, ActionMenuSkeleton};

use std::sync::Mutex;

/// Thread-safe storage for the current ActionMenu result + OCR text.
/// Written by process_snip, read by get_action_menu command.
pub struct ActionMenuState {
    pub menu: Mutex<Option<ActionMenu>>,
    pub ocr_text: Mutex<Option<String>>,
}

impl ActionMenuState {
    pub fn new() -> Self {
        Self {
            menu: Mutex::new(None),
            ocr_text: Mutex::new(None),
        }
    }
}
