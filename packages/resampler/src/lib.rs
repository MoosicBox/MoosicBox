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
    #[must_use]
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
#[allow(clippy::cast_precision_loss, clippy::float_cmp, clippy::doc_markdown)]
mod tests {
    use super::*;
    use symphonia::core::audio::Channels;

    /// Test that a resampler can be created with valid parameters
    #[test]
    fn test_resampler_creation() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        assert_eq!(resampler.spec.rate, 44100);
        assert_eq!(resampler.spec.channels.count(), 2);
        assert_eq!(resampler.duration, 1024);
    }

    /// Test that resampler returns None when insufficient samples are buffered
    #[test]
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
    #[test]
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
    #[test]
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
    #[test]
    fn test_flush_empty_buffer() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Flush without adding any samples
        let result = resampler.flush();
        assert!(result.is_none());
    }

    /// Test that flush processes partial buffers by padding with silence
    #[test]
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
    #[test]
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
    #[test]
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
    #[test]
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
    #[test]
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
    #[test]
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
}
