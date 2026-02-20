//! Build script for ocr-bench.
//!
//! Three-step process:
//! 1. swift-bridge-build generates FFI glue (Rust + Swift + C headers)
//! 2. swiftc compiles the Swift source + generated glue into a static library
//! 3. Cargo links the static library + macOS frameworks

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let swift_src_dir = manifest_dir.join("swift-src");
    let generated_dir = swift_src_dir.join("generated");

    // Rerun if Swift source or bridge declarations change
    println!("cargo:rerun-if-changed=src/main.rs");
    println!("cargo:rerun-if-changed=swift-src/ocr_bridge.swift");

    // Step 1: Generate FFI glue from #[swift_bridge::bridge] modules
    swift_bridge_build::parse_bridges(vec!["src/main.rs"])
        .write_all_concatenated(&generated_dir, env!("CARGO_PKG_NAME"));

    // Step 2: Compile Swift → static library using swiftc
    let lib_output = swift_src_dir.join("libocr_swift.a");

    let status = std::process::Command::new("swiftc")
        .args(["-emit-library", "-static"])
        .args(["-module-name", "ocr_swift"])
        .arg("-import-objc-header")
        .arg(swift_src_dir.join("bridging-header.h"))
        .arg(swift_src_dir.join("ocr_bridge.swift"))
        .arg(generated_dir.join("SwiftBridgeCore.swift"))
        .arg(generated_dir.join("ocr-bench/ocr-bench.swift"))
        .arg("-o")
        .arg(&lib_output)
        .arg("-O") // Optimized build
        .status()
        .expect("Failed to run swiftc — is Xcode Command Line Tools installed?");

    if !status.success() {
        panic!("swiftc compilation failed");
    }

    // Step 3: Link the static library + macOS frameworks
    println!(
        "cargo:rustc-link-search={}",
        swift_src_dir.display()
    );
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
