#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic Opus Decoder Usage Example
//!
//! This example demonstrates the core functionality of the `moosicbox_opus_native` decoder:
//! - Creating a decoder instance
//! - Decoding CELT-only packets to PCM audio
//! - Decoding SILK-only packets to PCM audio
//! - Handling different sample rates and channels
//! - Working with the decoder output

use moosicbox_opus_native::{Channels, Decoder, SampleRate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Opus Native - Basic Usage Example ===\n");

    // Example 1: CELT-only decoding (48kHz stereo)
    // CELT is optimized for music and full-bandwidth audio
    println!("Example 1: CELT-only decoding (48kHz stereo)");
    println!("---------------------------------------------------");

    #[cfg(feature = "celt")]
    {
        // Create a decoder for 48kHz stereo output
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo)?;

        // Simulate a CELT packet
        // TOC byte 0x7C = CELT-only mode, 48kHz, 10ms frame, stereo
        // In a real application, this would come from an Opus stream
        let celt_packet = vec![0x7C; 100];

        // Allocate output buffer
        // 10ms @ 48kHz = 480 samples per channel
        // Stereo = 960 total samples (interleaved L/R)
        let mut output = vec![0i16; 480 * 2];

        // Decode the packet
        let samples = decoder.decode(Some(&celt_packet), &mut output, false)?;

        println!("✓ Decoded {samples} samples per channel");
        println!(
            "  Buffer size: {} samples ({} L/R pairs)",
            output.len(),
            output.len() / 2
        );
        println!("  Sample range: {} to {}", i16::MIN, i16::MAX);
        println!(
            "  First 10 sample pairs: {:?}\n",
            &output[..20.min(output.len())]
        );
    }

    #[cfg(not(feature = "celt"))]
    {
        println!("⚠ CELT feature not enabled - skipping CELT example\n");
    }

    // Example 2: SILK-only decoding (16kHz mono)
    // SILK is optimized for speech and lower bandwidths
    println!("Example 2: SILK-only decoding (16kHz mono)");
    println!("---------------------------------------------------");

    #[cfg(feature = "silk")]
    {
        // Create a decoder for 16kHz mono output
        let mut decoder = Decoder::new(SampleRate::Hz16000, Channels::Mono)?;

        // Simulate a SILK packet
        // TOC byte 0x44 = SILK-only mode, 16kHz (Wideband), 20ms frame, mono
        let silk_packet = vec![0x44; 100];

        // Allocate output buffer
        // 20ms @ 16kHz = 320 samples per channel
        // Mono = 320 total samples
        let mut output = vec![0i16; 320];

        // Decode the packet
        let samples = decoder.decode(Some(&silk_packet), &mut output, false)?;

        println!("✓ Decoded {samples} samples per channel");
        println!("  Buffer size: {} samples", output.len());
        println!(
            "  First 10 samples: {:?}\n",
            &output[..10.min(output.len())]
        );
    }

    #[cfg(not(feature = "silk"))]
    {
        println!("⚠ SILK feature not enabled - skipping SILK example\n");
    }

    // Example 3: Handling packet loss with Packet Loss Concealment (PLC)
    println!("Example 3: Packet Loss Concealment");
    println!("---------------------------------------------------");

    #[cfg(feature = "celt")]
    {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono)?;
        let mut output = vec![0i16; 480];

        // Decode with None to simulate packet loss
        // The decoder will generate concealment audio (currently silence)
        let samples = decoder.decode(None, &mut output, false)?;

        println!("✓ Generated {samples} concealment samples");
        println!("  (PLC fills in missing audio during packet loss)\n");
    }

    #[cfg(not(feature = "celt"))]
    {
        println!("⚠ CELT feature not enabled - skipping PLC example\n");
    }

    // Example 4: Understanding sample rates and frame sizes
    println!("Example 4: Supported Sample Rates");
    println!("---------------------------------------------------");
    println!("The Opus codec supports the following output sample rates:");
    println!("  • 8kHz  (Narrowband)     - Phone quality");
    println!("  • 12kHz (Mediumband)     - Enhanced speech");
    println!("  • 16kHz (Wideband)       - High-quality speech");
    println!("  • 24kHz (Super-wideband) - High-fidelity speech");
    println!("  • 48kHz (Fullband)       - Music quality\n");

    println!("Frame sizes can be 2.5ms, 5ms, 10ms, 20ms, 40ms, or 60ms.");
    println!("Most common frame size is 20ms for speech, 10ms for music.\n");

    // Example 5: Feature-specific information
    println!("Example 5: Decoder Capabilities");
    println!("---------------------------------------------------");

    #[cfg(feature = "silk")]
    println!("✓ SILK decoder enabled  - Speech/narrowband");

    #[cfg(not(feature = "silk"))]
    println!("✗ SILK decoder disabled");

    #[cfg(feature = "celt")]
    println!("✓ CELT decoder enabled  - Music/fullband");

    #[cfg(not(feature = "celt"))]
    println!("✗ CELT decoder disabled");

    #[cfg(all(feature = "silk", feature = "celt"))]
    println!("✓ Hybrid mode enabled   - SILK+CELT combined");

    #[cfg(feature = "resampling")]
    println!("✓ Resampling enabled    - Automatic rate conversion");

    #[cfg(not(feature = "resampling"))]
    println!("✗ Resampling disabled   - Output rate must match internal rate");

    println!("\n=== Example Complete ===");
    println!("\nKey Takeaways:");
    println!("1. Create a decoder with Decoder::new(sample_rate, channels)");
    println!("2. Allocate output buffer with correct size for frame duration");
    println!("3. Call decode(Some(&packet), &mut output, false) to decode");
    println!("4. Use decode(None, ...) to handle packet loss with PLC");
    println!("5. Enable features (silk, celt, hybrid, resampling) as needed");

    Ok(())
}
