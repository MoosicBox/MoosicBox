#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "celt")]
pub mod celt;
pub mod error;
pub mod framing;
pub mod range;
#[cfg(feature = "silk")]
pub mod silk;
pub mod toc;
mod util;

pub use error::{Error, Result};
pub use toc::{Bandwidth, Configuration, FrameSize, OpusMode, Toc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Mono = 1,
    Stereo = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    Hz8000 = 8000,
    Hz12000 = 12000,
    Hz16000 = 16000,
    Hz24000 = 24000,
    Hz48000 = 48000,
}

impl SampleRate {
    /// Convert Hz value to `SampleRate` enum
    ///
    /// # Errors
    /// Returns error if rate not supported (must be 8/12/16/24/48 kHz)
    pub fn from_hz(hz: u32) -> Result<Self> {
        match hz {
            8000 => Ok(Self::Hz8000),
            12000 => Ok(Self::Hz12000),
            16000 => Ok(Self::Hz16000),
            24000 => Ok(Self::Hz24000),
            48000 => Ok(Self::Hz48000),
            _ => Err(Error::InvalidSampleRate(format!(
                "Unsupported sample rate: {hz} Hz (must be 8000/12000/16000/24000/48000)"
            ))),
        }
    }
}

pub struct Decoder {
    #[allow(dead_code)]
    sample_rate: SampleRate,
    #[allow(dead_code)]
    channels: Channels,

    #[cfg(all(feature = "silk", feature = "resampling"))]
    silk_resampler_state: Option<moosicbox_resampler::Resampler<i16>>,
    #[cfg(all(feature = "silk", feature = "resampling"))]
    silk_resampler_input_rate: u32,
    #[cfg(all(feature = "silk", feature = "resampling"))]
    silk_resampler_output_rate: u32,
    #[cfg(all(feature = "silk", feature = "resampling"))]
    silk_resampler_required_delay_ms: f32,
}

impl Decoder {
    /// Creates a new Opus decoder.
    ///
    /// # Errors
    ///
    /// Returns an error if decoder initialization fails (not yet implemented).
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(sample_rate: SampleRate, channels: Channels) -> Result<Self> {
        Ok(Self {
            sample_rate,
            channels,
            #[cfg(all(feature = "silk", feature = "resampling"))]
            silk_resampler_state: None,
            #[cfg(all(feature = "silk", feature = "resampling"))]
            silk_resampler_input_rate: 0,
            #[cfg(all(feature = "silk", feature = "resampling"))]
            silk_resampler_output_rate: 0,
            #[cfg(all(feature = "silk", feature = "resampling"))]
            silk_resampler_required_delay_ms: 0.0,
        })
    }

    /// Decodes an Opus packet to signed 16-bit PCM.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding fails (not yet implemented - will be implemented in Phase 6).
    pub fn decode(&mut self, input: Option<&[u8]>, output: &mut [i16], fec: bool) -> Result<usize> {
        let _ = (self, input, output, fec);
        todo!("Implement in Phase 6")
    }

    /// Decodes an Opus packet to floating point PCM.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding fails (not yet implemented - will be implemented in Phase 6).
    pub fn decode_float(
        &mut self,
        input: Option<&[u8]>,
        output: &mut [f32],
        fec: bool,
    ) -> Result<usize> {
        let _ = (self, input, output, fec);
        todo!("Implement in Phase 6")
    }

    /// Resets the decoder state.
    ///
    /// # Errors
    ///
    /// Returns an error if reset fails (not yet implemented - will be implemented in Phase 6).
    pub fn reset_state(&mut self) -> Result<()> {
        let _ = self;
        todo!("Implement in Phase 6")
    }

    /// Resample SILK output to target rate
    ///
    /// # RFC Reference
    /// * Lines 5724-5795: SILK resampling (normative delays only)
    /// * Lines 5766-5775: Table 54 - Resampler delay values (NORMATIVE)
    /// * Lines 5736-5738: "this delay is normative"
    /// * Lines 5757-5762: Allows non-integer delays, some tolerance acceptable
    ///
    /// # Arguments
    /// * `input` - SILK output at internal rate (i16 samples, interleaved)
    /// * `input_rate` - Internal SILK rate (8000/12000/16000 Hz)
    /// * `output_rate` - Target decoder rate
    /// * `channels` - Number of channels
    ///
    /// # Returns
    /// Resampled i16 samples at `output_rate` (interleaved)
    ///
    /// # Errors
    /// * Returns error if `input_rate` invalid
    /// * Returns error if resampling fails
    #[cfg(all(feature = "silk", feature = "resampling"))]
    #[allow(dead_code)]
    fn resample_silk(
        &mut self,
        input: &[i16],
        input_rate: u32,
        output_rate: u32,
        channels: Channels,
    ) -> Result<Vec<i16>> {
        use symphonia::core::audio::{AudioBuffer, Signal, SignalSpec};

        if input_rate == output_rate {
            return Ok(input.to_vec());
        }

        let required_delay_ms = match input_rate {
            8000 => 0.538,
            12000 => 0.692,
            16000 => 0.706,
            _ => {
                return Err(Error::InvalidSampleRate(format!(
                    "Invalid SILK internal rate: {input_rate} (must be 8000/12000/16000)"
                )));
            }
        };

        let num_channels = match channels {
            Channels::Mono => symphonia::core::audio::Channels::FRONT_LEFT,
            Channels::Stereo => {
                symphonia::core::audio::Channels::FRONT_LEFT
                    | symphonia::core::audio::Channels::FRONT_RIGHT
            }
        };

        if self.silk_resampler_state.is_none()
            || self.silk_resampler_input_rate != input_rate
            || self.silk_resampler_output_rate != output_rate
        {
            let num_samples = input.len() / channels as usize;
            let spec = SignalSpec::new(input_rate, num_channels);

            let resampler = moosicbox_resampler::Resampler::<i16>::new(
                spec,
                output_rate as usize,
                num_samples as u64,
            );

            self.silk_resampler_state = Some(resampler);
            self.silk_resampler_input_rate = input_rate;
            self.silk_resampler_output_rate = output_rate;
            self.silk_resampler_required_delay_ms = required_delay_ms;
        }

        let num_samples = input.len() / channels as usize;
        let mut audio_buffer = AudioBuffer::<f32>::new(
            num_samples as u64,
            SignalSpec::new(input_rate, num_channels),
        );

        for ch in 0..channels as usize {
            for sample_idx in 0..num_samples {
                let interleaved_idx = sample_idx * channels as usize + ch;
                #[allow(clippy::cast_precision_loss)]
                let sample_f32 = f32::from(input[interleaved_idx]) / 32768.0;
                audio_buffer.chan_mut(ch)[sample_idx] = sample_f32;
            }
        }

        let resampler = self
            .silk_resampler_state
            .as_mut()
            .ok_or_else(|| Error::DecodeFailed("Resampler not initialized".into()))?;

        let resampled_i16 = resampler
            .resample(&audio_buffer)
            .ok_or_else(|| Error::DecodeFailed("Resampling produced no output".into()))?;

        Ok(resampled_i16.to_vec())
    }
}
