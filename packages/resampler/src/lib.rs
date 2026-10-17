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
//! # use symphonia::core::audio::{AudioBuffer, AudioSpec, Audio, AudioMut, Position};
//! # use symphonia::core::units::Duration;
//! # fn example() {
//! // Create a signal specification for stereo 44.1kHz audio
//! let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
//!
//! // Create a resampler to convert to 48kHz with a duration of 1024 samples
//! let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);
//!
//! // Resample an audio buffer (in practice, this would come from a decoder)
//! # let input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1024);
//! if let Some(resampled) = resampler.resample(&input_buffer) {
//!     // Use the resampled audio data
//!     println!("Resampled {} samples", resampled.len());
//! }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::struct_field_names)]

use symphonia::core::audio::conv::{IntoSample, ReversibleSample};
use symphonia::core::audio::sample::Sample;
use symphonia::core::audio::{Audio, AudioBuffer, AudioMut, AudioSpec};

/// Audio resampler for converting between sample rates.
///
/// Uses FFT-based resampling to convert audio from one sample rate to another.
pub struct Resampler<T> {
    resampler: rubato::FftFixedIn<f32>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    interleaved: Vec<T>,
    duration: usize,
    output_spec: AudioSpec,
    /// Signal specification for the input audio.
    pub spec: AudioSpec,
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
    ///
    /// # Examples
    ///
    /// ```rust
    /// use moosicbox_resampler::Resampler;
    /// use symphonia::core::audio::{Position, AudioSpec};
    ///
    /// let spec = AudioSpec::new(44_100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
    /// let _resampler: Resampler<f32> = Resampler::new(spec.clone(), 48_000, 1_024);
    /// ```
    #[must_use]
    pub fn new(spec: AudioSpec, to_sample_rate: usize, duration: u64) -> Self {
        let duration = usize::try_from(duration).unwrap();
        let num_channels = spec.channels().count();

        let resampler = rubato::FftFixedIn::<f32>::new(
            spec.rate() as usize,
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
            output_spec: AudioSpec::new(
                u32::try_from(to_sample_rate).unwrap(),
                spec.channels().clone(),
            ),
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
    ///
    /// # Examples
    ///
    /// ```rust
    /// use moosicbox_resampler::Resampler;
    /// use symphonia::core::audio::{AudioBuffer, Position, Audio, AudioMut, AudioSpec};
    ///
    /// let spec = AudioSpec::new(44_100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
    /// let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48_000, 1_024);
    ///
    /// let mut input: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1_024);
    /// input.render_uninit(Some(1_024));
    ///
    /// let output = resampler.resample(&input);
    /// assert!(output.is_some());
    /// ```
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
    ///
    /// # Errors
    ///
    /// * Returns `AudioBufferConversionError` if the resampled interleaved data
    ///   does not contain a whole number of frames or has no channels.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use moosicbox_resampler::Resampler;
    /// use symphonia::core::audio::{AudioBuffer, Position, Audio, AudioMut, AudioSpec};
    ///
    /// let spec = AudioSpec::new(44_100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
    /// let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48_000, 1_024);
    ///
    /// let mut input: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1_024);
    /// input.render_uninit(Some(1_024));
    ///
    /// let output = resampler.resample_to_audio_buffer(&input).unwrap();
    /// assert!(output.is_some());
    /// ```
    pub fn resample_to_audio_buffer(
        &mut self,
        input: &AudioBuffer<f32>,
    ) -> Result<Option<AudioBuffer<T>>, AudioBufferConversionError> {
        let spec = self.output_spec.clone();
        self.resample(input)
            .map(|samples| to_audio_buffer(samples, spec))
            .transpose()
    }

    /// Output specification with destination rate and original channel order.
    #[must_use]
    pub const fn output_spec(&self) -> &AudioSpec {
        &self.output_spec
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
    ///
    /// # Examples
    ///
    /// ```rust
    /// use moosicbox_resampler::Resampler;
    /// use symphonia::core::audio::{AudioBuffer, Position, Audio, AudioMut, AudioSpec};
    ///
    /// let spec = AudioSpec::new(44_100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
    /// let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48_000, 1_024);
    ///
    /// let mut partial: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
    /// partial.render_uninit(Some(512));
    /// assert!(resampler.resample(&partial).is_none());
    ///
    /// let flushed = resampler.flush();
    /// assert!(flushed.is_some());
    /// ```
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
        let src = input.plane(c).unwrap();
        dst.extend(src.iter().map(|&s| s.into_sample()));
    }
}

/// Errors converting interleaved samples to planar audio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioBufferConversionError {
    /// A frame must contain at least one channel.
    NoChannels,
    /// Every frame must supply one sample per channel; no samples are dropped.
    IncompleteFrame { samples: usize, channels: usize },
}

impl std::fmt::Display for AudioBufferConversionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoChannels => formatter.write_str("audio specification has no channels"),
            Self::IncompleteFrame { samples, channels } => write!(
                formatter,
                "{samples} samples do not form complete {channels}-channel frames"
            ),
        }
    }
}

