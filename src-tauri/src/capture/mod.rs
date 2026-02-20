//! Screen capture domain — public API.
//!
//! This module owns all screen capture functionality.
//! External code should only use the public functions exported here.

mod region;
mod screenshot;

pub use region::crop_to_png_bytes;
pub use screenshot::capture_primary_monitor;

use image::DynamicImage;
use std::sync::Mutex;

/// Info needed by the overlay to display the screenshot.
/// Stored in CaptureState so the overlay can fetch it via a Tauri command
/// (eliminates the race condition where an event fires before JS loads).
#[derive(Clone, serde::Serialize)]
pub struct CaptureInfo {
    pub image_path: String,
    pub click_epoch_ms: f64,
}

/// Thread-safe storage for the current full-screen capture.
/// Held between capture and crop so the user can draw a rectangle.
pub struct CaptureState {
    pub screenshot: Mutex<Option<DynamicImage>>,
    pub capture_info: Mutex<Option<CaptureInfo>>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self {
            screenshot: Mutex::new(None),
            capture_info: Mutex::new(None),
        }
    }
}
