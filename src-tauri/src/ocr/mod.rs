//! OCR domain — platform-abstracted text recognition.
//!
//! Dispatches to the appropriate platform backend:
//! - macOS: Apple Vision Framework via swift-bridge FFI
//! - Windows: Windows.Media.Ocr via windows-rs (WinRT)
//!
//! External code uses the public functions here — the platform
//! backend is selected at compile time via #[cfg(target_os)].

pub mod heuristics;

#[cfg(target_os = "macos")]
mod apple_vision;

#[cfg(target_os = "windows")]
mod windows_ocr;

/// Recognition level for text recognition.
///
/// Maps to VNRequestTextRecognitionLevel on macOS.
/// On Windows, both levels use the same WinRT engine
/// (Windows.Media.Ocr doesn't expose accuracy levels).
#[derive(Debug, Clone, Copy)]
pub enum RecognitionLevel {
    Accurate = 0,
    Fast = 1,
}

impl Default for RecognitionLevel {
    fn default() -> Self {
        RecognitionLevel::Fast
    }
}

/// Result of OCR processing — platform-independent.
#[derive(Debug, Clone)]
pub struct OcrOutput {
    pub text: String,
    pub char_count: i64,
    #[allow(dead_code)] // set by FFI, reserved for future diagnostics
    pub latency_ms: f64,
    pub confidence: f64,
    #[allow(dead_code)] // set by FFI, reserved for future diagnostics
    pub recognition_level: String,
}

/// Run OCR on in-memory PNG bytes. Eliminates disk I/O from the pipeline.
///
/// Dispatches to Apple Vision (macOS) or Windows.Media.Ocr (Windows).
pub fn recognize_text_from_bytes(png_bytes: Vec<u8>, level: RecognitionLevel) -> OcrOutput {
    #[cfg(target_os = "macos")]
    {
        apple_vision::recognize_text_from_bytes(png_bytes, level)
    }

    #[cfg(target_os = "windows")]
    {
        windows_ocr::recognize_text_from_bytes(png_bytes, level)
    }
}

/// Run OCR on an image file and return extracted text with metadata.
///
/// Only available on macOS (Apple Vision supports path-based input).
/// On Windows, load the file bytes and use recognize_text_from_bytes instead.
#[cfg(target_os = "macos")]
#[allow(dead_code)] // path-based API reserved for future use
pub fn recognize_text(image_path: &str, level: RecognitionLevel) -> OcrOutput {
    apple_vision::recognize_text(image_path, level)
}

/// Warm up the OCR engine to avoid cold-start penalty on first snip.
/// Call once at startup.
pub fn warm_up() {
    #[cfg(target_os = "macos")]
    apple_vision::warm_up();

    #[cfg(target_os = "windows")]
    windows_ocr::warm_up();
}
