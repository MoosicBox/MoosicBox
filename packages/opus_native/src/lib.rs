//! Pure Rust Opus audio decoder implementation.
//!
//! This crate provides a native Rust implementation of the Opus audio decoder according to
//! RFC 6716. SILK, CELT and hybrid paths are under conformance development.
//! Feature availability is not a general PCM parity claim.
//!
//! # Features
//!
//! The crate supports optional feature flags for different codec modes:
//! * `silk` - SILK codec for voice (Narrowband/Mediumband/Wideband)
//! * `celt` - CELT codec for full-spectrum audio (all bandwidths)
//! * `hybrid` - Hybrid mode (SILK low frequencies + CELT high frequencies)
//! * `resampling` - Compatibility feature; raw SILK output uses the integer
//!   normative converter regardless of this flag.
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
//! ## Hybrid limitations
//!
//! Ordinary hybrid frames use shared entropy and reference-order CELT deemphasis,
//! conversion and saturating accumulation onto SILK PCM. Redundant transition
//! synthesis remains unsupported, as do FEC and packet-loss concealment.

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
#[cfg(feature = "celt")]
mod output;
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
    /// * Returns error if `hz` is not one of 8000, 12000, 16000, 24000, or 48000.
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
    #[cfg(feature = "celt")]
    celt_output: output::CeltOutput,
    #[cfg(feature = "celt")]
    celt_packet_channels: Option<Channels>,

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

    #[cfg(feature = "silk")]
    silk_resamplers: Vec<silk::resampler::SilkResampler>,
    #[cfg(feature = "silk")]
    silk_resampler_input_rate: u32,
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

            #[cfg(feature = "celt")]
            celt_output: output::CeltOutput::default(),
            #[cfg(feature = "celt")]
            celt_packet_channels: None,

            prev_mode: None,

            #[cfg(feature = "silk")]
            silk_delay_samples: 0,

            #[cfg(feature = "silk")]
            silk_configured_rate: SampleRate::Hz16000,
            #[cfg(feature = "silk")]
            silk_configured_channels: channels,
            #[cfg(feature = "silk")]
            silk_configured_frame_ms: 20,

            #[cfg(feature = "silk")]
            silk_resamplers: Vec::new(),
            #[cfg(feature = "silk")]
            silk_resampler_input_rate: 0,
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
    /// * `fec` - Forward Error Correction request (currently unsupported)
    ///
    /// # Returns
    /// Number of samples decoded per channel
    ///
    /// # Errors
    /// * `Error::InvalidPacket` - Packet violates RFC R1-R7
    /// * `Error::UnsupportedMode` - Mode not enabled via features
    /// * `Error::DecodeFailed` - Decoder error
    /// * `Error::Unsupported` - FEC or packet-loss concealment requested; state and output untouched
    #[allow(clippy::too_many_lines)]
    pub fn decode(&mut self, input: Option<&[u8]>, output: &mut [i16], fec: bool) -> Result<usize> {
        if fec {
            return Err(Error::Unsupported(
                "Forward error correction is not implemented".into(),
            ));
        }
        let packet = input.ok_or_else(|| {
            Error::Unsupported("Packet loss concealment is not implemented".into())
        })?;

        if packet.is_empty() {
            return Err(Error::InvalidPacket("Packet must be ≥1 byte (R1)".into()));
        }

        #[cfg(not(any(feature = "silk", feature = "celt")))]
        {
            let _ = output;
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

            let frames = framing::parse_frames(packet)?;

            if self.prev_mode.is_some_and(|previous| {
                previous != config.mode
                    && (previous == OpusMode::Hybrid || config.mode == OpusMode::Hybrid)
            }) {
                return Err(Error::Unsupported(
                    "Hybrid mode transition crossfade is not implemented".into(),
                ));
            }
            #[cfg(feature = "celt")]
            if config.mode == OpusMode::Hybrid
                && self
                    .celt_packet_channels
                    .is_some_and(|previous| previous != toc.channels())
            {
                return Err(Error::Unsupported(
                    "Hybrid coded channel transition requires reference state mapping".into(),
                ));
            }

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

                    self.silk_resamplers.clear();
                    self.silk_resampler_input_rate = 0;
                }

                #[cfg(feature = "celt")]
                if curr == toc::OpusMode::CeltOnly || curr == toc::OpusMode::Hybrid {
                    self.celt.reset();
                    self.celt_output = output::CeltOutput::default();
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

    /// Decodes an Opus packet to floating point PCM.
    ///
    /// # Errors
    ///
    /// * Returns an error if integer PCM decoding fails.
    ///
    /// The fixed-point decoder is used for synthesis. Samples are converted to
    /// floating point by dividing by 32768, without changing codec arithmetic.
    pub fn decode_float(
        &mut self,
        input: Option<&[u8]>,
        output: &mut [f32],
        fec: bool,
    ) -> Result<usize> {
        let mut pcm = vec![0_i16; output.len()];
        let samples = self.decode(input, &mut pcm, fec)?;
        let sample_count = samples * self.channels as usize;
        for (dst, &src) in output.iter_mut().zip(&pcm).take(sample_count) {
            *dst = f32::from(src) / 32768.0;
        }
        Ok(samples)
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
            8000 => 4,
            12000 => 9,
            16000 => 12,
            _ => 0, // Unknown rate - no delay compensation
        }
    }

    /// Returns the current SILK algorithmic delay in samples.
    ///
    /// This is the number of initial samples that contain lookahead data.
    /// Raw output includes this delay and must not be trimmed for reference parity.
    /// Reports only the same-rate resampler's 4/9/12 input-sample delay.
    /// The SILK API wrapper also preserves one sample of synthesis history for
    /// both mono and stereo; that history and cross-rate filter latency are not
    /// included in this metadata. This is not total codec/encoder lookahead.
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
    /// * Returns an error if constructing fresh codec state fails.
    ///
    /// The output sample rate and channel count are preserved. All prediction,
    /// overlap, mode-transition, and resampler state is discarded.
    pub fn reset_state(&mut self) -> Result<()> {
        *self = Self::new(self.sample_rate, self.channels)?;
        Ok(())
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
        {
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
        self.decode_silk_payload(&mut ec, config, channels, output)
    }

    #[cfg(feature = "silk")]
    #[allow(clippy::too_many_lines)]
    fn decode_silk_payload(
        &mut self,
        ec: &mut range::RangeDecoder,
        config: Configuration,
        channels: Channels,
        output: &mut [i16],
    ) -> Result<usize> {
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

        if !self.silk_resamplers.is_empty()
            && (self.silk_configured_rate != target_rate
                || self.silk_configured_channels != channels)
        {
            return Err(Error::Unsupported(
                "SILK rate/channel transition requires reference state migration".into(),
            ));
        }
        if !self.silk_resamplers.is_empty() && self.silk_configured_frame_ms != silk_frame_size_ms {
            return Err(Error::Unsupported(
                "SILK frame-duration transition requires prediction-state reconfiguration".into(),
            ));
        }
        if needs_reconfigure {
            self.silk = silk::SilkDecoder::new(target_rate, channels, silk_frame_size_ms)?;
            self.silk_configured_rate = target_rate;
            self.silk_configured_channels = channels;
            self.silk_configured_frame_ms = silk_frame_size_ms;
        }

        // One independently coded Opus SILK payload can contain 1–3 constituent
        // SILK frames. Reset entropy independence once, never per constituent.
        self.silk.begin_packet();

        let header_flags = Self::decode_silk_header_flags(ec, config.frame_size, channels)?;

        let _lbrr_frames = self.decode_lbrr_frames(ec, &header_flags, config, channels)?;

        // Same-rate delay is diagnostic metadata only. Raw output is never shifted.
        self.silk_delay_samples = if internal_rate == self.sample_rate as u32 {
            Self::calculate_silk_delay_samples(internal_rate)
        } else {
            0
        };

        let frame_samples =
            Self::calculate_samples(config.frame_size, internal_rate) / num_silk_frames;
        let synthesis_channels = if self.channels == Channels::Mono {
            Channels::Mono
        } else {
            channels
        };
        let num_channels = synthesis_channels as usize;
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

                let frame_offset = frame_idx * frame_samples * num_channels;
                let frame_end = frame_offset + frame_samples * num_channels;

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
                let decoded = if synthesis_channels == Channels::Mono {
                    self.silk.decode_silk_frame_stereo_to_mono(
                        ec,
                        (mid_vad, side_vad),
                        frame_buffer,
                    )?
                } else {
                    self.silk
                        .decode_silk_frame_stereo(ec, (mid_vad, side_vad), frame_buffer)?
                };

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
                    .decode_silk_frame(ec, vad_flag, &mut frame_buffer)?;

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
        let resampled =
            self.resample_silk(&silk_buffer, internal_rate, target_rate, synthesis_channels)?;
        if resampled.len() > output.len() {
            return Err(Error::DecodeFailed("SILK output buffer too small".into()));
        }
        if channels == Channels::Mono && self.channels == Channels::Stereo {
            if output.len() < resampled.len() * 2 {
                return Err(Error::DecodeFailed(
                    "SILK stereo output buffer too small".into(),
                ));
            }
            for (frame, sample) in resampled.iter().enumerate() {
                output[2 * frame] = *sample;
                output[2 * frame + 1] = *sample;
            }
        } else {
            output[..resampled.len()].copy_from_slice(&resampled);
        }
        Ok(resampled.len() / num_channels)
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
        config: Configuration,
        channels: Channels,
        output: &mut [i16],
    ) -> Result<usize> {
        if self
            .celt_packet_channels
            .is_some_and(|previous| previous != channels)
        {
            return Err(Error::Unsupported(
                "CELT coded channel transition requires reference overlap-state mapping".into(),
            ));
        }
        if self.celt_packet_channels.is_none() {
            self.celt.set_channels(channels);
            self.celt_packet_channels = Some(channels);
        }
        let mut ec = range::RangeDecoder::new(frame_data)?;
        let internal_samples = Self::calculate_samples(config.frame_size, 48000);
        self.celt.set_output_rate(self.sample_rate)?;
        self.celt.set_frame_size(internal_samples)?;
        self.celt.set_start_band(0);
        self.celt.set_end_band(match config.bandwidth {
            Bandwidth::Narrowband => 13,
            Bandwidth::Mediumband | Bandwidth::Wideband => 17,
            Bandwidth::SuperWideband => 19,
            Bandwidth::Fullband => 21,
        });
        let synthesis_channels = if self.channels == Channels::Mono {
            Channels::Mono
        } else {
            channels
        };
        self.celt.configure_synthesis_channels(synthesis_channels);
        let decoded_frame = self.celt.decode_celt_frame(&mut ec, frame_data.len())?;
        if decoded_frame.sample_rate != SampleRate::Hz48000
            || decoded_frame.channels != synthesis_channels
        {
            return Err(Error::DecodeFailed(
                "CELT synthesis configuration mismatch".into(),
            ));
        }
        if channels == Channels::Mono && self.channels == Channels::Stereo {
            let mut mono =
                vec![0; Self::calculate_samples(config.frame_size, self.sample_rate as u32)];
            let count = self.celt_output.process(
                &decoded_frame.samples,
                internal_samples,
                1,
                self.sample_rate as u32,
                &mut mono,
            )?;
            for (frame, sample) in mono.iter().enumerate() {
                output[2 * frame] = *sample;
                output[2 * frame + 1] = *sample;
            }
            Ok(count)
        } else {
            self.celt_output.process(
                &decoded_frame.samples,
                internal_samples,
                synthesis_channels as usize,
                self.sample_rate as u32,
                output,
            )
        }
    }

    /// Decode hybrid mode frame (SILK low-freq + CELT high-freq)
    ///
    /// This path is incomplete: reference-equivalent SILK/CELT accumulation before
    /// deemphasis and output conversion is not implemented. Successful execution
    /// is not a claim of PCM parity; do not reuse the CELT-only output stage here
    /// after independent i16 clipping.
    ///
    /// # RFC Reference
    /// * Lines 481-487: Hybrid overview
    /// * Lines 522-526: "Both layers use the same entropy coder"
    /// * Lines 1749-1750: "In a Hybrid frame, SILK operates in WB"
    /// * Lines 1999-2050: LBRR frame decoding
    /// * Line 5804: "first 17 bands (up to 8 kHz) are not coded"
    ///
    /// Decodes both layers from one entropy state and saturating-adds converted
    /// CELT output to the persistent resampled SILK PCM.
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
    /// * Returns unsupported for redundant transition synthesis
    #[cfg(all(feature = "silk", feature = "celt"))]
    #[allow(dead_code)]
    fn decode_hybrid(
        &mut self,
        frame_data: &[u8],
        config: Configuration,
        channels: Channels,
        output: &mut [i16],
    ) -> Result<usize> {
        let mut ec = range::RangeDecoder::new(frame_data)?;
        let samples = self.decode_silk_payload(
            &mut ec,
            Configuration {
                bandwidth: Bandwidth::Wideband,
                ..config
            },
            channels,
            output,
        )?;
        let mut frame_bytes = frame_data.len();
        if ec.ec_tell() as usize + 37 <= frame_bytes * 8 && ec.ec_dec_bit_logp(12)? {
            let _celt_to_silk = ec.ec_dec_bit_logp(1)?;
            let redundancy_bytes = ec.ec_dec_uint(256)? as usize + 2;
            frame_bytes = frame_bytes
                .checked_sub(redundancy_bytes)
                .ok_or_else(|| Error::InvalidPacket("hybrid redundancy exceeds payload".into()))?;
            if frame_bytes * 8 < ec.ec_tell() as usize {
                return Err(Error::InvalidPacket(
                    "hybrid redundancy overlaps entropy".into(),
                ));
            }
            ec.shrink_storage(frame_bytes)?;
            return Err(Error::Unsupported(
                "Hybrid redundancy transition synthesis is not implemented".into(),
            ));
        }
        self.celt.set_channels(channels);
        self.celt_packet_channels = Some(channels);
        let synthesis_channels = if self.channels == Channels::Mono {
            Channels::Mono
        } else {
            channels
        };
        self.celt.configure_synthesis_channels(synthesis_channels);
        let internal_samples = Self::calculate_samples(config.frame_size, 48000);
        self.celt.set_output_rate(self.sample_rate)?;
        let decoded = self.celt.decode_hybrid_highband(
            &mut ec,
            frame_bytes,
            internal_samples,
            if config.bandwidth == Bandwidth::SuperWideband {
                19
            } else {
                21
            },
        )?;
        let mut high = vec![0; samples * synthesis_channels as usize];
        self.celt_output.process(
            &decoded.samples,
            internal_samples,
            synthesis_channels as usize,
            self.sample_rate as u32,
            &mut high,
        )?;
        for frame in 0..samples {
            for channel in 0..self.channels as usize {
                let source = if synthesis_channels == Channels::Mono {
                    frame
                } else {
                    frame * 2 + channel
                };
                let target = frame * self.channels as usize + channel;
                output[target] = output[target].saturating_add(high[source]);
            }
        }
        Ok(samples)
    }

    /// Convert raw interleaved SILK LR/mono with one persistent integer resampler
    /// per channel, including normative delay for same-rate conversion.
    #[cfg(feature = "silk")]
    fn resample_silk(
        &mut self,
        input: &[i16],
        input_rate: u32,
        output_rate: u32,
        channels: Channels,
    ) -> Result<Vec<i16>> {
        let count = channels as usize;
        if input.is_empty()
            || !input.len().is_multiple_of(count)
            || output_rate != self.sample_rate as u32
        {
            return Err(Error::DecodeFailed(
                "invalid SILK resampler input shape/configuration".into(),
            ));
        }
        if self.silk_resamplers.is_empty() {
            self.silk_resamplers = (0..count)
                .map(|_| silk::resampler::SilkResampler::new(input_rate, output_rate))
                .collect::<Result<Vec<_>>>()?;
            self.silk_resampler_input_rate = input_rate;
        } else if self.silk_resampler_input_rate != input_rate
            || self.silk_resamplers.len() != count
        {
            return Err(Error::Unsupported(
                "SILK resampler rate/channel transition requires reference state migration".into(),
            ));
        }
        let frames = input.len() / count;
        let expected = frames
            .checked_mul(output_rate as usize)
            .ok_or_else(|| Error::DecodeFailed("SILK output size overflow".into()))?
            / input_rate as usize;
        let mut output = vec![0; expected * count];
        for channel in 0..count {
            let planar = input
                .iter()
                .skip(channel)
                .step_by(count)
                .copied()
                .collect::<Vec<_>>();
            let converted = self.silk_resamplers[channel].process(&planar)?;
            if converted.len() != expected {
                return Err(Error::DecodeFailed(
                    "SILK resampler returned wrong duration".into(),
                ));
            }
            for (frame, sample) in converted.into_iter().enumerate() {
                output[frame * count + channel] = sample;
            }
        }
        Ok(output)
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
        {
            const LBRR_40MS_ICDF: &[u8] = &[203, 150, 0];

            assert_eq!(LBRR_40MS_ICDF[0], 203);
            assert_eq!(LBRR_40MS_ICDF[1], 150);
            assert_eq!(LBRR_40MS_ICDF[2], 0);
        }
    }

    #[test_log::test]
    fn test_lbrr_60ms_icdf_values() {
        {
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
        assert_eq!(Decoder::calculate_silk_delay_samples(8000), 4);
        assert_eq!(Decoder::calculate_silk_delay_samples(12000), 9);
        assert_eq!(Decoder::calculate_silk_delay_samples(16000), 12);
    }

    #[cfg(feature = "silk")]
    #[test_log::test]
    fn test_calculate_silk_delay_samples_unknown_rate() {
        assert_eq!(Decoder::calculate_silk_delay_samples(48000), 0);
        assert_eq!(Decoder::calculate_silk_delay_samples(0), 0);
    }

    #[cfg(not(feature = "silk"))]
    #[test_log::test]
    fn test_algorithmic_delay_samples_without_silk() {
        let decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        assert_eq!(decoder.algorithmic_delay_samples(), 0);
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
    fn packet_loss_is_unsupported_without_writing_output() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let mut output = [42; 480];
        assert!(matches!(
            decoder.decode(None, &mut output, false),
            Err(Error::Unsupported(_))
        ));
        assert_eq!(output, [42; 480]);
    }
}

#[cfg(test)]
mod public_state_tests {
    use super::*;

    #[test_log::test]
    fn reset_restores_initial_configuration_and_mode() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
        decoder.prev_mode = Some(OpusMode::CeltOnly);
        #[cfg(feature = "silk")]
        {
            decoder.silk_delay_samples = 13;
            decoder.silk_configured_rate = SampleRate::Hz8000;
        }
        decoder.reset_state().unwrap();
        assert_eq!(decoder.sample_rate, SampleRate::Hz48000);
        assert_eq!(decoder.channels, Channels::Stereo);
        assert_eq!(decoder.prev_mode, None);
        #[cfg(feature = "silk")]
        {
            assert_eq!(decoder.silk_delay_samples, 0);
            assert_eq!(decoder.silk_configured_rate, SampleRate::Hz16000);
        }
    }

    #[test_log::test]
    fn float_packet_loss_is_unsupported_without_writing_output() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
        let mut output = vec![1.0; 960];
        assert!(matches!(
            decoder.decode_float(None, &mut output, false),
            Err(Error::Unsupported(_))
        ));
        assert!(
            output
                .iter()
                .all(|sample| sample.to_bits() == 1.0_f32.to_bits())
        );
    }

    #[test_log::test]
    fn float_decode_propagates_invalid_packet_without_writing_output() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        let mut output = vec![1.0; 480];
        assert!(decoder.decode_float(Some(&[]), &mut output, false).is_err());
        assert!(
            output
                .iter()
                .all(|&sample| sample.to_bits() == 1.0_f32.to_bits())
        );
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

    #[test_log::test]
    fn test_reset_state_succeeds_with_no_features() {
        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
        decoder.reset_state().unwrap();
    }
}

