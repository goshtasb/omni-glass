/// Omni-Glass OCR Bridge — Apple Vision Framework via swift-bridge FFI.
///
/// Called directly from Rust — no subprocess, no disk I/O overhead.
/// The image path is resolved on the Rust side before calling.

import Foundation
import Vision
import CoreGraphics
import ImageIO

/// Warm up Vision Framework by running a throwaway recognition on a 1x1 white image.
/// Call once at app startup to pay the ML model loading cost before the user snips.
func warm_up_vision() {
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    guard let ctx = CGContext(
        data: nil, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
        space: colorSpace, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ), let cgImage = ctx.makeImage() else { return }

    let request = VNRecognizeTextRequest { _, _ in }
    request.recognitionLevel = .fast
    let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
    try? handler.perform([request])
}

/// FFI entry point: run OCR on an image file at the given absolute path.
/// level: 0 = accurate, 1 = fast
func run_ocr_on_path(path: RustString, level: Int32) -> OcrResult {
    let pathStr = path.toString()

    let imageURL = URL(fileURLWithPath: pathStr)
    guard let imageSource = CGImageSourceCreateWithURL(imageURL as CFURL, nil),
          let cgImage = CGImageSourceCreateImageAtIndex(imageSource, 0, nil) else {
        return OcrResult(
            text: "ERROR: Failed to load image: \(pathStr)".intoRustString(),
            char_count: 0,
            latency_ms: 0.0,
            confidence: 0.0,
            recognition_level: "error".intoRustString()
        )
    }

    return performOCR(on: cgImage, level: level)
}

/// FFI entry point: run OCR on in-memory PNG data. Eliminates disk I/O.
/// level: 0 = accurate, 1 = fast
func run_ocr_on_png_data(data: RustVec<UInt8>, level: Int32) -> OcrResult {
    let length = Int(data.len())
    var bytes = [UInt8](repeating: 0, count: length)
    for i in 0..<length {
        bytes[i] = data.get(index: UInt(i)) ?? 0
    }
    let nsData = Data(bytes) as CFData

    guard let imageSource = CGImageSourceCreateWithData(nsData, nil),
          let cgImage = CGImageSourceCreateImageAtIndex(imageSource, 0, nil) else {
        return OcrResult(
            text: "ERROR: Failed to create image from \(length) bytes".intoRustString(),
            char_count: 0,
            latency_ms: 0.0,
            confidence: 0.0,
            recognition_level: "error".intoRustString()
        )
    }

    return performOCR(on: cgImage, level: level)
}

/// Core OCR logic — shared between path-based and bytes-based entry points.
private func performOCR(on cgImage: CGImage, level: Int32) -> OcrResult {
    let startTime = CFAbsoluteTimeGetCurrent()
    let recognitionLevel: VNRequestTextRecognitionLevel = (level == 1) ? .fast : .accurate

    var recognizedText = ""
    var totalConfidence: Double = 0.0
    var observationCount = 0

    let request = VNRecognizeTextRequest { request, error in
        guard error == nil,
              let observations = request.results as? [VNRecognizedTextObservation] else {
            return
        }
        for observation in observations {
            guard let candidate = observation.topCandidates(1).first else { continue }
            recognizedText += candidate.string + "\n"
            totalConfidence += Double(candidate.confidence)
            observationCount += 1
        }
    }

    request.recognitionLevel = recognitionLevel
    request.usesLanguageCorrection = true
    request.automaticallyDetectsLanguage = true

    let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])

    do {
        try handler.perform([request])
    } catch {
        return OcrResult(
            text: "ERROR: Vision request failed: \(error.localizedDescription)".intoRustString(),
            char_count: 0,
            latency_ms: 0.0,
            confidence: 0.0,
            recognition_level: "error".intoRustString()
        )
    }

    let elapsed = (CFAbsoluteTimeGetCurrent() - startTime) * 1000.0
    let avgConfidence = observationCount > 0 ? totalConfidence / Double(observationCount) : 0.0
    let levelName = recognitionLevel == .accurate ? "accurate" : "fast"
    let trimmed = recognizedText.trimmingCharacters(in: .whitespacesAndNewlines)

    return OcrResult(
        text: trimmed.intoRustString(),
        char_count: Int64(trimmed.count),
        latency_ms: elapsed,
        confidence: avgConfidence,
        recognition_level: levelName.intoRustString()
    )
}
