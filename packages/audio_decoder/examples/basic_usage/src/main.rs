#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic audio decoder example
//!
//! This example demonstrates how to decode an audio file and process the decoded samples.

use std::io::Write;

use moosicbox_audio_decoder::{
    AudioDecode, AudioDecodeError, AudioDecodeHandler, decode_file_path_str,
};
use symphonia::core::audio::{AudioBuffer, Signal};
use symphonia::core::formats::{Packet, Track};

/// A simple audio output that counts samples and prints information
struct SampleCounter {
    sample_count: usize,
    frame_count: usize,
    channels: usize,
    sample_rate: u32,
}

impl SampleCounter {
    const fn new(channels: usize, sample_rate: u32) -> Self {
        Self {
            sample_count: 0,
            frame_count: 0,
            channels,
            sample_rate,
        }
    }
}

impl AudioDecode for SampleCounter {
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        _packet: &Packet,
        _track: &Track,
    ) -> Result<(), AudioDecodeError> {
        // Count the number of frames (samples per channel)
        let frames = decoded.frames();
        self.frame_count += frames;
        self.sample_count += frames * self.channels;

        // Print progress every 1 second of audio
        let samples_per_second = self.sample_rate as usize * self.channels;
        if self.sample_count % samples_per_second < frames * self.channels {
            print!(".");
            std::io::stdout().flush().ok();
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        println!();
        println!("Decoding complete!");
        println!("  Total frames: {}", self.frame_count);
        println!("  Total samples: {}", self.sample_count);
        #[allow(clippy::cast_precision_loss)]
        let duration = self.frame_count as f64 / f64::from(self.sample_rate);
        println!("  Duration: {duration:.2} seconds");
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the audio file path from command line arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <audio_file_path>", args[0]);
        eprintln!();
        eprintln!("Example:");
        eprintln!("  {} /path/to/audio.flac", args[0]);
        eprintln!();
        eprintln!("Supported formats: FLAC, MP3, AAC, Opus, WAV, and more");
        eprintln!("(Format support depends on enabled cargo features)");
        std::process::exit(1);
    }

    let file_path = &args[1];

    println!("Decoding audio file: {file_path}");
    println!();

    // Create an audio decode handler that will process the decoded audio
    let mut handler = AudioDecodeHandler::new();

    // Add an output handler that creates our SampleCounter when the audio format is known
    handler = handler.with_output(Box::new(|spec, _duration| {
        // The spec contains information about the audio format
        let channels = spec.channels.count();
        let sample_rate = spec.rate;

        println!("Audio format detected:");
        println!("  Sample rate: {sample_rate} Hz");
        println!("  Channels: {channels}");
        println!("  Channel layout: {:?}", spec.channels);
        println!();
        println!("Decoding (. = 1 second of audio):");

        // Create and return our sample counter
        Ok(Box::new(SampleCounter::new(channels, sample_rate)))
    }));

    // Decode the file
    // Parameters:
    //   - file_path: path to the audio file
    //   - handler: our audio decode handler
    //   - enable_gapless: true for gapless playback support
    //   - verify: false to skip verification (faster)
    //   - track_num: None to select the first audio track
    //   - seek: None to start from the beginning
    decode_file_path_str(
        file_path,
        &mut handler,
        true,  // enable_gapless
        false, // verify
        None,  // track_num
        None,  // seek
    )?;

    Ok(())
}