#[cfg(all(test, feature = "silk"))]
mod silk_output_integration_tests {
    use super::*;

    #[test]
    fn raw_nb_reference_two_packets_keep_synthesis_and_resampler_history() {
        let packet = include_bytes!("silk/reference/nb_sine_mono.packet");
        let bytes = include_bytes!("silk/reference/nb_sine_mono.api.pcm");
        let expected = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|sample| i16::from_le_bytes(*sample))
            .collect::<Vec<_>>();
        let mut decoder = Decoder::new(SampleRate::Hz8000, Channels::Mono).unwrap();
        for index in 0..2 {
            let mut output = [0; 160];
            assert_eq!(
                decoder.decode(Some(packet), &mut output, false).unwrap(),
                160
            );
            assert_eq!(
                output,
                expected[index * 160..(index + 1) * 160],
                "raw packet {index}"
            );
            assert_eq!(decoder.algorithmic_delay_samples(), 4);
        }
        decoder.reset_state().unwrap();
        let mut output = [0; 160];
        decoder.decode(Some(packet), &mut output, false).unwrap();
        assert_eq!(output, expected[..160]);
    }

    #[test]
    fn planar_resamplers_preserve_independent_channels_packets_and_reset() {
        for input_rate in [8000, 12000, 16000] {
            for output_rate in [8000, 12000, 16000, 24000, 48000] {
                let mut decoder =
                    Decoder::new(SampleRate::from_hz(output_rate).unwrap(), Channels::Stereo)
                        .unwrap();
                let mut left =
                    silk::resampler::SilkResampler::new(input_rate, output_rate).unwrap();
                let mut right =
                    silk::resampler::SilkResampler::new(input_rate, output_rate).unwrap();
                for packet in 0..3 {
                    let n = input_rate as usize / 50;
                    let l = (0..n)
                        .map(|index| if index == packet { 12000 } else { 0 })
                        .collect::<Vec<_>>();
                    let r = (0..n)
                        .map(|index| if index % 2 == 0 { 4000 } else { -7000 })
                        .collect::<Vec<_>>();
                    let input = l
                        .iter()
                        .zip(&r)
                        .flat_map(|(l, r)| [*l, *r])
                        .collect::<Vec<_>>();
                    let expected_l = left.process(&l).unwrap();
                    let expected_r = right.process(&r).unwrap();
                    let actual = decoder
                        .resample_silk(&input, input_rate, output_rate, Channels::Stereo)
                        .unwrap();
                    let expected = expected_l
                        .iter()
                        .zip(&expected_r)
                        .flat_map(|(l, r)| [*l, *r])
                        .collect::<Vec<_>>();
                    assert_eq!(actual, expected);
                    assert_eq!(actual.len(), output_rate as usize / 50 * 2);
                }
                decoder.reset_state().unwrap();
                let zero = vec![0; input_rate as usize / 50 * 2];
                assert!(
                    decoder
                        .resample_silk(&zero, input_rate, output_rate, Channels::Stereo)
                        .unwrap()
                        .iter()
                        .all(|sample| *sample == 0)
                );
            }
        }
    }

    #[test]
    fn same_rate_delay_is_not_removed_and_transition_is_explicit() {
        for (rate, delay) in [(8000, 4), (12000, 9), (16000, 12)] {
            let mut decoder =
                Decoder::new(SampleRate::from_hz(rate).unwrap(), Channels::Mono).unwrap();
            let mut input = vec![0; rate as usize / 100];
            input[0] = 1234;
            let output = decoder
                .resample_silk(&input, rate, rate, Channels::Mono)
                .unwrap();
            assert_eq!(output[delay], 1234);
            assert!(output[..delay].iter().all(|sample| *sample == 0));
            assert_eq!(output.len(), input.len());
            assert!(
                decoder
                    .resample_silk(
                        &vec![0; 160],
                        if rate == 8000 { 16000 } else { 8000 },
                        rate,
                        Channels::Mono
                    )
                    .is_err()
            );
        }
    }
}
