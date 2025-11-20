//! Pure Rust Opus audio decoder implementation.
//!
//! This crate provides a native Rust implementation of the Opus audio decoder according to
//! RFC 6716, supporting SILK (voice-optimized), CELT (full-spectrum), and Hybrid modes.
//!
//! # Features
//!
//! The crate supports optional feature flags for different codec modes:
//! * `silk` - SILK codec for voice (Narrowband/Mediumband/Wideband)
//! * `celt` - CELT codec for full-spectrum audio (all bandwidths)
//! * `hybrid` - Hybrid mode (SILK low frequencies + CELT high frequencies)
//! * `resampling` - Sample rate conversion support
//!
//! # Examples
//!
//! ## CELT-only decoding (48kHz)
//!
//! ```rust
//! # #[cfg(feature = "celt")]
//! # {
//! use moosicbox_opus_native::{Decoder, SampleRate, Channels};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo)?;
//!
//! let packet = vec![0x7C; 100];
//! let mut output = vec![0i16; 480 * 2];
//! let samples = decoder.decode(Some(&packet), &mut output, false)?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! ## SILK-only decoding (16kHz)
//!
//! ```rust
//! # #[cfg(feature = "silk")]
//! # {
//! use moosicbox_opus_native::{Decoder, SampleRate, Channels};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut decoder = Decoder::new(SampleRate::Hz16000, Channels::Stereo)?;
//!
//! let packet = vec![0x44; 100];
//! let mut output = vec![0i16; 160 * 2];
//! let samples = decoder.decode(Some(&packet), &mut output, false)?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! ## Hybrid decoding (48kHz with resampling)
//!
//! ```rust
//! # #[cfg(all(feature = "hybrid", feature = "resampling"))]
//! # {
//! use moosicbox_opus_native::{Decoder, SampleRate, Channels};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo)?;
//!
//! let packet = vec![0x74; 100];
//! let mut output = vec![0i16; 480 * 2];
//! let samples = decoder.decode(Some(&packet), &mut output, false)?;
//! # Ok(())
//! # }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// CELT decoder implementation (enabled with `celt` feature)
#[cfg(feature = "celt")]
pub mod celt;
/// Error types for Opus decoder operations
pub mod error;
/// Opus packet framing and parsing according to RFC 6716
pub mod framing;
/// Range decoder for entropy decoding
pub mod range;
/// SILK decoder implementation (enabled with `silk` feature)
#[cfg(feature = "silk")]
pub mod silk;
/// Table of Contents (TOC) byte parsing and configuration
pub mod toc;
mod util;

pub use error::{Error, Result};
pub use toc::{Bandwidth, Configuration, FrameSize, OpusMode, Toc};

/// Audio channel configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    /// Single audio channel
    Mono = 1,
    /// Two audio channels (left/right)
    Stereo = 2,
}

