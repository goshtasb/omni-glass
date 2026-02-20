//! OCR Benchmark CLI for Omni-Glass.
//!
//! Benchmarks Apple Vision Framework text recognition via swift-bridge FFI.
//! No subprocess overhead — calls Vision Framework directly in-process.
//!
//! Usage:
//!   cargo run -- <image.png>                    Single image, accurate mode
//!   cargo run -- <image.png> --fast             Single image, fast mode
//!   cargo run -- <image.png> --compare          Single image, both modes side-by-side
//!   cargo run -- --batch <directory>            All PNGs in directory → CSV
//!   cargo run -- --batch <directory> --fast     Batch with fast mode
//!   cargo run -- --batch <directory> --compare  Batch with both modes

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

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
        fn warm_up_vision();
    }
}

/// Recognition level for Apple Vision Framework OCR.
///
/// This is the public API contract for the pipeline:
/// - CLASSIFY step uses `Fast` (16-30ms warm, good enough for content type ID)
/// - EXECUTE step uses `Accurate` when the action needs full-fidelity text
#[derive(Debug, Clone, Copy)]
pub enum RecognitionLevel {
    /// High accuracy, language correction enabled. 95-460ms depending on content.
    Accurate = 0,
    /// Fast recognition, lower accuracy. 16-70ms warm.
    Fast = 1,
}

impl RecognitionLevel {
    fn as_i32(self) -> i32 {
        self as i32
    }
}

impl Default for RecognitionLevel {
    fn default() -> Self {
        RecognitionLevel::Fast
    }
}

/// Public API: run OCR on an image file. This is the function Week 2 pipeline calls.
pub fn recognize_text(
    image_path: &str,
    level: RecognitionLevel,
) -> ffi::OcrResult {
    ffi::run_ocr_on_path(image_path.to_string(), level.as_i32())
}

/// Public API: warm up Vision Framework. Call once at app startup.
pub fn warm_up() {
    ffi::warm_up_vision();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  ocr-bench <image.png> [--fast] [--compare]");
        eprintln!("  ocr-bench --batch <directory> [--fast] [--compare]");
        std::process::exit(1);
    }

    let use_fast = args.contains(&"--fast".to_string());
    let compare = args.contains(&"--compare".to_string());
    let warm = args.contains(&"--warm".to_string());

    // Simulate app startup warm-up: fire a throwaway recognition request
    // so Vision Framework loads its ML model before the real benchmark.
    if warm {
        let warm_start = Instant::now();
        ffi::warm_up_vision();
        let warm_ms = warm_start.elapsed().as_micros() as f64 / 1000.0;
        eprintln!("[WARM-UP] Vision Framework initialized in {:.1}ms", warm_ms);
    }

    if args[1] == "--batch" {
        let dir = args.get(2).expect("--batch requires a directory path");
        run_batch(dir, use_fast, compare);
    } else {
        run_single(&args[1], use_fast, compare);
    }
}

/// Run OCR via FFI and measure wall-clock time from Rust side.
fn ocr_ffi(abs_path: &str, level: RecognitionLevel) -> (ffi::OcrResult, u128) {
    let start = Instant::now();
    let result = recognize_text(abs_path, level);
    let wall_us = start.elapsed().as_micros();
    (result, wall_us)
}

fn run_single(image_path: &str, use_fast: bool, compare: bool) {
    let abs_path = std::fs::canonicalize(image_path).unwrap_or_else(|_| {
        eprintln!("File not found: {}", image_path);
        std::process::exit(1);
    });
    let abs_str = abs_path.to_str().unwrap();

    if compare {
        // Run both modes
        let (accurate, accurate_wall_us) = ocr_ffi(abs_str, RecognitionLevel::Accurate);
        let (fast, fast_wall_us) = ocr_ffi(abs_str, RecognitionLevel::Fast);

        eprintln!("=== COMPARISON: {} ===", image_path);
        eprintln!();
        eprintln!("  ACCURATE:");
        eprintln!("    Vision latency: {:.1}ms", accurate.latency_ms);
        eprintln!("    Rust wall time: {:.2}ms", accurate_wall_us as f64 / 1000.0);
        eprintln!("    Chars: {}", accurate.char_count);
        eprintln!("    Confidence: {:.3}", accurate.confidence);
        eprintln!();
        eprintln!("  FAST:");
        eprintln!("    Vision latency: {:.1}ms", fast.latency_ms);
        eprintln!("    Rust wall time: {:.2}ms", fast_wall_us as f64 / 1000.0);
        eprintln!("    Chars: {}", fast.char_count);
        eprintln!("    Confidence: {:.3}", fast.confidence);
        eprintln!();
        eprintln!(
            "  Speedup: {:.1}x ({:.1}ms → {:.1}ms)",
            accurate.latency_ms / fast.latency_ms.max(0.1),
            accurate.latency_ms,
            fast.latency_ms
        );
    } else {
        let level = if use_fast { RecognitionLevel::Fast } else { RecognitionLevel::Accurate };
        let (result, wall_us) = ocr_ffi(abs_str, level);

        // Print JSON-like output for compatibility
        println!("{{");
        println!("  \"recognitionLevel\": \"{}\",", result.recognition_level);
        println!("  \"visionLatencyMs\": {:.2},", result.latency_ms);
        println!("  \"rustWallTimeMs\": {:.2},", wall_us as f64 / 1000.0);
        println!("  \"confidence\": {:.4},", result.confidence);
        println!("  \"charCount\": {},", result.char_count);
        // Truncate text to 200 chars for display
        let display_text = if result.text.len() > 200 {
            format!("{}...", &result.text[..200])
        } else {
            result.text.clone()
        };
        let escaped = display_text
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        println!("  \"textPreview\": \"{}\"", escaped);
        println!("}}");
    }
}

