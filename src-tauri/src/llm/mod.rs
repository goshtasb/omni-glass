//! LLM domain — Claude API integration.
//!
//! Public API for the Brain layer of Omni-Glass.
//! External code should only use the functions exported here.

mod classify;
mod prompts;
pub mod types;

pub use classify::{classify, classify_streaming};
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
