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
    sample_rate: SampleRate,
    #[allow(dead_code)]
    channels: Channels,

    #[cfg(feature = "silk")]
    silk: silk::SilkDecoder,

    #[cfg(feature = "celt")]
    celt: celt::CeltDecoder,

    #[allow(dead_code)]
    prev_mode: Option<OpusMode>,

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
    /// Returns an error if sub-decoder initialization fails.
    pub fn new(sample_rate: SampleRate, channels: Channels) -> Result<Self> {
        Ok(Self {
            sample_rate,
            channels,

            #[cfg(feature = "silk")]
            silk: silk::SilkDecoder::new(
                SampleRate::Hz16000, // Default WB rate (will be reconfigured per packet)
                channels,
                20, // Default frame size (will be updated per packet)
            )?,

            #[cfg(feature = "celt")]
            celt: celt::CeltDecoder::new(
                SampleRate::Hz48000, // CELT always operates at 48kHz internally
                channels,
                480, // Default: 10ms @ 48kHz (will be updated per packet)
            )?,

            prev_mode: None,

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

    /// Calculate samples for given frame size and rate
    ///
    /// # Arguments
    ///
    /// * `frame_size` - Frame duration
    /// * `sample_rate` - Sample rate in Hz
    ///
    /// # Returns
    ///
    /// Number of samples per channel
    #[must_use]
    const fn calculate_samples(frame_size: FrameSize, sample_rate: u32) -> usize {
        let duration_tenths_ms = match frame_size {
            FrameSize::Ms2_5 => 25,
            FrameSize::Ms5 => 50,
            FrameSize::Ms10 => 100,
            FrameSize::Ms20 => 200,
            FrameSize::Ms40 => 400,
            FrameSize::Ms60 => 600,
        };

        ((sample_rate * duration_tenths_ms) / 10000) as usize
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

    /// Decode SILK-only frame
    ///
    /// # RFC Reference
    /// * Lines 455-466: SILK-only overview
    /// * Lines 494-496: Internal sample rates (NB=8k, MB=12k, WB=16k)
    /// * Table 2 configs 0-11
    ///
    /// # Arguments
    /// * `frame_data` - Frame payload (complete frame)
    /// * `config` - Configuration from TOC (configs 0-11)
    /// * `channels` - Mono or stereo
    /// * `output` - Output buffer for PCM at decoder rate
    ///
    /// # Returns
    /// Number of samples written per channel
    ///
    /// # Errors
    /// * Returns error if SILK decode fails
    /// * Returns error if bandwidth invalid for SILK-only
    /// * Returns error if resampling fails
    #[cfg(feature = "silk")]
    #[allow(dead_code)]
    fn decode_silk_only(
        &mut self,
        frame_data: &[u8],
        config: Configuration,
        channels: Channels,
        output: &mut [i16],
    ) -> Result<usize> {
        use crate::range::RangeDecoder;

        let mut ec = RangeDecoder::new(frame_data)?;

        let internal_rate = match config.bandwidth {
            Bandwidth::Narrowband => 8000,
            Bandwidth::Mediumband => 12000,
            Bandwidth::Wideband => 16000,
            _ => {
                return Err(Error::DecodeFailed(format!(
                    "SILK-only supports NB/MB/WB only, got {:?}",
                    config.bandwidth
                )));
            }
        };

        let internal_samples = Self::calculate_samples(config.frame_size, internal_rate);
        let sample_count_with_channels = internal_samples * channels as usize;
        let mut silk_buffer = vec![0i16; sample_count_with_channels];

        let decoded = self
            .silk
            .decode_silk_frame(&mut ec, false, &mut silk_buffer)?;

        if decoded != internal_samples {
            return Err(Error::DecodeFailed(format!(
                "SILK sample count mismatch: expected {internal_samples}, got {decoded}"
            )));
        }

        let target_rate = self.sample_rate as u32;
        #[cfg(feature = "resampling")]
        if internal_rate != target_rate {
            let resampled =
                self.resample_silk(&silk_buffer, internal_rate, target_rate, channels)?;

            let target_samples = Self::calculate_samples(config.frame_size, target_rate);

            let copy_len = resampled.len().min(output.len());
            output[..copy_len].copy_from_slice(&resampled[..copy_len]);

            return Ok(target_samples);
        }

        #[cfg(not(feature = "resampling"))]
        if internal_rate != target_rate {
            return Err(Error::InvalidSampleRate(format!(
                "Resampling not available: SILK internal rate {internal_rate} != target rate {target_rate}"
            )));
        }

        let copy_len = silk_buffer.len().min(output.len());
        output[..copy_len].copy_from_slice(&silk_buffer[..copy_len]);
        Ok(internal_samples)
    }

    /// Decode CELT-only frame
    ///
    /// # RFC Reference
    /// * Lines 468-479: CELT-only overview
    /// * Line 498: "CELT operates at 48 kHz internally"
    /// * Table 2 configs 16-31
    ///
    /// # Arguments
    /// * `frame_data` - Frame payload
    /// * `config` - Configuration from TOC (configs 16-31)
    /// * `channels` - Mono or stereo
    /// * `output` - Output buffer for PCM at decoder rate
    ///
    /// # Returns
    /// Number of samples written per channel
    ///
    /// # Errors
    /// * Returns error if CELT decode fails
    /// * Returns error if decimation fails
    #[cfg(feature = "celt")]
    #[allow(dead_code)]
    fn decode_celt_only(
        &mut self,
        frame_data: &[u8],
        _config: Configuration,
        channels: Channels,
        output: &mut [i16],
    ) -> Result<usize> {
        use crate::celt::CELT_NUM_BANDS;
        use crate::range::RangeDecoder;

        let mut ec = RangeDecoder::new(frame_data)?;

        self.celt.set_start_band(0);
        self.celt.set_end_band(CELT_NUM_BANDS);
        self.celt.set_output_rate(self.sample_rate)?;

        let decoded_frame = self.celt.decode_celt_frame(&mut ec, frame_data.len())?;

        if decoded_frame.channels != channels {
            return Err(Error::DecodeFailed(format!(
                "CELT channel mismatch: expected {channels:?}, got {:?}",
                decoded_frame.channels
            )));
        }

        for (i, &sample) in decoded_frame.samples.iter().enumerate() {
            if i < output.len() {
                #[allow(clippy::cast_possible_truncation)]
                let sample_i16 = (sample.clamp(-1.0, 1.0) * 32768.0) as i16;
                output[i] = sample_i16;
            }
        }

        let samples_per_channel = decoded_frame.samples.len() / channels as usize;
        Ok(samples_per_channel)
    }

    /// Decode hybrid mode frame (SILK low-freq + CELT high-freq)
    ///
    /// # RFC Reference
    /// * Lines 481-487: Hybrid overview
    /// * Lines 522-526: "Both layers use the same entropy coder"
    /// * Lines 1749-1750: "In a Hybrid frame, SILK operates in WB"
    /// * Line 5804: "first 17 bands (up to 8 kHz) are not coded"
    ///
    /// # Critical Algorithm
    /// 1. SILK decodes first using range decoder
    /// 2. CELT continues with SAME range decoder (shared state!)
    /// 3. CELT skips bands 0-16 (`start_band=17`, RFC 5804)
    /// 4. Both outputs resampled to target, then summed
    ///
    /// # Arguments
    /// * `frame_data` - Complete frame payload (NOT pre-split!)
    /// * `config` - Configuration from TOC (configs 12-15)
    /// * `channels` - Mono or stereo
    /// * `output` - Output buffer for final PCM
    ///
    /// # Returns
    /// Number of samples written per channel
    ///
    /// # Errors
    /// * Returns error if SILK or CELT decode fails
    /// * Returns error if sample rate conversion fails
    #[cfg(all(feature = "silk", feature = "celt"))]
    #[allow(dead_code)]
    fn decode_hybrid(
        &mut self,
        frame_data: &[u8],
        config: Configuration,
        channels: Channels,
        output: &mut [i16],
    ) -> Result<usize> {
        use crate::celt::CELT_NUM_BANDS;
        use crate::range::RangeDecoder;

        const HYBRID_SILK_INTERNAL_RATE: u32 = 16000;
        const HYBRID_START_BAND: usize = 17;

        let mut ec = RangeDecoder::new(frame_data)?;

        let silk_samples_16k =
            Self::calculate_samples(config.frame_size, HYBRID_SILK_INTERNAL_RATE);
        let sample_count_with_channels = silk_samples_16k * channels as usize;
        let mut silk_16k = vec![0i16; sample_count_with_channels];

        let silk_decoded = self.silk.decode_silk_frame(&mut ec, false, &mut silk_16k)?;

        if silk_decoded != silk_samples_16k {
            return Err(Error::DecodeFailed(format!(
                "Hybrid SILK sample count mismatch: expected {silk_samples_16k}, got {silk_decoded}"
            )));
        }

        self.celt.set_start_band(HYBRID_START_BAND);
        self.celt.set_end_band(CELT_NUM_BANDS);

        let target_rate = self.sample_rate as u32;
        self.celt.set_output_rate(self.sample_rate)?;

        let decoded_frame = self.celt.decode_celt_frame(&mut ec, frame_data.len())?;

        if decoded_frame.channels != channels {
            return Err(Error::DecodeFailed(format!(
                "Hybrid CELT channel mismatch: expected {channels:?}, got {:?}",
                decoded_frame.channels
            )));
        }

        let target_samples = Self::calculate_samples(config.frame_size, target_rate);

        #[cfg(feature = "resampling")]
        let silk_target =
            self.resample_silk(&silk_16k, HYBRID_SILK_INTERNAL_RATE, target_rate, channels)?;

        #[cfg(not(feature = "resampling"))]
        let silk_target = if HYBRID_SILK_INTERNAL_RATE == target_rate {
            silk_16k.clone()
        } else {
            return Err(Error::InvalidSampleRate(format!(
                "Resampling not available: SILK rate {HYBRID_SILK_INTERNAL_RATE} != target rate {target_rate}"
            )));
        };

        let celt_i16: Vec<i16> = decoded_frame
            .samples
            .iter()
            .map(|&s| {
                #[allow(clippy::cast_possible_truncation)]
                let sample_i16 = (s.clamp(-1.0, 1.0) * 32768.0) as i16;
                sample_i16
            })
            .collect();

        let sample_count = target_samples * channels as usize;
        for i in 0..sample_count.min(output.len()) {
            let silk_sample = silk_target.get(i).copied().unwrap_or(0);
            let celt_sample = celt_i16.get(i).copied().unwrap_or(0);
            output[i] = silk_sample.saturating_add(celt_sample);
        }

        Ok(target_samples)
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
