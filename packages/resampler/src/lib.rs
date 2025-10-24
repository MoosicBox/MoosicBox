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
    /// Returns the resampled samples in an interleaved format.
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
    /// Returns the resampled samples as an `AudioBuffer`.
    pub fn resample_to_audio_buffer(&mut self, input: &AudioBuffer<f32>) -> Option<AudioBuffer<T>> {
        let spec = self.spec;
        self.resample(input)
            .map(|samples| to_audio_buffer(samples, spec))
    }

    /// Resample any remaining samples in the resample buffer.
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
