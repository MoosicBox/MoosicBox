#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

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

/// Opus decoder supporting SILK, CELT, and Hybrid modes.
///
/// # Feature Flags
///
/// This decoder supports optional compilation with no decoding features enabled
/// (`--no-default-features`). In this configuration:
/// * `Decoder::new()` - ✅ Succeeds (creates minimal decoder)
/// * `Decoder::decode()` - ❌ Returns `Error::UnsupportedMode`
/// * `Decoder::reset_state()` - ✅ Succeeds (no-op)
///
/// This is useful for minimal binaries that only need packet inspection,
/// or for verifying that feature-gating is correctly implemented.
///
/// To enable decoding, use at least one of:
/// * `silk` - SILK codec (NB/MB/WB)
/// * `celt` - CELT codec (NB/WB/SWB/FB)
/// * `hybrid` - Hybrid mode (implies both `silk` and `celt`)
pub struct Decoder {
    #[cfg_attr(not(any(feature = "silk", feature = "celt")), allow(dead_code))]
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

/// SILK header flags structure
///
/// Contains VAD flags, LBRR flags, and per-frame LBRR flags decoded from SILK header
#[cfg(feature = "silk")]
#[derive(Debug)]
struct SilkHeaderFlags {
    vad_flags: Vec<bool>,
    lbrr_flags: Vec<bool>,
    per_frame_lbrr: Vec<Vec<bool>>,
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
    /// # RFC Reference
    /// * Section 3.1: TOC byte parsing (lines 712-836)
    /// * Section 3.2: Frame packing (lines 838-1169)
    /// * Section 4: Decoding (lines 1257-1280)
    /// * R1: Packet must be ≥1 byte (line 714)
    ///
    /// # Arguments
    /// * `input` - Optional input packet (None = packet loss)
    /// * `output` - Output buffer for decoded PCM
    /// * `fec` - Forward Error Correction flag
    ///
    /// # Returns
    /// Number of samples decoded per channel
    ///
    /// # Errors
    /// * `Error::InvalidPacket` - Packet violates RFC R1-R7
    /// * `Error::UnsupportedMode` - Mode not enabled via features
    /// * `Error::DecodeFailed` - Decoder error
    #[allow(clippy::too_many_lines)]
    pub fn decode(&mut self, input: Option<&[u8]>, output: &mut [i16], fec: bool) -> Result<usize> {
        let Some(packet) = input else {
            return Ok(self.handle_packet_loss(output, fec));
        };

        if packet.is_empty() {
            return Err(Error::InvalidPacket("Packet must be ≥1 byte (R1)".into()));
        }

        #[cfg(not(any(feature = "silk", feature = "celt")))]
        {
            Err(Error::UnsupportedMode(
                "No decoding features enabled. Enable at least one of: 'silk', 'celt'".into(),
            ))
        }

        #[cfg(any(feature = "silk", feature = "celt"))]
        {
            let toc = toc::Toc::parse(packet[0]);
            let config = toc.configuration();

            if toc.channels() != self.channels {
                return Err(Error::InvalidPacket(format!(
                    "Channel mismatch: packet={:?}, decoder={:?}",
                    toc.channels(),
                    self.channels
                )));
            }

            let frames = framing::parse_frames(packet)?;

            // Mode changed - reset decoder state
            if let Some(prev) = self.prev_mode
                && prev != config.mode
            {
                let curr = config.mode;

                #[cfg(feature = "silk")]
                if prev == toc::OpusMode::CeltOnly
                    && (curr == toc::OpusMode::SilkOnly || curr == toc::OpusMode::Hybrid)
                {
                    self.silk.reset_decoder_state();

                    #[cfg(feature = "resampling")]
                    {
                        self.silk_resampler_state = None;
                    }
                }

                #[cfg(feature = "celt")]
                if curr == toc::OpusMode::CeltOnly || curr == toc::OpusMode::Hybrid {
                    self.celt.reset();
                }
            }

            let samples_per_frame =
                Self::calculate_samples(config.frame_size, self.sample_rate as u32);
            let total_samples = samples_per_frame * frames.len();
            let buffer_capacity = output.len() / self.channels as usize;

            if total_samples > buffer_capacity {
                return Err(Error::InvalidPacket(format!(
                    "Output buffer too small: {frames} frames × {samples_per_frame} samples/frame = {total_samples} total samples, buffer capacity {buffer_capacity} samples/channel",
                    frames = frames.len(),
                )));
            }

            let mut current_output_offset = 0;

            for (frame_idx, frame_data) in frames.iter().enumerate() {
                let frame_output_start = current_output_offset * self.channels as usize;
                let frame_output_end =
                    (current_output_offset + samples_per_frame) * self.channels as usize;
                let frame_output = &mut output[frame_output_start..frame_output_end];

                let samples = match config.mode {
                    #[cfg(feature = "silk")]
                    toc::OpusMode::SilkOnly => {
                        self.decode_silk_only(frame_data, config, toc.channels(), frame_output)?
                    }

                    #[cfg(feature = "celt")]
                    toc::OpusMode::CeltOnly => {
                        self.decode_celt_only(frame_data, config, toc.channels(), frame_output)?
                    }

                    #[cfg(all(feature = "silk", feature = "celt"))]
                    toc::OpusMode::Hybrid => {
                        self.decode_hybrid(frame_data, config, toc.channels(), frame_output)?
                    }

                    #[cfg(not(feature = "silk"))]
                    toc::OpusMode::SilkOnly => {
                        return Err(Error::UnsupportedMode(
                            "SILK mode requires 'silk' feature".into(),
                        ));
                    }

                    #[cfg(not(feature = "celt"))]
                    toc::OpusMode::CeltOnly => {
                        return Err(Error::UnsupportedMode(
                            "CELT mode requires 'celt' feature".into(),
                        ));
                    }

                    #[cfg(not(all(feature = "silk", feature = "celt")))]
                    toc::OpusMode::Hybrid => {
                        return Err(Error::UnsupportedMode(
                            "Hybrid mode requires both 'silk' and 'celt' features".into(),
                        ));
                    }
                };

                if samples != samples_per_frame {
                    return Err(Error::DecodeFailed(format!(
                        "Frame {frame_idx} sample count mismatch: expected {samples_per_frame}, got {samples}"
                    )));
                }

                current_output_offset += samples;
            }

            self.prev_mode = Some(config.mode);

            Ok(total_samples)
        }
    }

