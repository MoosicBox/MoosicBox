//! Audio resampling for converting between sample rates.
//!
//! This crate provides FFT-based audio resampling using the `rubato` library,
//! designed to work seamlessly with Symphonia audio buffers. It supports
//! converting audio from one sample rate to another with high quality.
//!
//! # Example
//!
//! ```rust
//! # use moosicbox_resampler::Resampler;
//! # use symphonia::core::audio::{AudioBuffer, SignalSpec, Signal, Channels};
//! # use symphonia::core::units::Duration;
//! # fn example() {
//! // Create a signal specification for stereo 44.1kHz audio
//! let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
//!
//! // Create a resampler to convert to 48kHz with a duration of 1024 samples
//! let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);
//!
//! // Resample an audio buffer (in practice, this would come from a decoder)
//! # let input_buffer: AudioBuffer<f32> = AudioBuffer::new(1024, spec);
//! if let Some(resampled) = resampler.resample(&input_buffer) {
//!     // Use the resampled audio data
//!     println!("Resampled {} samples", resampled.len());
//! }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::struct_field_names)]

use symphonia::core::audio::{AudioBuffer, Signal, SignalSpec};
use symphonia::core::conv::{IntoSample, ReversibleSample};
use symphonia::core::sample::Sample;

/// Audio resampler for converting between sample rates.
///
/// Uses FFT-based resampling to convert audio from one sample rate to another.
pub struct Resampler<T> {
    resampler: rubato::FftFixedIn<f32>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    interleaved: Vec<T>,
    duration: usize,
    /// Signal specification for the output audio.
    pub spec: SignalSpec,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl<T> Resampler<T>
where
    T: Sample + ReversibleSample<f32>,
{
    fn resample_inner(&mut self) -> &[T] {
        {
            let mut input: arrayvec::ArrayVec<&[f32], 32> = arrayvec::ArrayVec::default();

            for channel in &self.input {
                input.push(&channel[..self.duration]);
            }

            // Resample.
            rubato::Resampler::process_into_buffer(
                &mut self.resampler,
                &input,
                &mut self.output,
                None,
            )
            .unwrap();
        }

        // Remove consumed samples from the input buffer.
        for channel in &mut self.input {
            channel.drain(0..self.duration);
        }

        // Interleave the planar samples from Rubato.
        let num_channels = self.output.len();

        self.interleaved
            .resize(num_channels * self.output[0].len(), T::MID);

        for (i, frame) in self.interleaved.chunks_exact_mut(num_channels).enumerate() {
            for (ch, s) in frame.iter_mut().enumerate() {
                *s = self.output[ch][i].into_sample();
            }
        }

        &self.interleaved
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl<T> Resampler<T>
where
    T: Sample + ReversibleSample<f32>,
{
    /// Creates a new resampler.
    ///
    /// # Panics
    ///
    /// * If the `duration` cannot be converted to a `usize`
    /// * If failed to create the `FftFixedIn` resampler
    #[must_use]
    pub fn new(spec: SignalSpec, to_sample_rate: usize, duration: u64) -> Self {
        let duration = usize::try_from(duration).unwrap();
        let num_channels = spec.channels.count();

        let resampler = rubato::FftFixedIn::<f32>::new(
            spec.rate as usize,
            to_sample_rate,
            duration,
            2,
            num_channels,
        )
        .unwrap();

        // For 0.15.0:
        // let output = rubato::Resampler::output_buffer_allocate(&resampler, true);
        let output = rubato::Resampler::output_buffer_allocate(&resampler);

        let input = vec![Vec::with_capacity(duration); num_channels];

        Self {
            resampler,
            input,
            output,
            duration,
            spec,
            interleaved: Vec::default(),
        }
    }

    /// Resamples a planar/non-interleaved input.
    ///
    /// Returns the resampled samples in an interleaved format. Returns `None`
    /// if the internal buffer does not yet contain enough samples to produce output
    /// (requires at least `duration` samples accumulated).
    ///
    /// # Panics
    ///
    /// * If the internal resampler's `process_into_buffer` operation fails
    pub fn resample(&mut self, input: &AudioBuffer<f32>) -> Option<&[T]> {
        // Copy and convert samples into input buffer.
        convert_samples(input, &mut self.input);

        // Check if more samples are required.
        if self.input[0].len() < self.duration {
            return None;
        }

        Some(self.resample_inner())
    }

    /// Resamples a planar/non-interleaved input and returns an `AudioBuffer`.
    ///
    /// Returns the resampled samples as an `AudioBuffer`. Returns `None` if the
    /// internal buffer does not yet contain enough samples to produce output
    /// (requires at least `duration` samples accumulated).
    ///
    /// # Panics
    ///
    /// * If the internal resampler's `process_into_buffer` operation fails
    /// * If the audio is not stereo (2-channel) - the `to_audio_buffer` conversion will panic
    pub fn resample_to_audio_buffer(&mut self, input: &AudioBuffer<f32>) -> Option<AudioBuffer<T>> {
        let spec = self.spec;
        self.resample(input)
            .map(|samples| to_audio_buffer(samples, spec))
    }

    /// Resample any remaining samples in the resample buffer.
    ///
    /// This method should be called at the end of a stream to process any buffered
    /// samples that haven't been resampled yet. It pads the input with silence to
    /// meet the required `duration` and produces the final resampled output.
    ///
    /// Returns `None` if the internal buffer is empty (no samples to flush).
    ///
    /// # Panics
    ///
    /// * If the internal resampler's `process_into_buffer` operation fails
    #[allow(unused)]
    pub fn flush(&mut self) -> Option<&[T]> {
        let len = self.input[0].len();

        if len == 0 {
            return None;
        }

        let partial_len = len % self.duration;

        if partial_len != 0 {
            // Fill each input channel buffer with silence to the next multiple of the resampler
            // duration.
            for channel in &mut self.input {
                channel.resize(len + (self.duration - partial_len), f32::MID);
            }
        }

        Some(self.resample_inner())
    }
}

#[cfg_attr(feature = "profiling", profiling::function)]
fn convert_samples<S>(input: &AudioBuffer<S>, output: &mut [Vec<f32>])
where
    S: Sample + IntoSample<f32>,
{
    for (c, dst) in output.iter_mut().enumerate() {
        let src = input.chan(c);
        dst.extend(src.iter().map(|&s| s.into_sample()));
    }
}

/// Converts interleaved samples to an `AudioBuffer`.
///
/// **Note**: Currently only supports stereo (2-channel) audio. The function will panic
/// or produce incorrect results if used with mono or multi-channel audio.
///
/// # Panics
///
/// * If the audio is not stereo (2-channel) - the `chan_pair_mut()` call will panic
#[must_use]
#[cfg_attr(feature = "profiling", profiling::function)]
pub fn to_audio_buffer<S>(samples: &[S], spec: SignalSpec) -> AudioBuffer<S>
where
    S: Sample,
{
    let duration = samples.len() as u64;
    let mut buf: AudioBuffer<S> = AudioBuffer::new(duration, spec);
    buf.render_reserved(Some(samples.len() / spec.channels.count()));

    let (left, right) = buf.chan_pair_mut(0, 1);
    let mut is_left = true;
    let mut i = 0;

    for sample in samples {
        if is_left {
            left[i] = *sample;
            is_left = false;
        } else {
            right[i] = *sample;
            is_left = true;
            i += 1;
        }
    }

    buf
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;
    use symphonia::core::audio::Channels;

    /// Test that a resampler can be created with valid parameters
    #[test_log::test]
    fn test_resampler_creation() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        assert_eq!(resampler.spec.rate, 44100);
        assert_eq!(resampler.spec.channels.count(), 2);
        assert_eq!(resampler.duration, 1024);
    }

    /// Test that resampler returns None when insufficient samples are buffered
    #[test_log::test]
    fn test_resample_insufficient_samples() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Create a buffer with fewer samples than required duration
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer.render_reserved(Some(512));

        // Fill with some test data
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }

        // Should return None because we don't have enough samples yet
        let result = resampler.resample(&input_buffer);
        assert!(result.is_none());
    }

