//! macOS OCR via Apple Vision Framework (swift-bridge FFI).
//!
//! This module is only compiled on macOS. It uses swift-bridge to call
//! into Swift code that wraps VNRecognizeTextRequest.

use super::{OcrOutput, RecognitionLevel};

#[swift_bridge::bridge]
mod ffi {
    #[swift_bridge(swift_repr = "struct")]
    struct OcrResult {
        text: String,
        char_count: i64,
        latency_ms: f64,
        confidence: f64,
        recognition_level: String,
    }

    extern "Swift" {
        fn run_ocr_on_path(path: String, level: i32) -> OcrResult;
        fn run_ocr_on_png_data(data: Vec<u8>, level: i32) -> OcrResult;
        fn warm_up_vision();
    }
}

/// Run OCR on an image file path via Apple Vision.
#[allow(dead_code)] // path-based API reserved for future use
pub fn recognize_text(image_path: &str, level: RecognitionLevel) -> OcrOutput {
    let result = ffi::run_ocr_on_path(image_path.to_string(), level as i32);
    OcrOutput {
        text: result.text,
        char_count: result.char_count,
        latency_ms: result.latency_ms,
        confidence: result.confidence,
        recognition_level: result.recognition_level,
    }
}

/// Run OCR on in-memory PNG bytes via Apple Vision.
pub fn recognize_text_from_bytes(png_bytes: Vec<u8>, level: RecognitionLevel) -> OcrOutput {
    let result = ffi::run_ocr_on_png_data(png_bytes, level as i32);
    OcrOutput {
        text: result.text,
        char_count: result.char_count,
        latency_ms: result.latency_ms,
        confidence: result.confidence,
        recognition_level: result.recognition_level,
    }
}

/// Warm up Vision Framework with a throwaway recognition request.
pub fn warm_up() {
    ffi::warm_up_vision();
}
