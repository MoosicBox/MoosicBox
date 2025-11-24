#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::cast_precision_loss)]

//! Basic audio encoding example demonstrating all supported formats.
//!
//! This example shows how to:
//! - Create encoders for AAC, FLAC, MP3, and Opus formats
//! - Encode PCM audio data with each encoder
//! - Handle the different input types required by each format
//! - Work with the `EncodeInfo` result structure

/// Demonstrates AAC encoding with i16 PCM samples
#[cfg(feature = "aac")]
fn encode_aac_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_audio_encoder::aac::{encode_aac, encoder_aac};

    println!("\n=== AAC Encoding ===");

    // Create AAC encoder (44.1kHz stereo, ADTS format)
    let encoder = encoder_aac()?;

    // Generate sample PCM data (i16 samples)
    // In a real application, this would be actual audio data
    let input_samples: Vec<i16> = vec![0; 2048]; // 2048 stereo samples

    // Prepare output buffer
    let mut output_buffer = vec![0u8; 8192];

    // Encode the samples
    let encode_info = encode_aac(&encoder, &input_samples, &mut output_buffer)?;

    // Display encoding results
    println!("  Input samples: {}", input_samples.len());
    println!("  Samples consumed: {}", encode_info.input_consumed);
    println!("  Bytes encoded: {}", encode_info.output_size);
    println!(
        "  Compression ratio: {:.2}x",
        (input_samples.len() * 2) as f64 / encode_info.output_size as f64
    );

    Ok(())
}

/// Demonstrates FLAC encoding with i32 PCM samples
#[cfg(feature = "flac")]
fn encode_flac_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_audio_encoder::flac::{encode_flac, encoder_flac};

    println!("\n=== FLAC Encoding ===");

    // Create FLAC encoder (block size 512)
    let mut encoder = encoder_flac()?;

    // Generate sample PCM data (i32 samples)
    // FLAC uses i32 for higher bit depth support
    let input_samples: Vec<i32> = vec![0; 1024]; // 1024 stereo samples

    // Prepare output buffer
    let mut output_buffer = vec![0u8; 8192];

    // Encode the samples
    let encode_info = encode_flac(&mut encoder, &input_samples, &mut output_buffer)?;

    // Display encoding results
    println!("  Input samples: {}", input_samples.len());
    println!("  Samples consumed: {}", encode_info.input_consumed);
    println!("  Bytes encoded: {}", encode_info.output_size);
    println!("  Note: FLAC is lossless compression");

    Ok(())
}

/// Demonstrates MP3 encoding with i16 PCM samples
#[cfg(feature = "mp3")]
fn encode_mp3_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_audio_encoder::mp3::{encode_mp3, encoder_mp3};

    println!("\n=== MP3 Encoding ===");

    // Create MP3 encoder (320kbps, 44.1kHz stereo)
    let mut encoder = encoder_mp3()?;

    // Generate sample PCM data (i16 samples)
    let input_samples: Vec<i16> = vec![0; 2304]; // 2304 stereo samples (1152 per channel)

    // Encode the samples (MP3 encoder returns both output buffer and info)
    let (output_buffer, encode_info) = encode_mp3(&mut encoder, &input_samples)?;

    // Display encoding results
    println!("  Input samples: {}", input_samples.len());
    println!("  Samples consumed: {}", encode_info.input_consumed);
    println!("  Bytes encoded: {}", output_buffer.len());
    println!(
        "  Compression ratio: {:.2}x",
        (input_samples.len() * 2) as f64 / output_buffer.len() as f64
    );

    Ok(())
}

/// Demonstrates Opus encoding with f32 PCM samples
#[cfg(feature = "opus")]
fn encode_opus_example() -> Result<(), Box<dyn std::error::Error>> {
    use moosicbox_audio_encoder::opus::{encode_opus_float, encoder_opus};

    println!("\n=== Opus Encoding ===");

    // Create Opus encoder (48kHz stereo)
    let mut encoder = encoder_opus()?;

    // Generate sample PCM data (f32 samples)
    // Opus uses floating-point samples in the range [-1.0, 1.0]
    let input_samples: Vec<f32> = vec![0.0; 1920]; // 1920 samples (20ms at 48kHz stereo)

    // Prepare output buffer
    let mut output_buffer = vec![0u8; 4000];

    // Encode the samples
    let encode_info = encode_opus_float(&mut encoder, &input_samples, &mut output_buffer)?;

    // Display encoding results
    println!("  Input samples: {}", input_samples.len());
    println!("  Samples consumed: {}", encode_info.input_consumed);
    println!("  Bytes encoded: {}", encode_info.output_size);
    println!(
        "  Compression ratio: {:.2}x",
        (input_samples.len() * 4) as f64 / encode_info.output_size as f64
    );

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("MoosicBox Audio Encoder - Basic Encoding Example");
    println!("=================================================");

    // Demonstrate each encoding format that's enabled via features
    #[cfg(feature = "aac")]
    encode_aac_example()?;

    #[cfg(feature = "flac")]
    encode_flac_example()?;

    #[cfg(feature = "mp3")]
    encode_mp3_example()?;

    #[cfg(feature = "opus")]
    encode_opus_example()?;

    println!("\n=== Summary ===");
    println!("All enabled encoders demonstrated successfully!");
    println!("\nKey differences:");
    println!("  - AAC/MP3: Use i16 PCM samples");
    println!("  - FLAC: Uses i32 PCM samples (lossless)");
    println!("  - Opus: Uses f32 PCM samples in range [-1.0, 1.0]");
    println!("  - MP3: Returns owned output buffer");
    println!("  - AAC/FLAC/Opus: Write to provided buffer");

    Ok(())
}