fn run_batch(dir_path: &str, use_fast: bool, compare: bool) {
    let dir = Path::new(dir_path);
    if !dir.is_dir() {
        eprintln!("Not a directory: {}", dir_path);
        std::process::exit(1);
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("Failed to read directory")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|ext| ext == "png" || ext == "jpg" || ext == "jpeg")
                .unwrap_or(false)
        })
        .collect();
    entries.sort();

    if entries.is_empty() {
        eprintln!("No image files found in {}", dir_path);
        std::process::exit(1);
    }

    if compare {
        // CSV header for comparison mode
        println!(
            "filename,chars_accurate,vision_ms_accurate,wall_ms_accurate,conf_accurate,\
             chars_fast,vision_ms_fast,wall_ms_fast,conf_fast,speedup"
        );
    } else {
        println!("filename,char_count,vision_ms,wall_ms,confidence,recognition_level");
    }

    let mut latencies_accurate: Vec<f64> = Vec::new();
    let mut latencies_fast: Vec<f64> = Vec::new();

    for image_path in &entries {
        let filename = image_path.file_name().unwrap().to_string_lossy().to_string();
        let abs_str = image_path.to_str().unwrap();

        if compare {
            let (accurate, accurate_wall_us) = ocr_ffi(abs_str, RecognitionLevel::Accurate);
            let (fast, fast_wall_us) = ocr_ffi(abs_str, RecognitionLevel::Fast);
            let speedup = accurate.latency_ms / fast.latency_ms.max(0.1);

            println!(
                "{},{},{:.1},{:.2},{:.3},{},{:.1},{:.2},{:.3},{:.1}x",
                filename,
                accurate.char_count,
                accurate.latency_ms,
                accurate_wall_us as f64 / 1000.0,
                accurate.confidence,
                fast.char_count,
                fast.latency_ms,
                fast_wall_us as f64 / 1000.0,
                fast.confidence,
                speedup
            );

            latencies_accurate.push(accurate.latency_ms);
            latencies_fast.push(fast.latency_ms);
        } else {
            let level = if use_fast { RecognitionLevel::Fast } else { RecognitionLevel::Accurate };
            let (result, wall_us) = ocr_ffi(abs_str, level);

            println!(
                "{},{},{:.1},{:.2},{:.3},{}",
                filename,
                result.char_count,
                result.latency_ms,
                wall_us as f64 / 1000.0,
                result.confidence,
                result.recognition_level
            );

            if use_fast {
                latencies_fast.push(result.latency_ms);
            } else {
                latencies_accurate.push(result.latency_ms);
            }
        }

        std::io::stdout().flush().ok();
    }

    // Print summary
    eprintln!("\n--- Benchmark Summary ---");
    eprintln!("  Images processed: {}", entries.len());

    if !latencies_accurate.is_empty() {
        print_latency_summary("Accurate", &mut latencies_accurate, 300.0);
    }
    if !latencies_fast.is_empty() {
        print_latency_summary("Fast", &mut latencies_fast, 100.0);
    }
}

fn print_latency_summary(label: &str, latencies: &mut [f64], target_ms: f64) {
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = latencies[latencies.len() / 2];
    let p99_idx = ((latencies.len() as f64 * 0.99).ceil() as usize).min(latencies.len() - 1);
    let p99 = latencies[p99_idx];
    let avg: f64 = latencies.iter().sum::<f64>() / latencies.len() as f64;

    eprintln!("  [{}]", label);
    eprintln!("    Median: {:.1}ms", median);
    eprintln!("    Average: {:.1}ms", avg);
    eprintln!("    P99: {:.1}ms", p99);
    eprintln!(
        "    Target (< {:.0}ms): {}",
        target_ms,
        if median < target_ms { "PASS" } else { "FAIL" }
    );
}