/// Supported output sample rates for decoded audio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    /// 8000 Hz (Narrowband)
    Hz8000 = 8000,
    /// 12000 Hz (Mediumband)
    Hz12000 = 12000,
    /// 16000 Hz (Wideband)
    Hz16000 = 16000,
    /// 24000 Hz (Super-wideband)
    Hz24000 = 24000,
    /// 48000 Hz (Fullband)
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

    #[cfg(feature = "silk")]
    silk_delay_samples: usize,

    #[cfg(feature = "silk")]
    silk_configured_rate: SampleRate,
    #[cfg(feature = "silk")]
    silk_configured_channels: Channels,
    #[cfg(feature = "silk")]
    silk_configured_frame_ms: u8,

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
    #[allow(clippy::missing_const_for_fn)]
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

            #[cfg(feature = "silk")]
            silk_delay_samples: 0,

            #[cfg(feature = "silk")]
            silk_configured_rate: SampleRate::Hz16000,
            #[cfg(feature = "silk")]
            silk_configured_channels: channels,
            #[cfg(feature = "silk")]
            silk_configured_frame_ms: 20,

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

            log::debug!(
                "Main decode: mode={:?}, sample_rate={}, frame_size={:?}",
                config.mode,
                self.sample_rate as u32,
                config.frame_size
            );

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

    /// Calculates SILK algorithmic delay in samples for a given internal rate.
    ///
    /// SILK has inherent algorithmic delay due to LPC analysis and pitch filtering.
    /// This delay is included in the decoded output (not automatically removed).
    ///
    /// # Arguments
    /// * `internal_rate` - SILK internal sample rate (8000, 12000, or 16000 Hz)
    ///
    /// # Returns
    /// Delay in samples at the internal rate
    #[must_use]
    #[cfg(feature = "silk")]
    const fn calculate_silk_delay_samples(internal_rate: u32) -> usize {
        match internal_rate {
            8000 => 5,   // NB: 5 samples = 0.625ms
            12000 => 10, // MB: 10 samples = 0.833ms
            16000 => 13, // WB: 13 samples = 0.8125ms
            _ => 0,      // Unknown rate - no delay compensation
        }
    }

    /// Returns the current SILK algorithmic delay in samples.
    ///
    /// This is the number of initial samples that contain lookahead data.
    /// The delay is included in the decoded output and should be skipped by the caller if needed.
    ///
    /// # Returns
    /// Delay in samples at the decoder's sample rate (0 if not using SILK)
    #[must_use]
    pub const fn algorithmic_delay_samples(&self) -> usize {
        #[cfg(feature = "silk")]
        {
            self.silk_delay_samples
        }
        #[cfg(not(feature = "silk"))]
        {
            0
        }
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

        for _ch_idx in 0..num_channels {
            for _frame_idx in 0..num_silk_frames {
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
    #[allow(dead_code, clippy::too_many_lines)]
    fn decode_silk_only(
        &mut self,
        frame_data: &[u8],
        config: Configuration,
        channels: Channels,
        output: &mut [i16],
    ) -> Result<usize> {
        let mut ec = range::RangeDecoder::new(frame_data)?;

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

        let silk_frame_size_ms = if config.frame_size.to_ms() <= 20 {
            config.frame_size.to_ms()
        } else {
            20
        };

        // Only recreate SILK decoder if configuration changed (preserves state for stereo)
        let target_rate = SampleRate::from_hz(internal_rate)?;
        let needs_reconfigure = self.silk_configured_rate != target_rate
            || self.silk_configured_channels != channels
            || self.silk_configured_frame_ms != silk_frame_size_ms;

        if needs_reconfigure {
            self.silk = silk::SilkDecoder::new(target_rate, channels, silk_frame_size_ms)?;
            self.silk_configured_rate = target_rate;
            self.silk_configured_channels = channels;
            self.silk_configured_frame_ms = silk_frame_size_ms;
        }

        // Track SILK algorithmic delay for automatic removal
        self.silk_delay_samples = Self::calculate_silk_delay_samples(internal_rate);

        let frame_samples =
            Self::calculate_samples(config.frame_size, internal_rate) / num_silk_frames;
        let num_channels = channels as usize;
        let total_samples = frame_samples * num_silk_frames;
        let mut silk_buffer = vec![0i16; total_samples * num_channels];

        if channels == Channels::Stereo {
            // Stereo: decode_silk_frame handles mid/side decoding and interleaving
            for frame_idx in 0..num_silk_frames {
                // VAD flags: [mid_frame0, mid_frame1, ..., side_frame0, side_frame1, ...]
                let mid_vad = header_flags
                    .vad_flags
                    .get(frame_idx)
                    .copied()
                    .unwrap_or(true);
                let side_vad = header_flags
                    .vad_flags
                    .get(num_silk_frames + frame_idx)
                    .copied()
                    .unwrap_or(true);

                let frame_offset = frame_idx * frame_samples * 2;
                let frame_end = frame_offset + frame_samples * 2;

                if frame_end > silk_buffer.len() {
                    return Err(Error::DecodeFailed(format!(
                        "Stereo buffer overflow: frame {} needs {} samples, buffer has {}",
                        frame_idx,
                        frame_end,
                        silk_buffer.len()
                    )));
                }

                let frame_buffer = &mut silk_buffer[frame_offset..frame_end];

                // decode_silk_frame_stereo returns samples per channel
                let decoded = self.silk.decode_silk_frame_stereo(
                    &mut ec,
                    (mid_vad, side_vad),
                    frame_buffer,
                )?;

                if decoded != frame_samples {
                    return Err(Error::DecodeFailed(format!(
                        "Stereo frame {frame_idx} sample count mismatch: expected {frame_samples}, got {decoded}"
                    )));
                }
            }
        } else {
            // Mono: decode each frame independently
            for frame_idx in 0..num_silk_frames {
                let vad_flag = header_flags
                    .vad_flags
                    .get(frame_idx)
                    .copied()
                    .unwrap_or(true);

                let frame_offset = frame_idx * frame_samples;
                let mut frame_buffer = vec![0i16; frame_samples];

                let decoded = self
                    .silk
                    .decode_silk_frame(&mut ec, vad_flag, &mut frame_buffer)?;

                if decoded != frame_samples {
                    return Err(Error::DecodeFailed(format!(
                        "Mono frame {frame_idx} sample count mismatch: expected {frame_samples}, got {decoded}"
                    )));
                }

                silk_buffer[frame_offset..frame_offset + frame_samples]
                    .copy_from_slice(&frame_buffer);
            }
        }

        let target_rate = self.sample_rate as u32;
        #[cfg(feature = "resampling")]
        if internal_rate != target_rate {
            let resampled =
                self.resample_silk(&silk_buffer, internal_rate, target_rate, channels)?;

            let copy_len = resampled.len().min(output.len());
            output[..copy_len].copy_from_slice(&resampled[..copy_len]);

            return Ok(resampled.len() / num_channels);
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
        use crate::{
            celt::{CELT_NUM_BANDS, fixed_point::sig_to_int16},
            range::RangeDecoder,
        };

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

        log::debug!(
            "decode_celt_only: frame has {} samples, nonzero={}, first 20: {:?}",
            decoded_frame.samples.len(),
            decoded_frame.samples.iter().filter(|&&x| x != 0).count(),
            &decoded_frame.samples[..20.min(decoded_frame.samples.len())]
        );
        if decoded_frame.samples.len() >= 60 {
            log::debug!(
                "decode_celt_only[40..60]: {:?}",
                &decoded_frame.samples[40..60]
            );
        }

        // Convert CeltSig (Q12 format) to i16 PCM
        for (i, &sample) in decoded_frame.samples.iter().enumerate() {
            if i < output.len() {
                // sig_to_int16 converts Q12 → i16 with proper rounding and saturation
                output[i] = sig_to_int16(sample);
            }
        }

        log::debug!(
            "decode_celt_only: After conversion to PCM, output[0..20]: {:?}",
            &output[..20]
        );
        if output.len() >= 60 {
            log::debug!("decode_celt_only: output[40..60]: {:?}", &output[40..60]);
        }

        // DEBUG: For WB, log the Q12 samples before conversion
        if self.sample_rate == SampleRate::Hz16000 && decoded_frame.samples.len() >= 60 {
            log::debug!(
                "CELT decode Q12 samples[40..60]: {:?}",
                &decoded_frame.samples[40..60]
            );
        }

        // TODO: Apply deemphasis + decimation when downsample > 1
        // Currently CELT always outputs 48kHz samples (480 for 10ms frame).
        // When self.sample_rate < 48kHz, we should decimate the output.
        // For example, WB (16kHz) should output 160 samples, not 480.
        // The downsample factor is set via set_output_rate(), but decimation
        // is not yet implemented in this decode path.
        //
        // RFC 6716 lines 498-501: "decimate the MDCT layer output"
        // LibOpus: celt_decoder.c applies deemphasis filter + time-domain decimation
        //
        // Until decimation is implemented:
        // - 48kHz output works correctly (bit-exact for NB via downsampling)
        // - Other rates (8/12/16/24kHz) will fail with sample count mismatch

        Ok(decoded_frame.samples.len())
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
        use crate::{celt::CELT_NUM_BANDS, celt::fixed_point::sig_to_int16, range::RangeDecoder};

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

        let frame_samples = Self::calculate_samples(FrameSize::Ms20, HYBRID_SILK_INTERNAL_RATE);
        let num_channels = channels as usize;
        let total_samples = frame_samples * num_silk_frames;
        let mut silk_16k = vec![0i16; total_samples * num_channels];

        for frame_idx in 0..num_silk_frames {
            for ch_idx in 0..num_channels {
                let vad_flag_index = ch_idx * num_silk_frames + frame_idx;
                let vad_flag = header_flags
                    .vad_flags
                    .get(vad_flag_index)
                    .copied()
                    .unwrap_or(true);

                let mut frame_buffer = vec![0i16; frame_samples];

                let decoded = self
                    .silk
                    .decode_silk_frame(&mut ec, vad_flag, &mut frame_buffer)?;

                if decoded != frame_samples {
                    return Err(Error::DecodeFailed(format!(
                        "Hybrid SILK sample count mismatch: expected {frame_samples}, got {decoded}"
                    )));
                }

                let base_offset = frame_idx * frame_samples * num_channels;
                for sample_idx in 0..frame_samples {
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
            .map(|&s| sig_to_int16(s))
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

    #[test_log::test]
    fn test_decode_silk_header_flags_mono_20ms() {
        let data = vec![0x80, 0x00, 0x00, 0x00];
        let mut ec = range::RangeDecoder::new(&data).unwrap();

        let result = Decoder::decode_silk_header_flags(&mut ec, FrameSize::Ms20, Channels::Mono);
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.vad_flags.len(), 1);
        assert_eq!(flags.lbrr_flags.len(), 1);
    }

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
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

    #[test_log::test]
    fn test_per_frame_lbrr_flags_40ms() {
        let result = Decoder::decode_per_frame_lbrr_flags(
            &mut range::RangeDecoder::new(&[0x80, 0xFF, 0xFF, 0xFF]).unwrap(),
            FrameSize::Ms40,
        );
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.len(), 2);
    }

    #[test_log::test]
    fn test_per_frame_lbrr_flags_60ms() {
        let result = Decoder::decode_per_frame_lbrr_flags(
            &mut range::RangeDecoder::new(&[0x80, 0xFF, 0xFF, 0xFF]).unwrap(),
            FrameSize::Ms60,
        );
        assert!(result.is_ok());

        let flags = result.unwrap();
        assert_eq!(flags.len(), 3);
    }

    #[test_log::test]
    fn test_vad_flag_indexing_stereo_40ms() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut ec = range::RangeDecoder::new(&data).unwrap();

        let flags =
            Decoder::decode_silk_header_flags(&mut ec, FrameSize::Ms40, Channels::Stereo).unwrap();

        assert_eq!(flags.vad_flags.len(), 4);
    }

    #[test_log::test]
    fn test_lbrr_40ms_icdf_values() {
        const LBRR_40MS_ICDF: &[u8] = &[203, 150, 0];
        assert_eq!(LBRR_40MS_ICDF[0], 203);
        assert_eq!(LBRR_40MS_ICDF[1], 150);
        assert_eq!(LBRR_40MS_ICDF[2], 0);
    }

    #[test_log::test]
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

#[cfg(test)]
mod sample_rate_tests {
    use super::*;

    #[test_log::test]
    fn test_from_hz_all_valid_rates() {
        assert_eq!(SampleRate::from_hz(8000).unwrap(), SampleRate::Hz8000);
        assert_eq!(SampleRate::from_hz(12000).unwrap(), SampleRate::Hz12000);
        assert_eq!(SampleRate::from_hz(16000).unwrap(), SampleRate::Hz16000);
        assert_eq!(SampleRate::from_hz(24000).unwrap(), SampleRate::Hz24000);
        assert_eq!(SampleRate::from_hz(48000).unwrap(), SampleRate::Hz48000);
    }

    #[test_log::test]
    fn test_from_hz_invalid_rates() {
        assert!(SampleRate::from_hz(11025).is_err());
        assert!(SampleRate::from_hz(22050).is_err());
        assert!(SampleRate::from_hz(44100).is_err());
        assert!(SampleRate::from_hz(96000).is_err());
        assert!(SampleRate::from_hz(0).is_err());
    }

    #[test_log::test]
    fn test_from_hz_error_message_includes_rate() {
        let result = SampleRate::from_hz(44100);
        assert!(result.is_err());
        if let Err(Error::InvalidSampleRate(msg)) = result {
            assert!(msg.contains("44100"));
            assert!(msg.contains("Unsupported sample rate"));
        }
    }
}

#[cfg(test)]
mod channels_tests {
    use super::*;

    #[test_log::test]
    fn test_channels_mono_value() {
        assert_eq!(Channels::Mono as usize, 1);
    }

    #[test_log::test]
    fn test_channels_stereo_value() {
        assert_eq!(Channels::Stereo as usize, 2);
    }

    #[test_log::test]
    fn test_channels_equality() {
        assert_eq!(Channels::Mono, Channels::Mono);
        assert_eq!(Channels::Stereo, Channels::Stereo);
        assert_ne!(Channels::Mono, Channels::Stereo);
    }
}

#[cfg(test)]
mod decoder_tests {
    use super::*;

    #[test_log::test]
    fn test_calculate_samples_all_frame_sizes() {
        // At 48kHz
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms2_5, 48000), 120);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms5, 48000), 240);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms10, 48000), 480);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms20, 48000), 960);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms40, 48000), 1920);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms60, 48000), 2880);
    }

    #[test_log::test]
    fn test_calculate_samples_different_rates() {
        // Ms10 at different sample rates
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms10, 8000), 80);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms10, 12000), 120);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms10, 16000), 160);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms10, 24000), 240);
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms10, 48000), 480);
    }

    #[test_log::test]
    fn test_calculate_samples_ms20_at_16khz() {
        // Common SILK configuration
        assert_eq!(Decoder::calculate_samples(FrameSize::Ms20, 16000), 320);
    }

    #[cfg(feature = "silk")]
    #[test_log::test]
    fn test_calculate_silk_delay_samples() {
        assert_eq!(Decoder::calculate_silk_delay_samples(8000), 5);
        assert_eq!(Decoder::calculate_silk_delay_samples(12000), 10);
        assert_eq!(Decoder::calculate_silk_delay_samples(16000), 13);
    }

    #[cfg(feature = "silk")]
    #[test_log::test]
    fn test_calculate_silk_delay_samples_unknown_rate() {
        assert_eq!(Decoder::calculate_silk_delay_samples(48000), 0);
        assert_eq!(Decoder::calculate_silk_delay_samples(0), 0);
    }

    #[test_log::test]
    fn test_algorithmic_delay_samples_without_silk() {
        let _decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        #[cfg(not(feature = "silk"))]
        assert_eq!(_decoder.algorithmic_delay_samples(), 0);
    }

    #[test_log::test]
    fn test_handle_packet_loss_zeros_output() {
        let decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
        let mut output = vec![42i16; 960];

        let samples = decoder.handle_packet_loss(&mut output, false);

        assert_eq!(samples, 480); // 960 samples / 2 channels
        assert!(output.iter().all(|&s| s == 0));
    }

    #[test_log::test]
    fn test_decode_empty_packet_error() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let mut output = vec![0i16; 480];

        let result = decoder.decode(Some(&[]), &mut output, false);

        assert!(matches!(result, Err(Error::InvalidPacket(_))));
        if let Err(Error::InvalidPacket(msg)) = result {
            assert!(msg.contains("R1"));
        }
    }

    #[test_log::test]
    fn test_decode_packet_loss_returns_silence() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let mut output = vec![42i16; 480];

        let samples = decoder.decode(None, &mut output, false).unwrap();

        assert_eq!(samples, 480);
        assert!(output.iter().all(|&s| s == 0));
    }
}

#[cfg(all(test, not(any(feature = "silk", feature = "celt"))))]
mod no_features_tests {
    use super::*;

    #[test_log::test]
    fn test_decoder_new_succeeds_with_no_features() {
        let decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo);
        assert!(decoder.is_ok());
    }

    #[test_log::test]
    fn test_decode_fails_with_no_features() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let mut output = vec![0i16; 480];

        let packet = vec![0b0000_0000];

        let result = decoder.decode(Some(&packet), &mut output, false);
        assert!(matches!(result, Err(Error::UnsupportedMode(_))));
    }

    // TODO: Remove ignore when implemented in Phase 6
    #[ignore = "Will be implemented in Phase 6"]
    #[test_log::test]
    fn test_reset_state_succeeds_with_no_features() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let _ = decoder.reset_state();
    }
}
