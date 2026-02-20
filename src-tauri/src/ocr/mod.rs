//! OCR domain — Apple Vision Framework via swift-bridge FFI.
//!
//! Wraps the Swift OCR bridge for use in the Tauri app pipeline.
//! External code should only use the public functions here.

pub mod heuristics;

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

/// Recognition level for Apple Vision text recognition.
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

/// Result of OCR processing, converted from the FFI type.
#[derive(Debug, Clone)]
pub struct OcrOutput {
    pub text: String,
    pub char_count: i64,
    pub latency_ms: f64,
    pub confidence: f64,
    pub recognition_level: String,
}

/// Run OCR on an image file and return extracted text with metadata.
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

/// Run OCR on in-memory PNG bytes. Eliminates disk I/O from the pipeline.
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
/// Call once at startup to avoid cold-start penalty on first snip.
pub fn warm_up() {
    ffi::warm_up_vision();
}
