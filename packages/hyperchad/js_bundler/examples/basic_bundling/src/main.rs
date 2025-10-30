#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic JavaScript bundling example using `hyperchad_js_bundler`.
//!
//! This example demonstrates how to use the `hyperchad_js_bundler` package to bundle
//! JavaScript files using either the SWC or esbuild bundler.

use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== HyperChad JavaScript Bundler Example ===\n");

    // Get the example directory path
    let example_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_dir = example_dir.join("js_source");
    let output_dir = example_dir.join("dist");

    // Create output directory if it doesn't exist
    fs::create_dir_all(&output_dir)?;

    // Input and output paths
    let input_file = source_dir.join("index.js");
    let output_file = output_dir.join("bundle.js");

    println!("Input file: {}", input_file.display());
    println!("Output file: {}", output_file.display());
    println!();

    // Verify input file exists
    if !input_file.exists() {
        eprintln!("Error: Input file does not exist: {}", input_file.display());
        return Err("Input file not found".into());
    }

    // Display source files
    println!("Source files to bundle:");
    if let Ok(entries) = fs::read_dir(&source_dir) {
        for entry in entries.flatten() {
            if let Some(ext) = entry.path().extension()
                && ext == "js"
            {
                println!("  - {}", entry.file_name().to_string_lossy());
            }
        }
    }
    println!();

    // Perform bundling
    println!("Starting bundling process...");
    bundle_javascript(&input_file, &output_file);
    println!();

    // Display results
    if output_file.exists() {
        let metadata = fs::metadata(&output_file)?;
        let size_kb = metadata.len() / 1024;
        println!("✓ Bundling successful!");
        println!("Output file size: {size_kb} KB");

        // Display first few lines of the output
        println!("\nFirst 5 lines of bundled output:");
        if let Ok(content) = fs::read_to_string(&output_file) {
            for (i, line) in content.lines().take(5).enumerate() {
                println!("  {}: {}", i + 1, line);
            }
            if content.lines().count() > 5 {
                println!("  ... ({} more lines)", content.lines().count() - 5);
            }
        }
    } else {
        eprintln!("✗ Bundling failed - output file not created");
        return Err("Bundling failed".into());
    }

    println!("\n=== Example Complete ===");
    Ok(())
}

/// Bundle JavaScript files using the configured bundler.
///
/// This function uses the unified `bundle` function from `hyperchad_js_bundler`,
/// which automatically selects between SWC and esbuild based on enabled features.
fn bundle_javascript(input: &Path, output: &Path) {
    println!("Using bundler: {}", get_bundler_name());

    // The unified bundle function handles both SWC and esbuild
    hyperchad_js_bundler::bundle(input, output);
}

/// Get the name of the bundler being used based on feature flags.
#[allow(unexpected_cfgs)]
const fn get_bundler_name() -> &'static str {
    if cfg!(feature = "swc") {
        "SWC (Rust-based bundler with minification)"
    } else if cfg!(feature = "esbuild") {
        "esbuild (Fast external bundler)"
    } else {
        "Unknown"
    }
}