impl std::error::Error for AudioBufferConversionError {}

/// Convert interleaved samples to planar audio in specification channel order.
///
/// Empty input yields zero frames. Frame count and capacity are exactly
/// `samples.len() / channels`; sample values are copied without conversion.
///
/// # Errors
///
/// * `NoChannels` if the specification contains no channels (even for empty input).
/// * `IncompleteFrame` if the sample count is not divisible by the channel count.
///
/// # Examples
///
/// ```rust
/// use moosicbox_resampler::to_audio_buffer;
/// use symphonia::core::audio::{Position, Audio, AudioMut, AudioSpec};
///
/// let spec = AudioSpec::new(48_000, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
/// let samples = vec![0.1_f32, 0.2, 0.3, 0.4];
/// let buffer = to_audio_buffer(&samples, spec).unwrap();
///
/// assert_eq!(buffer.frames(), 2);
/// ```
#[cfg_attr(feature = "profiling", profiling::function)]
pub fn to_audio_buffer<S>(
    samples: &[S],
    spec: AudioSpec,
) -> Result<AudioBuffer<S>, AudioBufferConversionError>
where
    S: Sample,
{
    let channels = spec.channels().count();
    if channels == 0 {
        return Err(AudioBufferConversionError::NoChannels);
    }
    if !samples.len().is_multiple_of(channels) {
        return Err(AudioBufferConversionError::IncompleteFrame {
            samples: samples.len(),
            channels,
        });
    }
    let frames = samples.len() / channels;
    let mut buffer = AudioBuffer::new(spec, frames);
    buffer.render_uninit(Some(frames));
    for (channel, plane) in buffer.iter_planes_mut().enumerate() {
        for (sample, frame) in plane.iter_mut().zip(samples.chunks_exact(channels)) {
            *sample = frame[channel];
        }
    }
    Ok(buffer)
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::suboptimal_flops
)]
mod tests {
    use super::*;
    use symphonia::core::audio::Position;

    fn channel_spec(channels: usize, rate: u32) -> AudioSpec {
        use symphonia::core::audio::{ChannelLabel, Channels};
        AudioSpec::new(
            rate,
            Channels::from(
                (0..channels)
                    .map(|channel| ChannelLabel::Discrete(channel as u16))
                    .collect::<Vec<_>>(),
            ),
        )
    }

    #[test]
    fn interleaved_materialization_all_channels_and_exact_bits() {
        for channels in [1, 2, 3, 6] {
            let samples: Vec<f32> = (0..channels * 5)
                .map(|i| f32::from_bits(0x3f00_0000 + i as u32))
                .collect();
            let buffer = to_audio_buffer(&samples, channel_spec(channels, 48000)).unwrap();
            assert_eq!(buffer.frames(), 5);
            assert_eq!(buffer.capacity(), 5);
            for channel in 0..channels {
                for frame in 0..5 {
                    assert_eq!(
                        buffer.plane(channel).unwrap()[frame].to_bits(),
                        samples[frame * channels + channel].to_bits()
                    );
                }
            }
            let empty = to_audio_buffer::<f32>(&[], channel_spec(channels, 48000)).unwrap();
            assert_eq!(empty.frames(), 0);
            assert_eq!(empty.capacity(), 0);
        }
        let specials = [0.0_f32, -0.0, f32::from_bits(0x7fc0_0042), f32::INFINITY];
        let stereo = to_audio_buffer(&specials, channel_spec(2, 48000)).unwrap();
        assert_eq!(
            stereo
                .plane(0)
                .unwrap()
                .iter()
                .map(|s| s.to_bits())
                .collect::<Vec<_>>(),
            [specials[0].to_bits(), specials[2].to_bits()]
        );
        assert_eq!(
            stereo
                .plane(1)
                .unwrap()
                .iter()
                .map(|s| s.to_bits())
                .collect::<Vec<_>>(),
            [specials[1].to_bits(), specials[3].to_bits()]
        );
    }

