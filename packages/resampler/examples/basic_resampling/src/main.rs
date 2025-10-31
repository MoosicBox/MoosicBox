#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    clippy::multiple_crate_versions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

//! Basic audio resampling example demonstrating conversion between sample rates.
//!
//! This example shows how to:
//! - Create a resampler with specific input/output sample rates
//! - Generate synthetic audio data in an `AudioBuffer`
//! - Resample audio from 44.1kHz to 48kHz
//! - Handle the interleaved output
//! - Flush remaining samples at the end of a stream

use moosicbox_resampler::Resampler;
use symphonia::core::audio::{AudioBuffer, Channels, Signal, SignalSpec};
use symphonia::core::conv::IntoSample;

/// Sample rate of the input audio (CD quality)
const INPUT_SAMPLE_RATE: u32 = 44100;

/// Sample rate of the output audio (common for digital audio)
const OUTPUT_SAMPLE_RATE: usize = 48000;

/// Number of samples to process per chunk
const CHUNK_DURATION: u64 = 1024;

/// Number of chunks to process in this example
const NUM_CHUNKS: usize = 5;

fn main() {
    println!("=== MoosicBox Resampler: Basic Resampling Example ===\n");

    // Step 1: Create a signal specification for stereo audio at 44.1kHz
    let channels = Channels::FRONT_LEFT | Channels::FRONT_RIGHT;
    let input_spec = SignalSpec::new(INPUT_SAMPLE_RATE, channels);

    println!("Input Configuration:");
    println!("  Sample Rate: {INPUT_SAMPLE_RATE} Hz");
    println!("  Channels: {} (stereo)", input_spec.channels.count());
    println!("  Chunk Size: {CHUNK_DURATION} samples");
    println!();

    println!("Output Configuration:");
    println!("  Sample Rate: {OUTPUT_SAMPLE_RATE} Hz");
    println!();

    // Step 2: Create a resampler to convert from 44.1kHz to 48kHz
    let mut resampler: Resampler<f32> =
        Resampler::new(input_spec, OUTPUT_SAMPLE_RATE, CHUNK_DURATION);

    println!("Created resampler: {INPUT_SAMPLE_RATE}Hz -> {OUTPUT_SAMPLE_RATE}Hz");
    println!();

    // Step 3: Process multiple chunks of audio
    let mut total_input_samples = 0;
    let mut total_output_samples = 0;

    for chunk_num in 0..NUM_CHUNKS {
        // Generate a synthetic audio buffer with a simple sine wave
        let audio_buffer = generate_test_audio(input_spec, CHUNK_DURATION);
        total_input_samples += audio_buffer.frames() * input_spec.channels.count();

        println!("Processing chunk {}/{}:", chunk_num + 1, NUM_CHUNKS);
        println!("  Input frames: {}", audio_buffer.frames());

        // Step 4: Resample the audio buffer
        // The resampler returns None if it needs more samples before producing output
        if let Some(resampled_samples) = resampler.resample(&audio_buffer) {
            total_output_samples += resampled_samples.len();

            println!(
                "  Output samples (interleaved): {}",
                resampled_samples.len()
            );
            println!(
                "  Output frames: {}",
                resampled_samples.len() / input_spec.channels.count()
            );

            // Show sample values from the middle of the output
            let mid_idx = resampled_samples.len() / 2;
            println!(
                "  Sample values (mid): L={:.4}, R={:.4}",
                resampled_samples[mid_idx],
                resampled_samples[mid_idx + 1]
            );
        } else {
            println!("  (Buffering - need more samples before output)");
        }

        println!();
    }

    // Step 5: Flush any remaining samples from the resampler's internal buffer
    println!("Flushing remaining samples...");
    if let Some(final_samples) = resampler.flush() {
        total_output_samples += final_samples.len();
        println!("  Flushed {} final samples", final_samples.len());
    } else {
        println!("  No remaining samples to flush");
    }

    println!();
    println!("=== Summary ===");
    println!("Total input samples processed: {total_input_samples}");
    println!("Total output samples produced: {total_output_samples}");
    println!(
        "Conversion ratio: {:.4}",
        total_output_samples as f64 / total_input_samples as f64
    );
    println!(
        "Expected ratio: {:.4}",
        f64::from(OUTPUT_SAMPLE_RATE as u32) / f64::from(INPUT_SAMPLE_RATE)
    );
}

/// Generates a test audio buffer with a simple sine wave.
///
/// Creates a planar (non-interleaved) audio buffer filled with synthetic
/// audio data for demonstration purposes.
fn generate_test_audio(spec: SignalSpec, duration: u64) -> AudioBuffer<f32> {
    let frames = usize::try_from(duration).expect("Duration fits in usize");
    let mut buffer = AudioBuffer::new(duration, spec);

    // Render the buffer to allocate the internal storage
    buffer.render_reserved(Some(frames));

    // Fill the buffer with a simple sine wave (440 Hz = A4 note)
    let frequency = 440.0; // Hz
    let sample_rate = spec.rate as f32;

    for channel_idx in 0..spec.channels.count() {
        let channel = buffer.chan_mut(channel_idx);

        for (frame_idx, sample) in channel.iter_mut().enumerate() {
            // Generate a sine wave with slight phase offset per channel for stereo effect
            let phase_offset = channel_idx as f32 * 0.1;
            let time = frame_idx as f32 / sample_rate;
            let angle = (2.0 * std::f32::consts::PI * frequency).mul_add(time, phase_offset);

            // Amplitude of 0.3 to avoid clipping
            *sample = (0.3 * angle.sin()).into_sample();
        }
    }

    buffer
}