    /// Handle packet loss
    ///
    /// Returns silence for now. Phase 6 will implement proper PLC.
    ///
    /// # Arguments
    /// * `output` - Output buffer to fill with concealed samples
    /// * `_fec` - FEC flag (unused in Phase 5)
    ///
    /// # Returns
    /// Number of samples written per channel
    fn handle_packet_loss(&self, output: &mut [i16], _fec: bool) -> usize {
        for sample in output.iter_mut() {
            *sample = 0;
        }
        output.len() / self.channels as usize
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
    #[cfg_attr(not(any(feature = "silk", feature = "celt")), allow(dead_code))]
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

    /// Decode SILK header flags (VAD + LBRR)
    ///
    /// # RFC Reference
    /// * Lines 1867-1870: Header structure (VAD + LBRR)
    /// * Lines 1953-1958: Header bits description
    /// * Figure 15: Mono frame structure
    /// * Figure 16: Stereo frame structure
    ///
    /// # Arguments
    /// * `range_decoder` - Range decoder positioned at start of frame
    /// * `frame_size` - Frame duration (determines number of SILK frames)
    /// * `channels` - Mono or stereo
    ///
    /// # Returns
    /// Complete header flags including VAD, LBRR, and per-frame LBRR
    ///
    /// # Errors
    /// * Returns error if range decoding fails
    #[cfg(feature = "silk")]
    fn decode_silk_header_flags(
        range_decoder: &mut range::RangeDecoder,
        frame_size: FrameSize,
        channels: Channels,
    ) -> Result<SilkHeaderFlags> {
        let num_silk_frames = match frame_size.to_ms() {
            10 | 20 => 1,
            40 => 2,
            60 => 3,
            _ => return Err(Error::DecodeFailed("Invalid SILK frame size".into())),
        };

        let num_channels = channels as usize;
        let mut vad_flags = Vec::with_capacity(num_silk_frames * num_channels);
        let mut lbrr_flags = Vec::with_capacity(num_channels);
        let mut per_frame_lbrr = Vec::new();

        for _ in 0..num_channels {
            for _ in 0..num_silk_frames {
                let vad = range_decoder.ec_dec_bit_logp(1)?;
                vad_flags.push(vad);
            }

            let lbrr = range_decoder.ec_dec_bit_logp(1)?;
            lbrr_flags.push(lbrr);
        }

        for &lbrr_flag in &lbrr_flags {
            if lbrr_flag {
                let per_frame = Self::decode_per_frame_lbrr_flags(range_decoder, frame_size)?;
                per_frame_lbrr.push(per_frame);
            } else {
                per_frame_lbrr.push(Vec::new());
            }
        }

        Ok(SilkHeaderFlags {
            vad_flags,
            lbrr_flags,
            per_frame_lbrr,
        })
    }

    /// Decode per-frame LBRR flags
    ///
    /// # RFC Reference
    /// * Lines 1974-1998: Per-frame LBRR flags
    /// * Table 4: LBRR flag PDFs
    ///
    /// # Errors
    /// * Returns error if range decoding fails
    #[cfg(feature = "silk")]
    fn decode_per_frame_lbrr_flags(
        range_decoder: &mut range::RangeDecoder,
        frame_size: FrameSize,
    ) -> Result<Vec<bool>> {
        match frame_size.to_ms() {
            10 | 20 => Ok(vec![true]),
            40 => {
                const LBRR_40MS_ICDF: &[u8] = &[203, 150, 0];
                let flags_value = range_decoder.ec_dec_icdf(LBRR_40MS_ICDF, 8)?;
                Ok(vec![(flags_value & 1) != 0, (flags_value & 2) != 0])
            }
            60 => {
                const LBRR_60MS_ICDF: &[u8] = &[215, 195, 166, 125, 110, 82, 0];
                let flags_value = range_decoder.ec_dec_icdf(LBRR_60MS_ICDF, 8)?;
                Ok(vec![
                    (flags_value & 1) != 0,
                    (flags_value & 2) != 0,
                    (flags_value & 4) != 0,
                ])
            }
            _ => Err(Error::DecodeFailed("Invalid frame size".into())),
        }
    }

    /// Decode LBRR frames
    ///
    /// # RFC Reference
    /// * Lines 1999-2050: LBRR frame decoding
    /// * Lines 2041-2047: "the LBRR frames themselves are interleaved"
    /// * LBRR frames are decoded BEFORE regular frames
    /// * For stereo: frames MUST be interleaved mid1, side1, mid2, side2, mid3, side3
    ///
    /// # Critical Implementation Notes
    /// * RFC 6716 lines 2044-2047: "The decoder parses an LBRR frame for the mid channel
    ///   of a given 20 ms interval (if present) and then immediately parses the
    ///   corresponding LBRR frame for the side channel (if present), before proceeding
    ///   to the next 20 ms interval."
    /// * Loop order MUST be frame-major (outer: time intervals, inner: channels)
    /// * Channel-major order violates RFC and breaks bitstream parsing
    ///
    /// # Arguments
    /// * `range_decoder` - Range decoder positioned after header flags
    /// * `header_flags` - Decoded header flags with LBRR info
    /// * `config` - Configuration from TOC
    /// * `channels` - Mono or stereo
    ///
    /// # Returns
    /// Vector of LBRR frames (empty if no LBRR present)
    ///
    /// # Errors
    /// * Returns error if SILK frame decode fails
    #[cfg(feature = "silk")]
    fn decode_lbrr_frames(
        &mut self,
        range_decoder: &mut range::RangeDecoder,
        header_flags: &SilkHeaderFlags,
        config: Configuration,
        channels: Channels,
    ) -> Result<Vec<Vec<i16>>> {
        let num_silk_frames = match config.frame_size.to_ms() {
            10 | 20 => 1,
            40 => 2,
            60 => 3,
            _ => return Err(Error::DecodeFailed("Invalid SILK frame size".into())),
        };

        let num_channels = channels as usize;
        let mut lbrr_frames = Vec::new();

        for frame_idx in 0..num_silk_frames {
            for ch_idx in 0..num_channels {
                if !header_flags
                    .lbrr_flags
                    .get(ch_idx)
                    .copied()
                    .unwrap_or(false)
                {
                    continue;
                }

                let per_frame_flags = &header_flags.per_frame_lbrr[ch_idx];

                if !per_frame_flags.get(frame_idx).copied().unwrap_or(false) {
                    continue;
                }

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

                let frame_samples = Self::calculate_samples(FrameSize::Ms20, internal_rate);
                let mut lbrr_buffer = vec![0i16; frame_samples * channels as usize];

                let _decoded =
                    self.silk
                        .decode_silk_frame(range_decoder, true, &mut lbrr_buffer)?;

                lbrr_frames.push(lbrr_buffer);
            }
        }

        Ok(lbrr_frames)
    }

    /// Decode SILK-only frame
    ///
    /// # RFC Reference
    /// * Lines 455-466: SILK-only overview
    /// * Lines 494-496: Internal sample rates (NB=8k, MB=12k, WB=16k)
    /// * Lines 1954-1972: VAD flags in header
    /// * Lines 1999-2050: LBRR frame decoding
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

        let header_flags = Self::decode_silk_header_flags(&mut ec, config.frame_size, channels)?;

        let _lbrr_frames = self.decode_lbrr_frames(&mut ec, &header_flags, config, channels)?;

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

        let num_silk_frames = match config.frame_size.to_ms() {
            10 | 20 => 1,
            40 => 2,
            60 => 3,
            _ => return Err(Error::DecodeFailed("Invalid SILK frame size".into())),
        };

        let frame_20ms_samples = Self::calculate_samples(FrameSize::Ms20, internal_rate);
        let num_channels = channels as usize;
        let total_samples = frame_20ms_samples * num_silk_frames;
        let mut silk_buffer = vec![0i16; total_samples * num_channels];

        for frame_idx in 0..num_silk_frames {
            for ch_idx in 0..num_channels {
                let vad_flag_index = ch_idx * num_silk_frames + frame_idx;
                let vad_flag = header_flags
                    .vad_flags
                    .get(vad_flag_index)
                    .copied()
                    .unwrap_or(true);

                let mut frame_buffer = vec![0i16; frame_20ms_samples];

                let decoded = self
                    .silk
                    .decode_silk_frame(&mut ec, vad_flag, &mut frame_buffer)?;

                if decoded != frame_20ms_samples {
                    return Err(Error::DecodeFailed(format!(
                        "SILK sample count mismatch: expected {frame_20ms_samples}, got {decoded}"
                    )));
                }

                let base_offset = frame_idx * frame_20ms_samples * num_channels;
                for sample_idx in 0..frame_20ms_samples {
                    silk_buffer[base_offset + sample_idx * num_channels + ch_idx] =
                        frame_buffer[sample_idx];
                }
            }
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
        Ok(total_samples)
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
    /// * Lines 1999-2050: LBRR frame decoding
    /// * Line 5804: "first 17 bands (up to 8 kHz) are not coded"
    ///
    /// # Critical Algorithm
    /// 1. SILK decodes first using range decoder (with multi-frame support)
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

        let header_flags = Self::decode_silk_header_flags(&mut ec, config.frame_size, channels)?;

        let _lbrr_frames = self.decode_lbrr_frames(&mut ec, &header_flags, config, channels)?;

        let num_silk_frames = match config.frame_size.to_ms() {
            10 | 20 => 1,
            40 => 2,
            60 => 3,
            _ => return Err(Error::DecodeFailed("Invalid SILK frame size".into())),
        };

        let frame_20ms_samples =
            Self::calculate_samples(FrameSize::Ms20, HYBRID_SILK_INTERNAL_RATE);
        let num_channels = channels as usize;
        let total_samples = frame_20ms_samples * num_silk_frames;
        let mut silk_16k = vec![0i16; total_samples * num_channels];

        for frame_idx in 0..num_silk_frames {
            for ch_idx in 0..num_channels {
                let vad_flag_index = ch_idx * num_silk_frames + frame_idx;
                let vad_flag = header_flags
                    .vad_flags
                    .get(vad_flag_index)
                    .copied()
                    .unwrap_or(true);

                let mut frame_buffer = vec![0i16; frame_20ms_samples];

                let decoded = self
                    .silk
                    .decode_silk_frame(&mut ec, vad_flag, &mut frame_buffer)?;

                if decoded != frame_20ms_samples {
                    return Err(Error::DecodeFailed(format!(
                        "Hybrid SILK sample count mismatch: expected {frame_20ms_samples}, got {decoded}"
                    )));
                }

                let base_offset = frame_idx * frame_20ms_samples * num_channels;
                for sample_idx in 0..frame_20ms_samples {
                    silk_16k[base_offset + sample_idx * num_channels + ch_idx] =
                        frame_buffer[sample_idx];
                }
            }
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

#[cfg(all(test, feature = "silk"))]
mod lbrr_tests {
    use super::*;

    #[test]
    fn test_decode_silk_header_flags_mono_20ms() {
        let data = vec![0x80, 0x00, 0x00, 0x00];
        let mut ec = range::RangeDecoder::new(&data).unwrap();

        let result = Decoder::decode_silk_header_flags(&mut ec, FrameSize::Ms20, Channels::Mono);
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.vad_flags.len(), 1);
        assert_eq!(flags.lbrr_flags.len(), 1);
    }

    #[test]
    fn test_decode_silk_header_flags_stereo_40ms() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut ec = range::RangeDecoder::new(&data).unwrap();

        let result = Decoder::decode_silk_header_flags(&mut ec, FrameSize::Ms40, Channels::Stereo);
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.vad_flags.len(), 4);
        assert_eq!(flags.lbrr_flags.len(), 2);
        assert_eq!(flags.per_frame_lbrr.len(), 2);
    }

    #[test]
    fn test_decode_silk_header_flags_stereo_60ms() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut ec = range::RangeDecoder::new(&data).unwrap();

        let result = Decoder::decode_silk_header_flags(&mut ec, FrameSize::Ms60, Channels::Stereo);
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.vad_flags.len(), 6);
        assert_eq!(flags.lbrr_flags.len(), 2);
        assert_eq!(flags.per_frame_lbrr.len(), 2);
    }

