#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic audio decoder example demonstrating core functionality.
//!
//! This example shows how to:
//! - Implement the `AudioDecode` trait
//! - Create and configure an `AudioDecodeHandler`
//! - Decode audio from a file
//! - Process decoded audio buffers

use moosicbox_audio_decoder::{
    AudioDecode, AudioDecodeError, AudioDecodeHandler, decode_file_path_str,
};
use symphonia::core::audio::{AudioBuffer, Signal};
use symphonia::core::formats::{Packet, Track};
use thiserror::Error;

/// Custom errors for our example
#[derive(Debug, Error)]
enum ExampleError {
    #[error("Decode error: {0}")]
    Decode(#[from] moosicbox_audio_decoder::DecodeError),
}

/// A simple audio decoder implementation that collects statistics about the decoded audio.
///
/// This demonstrates the minimum implementation needed for the `AudioDecode` trait.
struct SimpleAudioDecoder {
    /// Total number of packets decoded
    packet_count: usize,
    /// Total number of samples processed
    sample_count: usize,
    /// Sample rate of the audio
    sample_rate: u32,
    /// Number of channels
    channels: usize,
}

impl SimpleAudioDecoder {
    /// Creates a new `SimpleAudioDecoder` with the given audio specifications.
    fn new(sample_rate: u32, channels: usize) -> Self {
        println!("Initializing decoder:");
        println!("  Sample rate: {sample_rate} Hz");
        println!("  Channels: {channels}");
        println!();

        Self {
            packet_count: 0,
            sample_count: 0,
            sample_rate,
            channels,
        }
    }

    /// Prints a summary of the decoded audio.
    #[allow(dead_code, clippy::cast_precision_loss)]
    fn print_summary(&self) {
        println!("\nDecoding Complete!");
        println!("==================");
        println!("Packets decoded: {}", self.packet_count);
        println!("Total samples: {}", self.sample_count);
        println!(
            "Duration: {:.2} seconds",
            self.sample_count as f64 / f64::from(self.sample_rate) / self.channels as f64
        );
    }
}

impl AudioDecode for SimpleAudioDecoder {
    /// Called for each successfully decoded audio packet.
    ///
    /// This is where you would normally write audio to a device, file, or process it further.
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), AudioDecodeError> {
        // Get the number of frames (samples per channel) in this buffer
        let frames = decoded.frames();

        // Get the number of channels
        let channels = decoded.spec().channels.count();

        // Update statistics
        self.packet_count += 1;
        self.sample_count += frames * channels;

        // Print progress every 100 packets
        if self.packet_count.is_multiple_of(100) {
            let packet_count = self.packet_count;
            let sample_count = self.sample_count;
            println!("Decoded {packet_count} packets ({sample_count} samples)...");
        }

        // Example: Access the first channel's samples
        // In a real application, you would write these samples to an output device or file
        if channels > 0 {
            let channel_samples = decoded.chan(0);

            // Calculate peak amplitude for this packet (for demonstration)
            let peak = channel_samples
                .iter()
                .map(|&s| s.abs())
                .fold(0.0_f32, f32::max);

            // Log very loud samples (optional - for demonstration)
            if peak > 0.99 {
                println!("  Warning: High amplitude detected: {peak:.2}");
            }
        }

        Ok(())
    }

    /// Called at the end of decoding to flush any buffered data.
    ///
    /// This is important for real-time audio output to ensure all audio is played.
    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        println!("Flushing decoder...");
        Ok(())
    }
}

fn main() -> Result<(), ExampleError> {
    // Initialize logging (optional but helpful)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("MoosicBox Audio Decoder - Basic Example");
    println!("========================================\n");

    // Get the audio file path from command line arguments
    let args: Vec<String> = std::env::args().collect();
    let file_path = if args.len() > 1 {
        &args[1]
    } else {
        println!("Usage: {} <audio_file>", args[0]);
        println!("\nExample:");
        println!("  {} path/to/audio.flac", args[0]);
        println!("\nSupported formats: MP3, FLAC, AAC, Opus, WAV, Vorbis, and more");
        println!("(Format support depends on enabled features)");
        std::process::exit(1);
    };

    println!("Decoding file: {file_path}\n");

    // Create an audio decode handler with our custom decoder
    let mut handler = AudioDecodeHandler::new().with_output(Box::new(|spec, _duration| {
        // This closure is called once the audio format is determined
        // Here we create our decoder with the actual audio specifications
        let decoder = SimpleAudioDecoder::new(spec.rate, spec.channels.count());
        Ok(Box::new(decoder))
    }));

    // Decode the audio file
    // Parameters:
    //   - file_path: Path to the audio file
    //   - handler: Our decode handler with the custom decoder
    //   - enable_gapless: Enable gapless playback (true for accurate timing)
    //   - verify: Enable decoder verification (false for speed, true for debugging)
    //   - track_num: Select specific track (None = first track)
    //   - seek: Seek to position in seconds (None = start from beginning)
    let result = decode_file_path_str(
        file_path,
        &mut handler,
        true,  // enable_gapless
        false, // verify
        None,  // track_num
        None,  // seek
    )?;

    println!("\nDecoding result code: {result}");

    // Note: We can't access the decoder here to print the summary because
    // it's owned by the handler. In a real application, you would need to
    // structure this differently (e.g., using Arc<Mutex<_>>) if you need
    // to access the decoder after decoding completes.

    Ok(())
}
