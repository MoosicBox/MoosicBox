#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    clippy::multiple_crate_versions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

//! Basic audio encoding example demonstrating AAC, FLAC, MP3, and Opus encoding.
//!
//! This example shows how to:
//! - Generate test PCM audio data
//! - Create encoders for different audio formats
//! - Encode audio samples
//! - Handle encoding results and errors

use moosicbox_audio_encoder::EncodeInfo;

/// Generates a simple sine wave test signal as PCM samples.
///
/// Creates a 440 Hz tone (A4 note) for demonstration purposes.
fn generate_test_pcm_i16(sample_count: usize) -> Vec<i16> {
    let sample_rate = 44100.0;
    let frequency = 440.0; // A4 note

    (0..sample_count)
        .map(|i| {
            let t = i as f64 / sample_rate;
            let sample = (2.0 * std::f64::consts::PI * frequency * t).sin();
            (sample * f64::from(i16::MAX) * 0.5) as i16
        })
        .collect()
}

/// Generates test PCM samples as 32-bit integers for FLAC encoding.
fn generate_test_pcm_i32(sample_count: usize) -> Vec<i32> {
    let sample_rate = 44100.0;
    let frequency = 440.0;

    (0..sample_count)
        .map(|i| {
            let t = i as f64 / sample_rate;
            let sample = (2.0 * std::f64::consts::PI * frequency * t).sin();
            (sample * f64::from(i16::MAX) * 0.5) as i32
        })
        .collect()
}

/// Generates test PCM samples as floating-point values for Opus encoding.
fn generate_test_pcm_f32(sample_count: usize) -> Vec<f32> {
    let sample_rate = 44100.0;
    let frequency = 440.0;

    (0..sample_count)
        .map(|i| {
            let t = i as f32 / sample_rate;
            (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5
        })
        .collect()
}

/// Demonstrates AAC encoding with the fdk-aac encoder.
#[cfg(feature = "aac")]
fn encode_aac_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_audio_encoder::aac::{encode_aac, encoder_aac};

    println!("\n--- AAC Encoding Example ---");

    // Create the AAC encoder (44.1kHz stereo, ADTS format)
    let encoder = encoder_aac()?;
    println!("✓ Created AAC encoder (44.1kHz stereo, VBR Very High, ADTS format)");

    // Generate test PCM data (2048 samples = 1024 samples per channel for stereo)
    let input = generate_test_pcm_i16(2048);
    println!("✓ Generated {} PCM samples (i16)", input.len());

    // Prepare output buffer (typically needs ~2x input size for AAC)
    let mut output = vec![0u8; 8192];

    // Encode the audio
    let info: EncodeInfo = encode_aac(&encoder, &input, &mut output)?;

    println!("✓ Encoded successfully:");
    println!("  - Input samples consumed: {}", info.input_consumed);
    println!("  - Output bytes produced: {}", info.output_size);
    println!(
        "  - Compression ratio: {:.2}x",
        (info.input_consumed * 2) as f64 / info.output_size as f64
    );

    Ok(())
}

/// Demonstrates FLAC encoding with lossless compression.
#[cfg(feature = "flac")]
fn encode_flac_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_audio_encoder::flac::{encode_flac, encoder_flac};

    println!("\n--- FLAC Encoding Example ---");

    // Create the FLAC encoder (block size: 512)
    let mut encoder = encoder_flac()?;
    println!("✓ Created FLAC encoder (block size: 512)");

    // Generate test PCM data (1024 i32 samples)
    let input = generate_test_pcm_i32(1024);
    println!("✓ Generated {} PCM samples (i32)", input.len());

    // Prepare output buffer
    let mut output = vec![0u8; 8192];

    // Encode the audio
    let info = encode_flac(&mut encoder, &input, &mut output)?;

    println!("✓ Encoded successfully:");
    println!("  - Input samples consumed: {}", info.input_consumed);
    println!("  - Output bytes produced: {}", info.output_size);
    println!("  - Note: FLAC is lossless compression");

    Ok(())
}

/// Demonstrates MP3 encoding with LAME encoder.
#[cfg(feature = "mp3")]
fn encode_mp3_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_audio_encoder::mp3::{encode_mp3, encoder_mp3};

    println!("\n--- MP3 Encoding Example ---");

    // Create the MP3 encoder (320kbps, 44.1kHz stereo, best quality)
    let mut encoder = encoder_mp3()?;
    println!("✓ Created MP3 encoder (320kbps, 44.1kHz stereo, best quality)");

    // Generate test PCM data (2048 samples)
    let input = generate_test_pcm_i16(2048);
    println!("✓ Generated {} PCM samples (i16)", input.len());

    // Encode the audio (MP3 encoder returns the output buffer)
    let (output, info) = encode_mp3(&mut encoder, &input)?;

    println!("✓ Encoded successfully:");
    println!("  - Input samples consumed: {}", info.input_consumed);
    println!("  - Output bytes produced: {}", info.output_size);
    println!("  - Output buffer length: {}", output.len());
    println!(
        "  - Compression ratio: {:.2}x",
        (info.input_consumed * 2) as f64 / info.output_size as f64
    );

    Ok(())
}

/// Demonstrates Opus encoding with float samples.
#[cfg(feature = "opus")]
fn encode_opus_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_audio_encoder::opus::{encode_opus_float, encoder_opus};

    println!("\n--- Opus Encoding Example ---");

    // Create the Opus encoder (48kHz stereo)
    let mut encoder = encoder_opus()?;
    println!("✓ Created Opus encoder (48kHz stereo)");

    // Generate test PCM data (1920 samples = 20ms at 48kHz stereo)
    // Opus frame sizes must be: 2.5, 5, 10, 20, 40, or 60 ms
    let input = generate_test_pcm_f32(1920);
    println!("✓ Generated {} PCM samples (f32, 20ms frame)", input.len());

    // Prepare output buffer
    let mut output = vec![0u8; 4096];

    // Encode the audio
    let info = encode_opus_float(&mut encoder, &input, &mut output)?;

    println!("✓ Encoded successfully:");
    println!("  - Input samples consumed: {}", info.input_consumed);
    println!("  - Output bytes produced: {}", info.output_size);
    println!(
        "  - Compression ratio: {:.2}x",
        (info.input_consumed * 4) as f64 / info.output_size as f64
    );

    Ok(())
}

fn main() {
    println!("=== MoosicBox Audio Encoder - Basic Encoding Examples ===\n");
    println!("This example demonstrates encoding PCM audio to various formats.");

    // Run each encoder example if the feature is enabled
    #[cfg(feature = "aac")]
    if let Err(e) = encode_aac_example() {
        eprintln!("AAC encoding failed: {e}");
    }

    #[cfg(feature = "flac")]
    if let Err(e) = encode_flac_example() {
        eprintln!("FLAC encoding failed: {e}");
    }

    #[cfg(feature = "mp3")]
    if let Err(e) = encode_mp3_example() {
        eprintln!("MP3 encoding failed: {e}");
    }

    #[cfg(feature = "opus")]
    if let Err(e) = encode_opus_example() {
        eprintln!("Opus encoding failed: {e}");
    }

    // Show a message if no features are enabled
    #[cfg(not(any(feature = "aac", feature = "flac", feature = "mp3", feature = "opus")))]
    {
        eprintln!("Error: No encoding features enabled!");
        eprintln!("Enable at least one feature: aac, flac, mp3, or opus");
        std::process::exit(1);
    }

    println!("\n=== All Examples Completed Successfully ===");
}
