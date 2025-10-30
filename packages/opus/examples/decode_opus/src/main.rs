#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::cast_precision_loss)]

//! This example demonstrates how to decode Opus audio files using the `moosicbox_opus` codec
//! with the Symphonia framework. It shows packet parsing, codec registration, and basic
//! decoding workflow.

use std::{env, fs::File, io::Write, path::Path};

use symphonia::core::{
    audio::{AudioBufferRef, Signal},
    codecs::{CODEC_TYPE_OPUS, DecoderOptions},
    formats::{FormatOptions, FormatReader},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <opus_file.opus>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} audio.opus", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];

    // Verify file exists
    if !Path::new(file_path).exists() {
        eprintln!("Error: File not found: {file_path}");
        std::process::exit(1);
    }

    println!("Decoding Opus file: {file_path}\n");

    // Create a custom codec registry with Opus support
    let codec_registry = moosicbox_opus::create_opus_registry();

    // Open the media file
    let file = Box::new(File::open(file_path)?);
    let mss = MediaSourceStream::new(file, MediaSourceStreamOptions::default());

    // Create a hint to help the format registry guess the file type
    let mut hint = Hint::new();
    hint.with_extension("opus");

    // Probe the media source stream for a format
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let probed =
        symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts)?;

    let mut format: Box<dyn FormatReader> = probed.format;

    // Find the first audio track with Opus codec
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec == CODEC_TYPE_OPUS)
        .ok_or("No Opus audio track found")?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params.sample_rate;

    println!("Track information:");
    println!("  Codec: Opus");
    if let Some(rate) = sample_rate {
        println!("  Sample rate: {rate} Hz");
    }
    if let Some(channels) = codec_params.channels {
        let channel_count = channels.count();
        println!("  Channels: {channel_count}");
    }
    if let Some(n_frames) = codec_params.n_frames
        && let Some(rate) = sample_rate
    {
        let duration_secs = n_frames as f64 / f64::from(rate);
        println!("  Duration: {duration_secs:.2} seconds");
    }
    println!();

    // Create the Opus decoder
    let decoder_opts = DecoderOptions::default();
    let mut decoder = codec_registry.make(&codec_params, &decoder_opts)?;

    println!("Decoding packets (. = 100 packets):");
    let mut packet_count = 0u64;
    let mut frame_count = 0u64;
    let mut sample_count = 0u64;
    let mut error_count = 0u64;

    // Decode all packets in the track
    loop {
        // Get the next packet from the format reader
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                // End of stream
                break;
            }
            Err(e) => return Err(e.into()),
        };

        // Skip packets not from our target track
        if packet.track_id() != track_id {
            continue;
        }

        packet_count += 1;

        // Print progress indicator
        if packet_count.is_multiple_of(100) {
            print!(".");
            std::io::stdout().flush()?;
        }

        // Decode the packet
        #[allow(clippy::similar_names)]
        match decoder.decode(&packet) {
            Ok(decoded_audio) => {
                // Process the decoded audio
                let frames = decoded_audio.frames();
                frame_count += frames as u64;

                // Count total samples across all channels
                let channels = decoded_audio.spec().channels.count();
                sample_count += (frames * channels) as u64;
            }
            Err(e) => {
                error_count += 1;
                eprintln!("\nWarning: Failed to decode packet {packet_count}: {e}");
            }
        }
    }

    println!("\n\nDecoding complete!");
    println!("  Total packets: {packet_count}");
    println!("  Total frames decoded: {frame_count}");
    println!("  Total samples: {sample_count}");

    if error_count > 0 {
        println!("  Decode errors: {error_count}");
    }

    // Calculate duration from decoded frames
    if let Some(rate) = sample_rate {
        let duration_secs = frame_count as f64 / f64::from(rate);
        println!("  Actual duration: {duration_secs:.2} seconds");
    }

    Ok(())
}

// Helper function to print audio buffer information (for educational purposes)
#[allow(dead_code)]
fn print_audio_buffer_info(decoded: &AudioBufferRef<'_>) {
    let spec = decoded.spec();
    let frames = decoded.frames();

    let channel_count = spec.channels.count();
    let rate = spec.rate;
    println!("Decoded buffer:");
    println!("  Frames: {frames}");
    println!("  Channels: {channel_count}");
    println!("  Sample rate: {rate} Hz");

    // Print sample values for first few frames (if any)
    if frames > 0 {
        println!("  First frame samples:");
        for ch in 0..spec.channels.count() {
            match decoded {
                AudioBufferRef::F32(buf) => {
                    let channel_data = buf.chan(ch);
                    if !channel_data.is_empty() {
                        let sample = channel_data[0];
                        println!("    Channel {ch}: {sample:.6}");
                    }
                }
                AudioBufferRef::F64(buf) => {
                    let channel_data = buf.chan(ch);
                    if !channel_data.is_empty() {
                        let sample = channel_data[0];
                        println!("    Channel {ch}: {sample:.6}");
                    }
                }
                _ => {
                    println!("    Channel {ch}: (unsupported format)");
                }
            }
        }
    }
}