    /// Test that resampler produces output once sufficient samples are accumulated
    #[test_log::test]
    fn test_resample_sufficient_samples() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Create a buffer with exactly the required duration
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(1024, spec);
        input_buffer.render_reserved(Some(1024));

        // Fill with test data
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 1024.0;
            }
        }

        // Should return Some with resampled output
        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        // Output should be interleaved stereo
        assert!(!output.is_empty());
        // Should be even number of samples (stereo)
        assert_eq!(output.len() % 2, 0);
    }

    /// Test that resampler accumulates samples across multiple calls
    #[test_log::test]
    fn test_resample_accumulation() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // First call with 512 samples - should return None
        let mut input_buffer1: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer1.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer1.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.3;
            }
        }
        assert!(resampler.resample(&input_buffer1).is_none());

        // Second call with another 512 samples - should now return Some
        let mut input_buffer2: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer2.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer2.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.7;
            }
        }
        let result = resampler.resample(&input_buffer2);
        assert!(result.is_some());
        assert!(!result.unwrap().is_empty());
    }

    /// Test that flush returns None when buffer is empty
    #[test_log::test]
    fn test_flush_empty_buffer() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Flush without adding any samples
        let result = resampler.flush();
        assert!(result.is_none());
    }

    /// Test that flush processes partial buffers by padding with silence
    #[test_log::test]
    fn test_flush_partial_buffer() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Add partial buffer (less than duration)
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }

        // Resample returns None due to insufficient samples
        assert!(resampler.resample(&input_buffer).is_none());

        // Flush should process the partial buffer
        let result = resampler.flush();
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(!output.is_empty());
        assert_eq!(output.len() % 2, 0);
    }

    /// Test that flush handles exact multiples of duration correctly
    #[test_log::test]
    fn test_flush_exact_multiple() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Add exactly duration samples
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(1024, spec);
        input_buffer.render_reserved(Some(1024));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }

        // This should consume all samples
        assert!(resampler.resample(&input_buffer).is_some());

        // Flush should now return None since buffer is empty
        let result = resampler.flush();
        assert!(result.is_none());
    }

    /// Test to_audio_buffer conversion for stereo audio
    #[test_log::test]
    fn test_to_audio_buffer_stereo() {
        let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);

        // Create interleaved stereo samples: [L0, R0, L1, R1, L2, R2, ...]
        let samples: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];

        let audio_buffer = to_audio_buffer(&samples, spec);

        // Check buffer size
        assert_eq!(audio_buffer.frames(), 4);

        // Check that samples were correctly de-interleaved
        let left = audio_buffer.chan(0);
        let right = audio_buffer.chan(1);

        assert_eq!(left[0], 0.1);
        assert_eq!(right[0], 0.2);
        assert_eq!(left[1], 0.3);
        assert_eq!(right[1], 0.4);
        assert_eq!(left[2], 0.5);
        assert_eq!(right[2], 0.6);
        assert_eq!(left[3], 0.7);
        assert_eq!(right[3], 0.8);
    }

    /// Test resample_to_audio_buffer method
    #[test_log::test]
    fn test_resample_to_audio_buffer() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Create input buffer
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(1024, spec);
        input_buffer.render_reserved(Some(1024));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 1024.0;
            }
        }

        // Should return Some with AudioBuffer
        let result = resampler.resample_to_audio_buffer(&input_buffer);
        assert!(result.is_some());

        let output_buffer = result.unwrap();
        assert!(output_buffer.frames() > 0);
        assert_eq!(output_buffer.spec().channels.count(), 2);
    }

    /// Test that resampler handles different sample rates correctly
    #[test_log::test]
    fn test_resample_rate_change() {
        let input_rate = 44100;
        let output_rate = 48000;
        let spec = SignalSpec::new(input_rate, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, output_rate, 1024);

        // Create input buffer
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(1024, spec);
        input_buffer.render_reserved(Some(1024));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        // Verify output is stereo (even number of samples)
        assert_eq!(output.len() % 2, 0);
        // Verify we got some output samples
        assert!(!output.is_empty());
        // The actual output size depends on the resampler's FFT implementation details,
        // but we can verify it's in a reasonable range for the rate conversion
        assert!(
            output.len() > 1000 && output.len() < 3000,
            "Expected reasonable output size for 44.1kHz -> 48kHz conversion, got {} samples",
            output.len()
        );
    }

    /// Test resampling with mono audio
    #[test_log::test]
    fn test_resample_mono() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(1024, spec);
        input_buffer.render_reserved(Some(1024));
        let channel = input_buffer.chan_mut(0);
        for (i, sample) in channel.iter_mut().enumerate() {
            *sample = (i as f32) / 1024.0;
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        // Mono output should have exactly as many samples as frames
        assert!(!output.is_empty());
    }

    /// Test downsampling (higher to lower sample rate)
    #[test_log::test]
    fn test_resample_downsampling() {
        let input_rate = 48000;
        let output_rate = 44100;
        let spec = SignalSpec::new(input_rate, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, output_rate, 1024);

        // Create input buffer with a simple sine wave pattern
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(1024, spec);
        input_buffer.render_reserved(Some(1024));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                // Use a simple pattern to verify data flows through correctly
                *sample = ((i as f32) * std::f32::consts::PI / 128.0).sin();
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        // Verify output is stereo (even number of samples)
        assert_eq!(output.len() % 2, 0);
        // Verify we got some output samples
        assert!(!output.is_empty());
        // For downsampling 48kHz -> 44.1kHz, we expect fewer output samples than input
        // Since we have 1024 frames * 2 channels = 2048 input samples interleaved,
        // and the ratio is approximately 44100/48000 ≈ 0.919, we expect roughly
        // 1024 * 0.919 ≈ 941 frames, so ~1882 interleaved samples
        let output_frames = output.len() / 2;
        assert!(
            output_frames < 1024,
            "Downsampling should produce fewer frames: got {output_frames} frames from 1024 input frames"
        );
    }

    /// Test multiple consecutive resample cycles to verify state is properly managed
    #[test_log::test]
    fn test_multiple_resample_cycles() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Perform multiple resample cycles
        for cycle in 0..3 {
            let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(1024, spec);
            input_buffer.render_reserved(Some(1024));
            for ch in 0..spec.channels.count() {
                let channel = input_buffer.chan_mut(ch);
                for (i, sample) in channel.iter_mut().enumerate() {
                    // Use cycle-dependent values to ensure we're getting fresh data each time
                    *sample = ((i + cycle * 1024) as f32) / 3072.0;
                }
            }

            let result = resampler.resample(&input_buffer);
            assert!(result.is_some(), "Cycle {cycle} should produce output");

            let output = result.unwrap();
            assert!(
                !output.is_empty(),
                "Cycle {cycle} output should not be empty"
            );
            assert_eq!(output.len() % 2, 0, "Cycle {cycle} output should be stereo");
        }
    }

    /// Test that flush handles remainder after multiple partial buffers
    #[test_log::test]
    fn test_flush_after_multiple_partial_buffers() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Add 300 samples
        let mut input_buffer1: AudioBuffer<f32> = AudioBuffer::new(300, spec);
        input_buffer1.render_reserved(Some(300));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer1.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.3;
            }
        }
        assert!(resampler.resample(&input_buffer1).is_none());

        // Add another 200 samples (total 500, still less than 1024)
        let mut input_buffer2: AudioBuffer<f32> = AudioBuffer::new(200, spec);
        input_buffer2.render_reserved(Some(200));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer2.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }
        assert!(resampler.resample(&input_buffer2).is_none());

        // Add another 100 samples (total 600, still less than 1024)
        let mut input_buffer3: AudioBuffer<f32> = AudioBuffer::new(100, spec);
        input_buffer3.render_reserved(Some(100));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer3.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.7;
            }
        }
        assert!(resampler.resample(&input_buffer3).is_none());

        // Flush should process all accumulated samples (600 total) padded to 1024
        let result = resampler.flush();
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(!output.is_empty());
        assert_eq!(output.len() % 2, 0);
    }

    /// Test resampling with more than duration samples produces output and keeps remainder
    #[test_log::test]
    fn test_resample_excess_samples() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Add 1500 samples (more than duration of 1024)
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(1500, spec);
        input_buffer.render_reserved(Some(1500));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 1500.0;
            }
        }

        // First resample should consume 1024 samples
        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        // There should be 476 samples remaining (1500 - 1024)
        // Adding another 548 samples should trigger another output (476 + 548 = 1024)
        let mut input_buffer2: AudioBuffer<f32> = AudioBuffer::new(548, spec);
        input_buffer2.render_reserved(Some(548));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer2.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }

        let result2 = resampler.resample(&input_buffer2);
        assert!(
            result2.is_some(),
            "Should have enough samples after accumulation"
        );
    }

    /// Test to_audio_buffer with empty samples
    #[test_log::test]
    fn test_to_audio_buffer_empty() {
        let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let samples: Vec<f32> = vec![];

        let audio_buffer = to_audio_buffer(&samples, spec);
        assert_eq!(audio_buffer.frames(), 0);
    }

    /// Test extreme sample rate conversion (large ratio)
    #[test_log::test]
    fn test_resample_large_rate_ratio() {
        // Test 8kHz to 48kHz (6x upsampling)
        let input_rate = 8000;
        let output_rate = 48000;
        let spec = SignalSpec::new(input_rate, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, output_rate, 512);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = ((i as f32) * std::f32::consts::PI / 64.0).sin();
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        assert!(!output.is_empty());
        assert_eq!(output.len() % 2, 0);

        // With 6x upsampling, we expect significantly more output samples
        let output_frames = output.len() / 2;
        assert!(
            output_frames > 512,
            "6x upsampling should produce more frames: got {output_frames} frames from 512 input"
        );
    }

    /// Test that resample_to_audio_buffer returns None when insufficient samples
    #[test_log::test]
    fn test_resample_to_audio_buffer_insufficient() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }

        let result = resampler.resample_to_audio_buffer(&input_buffer);
        assert!(result.is_none());
    }

    /// Test flush when buffer contains exactly duration samples (not consumed by resample)
    ///
    /// This tests the case in flush() where len % duration == 0 but len > 0,
    /// meaning no padding is needed but samples still need processing.
    #[test_log::test]
    fn test_flush_exact_duration_not_consumed() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let duration = 256;
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, duration);

        // Add exactly duration samples via two smaller buffers
        // but don't call resample() enough times to consume them
        let mut input_buffer1: AudioBuffer<f32> = AudioBuffer::new(128, spec);
        input_buffer1.render_reserved(Some(128));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer1.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 128.0;
            }
        }
        // Returns None - only 128 samples buffered
        assert!(resampler.resample(&input_buffer1).is_none());

        let mut input_buffer2: AudioBuffer<f32> = AudioBuffer::new(128, spec);
        input_buffer2.render_reserved(Some(128));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer2.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = 0.5 + (i as f32) / 256.0;
            }
        }
        // Returns None - 256 samples buffered but we want to test flush instead
        // Note: resample would return Some here, but we skip it to test flush directly
        // by NOT calling resample again

        // Actually, let's add just under duration to test the exact boundary
        // Reset and try again with exact buffer
        let mut resampler2: Resampler<f32> = Resampler::new(spec, 48000, duration);

        // Add exactly 256 samples at once
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(256, spec);
        input_buffer.render_reserved(Some(256));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = ((i as f32) * std::f32::consts::PI / 32.0).sin() * 0.8;
            }
        }

        // This returns Some and consumes the samples
        let result = resampler2.resample(&input_buffer);
        assert!(result.is_some());

        // Now test with 2x duration samples where we only consume once
        let mut resampler3: Resampler<f32> = Resampler::new(spec, 48000, duration);

        // Add 2x duration samples
        let mut large_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        large_buffer.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = large_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = ((i as f32) * std::f32::consts::PI / 64.0).sin() * 0.7;
            }
        }

        // First resample consumes 256 samples, leaving 256 in buffer
        let result = resampler3.resample(&large_buffer);
        assert!(result.is_some());

        // Now flush should process the remaining exactly 256 samples
        // partial_len = 256 % 256 = 0, so no padding needed
        let flush_result = resampler3.flush();
        assert!(
            flush_result.is_some(),
            "Flush should return Some when exactly duration samples remain"
        );
        let output = flush_result.unwrap();
        assert!(!output.is_empty());
        assert_eq!(output.len() % 2, 0, "Output should be stereo");
    }

    /// Test resampling with i16 sample type to verify generic type conversion works
    ///
    /// The resampler internally works with f32 and converts to the output type T.
    /// This test verifies the conversion path for integer sample types works correctly.
    #[test_log::test]
    fn test_resample_i16_output() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<i16> = Resampler::new(spec, 48000, 512);

        // Create input buffer with a simple pattern
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                // Use a sine wave pattern with values in [-1, 1] range
                *sample = ((i as f32) * std::f32::consts::PI / 64.0).sin() * 0.9;
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some(), "Should produce output");

        let output = result.unwrap();
        assert!(!output.is_empty(), "Output should not be empty");
        assert_eq!(output.len() % 2, 0, "Output should be stereo");

        // Verify output contains non-zero samples (signal is present after conversion)
        let has_non_zero = output.iter().any(|&sample| sample != 0);
        assert!(has_non_zero, "Output should contain non-zero samples");
    }

    /// Test resampling with identity sample rate (input rate == output rate)
    ///
    /// While unusual, this edge case ensures the resampler handles a 1:1 ratio
    /// without issues. This exercises the rubato FFT resampler with ratio = 1.0.
    #[test_log::test]
    fn test_resample_identity_rate() {
        let spec = SignalSpec::new(48000, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 512);

        // Create input buffer with a known pattern
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 512.0;
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(
            result.is_some(),
            "Should produce output even with identity rate"
        );

        let output = result.unwrap();
        assert!(!output.is_empty(), "Output should not be empty");
        assert_eq!(output.len() % 2, 0, "Output should be stereo");

        // With identity rate, output frame count should be very close to input frame count
        let output_frames = output.len() / 2;
        let diff = output_frames.abs_diff(512);
        assert!(
            diff < 50,
            "Identity rate should produce approximately same frame count: got {output_frames} from 512"
        );
    }

    /// Test flush with exactly 2x duration samples remaining
    ///
    /// This tests the flush code path when there's more than one duration's worth
    /// of samples but it's an exact multiple (no padding needed).
    #[test_log::test]
    fn test_flush_double_duration_remaining() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let duration = 256;
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, duration);

        // Add 3x duration samples
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(768, spec);
        input_buffer.render_reserved(Some(768));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = ((i as f32) * std::f32::consts::PI / 96.0).sin() * 0.6;
            }
        }

        // First resample consumes 256 samples, leaving 512 (2x duration)
        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        // Flush should process the remaining 512 samples (2x duration)
        // partial_len = 512 % 256 = 0, so no padding needed
        let flush_result = resampler.flush();
        assert!(
            flush_result.is_some(),
            "Flush should return Some when 2x duration samples remain"
        );
        let output = flush_result.unwrap();
        assert!(!output.is_empty());
        assert_eq!(output.len() % 2, 0, "Output should be stereo");
    }

    /// Test multi-channel resampling with 4 channels (quadraphonic)
    ///
    /// Verifies that the resampler correctly handles more than 2 channels.
    /// The output should be properly interleaved across all channels.
    #[test_log::test]
    fn test_resample_quad_channel() {
        let spec = SignalSpec::new(
            44100,
            Channels::FRONT_LEFT
                | Channels::FRONT_RIGHT
                | Channels::REAR_LEFT
                | Channels::REAR_RIGHT,
        );
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 512);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer.render_reserved(Some(512));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                // Different pattern per channel for verification
                *sample =
                    ((ch as f32).mul_add(50.0, i as f32) * std::f32::consts::PI / 64.0).sin() * 0.7;
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some(), "Should produce output for quad audio");

        let output = result.unwrap();
        assert!(!output.is_empty(), "Output should not be empty");
        // Output should have samples for all 4 channels interleaved
        assert_eq!(output.len() % 4, 0, "Output should be interleaved quad");
    }

    /// Test resampling with extreme sample values at boundaries
    ///
    /// Verifies that the resampler handles samples at the maximum (+1.0)
    /// and minimum (-1.0) values without overflow or unexpected behavior.
    #[test_log::test]
    fn test_resample_boundary_values() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 512);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(512, spec);
        input_buffer.render_reserved(Some(512));

        // Left channel: alternating between +1.0 and -1.0 (maximum swing)
        let left = input_buffer.chan_mut(0);
        for (i, sample) in left.iter_mut().enumerate() {
            *sample = if i % 2 == 0 { 1.0 } else { -1.0 };
        }

        // Right channel: constant at 0.0 (silence) for contrast
        let right = input_buffer.chan_mut(1);
        for sample in right.iter_mut() {
            *sample = 0.0;
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some(), "Should handle boundary values");

        let output = result.unwrap();
        assert!(!output.is_empty(), "Output should not be empty");
        assert_eq!(output.len() % 2, 0, "Output should be stereo");

        // Verify no NaN or Inf values in output
        for &sample in output {
            assert!(sample.is_finite(), "All samples should be finite values");
        }
    }

    /// Test flush when buffer length is just under duration (requires padding)
    ///
    /// Tests the padding logic when partial_len is close to duration.
    #[test_log::test]
    fn test_flush_just_under_duration() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let duration = 256;
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, duration);

        // Add 255 samples (just 1 short of duration)
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(255, spec);
        input_buffer.render_reserved(Some(255));
        for ch in 0..spec.channels.count() {
            let channel = input_buffer.chan_mut(ch);
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 255.0 * 0.8;
            }
        }

        // Returns None because we have only 255 samples (< duration)
        assert!(resampler.resample(&input_buffer).is_none());

        // Flush should pad the 255 samples to 256 and process
        let flush_result = resampler.flush();
        assert!(
            flush_result.is_some(),
            "Flush should pad and process 255 samples"
        );
        let output = flush_result.unwrap();
        assert!(!output.is_empty());
        assert_eq!(output.len() % 2, 0, "Output should be stereo");
    }
}
