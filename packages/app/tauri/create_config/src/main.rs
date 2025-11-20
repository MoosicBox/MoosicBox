//! Command-line tool for generating `MoosicBox` app configuration files.
//!
//! This binary provides a CLI interface to generate TypeScript configuration
//! files that define build-time settings for the `MoosicBox` Tauri application.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clap::Parser;

/// Command-line arguments for the configuration generator.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Whether the build is bundled (affects the `bundled` field in the generated config).
    #[arg(long)]
    bundled: bool,

    /// The path where the TypeScript configuration file will be written.
    #[arg(short, long)]
    output: String,
}

/// Entry point for the configuration generator CLI.
///
/// Parses command-line arguments and generates a TypeScript configuration file
/// for the `MoosicBox` app with the specified settings.
///
/// # Panics
///
/// * If logging initialization fails
fn main() {
    moosicbox_logging::init(None, None).expect("Failed to initialize FreeLog");

    let args = Args::parse();

    moosicbox_app_create_config::generate(args.bundled, args.output);
}