    #[test]
    fn materialization_rejects_invalid_frame_shapes() {
        for samples in [&[][..], &[1.0_f32][..]] {
            assert_eq!(
                to_audio_buffer(samples, channel_spec(0, 48000))
                    .err()
                    .unwrap(),
                AudioBufferConversionError::NoChannels
            );
        }
        for channels in [2, 3, 6] {
            let samples = vec![1.0_f32; channels + 1];
            assert_eq!(
                to_audio_buffer(&samples, channel_spec(channels, 48000))
                    .err()
                    .unwrap(),
                AudioBufferConversionError::IncompleteFrame {
                    samples: channels + 1,
                    channels
                }
            );
        }
    }

    #[test]
    fn resampled_materialization_matches_interleaved_all_channels() {
        for channels in [1, 2, 3, 6] {
            let spec = channel_spec(channels, 44100);
            let samples: Vec<f32> = (0..1024)
                .flat_map(|frame| {
                    (0..channels).map(move |channel| {
                        ((frame as f32) * 0.05).sin() * (channel + 1) as f32 * 0.1
                    })
                })
                .collect();
            let input = to_audio_buffer(&samples, spec.clone()).unwrap();
            let mut interleaved: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);
            let mut planar: Resampler<f32> = Resampler::new(spec, 48000, 1024);
            assert_eq!(planar.output_spec().rate(), 48000);
            let expected = interleaved.resample(&input).unwrap();
            assert!(expected.iter().any(|sample| sample.abs() > 0.01));
            let actual = planar.resample_to_audio_buffer(&input).unwrap().unwrap();
            assert_eq!(actual.spec().rate(), 48000);
            assert_eq!(actual.frames() * channels, expected.len());
            for channel in 0..channels {
                for (frame, sample) in actual.plane(channel).unwrap().iter().enumerate() {
                    assert_eq!(
                        sample.to_bits(),
                        expected[frame * channels + channel].to_bits()
                    );
                }
            }
        }
    }

    /// Test that a resampler can be created with valid parameters
    #[test_log::test]
    fn test_resampler_creation() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        assert_eq!(resampler.spec.rate(), 44100);
        assert_eq!(resampler.spec.channels().count(), 2);
        assert_eq!(resampler.duration, 1024);
    }

    /// Test that resampler returns None when insufficient samples are buffered
    #[test_log::test]
    fn test_resample_insufficient_samples() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // Create a buffer with fewer samples than required duration
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer.render_uninit(Some(512));

        // Fill with some test data
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
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
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // Create a buffer with exactly the required duration
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1024);
        input_buffer.render_uninit(Some(1024));

        // Fill with test data
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 1024.0;
            }
        }

        // Should return Some with resampled output
        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        // Output should be interleaved stereo
        assert_ne!(output, []);
        // Should be even number of samples (stereo)
        assert_eq!(output.len() % 2, 0);
    }

    /// Test that resampler accumulates samples across multiple calls
    #[test_log::test]
    fn test_resample_accumulation() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // First call with 512 samples - should return None
        let mut input_buffer1: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer1.render_uninit(Some(512));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer1.plane_mut(ch).unwrap();
            for sample in channel.iter_mut() {
                *sample = 0.3;
            }
        }
        assert!(resampler.resample(&input_buffer1).is_none());

        // Second call with another 512 samples - should now return Some
        let mut input_buffer2: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer2.render_uninit(Some(512));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer2.plane_mut(ch).unwrap();
            for sample in channel.iter_mut() {
                *sample = 0.7;
            }
        }
        let result = resampler.resample(&input_buffer2);
        assert!(result.is_some());
        assert_ne!(result.unwrap(), []);
    }

    /// Test that flush returns None when buffer is empty
    #[test_log::test]
    fn test_flush_empty_buffer() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec, 48000, 1024);

        // Flush without adding any samples
        let result = resampler.flush();
        assert!(result.is_none());
    }

    /// Test that flush processes partial buffers by padding with silence
    #[test_log::test]
    fn test_flush_partial_buffer() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // Add partial buffer (less than duration)
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer.render_uninit(Some(512));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
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
        assert_ne!(output, []);
        assert_eq!(output.len() % 2, 0);
    }

    /// Test that flush handles exact multiples of duration correctly
    #[test_log::test]
    fn test_flush_exact_multiple() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // Add exactly duration samples
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1024);
        input_buffer.render_uninit(Some(1024));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
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
        let spec = AudioSpec::new(48000, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());

        // Create interleaved stereo samples: [L0, R0, L1, R1, L2, R2, ...]
        let samples: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];

        let audio_buffer = to_audio_buffer(&samples, spec).unwrap();

        // Check buffer size
        assert_eq!(audio_buffer.frames(), 4);

        // Check that samples were correctly de-interleaved
        let left = audio_buffer.plane(0).unwrap();
        let right = audio_buffer.plane(1).unwrap();

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
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // Create input buffer
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1024);
        input_buffer.render_uninit(Some(1024));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 1024.0;
            }
        }

        // Should return Some with AudioBuffer
        let result = resampler.resample_to_audio_buffer(&input_buffer).unwrap();
        assert!(result.is_some());

        let output_buffer = result.unwrap();
        assert!(output_buffer.frames() > 0);
        assert_eq!(output_buffer.spec().channels().count(), 2);
    }

    /// Test that resampler handles different sample rates correctly
    #[test_log::test]
    fn test_resample_rate_change() {
        let input_rate = 44100;
        let output_rate = 48000;
        let spec = AudioSpec::new(
            input_rate,
            (Position::FRONT_LEFT | Position::FRONT_RIGHT).into(),
        );
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), output_rate, 1024);

        // Create input buffer
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1024);
        input_buffer.render_uninit(Some(1024));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
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
        assert_ne!(output, []);
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
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec, 1024);
        input_buffer.render_uninit(Some(1024));
        let channel = input_buffer.plane_mut(0).unwrap();
        for (i, sample) in channel.iter_mut().enumerate() {
            *sample = (i as f32) / 1024.0;
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        // Mono output should have exactly as many samples as frames
        assert_ne!(output, []);
    }

    /// Test downsampling (higher to lower sample rate)
    #[test_log::test]
    fn test_resample_downsampling() {
        let input_rate = 48000;
        let output_rate = 44100;
        let spec = AudioSpec::new(
            input_rate,
            (Position::FRONT_LEFT | Position::FRONT_RIGHT).into(),
        );
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), output_rate, 1024);

        // Create input buffer with a simple sine wave pattern
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1024);
        input_buffer.render_uninit(Some(1024));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
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
        assert_ne!(output, []);
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
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // Perform multiple resample cycles
        for cycle in 0..3 {
            let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1024);
            input_buffer.render_uninit(Some(1024));
            for ch in 0..spec.channels().count() {
                let channel = input_buffer.plane_mut(ch).unwrap();
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
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // Add 300 samples
        let mut input_buffer1: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 300);
        input_buffer1.render_uninit(Some(300));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer1.plane_mut(ch).unwrap();
            for sample in channel.iter_mut() {
                *sample = 0.3;
            }
        }
        assert!(resampler.resample(&input_buffer1).is_none());

        // Add another 200 samples (total 500, still less than 1024)
        let mut input_buffer2: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 200);
        input_buffer2.render_uninit(Some(200));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer2.plane_mut(ch).unwrap();
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }
        assert!(resampler.resample(&input_buffer2).is_none());

        // Add another 100 samples (total 600, still less than 1024)
        let mut input_buffer3: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 100);
        input_buffer3.render_uninit(Some(100));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer3.plane_mut(ch).unwrap();
            for sample in channel.iter_mut() {
                *sample = 0.7;
            }
        }
        assert!(resampler.resample(&input_buffer3).is_none());

        // Flush should process all accumulated samples (600 total) padded to 1024
        let result = resampler.flush();
        assert!(result.is_some());
        let output = result.unwrap();
        assert_ne!(output, []);
        assert_eq!(output.len() % 2, 0);
    }

    /// Test resampling with more than duration samples produces output and keeps remainder
    #[test_log::test]
    fn test_resample_excess_samples() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        // Add 1500 samples (more than duration of 1024)
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 1500);
        input_buffer.render_uninit(Some(1500));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 1500.0;
            }
        }

        // First resample should consume 1024 samples
        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        // There should be 476 samples remaining (1500 - 1024)
        // Adding another 548 samples should trigger another output (476 + 548 = 1024)
        let mut input_buffer2: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 548);
        input_buffer2.render_uninit(Some(548));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer2.plane_mut(ch).unwrap();
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
        let spec = AudioSpec::new(48000, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let samples: Vec<f32> = vec![];

        let audio_buffer = to_audio_buffer(&samples, spec).unwrap();
        assert_eq!(audio_buffer.frames(), 0);
    }

    /// Test extreme sample rate conversion (large ratio)
    #[test_log::test]
    fn test_resample_large_rate_ratio() {
        // Test 8kHz to 48kHz (6x upsampling)
        let input_rate = 8000;
        let output_rate = 48000;
        let spec = AudioSpec::new(
            input_rate,
            (Position::FRONT_LEFT | Position::FRONT_RIGHT).into(),
        );
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), output_rate, 512);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer.render_uninit(Some(512));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = ((i as f32) * std::f32::consts::PI / 64.0).sin();
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        assert_ne!(output, []);
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
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer.render_uninit(Some(512));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
            for sample in channel.iter_mut() {
                *sample = 0.5;
            }
        }

        let result = resampler.resample_to_audio_buffer(&input_buffer).unwrap();
        assert!(result.is_none());
    }

    /// Test flush when buffer contains exactly duration samples (not consumed by resample)
    ///
    /// This tests the case in flush() where len % duration == 0 but len > 0,
    /// meaning no padding is needed but samples still need processing.
    #[test_log::test]
    fn test_flush_exact_duration_not_consumed() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let duration = 256;
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, duration);

        // Add exactly duration samples via two smaller buffers
        // but don't call resample() enough times to consume them
        let mut input_buffer1: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 128);
        input_buffer1.render_uninit(Some(128));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer1.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = (i as f32) / 128.0;
            }
        }
        // Returns None - only 128 samples buffered
        assert!(resampler.resample(&input_buffer1).is_none());

        let mut input_buffer2: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 128);
        input_buffer2.render_uninit(Some(128));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer2.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = 0.5 + (i as f32) / 256.0;
            }
        }
        // Returns None - 256 samples buffered but we want to test flush instead
        // Note: resample would return Some here, but we skip it to test flush directly
        // by NOT calling resample again

        // Actually, let's add just under duration to test the exact boundary
        // Reset and try again with exact buffer
        let mut resampler2: Resampler<f32> = Resampler::new(spec.clone(), 48000, duration);

        // Add exactly 256 samples at once
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 256);
        input_buffer.render_uninit(Some(256));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = ((i as f32) * std::f32::consts::PI / 32.0).sin() * 0.8;
            }
        }

        // This returns Some and consumes the samples
        let result = resampler2.resample(&input_buffer);
        assert!(result.is_some());

        // Now test with 2x duration samples where we only consume once
        let mut resampler3: Resampler<f32> = Resampler::new(spec.clone(), 48000, duration);

        // Add 2x duration samples
        let mut large_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        large_buffer.render_uninit(Some(512));
        for ch in 0..spec.channels().count() {
            let channel = large_buffer.plane_mut(ch).unwrap();
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
        assert_ne!(output, []);
        assert_eq!(output.len() % 2, 0, "Output should be stereo");
    }

    /// Test resampling with i16 sample type to verify generic type conversion works
    ///
    /// The resampler internally works with f32 and converts to the output type T.
    /// This test verifies the conversion path for integer sample types works correctly.
    #[test_log::test]
    fn test_resample_i16_output() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<i16> = Resampler::new(spec.clone(), 48000, 512);

        // Create input buffer with a simple pattern
        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer.render_uninit(Some(512));
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
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

    /// Test resampling with quad (4-channel) audio to verify multi-channel interleaving
    ///
    /// The resample_inner() function interleaves output channels in a loop. This test
    /// verifies the interleaving logic works correctly for more than 2 channels.
    /// All channel counts are supported by both output representations.
    #[test_log::test]
    fn test_resample_quad_channel() {
        // Create a 4-channel (quadraphonic) spec
        let spec = AudioSpec::new(
            44100,
            (Position::FRONT_LEFT
                | Position::FRONT_RIGHT
                | Position::REAR_LEFT
                | Position::REAR_RIGHT)
                .into(),
        );
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 512);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer.render_uninit(Some(512));

        // Fill each channel with a distinct pattern so we can verify interleaving
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                // Each channel gets a different base value plus a pattern
                let base = (ch as f32) * 0.2;
                *sample = base + ((i as f32) * std::f32::consts::PI / 64.0).sin() * 0.1;
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some(), "Should produce output");

        let output = result.unwrap();
        assert!(!output.is_empty(), "Output should not be empty");

        // Output should be interleaved as groups of 4 samples
        assert_eq!(
            output.len() % 4,
            0,
            "Output length should be divisible by channel count (4)"
        );

        // Verify output frames count is positive (actual count depends on FFT internals)
        let output_frames = output.len() / 4;
        assert!(
            output_frames > 0,
            "Expected positive frame count, got {output_frames}"
        );
    }

    /// Test that resampling preserves relative signal characteristics between channels
    ///
    /// This verifies that when different channels have different amplitude signals,
    /// the resampler maintains those differences in the output (testing interleaving
    /// doesn't mix up channels).
    #[test_log::test]
    fn test_resample_channel_separation() {
        let spec = AudioSpec::new(44100, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 1024);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec, 1024);
        input_buffer.render_uninit(Some(1024));

        // Left channel: high amplitude signal (0.9)
        let left = input_buffer.plane_mut(0).unwrap();
        for (i, sample) in left.iter_mut().enumerate() {
            *sample = ((i as f32) * std::f32::consts::PI / 64.0).sin() * 0.9;
        }

        // Right channel: low amplitude signal (0.1)
        let right = input_buffer.plane_mut(1).unwrap();
        for (i, sample) in right.iter_mut().enumerate() {
            *sample = ((i as f32) * std::f32::consts::PI / 64.0).sin() * 0.1;
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();

        // Calculate average absolute values for left and right channels
        let mut left_sum: f32 = 0.0;
        let mut right_sum: f32 = 0.0;
        let frame_count = output.len() / 2;

        for frame in output.as_chunks::<2>().0 {
            left_sum += frame[0].abs();
            right_sum += frame[1].abs();
        }

        let left_avg = left_sum / frame_count as f32;
        let right_avg = right_sum / frame_count as f32;

        // Left channel should have significantly higher average amplitude than right
        // (original ratio was 9:1, allowing for some variance from resampling)
        assert!(
            left_avg > right_avg * 3.0,
            "Left channel should have higher amplitude: left_avg={left_avg}, right_avg={right_avg}"
        );
    }

    /// Test resampling with same input and output sample rate (1:1 ratio)
    ///
    /// Even when no rate conversion is needed, the resampler should still work
    /// correctly, passing through samples with minimal modification.
    #[test_log::test]
    fn test_resample_same_rate() {
        let spec = AudioSpec::new(48000, (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let mut resampler: Resampler<f32> = Resampler::new(spec.clone(), 48000, 512);

        let mut input_buffer: AudioBuffer<f32> = AudioBuffer::new(spec.clone(), 512);
        input_buffer.render_uninit(Some(512));

        // Fill with a known pattern
        for ch in 0..spec.channels().count() {
            let channel = input_buffer.plane_mut(ch).unwrap();
            for (i, sample) in channel.iter_mut().enumerate() {
                *sample = ((i as f32) * std::f32::consts::PI / 64.0).sin() * 0.8;
            }
        }

        let result = resampler.resample(&input_buffer);
        assert!(result.is_some());

        let output = result.unwrap();
        assert_ne!(output, []);

        // With 1:1 ratio, output frames should be very close to input frames
        let output_frames = output.len() / 2;
        assert!(
            (output_frames as i32 - 512).abs() < 10,
            "1:1 ratio should produce similar frame count: got {output_frames} from 512 input"
        );

        // Verify output has similar characteristics to input (signal preserved)
        let has_positive = output.iter().any(|&s| s > 0.5);
        let has_negative = output.iter().any(|&s| s < -0.5);
        assert!(
            has_positive && has_negative,
            "Signal should be preserved through 1:1 resampling"
        );
    }
}