    #[test]
    fn test_per_frame_lbrr_flags_20ms() {
        let result = Decoder::decode_per_frame_lbrr_flags(
            &mut range::RangeDecoder::new(&[0x80, 0x00, 0x00, 0x00]).unwrap(),
            FrameSize::Ms20,
        );
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.len(), 1);
        assert!(flags[0]);
    }

    #[test]
    fn test_per_frame_lbrr_flags_40ms() {
        let result = Decoder::decode_per_frame_lbrr_flags(
            &mut range::RangeDecoder::new(&[0x80, 0xFF, 0xFF, 0xFF]).unwrap(),
            FrameSize::Ms40,
        );
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.len(), 2);
    }

    #[test]
    fn test_per_frame_lbrr_flags_60ms() {
        let result = Decoder::decode_per_frame_lbrr_flags(
            &mut range::RangeDecoder::new(&[0x80, 0xFF, 0xFF, 0xFF]).unwrap(),
            FrameSize::Ms60,
        );
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.len(), 3);
    }

    #[test]
    fn test_vad_flag_indexing_stereo_40ms() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut ec = range::RangeDecoder::new(&data).unwrap();

        let flags =
            Decoder::decode_silk_header_flags(&mut ec, FrameSize::Ms40, Channels::Stereo).unwrap();

        assert_eq!(flags.vad_flags.len(), 4);
    }

    #[test]
    fn test_lbrr_40ms_icdf_values() {
        const LBRR_40MS_ICDF: &[u8] = &[203, 150, 0];
        assert_eq!(LBRR_40MS_ICDF[0], 203);
        assert_eq!(LBRR_40MS_ICDF[1], 150);
        assert_eq!(LBRR_40MS_ICDF[2], 0);
    }

    #[test]
    fn test_lbrr_60ms_icdf_values() {
        const LBRR_60MS_ICDF: &[u8] = &[215, 195, 166, 125, 110, 82, 0];
        assert_eq!(LBRR_60MS_ICDF[0], 215);
        assert_eq!(LBRR_60MS_ICDF[1], 195);
        assert_eq!(LBRR_60MS_ICDF[2], 166);
        assert_eq!(LBRR_60MS_ICDF[3], 125);
        assert_eq!(LBRR_60MS_ICDF[4], 110);
        assert_eq!(LBRR_60MS_ICDF[5], 82);
        assert_eq!(LBRR_60MS_ICDF[6], 0);
    }
}

#[cfg(all(test, not(any(feature = "silk", feature = "celt"))))]
mod no_features_tests {
    use super::*;

    #[test]
    fn test_decoder_new_succeeds_with_no_features() {
        let decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_decode_fails_with_no_features() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let mut output = vec![0i16; 480];

        let packet = vec![0b0000_0000];

        let result = decoder.decode(Some(&packet), &mut output, false);
        assert!(matches!(result, Err(Error::UnsupportedMode(_))));
    }

    // TODO: Remove ignore when implemented in Phase 6
    #[ignore = "Will be implemented in Phase 6"]
    #[test]
    fn test_reset_state_succeeds_with_no_features() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let _ = decoder.reset_state();
    }
}
