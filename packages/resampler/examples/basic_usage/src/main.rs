#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic resampler example
//!
//! This example demonstrates how to use the resampler to convert audio between different sample rates.

use moosicbox_resampler::Resampler;
use symphonia::core::audio::{AudioBuffer, Layout, Signal, SignalSpec};

fn main() {
    println!("=== MoosicBox Resampler Example ===");
    println!();

    // Define audio parameters
    let input_sample_rate: u32 = 44100; // CD quality
    let output_sample_rate: usize = 48000; // Professional audio standard
    let num_channels = 2; // Stereo
    let chunk_size = 2048; // Processing chunk size in frames

    println!("Configuration:");
    println!("  Input sample rate:  {input_sample_rate} Hz");
    println!("  Output sample rate: {output_sample_rate} Hz");
    println!("  Channels:           {num_channels}");
    println!("  Chunk size:         {chunk_size} frames");
    println!();

    // Create a signal specification for the input audio
    let spec = SignalSpec::new(input_sample_rate, Layout::Stereo.into_channels());

    // Create a resampler that converts from 44.1kHz to 48kHz
    let mut resampler: Resampler<f32> = Resampler::new(spec, output_sample_rate, chunk_size);

    #[allow(clippy::integer_division)]
    let input_khz = input_sample_rate / 1000;
    #[allow(clippy::integer_division)]
    let output_khz = output_sample_rate / 1000;
    println!("Created resampler: {input_khz}kHz → {output_khz}kHz");
    println!();

    // Generate some test audio data (a simple sine wave)
    println!("Generating test audio (440 Hz sine wave)...");
    let test_duration_seconds = 2.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total_input_frames = (f64::from(input_sample_rate) * test_duration_seconds) as u64;

    println!("  Duration: {test_duration_seconds:.1} seconds");
    println!("  Total input frames: {total_input_frames}");
    println!();

    // Process audio in chunks
    let mut total_output_samples = 0;
    let mut frames_processed = 0;

    println!("Resampling audio...");

    while frames_processed < total_input_frames {
        // Determine chunk size for this iteration
        let remaining_frames = total_input_frames - frames_processed;
        let current_chunk_size = remaining_frames.min(chunk_size);

        // Create an input buffer with generated audio
        let input_buffer = generate_sine_wave(
            spec,
            current_chunk_size,
            frames_processed,
            f64::from(input_sample_rate),
        );

        // Resample the audio chunk
        if let Some(output_samples) = resampler.resample(&input_buffer) {
            total_output_samples += output_samples.len();

            // Print progress every 0.5 seconds of input
            if frames_processed % (u64::from(input_sample_rate) / 2) < chunk_size {
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
        }

        frames_processed += current_chunk_size;
    }

    // Flush any remaining samples from the resampler buffer
    if let Some(final_samples) = resampler.flush() {
        total_output_samples += final_samples.len();
    }

    println!();
    println!();

    // Calculate and display results
    let output_frames = total_output_samples / num_channels;
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let expected_output_frames = (total_input_frames as f64 * output_sample_rate as f64
        / f64::from(input_sample_rate)) as usize;

    #[allow(clippy::cast_precision_loss)]
    let input_duration = total_input_frames as f64 / f64::from(input_sample_rate);
    #[allow(clippy::cast_precision_loss)]
    let output_duration = output_frames as f64 / output_sample_rate as f64;

    println!("Results:");
    println!("  Input frames:            {total_input_frames}");
    println!("  Input duration:          {input_duration:.3} seconds");
    println!("  Output frames:           {output_frames}");
    println!("  Output duration:         {output_duration:.3} seconds");
    println!("  Expected output frames:  {expected_output_frames}");
    #[allow(clippy::cast_possible_wrap)]
    let diff = (output_frames as i64 - expected_output_frames as i64).abs();
    println!("  Difference:              {diff} frames");
    println!();

    // Verify the resampling preserved the audio duration
    let duration_difference = (output_duration - input_duration).abs();
    if duration_difference < 0.01 {
        println!("✓ Success! Audio duration preserved through resampling.");
    } else {
        println!("⚠ Warning: Duration mismatch of {duration_difference:.3} seconds");
    }

    println!();
    println!("Resampling complete!");
}

/// Generates a sine wave audio buffer for testing.
///
/// Creates a stereo sine wave at 440 Hz (A4 note) with the specified parameters.
fn generate_sine_wave(
    spec: SignalSpec,
    duration: u64,
    offset: u64,
    sample_rate: f64,
) -> AudioBuffer<f32> {
    let mut buffer: AudioBuffer<f32> = AudioBuffer::new(duration, spec);
    #[allow(clippy::cast_possible_truncation)]
    buffer.render_reserved(Some(duration as usize));

    // Generate a 440 Hz sine wave (A4 note)
    let frequency = 440.0;
    let amplitude = 0.5;

    // Get mutable references to left and right channels
    let (left, right) = buffer.chan_pair_mut(0, 1);

    #[allow(clippy::cast_possible_truncation)]
    for (i, (left_sample, right_sample)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
        let frame_index = offset + i as u64;
        #[allow(clippy::cast_precision_loss)]
        let t = frame_index as f64 / sample_rate;
        #[allow(clippy::cast_possible_truncation)]
        let value = (amplitude * (2.0 * std::f64::consts::PI * frequency * t).sin()) as f32;

        *left_sample = value;
        *right_sample = value;
    }

    buffer
}
