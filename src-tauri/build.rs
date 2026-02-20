//! Build script for Omni-Glass Tauri app.
//!
//! Two-phase build:
//! 1. Tauri build (generates Tauri-specific code)
//! 2. swift-bridge: generate FFI glue, compile Swift OCR bridge, link frameworks
//!
//! All generated files go to OUT_DIR (inside target/) to avoid triggering
//! Tauri's file watcher on every build.

use std::path::PathBuf;

fn main() {
    // Phase 1: Tauri
    tauri_build::build();

    // Phase 2: Swift OCR bridge via swift-bridge
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let swift_src_dir = manifest_dir.join("swift-src");
    let generated_dir = out_dir.join("swift-bridge-generated");

    println!("cargo:rerun-if-changed=src/ocr/mod.rs");
    println!("cargo:rerun-if-changed=swift-src/ocr_bridge.swift");

    // Step 1: Generate FFI glue to OUT_DIR (not inside src-tauri/)
    swift_bridge_build::parse_bridges(vec!["src/ocr/mod.rs"])
        .write_all_concatenated(&generated_dir, env!("CARGO_PKG_NAME"));

    // Step 2: Generate bridging header dynamically with absolute paths
    let bridging_header = out_dir.join("bridging-header.h");
    std::fs::write(
        &bridging_header,
        format!(
            "#ifndef BridgingHeader_h\n\
             #define BridgingHeader_h\n\
             #include \"{generated}/SwiftBridgeCore.h\"\n\
             #include \"{generated}/omni-glass/omni-glass.h\"\n\
             #endif\n",
            generated = generated_dir.display(),
        ),
    )
    .expect("Failed to write bridging header");

    // Step 3: Compile Swift → static library in OUT_DIR
    let lib_output = out_dir.join("libocr_swift.a");

    let status = std::process::Command::new("swiftc")
        .args(["-emit-library", "-static"])
        .args(["-module-name", "ocr_swift"])
        .arg("-import-objc-header")
        .arg(&bridging_header)
        .arg(swift_src_dir.join("ocr_bridge.swift"))
        .arg(generated_dir.join("SwiftBridgeCore.swift"))
        .arg(generated_dir.join("omni-glass/omni-glass.swift"))
        .arg("-o")
        .arg(&lib_output)
        .arg("-O")
        .status()
        .expect("Failed to run swiftc — is Xcode Command Line Tools installed?");

    if !status.success() {
        panic!("swiftc compilation failed");
    }

    // Step 4: Link the static library + macOS frameworks
    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ocr_swift");

    // Apple frameworks required for Vision OCR
    println!("cargo:rustc-link-lib=framework=Vision");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=ImageIO");

    // Swift runtime search paths
    let xcode_path = std::process::Command::new("xcode-select")
        .arg("--print-path")
        .output()
        .map(|o| String::from_utf8(o.stdout).unwrap().trim().to_string())
        .unwrap_or_else(|_| "/Applications/Xcode.app/Contents/Developer".to_string());

    println!(
        "cargo:rustc-link-search={}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx/",
        xcode_path
    );
    println!("cargo:rustc-link-search=/usr/lib/swift");
}
