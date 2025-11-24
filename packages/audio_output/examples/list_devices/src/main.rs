#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating audio device enumeration using `moosicbox_audio_output`.
//!
//! This example scans for available audio output devices and displays their
//! specifications including sample rate and channel configuration.

use moosicbox_audio_output::{default_output_factory, output_factories, scan_outputs};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see debug information
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("MoosicBox Audio Output - Device Enumeration Example");
    println!("====================================================\n");

    // Scan for available audio output devices
    println!("Scanning for audio output devices...");
    scan_outputs().await?;
    println!("Scan complete!\n");

    // Get all available audio output factories
    let factories = output_factories().await;

    if factories.is_empty() {
        println!("No audio output devices found.");
        return Ok(());
    }

    println!("Found {} audio output device(s):\n", factories.len());

    // Display information about each device
    for (index, factory) in factories.iter().enumerate() {
        println!("Device {}: {}", index + 1, factory.name);
        println!("  ID: {}", factory.id);
        println!("  Sample Rate: {} Hz", factory.spec.rate);
        println!("  Channels: {}", factory.spec.channels.count());
        println!("  Channel Layout: {:?}", factory.spec.channels);
        println!();
    }

    // Get and display the default audio output device
    if let Some(default_factory) = default_output_factory().await {
        println!("Default Audio Output Device:");
        println!("  Name: {}", default_factory.name);
        println!("  ID: {}", default_factory.id);
        println!("  Sample Rate: {} Hz", default_factory.spec.rate);
        println!("  Channels: {}", default_factory.spec.channels.count());
        println!("  Channel Layout: {:?}", default_factory.spec.channels);
    } else {
        println!("No default audio output device found.");
    }

    println!("\nExample completed successfully!");

    Ok(())
}
