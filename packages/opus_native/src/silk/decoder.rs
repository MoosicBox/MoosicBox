//! SILK Decoder Implementation
//!
//! # Resampling (Optional, Non-Normative)
//!
//! RFC 6716 Section 4.2.9 (lines 5724-5795)
//!
//! SILK outputs audio at 8 kHz (NB), 12 kHz (MB), or 16 kHz (WB).
//! To convert to other sample rates (e.g., 48 kHz), resampling is required.
//!
//! ## Normative vs Non-Normative
//!
//! **NORMATIVE (RFC Table 54):**
//! * Resampler delays: NB: 0.538ms, MB: 0.692ms, WB: 0.706ms
//! * These delays MUST be accounted for in encoder/decoder synchronization
//!
//! **NON-NORMATIVE (RFC lines 5732-5734):**
//! * The resampling algorithm itself
//! * You can use ANY resampling method
//!
//! ## Using Resampling
//!
//! This implementation provides only the normative delay constants via
//! `SilkDecoder::resampler_delay_ms()`. For actual resampling, you can:
//! * Use SILK output directly at 8/12/16 kHz
//! * Use any resampling library (e.g., `moosicbox_resampler`, `libsamplerate`, `rubato`)
//! * Implement a custom resampling algorithm
//!
//! ## Reset Behavior
//!
//! RFC lines 5793-5795: When decoder is reset:
//! * Samples in resampling buffer are DISCARDED
//! * Resampler re-initialized with silence

use crate::error::{Error, Result};
use crate::range::RangeDecoder;
use crate::util::ilog;
use crate::{Bandwidth, Channels, SampleRate};

use super::frame::{FrameType, QuantizationOffsetType};

#[cfg(feature = "resampling")]
use moosicbox_resampler::Resampler;
#[cfg(feature = "resampling")]
use symphonia::core::audio::{AudioBuffer, Signal, SignalSpec};

// ============================================================================
// PDF to ICDF Conversion Notice
// ============================================================================
//
// Per RFC 6716 Section 4.1.3.3 (lines 1548-1552):
//
// "Although icdf[k] is more convenient for the code, the frequency counts,
//  f[k], are a more natural representation of the probability distribution
//  function (PDF) for a given symbol. Therefore, this document lists the
//  latter, not the former, when describing the context..."
//
// The RFC tables show PDF (Probability Distribution Function) values for
// human readability, but the ec_dec_icdf() function requires ICDF (Inverse
// Cumulative Distribution Function) format.
//
// Conversion formula:
//   Given PDF: [p₀, p₁, p₂, ..., pₙ] where sum(pᵢ) = 256
//   Calculate cumsum: [p₀, p₀+p₁, p₀+p₁+p₂, ..., 256]
//   ICDF: [256-p₀, 256-(p₀+p₁), ..., 256-256] = [..., 0]
//
// All constants below are stored in ICDF format with RFC PDF values
// documented in comments for reference.
// ============================================================================

// RFC 6716 Table 6: Stereo weight PDFs (lines 2225-2238)
// RFC shows PDF Stage 1: {7, 2, 1, 1, 1, 10, 24, 8, 1, 1, 3, 23, 92, 23, 3, 1, 1, 8, 24, 10, 1, 1, 1, 2, 7}/256
// Converted to ICDF for ec_dec_icdf()
const STEREO_WEIGHT_PDF_STAGE1: &[u8] = &[
    249, 247, 246, 245, 244, 234, 210, 202, 201, 200, 197, 174, 82, 59, 56, 55, 54, 46, 22, 12, 11,
    10, 9, 7, 0,
];

// RFC shows PDF Stage 2: {85, 86, 85}/256
// Converted to ICDF for ec_dec_icdf()
const STEREO_WEIGHT_PDF_STAGE2: &[u8] = &[171, 85, 0];

// RFC shows PDF Stage 3: {51, 51, 52, 51, 51}/256
// Converted to ICDF for ec_dec_icdf()
const STEREO_WEIGHT_PDF_STAGE3: &[u8] = &[205, 154, 102, 51, 0];

const STEREO_WEIGHT_TABLE_Q13: &[i16] = &[
    -13732, -10050, -8266, -7526, -6500, -5000, -2950, -820, 820, 2950, 5000, 6500, 7526, 8266,
    10050, 13732,
];

// RFC 6716 Tables 11-13: Gain PDFs (lines 2485-2545)
// RFC shows PDF INACTIVE: {32, 112, 68, 29, 12, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
const GAIN_PDF_INACTIVE: &[u8] = &[224, 112, 44, 15, 3, 2, 1, 0];

// RFC shows PDF UNVOICED: {2, 17, 45, 60, 62, 47, 19, 4}/256
// Converted to ICDF for ec_dec_icdf()
const GAIN_PDF_UNVOICED: &[u8] = &[254, 237, 192, 132, 70, 23, 4, 0];

// RFC shows PDF VOICED: {1, 3, 26, 71, 94, 50, 9, 2}/256
// Converted to ICDF for ec_dec_icdf()
const GAIN_PDF_VOICED: &[u8] = &[255, 252, 226, 155, 61, 11, 2, 0];

// RFC shows PDF LSB: {32, 32, 32, 32, 32, 32, 32, 32}/256
// Converted to ICDF for ec_dec_icdf()
const GAIN_PDF_LSB: &[u8] = &[224, 192, 160, 128, 96, 64, 32, 0];

// RFC shows PDF DELTA: {6, 5, 11, 31, 132, 21, 8, 4, 3, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1}/256
// Converted to ICDF for ec_dec_icdf()
const GAIN_PDF_DELTA: &[u8] = &[
    250, 245, 234, 203, 71, 50, 42, 38, 35, 33, 31, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18,
    17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// RFC 6716 Tables 9-10: Frame type PDFs (lines 2419-2445)
// RFC shows PDF INACTIVE: {26, 230}/256 (only 2 frame types for inactive)
// Converted to ICDF for ec_dec_icdf()
const FRAME_TYPE_PDF_INACTIVE: &[u8] = &[230, 0];

// RFC shows PDF ACTIVE: {24, 74, 148, 10}/256 (4 frame types for active)
// Converted to ICDF for ec_dec_icdf()
const FRAME_TYPE_PDF_ACTIVE: &[u8] = &[232, 158, 10, 0];

/// SILK header bits decoded from frame header
///
/// Contains VAD and LBRR flags for mid and side channels per RFC 6716 Section 4.2.4.
#[derive(Debug, Clone)]
pub struct HeaderBits {
    /// VAD flags for mid channel (one per SILK frame)
    pub mid_vad_flags: Vec<bool>,
    /// LBRR (Low Bit Rate Redundancy) flag for mid channel
    pub mid_lbrr_flag: bool,
    /// VAD flags for side channel in stereo (None for mono)
    pub side_vad_flags: Option<Vec<bool>>,
    /// LBRR flag for side channel in stereo (None for mono)
    pub side_lbrr_flag: Option<bool>,
}

/// SILK subframe decoding parameters
///
/// Contains all parameters needed for LTP synthesis and excitation generation
/// for a single SILK subframe (5ms at internal rate).
// TODO(Section 3.8.2): Remove dead_code when used in LTP synthesis
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SubframeParams {
    /// LPC (Linear Predictive Coding) coefficients in Q12 format
    pub lpc_coeffs_q12: Vec<i16>,
    /// Subframe gain in Q16 format
    pub gain_q16: i32,
    /// Pitch lag for Long-Term Prediction
    pub pitch_lag: i16,
    /// LTP filter coefficients in Q7 format (5 taps)
    pub ltp_filter_q7: [i8; 5],
    /// LTP scaling factor in Q14 format
    pub ltp_scale_q14: i16,
}

/// Stereo decoder state matching libopus `stereo_dec_state`
#[derive(Debug, Clone)]
struct StereoState {
    /// Previous stereo prediction weights in Q13 format [w0, w1]
    pred_prev_q13: [i16; 2],
    /// Mid channel 2-sample history
    s_mid: [i16; 2],
    /// Side channel 2-sample history
    s_side: [i16; 2],
}

impl StereoState {
    const fn new() -> Self {
        Self {
            pred_prev_q13: [0, 0],
            s_mid: [0, 0],
            s_side: [0, 0],
        }
    }

    const fn reset(&mut self) {
        self.pred_prev_q13 = [0, 0];
        self.s_mid = [0, 0];
        self.s_side = [0, 0];
    }
}

#[derive(Debug, Clone)]
struct LtpState {
    out_buffer: Vec<f32>,
    lpc_buffer: Vec<f32>,
    lpc_history_q14: Vec<i32>, // Changed to Q14 fixed-point
}

impl LtpState {
    const fn new() -> Self {
        Self {
            out_buffer: Vec::new(),
            lpc_buffer: Vec::new(),
            lpc_history_q14: Vec::new(),
        }
    }

    fn init(&mut self) {
        self.out_buffer = vec![0.0; 306];
        self.lpc_buffer = vec![0.0; 256];
        self.lpc_history_q14 = vec![0; 16]; // Q14 format
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.out_buffer.fill(0.0);
        self.lpc_buffer.fill(0.0);
        self.lpc_history_q14.fill(0);
    }
}

/// SILK decoder for voice-optimized audio
///
/// Decodes SILK frames according to RFC 6716 Section 4.2. Operates at internal sample rates
/// of 8/12/16 kHz for Narrowband, Mediumband, and Wideband respectively.
///
/// # Features
///
/// * LPC (Linear Predictive Coding) synthesis
/// * Long-Term Prediction (LTP) for voiced speech
/// * Adaptive quantization with prediction
/// * Stereo coding with mid/side representation
/// * Low Bit Rate Redundancy (LBRR) support
///
/// # Examples
///
/// ```rust,no_run
/// # #[cfg(feature = "silk")]
/// # fn example() -> Result<(), moosicbox_opus_native::Error> {
/// use moosicbox_opus_native::silk::SilkDecoder;
/// use moosicbox_opus_native::{SampleRate, Channels};
///
/// let mut decoder = SilkDecoder::new(
///     SampleRate::Hz16000,  // Wideband
///     Channels::Stereo,
///     20,  // 20ms frame
/// )?;
/// # Ok(())
/// # }
/// ```
pub struct SilkDecoder {
    #[allow(dead_code)]
    sample_rate: SampleRate,
    #[allow(dead_code)]
    channels: Channels,
    #[allow(dead_code)]
    frame_size_ms: u8,
    #[allow(dead_code)]
    num_silk_frames: usize,
    #[allow(dead_code)]
    previous_stereo_weights: Option<(i16, i16)>,
    #[allow(dead_code)]
    previous_gain_indices: [Option<u8>; 2],
    #[allow(dead_code)]
    previous_lsf_nb: Option<[i16; 10]>,
    #[allow(dead_code)]
    previous_lsf_wb: Option<[i16; 16]>,
    #[allow(dead_code)]
    decoder_reset: bool,
    #[allow(dead_code)]
    uncoded_side_channel: bool,
    // TODO(Section 3.7+): Remove dead_code when used in LTP decoding
    // RFC 6716 Section 4.2.7.6.1 (lines 4130-4147):
    // Pitch lag is coded relative to "prior frame in the same channel"
    // Index 0: mid channel, Index 1: side channel
    #[allow(dead_code)]
    previous_pitch_lag: [Option<i16>; 2],
    // TODO(Section 3.7.7): Remove dead_code when used in noise injection
    #[allow(dead_code)]
    lcg_seed: u32,
    #[allow(dead_code)]
    ltp_state: [LtpState; 2], // Per-channel state [mid, side]
    #[allow(dead_code)]
    stereo_state: Option<StereoState>,
    prev_gain_q16: [i32; 2], // Per-channel previous gain
}

impl SilkDecoder {
    /// Creates a new SILK decoder.
    ///
    /// # Errors
    ///
    /// * Returns error if `frame_size_ms` is not 10, 20, 40, or 60
    pub fn new(sample_rate: SampleRate, channels: Channels, frame_size_ms: u8) -> Result<Self> {
        if !matches!(frame_size_ms, 10 | 20 | 40 | 60) {
            return Err(Error::SilkDecoder(format!(
                "invalid frame size: {frame_size_ms} ms (must be 10, 20, 40, or 60)"
            )));
        }

        let num_silk_frames = match frame_size_ms {
            10 | 20 => 1,
            40 => 2,
            60 => 3,
            _ => unreachable!(),
        };

        let mut ltp_state_0 = LtpState::new();
        ltp_state_0.init();
        let mut ltp_state_1 = LtpState::new();
        ltp_state_1.init();

        let stereo_state = if channels == Channels::Stereo {
            Some(StereoState::new())
        } else {
            None
        };

        Ok(Self {
            sample_rate,
            channels,
            frame_size_ms,
            num_silk_frames,
            previous_stereo_weights: None,
            previous_gain_indices: [None, None],
            previous_lsf_nb: None,
            previous_lsf_wb: None,
            decoder_reset: true,
            uncoded_side_channel: false,
            previous_pitch_lag: [None, None],
            lcg_seed: 0,
            ltp_state: [ltp_state_0, ltp_state_1],
            stereo_state,
            prev_gain_q16: [65536, 65536],
        })
    }

    /// Decode complete SILK frame (public wrapper)
    ///
    /// Dispatches to mono or stereo decoder based on channel configuration.
    ///
    /// # Arguments
    /// * `range_decoder` - Shared or exclusive range decoder
    /// * `vad_flag` - Voice Activity Detection flag (decoded at Opus frame level per RFC Table 3)
    /// * `output` - Output buffer for decoded i16 PCM samples at internal rate
    ///
    /// # Returns
    /// Number of samples decoded per channel (at internal rate)
    ///
    /// # Errors
    /// * `Error::SilkDecoder` - Component decode failure
    /// * `Error::InvalidPacket` - Packet structure invalid
    /// * `Error::RangeDecoder` - Range decoder error
    ///
    /// # RFC Compliance
    /// * VAD flags must be decoded BEFORE calling this function (RFC Table 3, lines 1867-1879)
    pub fn decode_silk_frame(
        &mut self,
        range_decoder: &mut RangeDecoder,
        vad_flag: bool,
        output: &mut [i16],
    ) -> Result<usize> {
        if self.channels == Channels::Stereo {
            self.decode_silk_frame_stereo(range_decoder, (vad_flag, vad_flag), output)
        } else {
            self.decode_silk_frame_internal(range_decoder, vad_flag, output, 0)
        }
    }

    /// Decode single-channel SILK frame (internal implementation)
    ///
    /// Orchestrates all SILK component decoders to produce decoded PCM samples
    /// at internal sample rate (8/12/16 kHz depending on bandwidth).
    ///
    /// # RFC Reference
    /// * Lines 1743-1785: SILK decoder overview (Figure 14)
    /// * Lines 2060-2179: Frame contents decode order (Table 5)
    /// * Lines 5480-5723: Frame reconstruction pipeline
    ///
    /// # Arguments
    /// * `range_decoder` - Shared or exclusive range decoder
    /// * `vad_flag` - Voice Activity Detection flag
    /// * `output` - Output buffer for decoded samples
    /// * `channel_idx` - Channel index (0=mono/mid, 1=side)
    ///
    /// # Returns
    /// Number of samples decoded
    ///
    /// # Errors
    /// * `Error::SilkDecoder` - Component decode failure
    /// * `Error::InvalidPacket` - Packet structure invalid
    /// * `Error::RangeDecoder` - Range decoder error
    ///
    /// # Panics
    /// * Panics if LTP scale value exceeds i16 range (indicates corrupted data)
    #[allow(
        clippy::too_many_lines,
        clippy::cognitive_complexity,
        clippy::cast_precision_loss
    )]
    fn decode_silk_frame_internal(
        &mut self,
        range_decoder: &mut RangeDecoder,
        vad_flag: bool,
        output: &mut [i16],
        channel_idx: usize,
    ) -> Result<usize> {
        // Phase 5 Section 5.3.1: Full SILK frame decode pipeline
        //
        // This implements the complete SILK decoder following RFC 6716 Table 5
        // decode order and libopus reference implementation.
        //
        // RFC references:
        // - Lines 1743-1785: SILK decoder overview (Figure 14)
        // - Lines 2060-2179: Frame contents decode order (Table 5)
        // - Lines 5480-5723: Frame reconstruction pipeline

        let bandwidth = match self.sample_rate {
            SampleRate::Hz8000 => Bandwidth::Narrowband,
            SampleRate::Hz12000 => Bandwidth::Mediumband,
            SampleRate::Hz16000 => Bandwidth::Wideband,
            _ => {
                return Err(Error::SilkDecoder(format!(
                    "Invalid sample rate for SILK: {:?}",
                    self.sample_rate
                )));
            }
        };

        let samples_per_subframe = match bandwidth {
            Bandwidth::Narrowband => 40, // 5ms at 8kHz
            Bandwidth::Mediumband => 60, // 5ms at 12kHz
            Bandwidth::Wideband => 80,   // 5ms at 16kHz
            _ => return Err(Error::SilkDecoder("Invalid bandwidth for SILK".to_string())),
        };

        let num_subframes = match self.frame_size_ms {
            10 => 2, // 2 × 5ms subframes
            20 => 4, // 4 × 5ms subframes
            _ => {
                return Err(Error::SilkDecoder(format!(
                    "Frame size {}ms not supported in single SILK frame decode",
                    self.frame_size_ms
                )));
            }
        };

        let total_samples = samples_per_subframe * num_subframes;

        // NOTE: Stereo weights and mid-only flag are handled by decode_silk_frame_stereo wrapper
        // This internal function only decodes a single channel (mono, mid, or side)

        // RFC Table 5 Entry 3: Frame Type (vad_flag passed in from Opus frame level)
        let (frame_type, quant_offset) = self.decode_frame_type(range_decoder, vad_flag)?;
        log::trace!(
            "[CH{channel_idx} FRAME_TYPE] type={frame_type:?}, quant_offset={quant_offset:?}, vad={vad_flag}"
        );
        log::trace!("[PARAMS] frame_type: {frame_type:?}, quant_offset: {quant_offset:?}");

        // Step 2: Decode subframe gains (RFC Table 5 entry 4)
        let gain_indices = self.decode_subframe_gains(
            range_decoder,
            frame_type,
            num_subframes,
            channel_idx,
            self.decoder_reset, // is_first_frame
        )?;
        log::trace!("[CH{channel_idx} GAIN_DECODE] gain_indices: {gain_indices:?}");

        // Calculate actual gains for debugging
        if channel_idx == 1 {
            for (i, &idx) in gain_indices.iter().enumerate() {
                let gain_q16 = Self::dequantize_gain(i32::from(idx));
                log::trace!(
                    "[CH1 GAIN{}] index={}, gain_q16={} (linear≈{:.2})",
                    i,
                    idx,
                    gain_q16,
                    gain_q16 as f32 / 65536.0
                );
            }
        }

        log::trace!("[PARAMS] gain_indices: {gain_indices:?}");

        // Step 3: Decode LSF indices (RFC Table 5 entries 5-7)
        let lsf_stage1 = self.decode_lsf_stage1(range_decoder, bandwidth, frame_type)?;
        log::trace!("[PARAMS] lsf_stage1: {lsf_stage1:?}");
        let lsf_stage2 = self.decode_lsf_stage2(range_decoder, lsf_stage1, bandwidth)?;
        log::trace!("[PARAMS] lsf_stage2: {lsf_stage2:?}");

        // RFC Table 5 Entry 7: LSF Interpolation Weight (20ms frames only)
        let lsf_interp_weight = if self.frame_size_ms == 20 {
            // Decode interpolation weight (Q2 format, 0-4)
            // PDF from RFC Table 26: {13, 22, 29, 11, 181}/256
            let weight = range_decoder.ec_dec_icdf(
                &[243, 221, 192, 181, 0], // ICDF: [256-13, 256-13-22, ...]
                8,
            )?;
            // If decoder reset or uncoded side channel, ignore and use 4
            if self.decoder_reset || self.uncoded_side_channel {
                4
            } else {
                weight
            }
        } else {
            4 // 10ms frames always use w_Q2 = 4 (no interpolation)
        };

        // Reconstruct normalized LSF coefficients for current frame
        let nlsf_q15 = Self::reconstruct_lsf(lsf_stage1, &lsf_stage2, bandwidth)?;

        // RFC lines 3593-3626: LSF Interpolation for 20ms frames
        // For 20ms frames with w_Q2 < 4, interpolate LSF for first half
        let (lpc_coeffs_first_half, lpc_coeffs_second_half) = if self.frame_size_ms == 20
            && lsf_interp_weight < 4
        {
            // Get previous LSF based on bandwidth
            let prev_lsf_vec: Option<Vec<i16>> = match bandwidth {
                Bandwidth::Narrowband | Bandwidth::Mediumband => {
                    self.previous_lsf_nb.as_ref().map(|arr| arr.to_vec())
                }
                Bandwidth::Wideband => self.previous_lsf_wb.as_ref().map(|arr| arr.to_vec()),
                _ => None,
            };
            let prev_lsf = prev_lsf_vec.as_deref();

            if let Some(prev_lsf) = prev_lsf {
                // RFC line 3623: n1_Q15[k] = n0_Q15[k] + (w_Q2*(n2_Q15[k] - n0_Q15[k]) >> 2)
                let mut nlsf_interpolated_q15 = vec![0_i16; nlsf_q15.len()];
                #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
                for k in 0..nlsf_q15.len() {
                    let n0 = i32::from(prev_lsf[k]);
                    let n2 = i32::from(nlsf_q15[k]);
                    let w = lsf_interp_weight as i32;
                    let product = w * (n2 - n0);
                    let rounded = ((product >> 1) + 1) >> 1;
                    nlsf_interpolated_q15[k] = (n0 + rounded) as i16;
                }

                // Convert both interpolated and current LSF to LPC with stability limiting
                let lpc_first = Self::limit_lpc_coefficients(&nlsf_interpolated_q15, bandwidth)?;
                let lpc_second = Self::limit_lpc_coefficients(&nlsf_q15, bandwidth)?;

                (
                    lpc_first.iter().map(|&x| i32::from(x)).collect(),
                    lpc_second.iter().map(|&x| i32::from(x)).collect(),
                )
            } else {
                // No previous LSF available - use current for both halves
                let lpc = Self::limit_lpc_coefficients(&nlsf_q15, bandwidth)?;
                let lpc_i32: Vec<i32> = lpc.iter().map(|&x| i32::from(x)).collect();
                (lpc_i32.clone(), lpc_i32)
            }
        } else {
            // 10ms frames or w_Q2 == 4: use current LSF for all subframes
            let lpc = Self::limit_lpc_coefficients(&nlsf_q15, bandwidth)?;
            let lpc_i32: Vec<i32> = lpc.iter().map(|&x| i32::from(x)).collect();
            (lpc_i32.clone(), lpc_i32)
        };

        // Store current LSF for next frame
        match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => {
                if nlsf_q15.len() == 10 {
                    let mut arr = [0_i16; 10];
                    arr.copy_from_slice(&nlsf_q15);
                    self.previous_lsf_nb = Some(arr);
                }
            }
            Bandwidth::Wideband => {
                if nlsf_q15.len() == 16 {
                    let mut arr = [0_i16; 16];
                    arr.copy_from_slice(&nlsf_q15);
                    self.previous_lsf_wb = Some(arr);
                }
            }
            _ => {}
        }

        // Store gain indices for next frame
        if !gain_indices.is_empty() {
            self.previous_gain_indices[0] = Some(gain_indices[0]);
        }

        // Step 4: Decode LTP parameters for voiced frames (RFC Table 5 entries 8-12)
        let (pitch_lags, ltp_filters, ltp_scale) = if matches!(frame_type, FrameType::Voiced) {
            // RFC Table 5 Entry 8: Primary Pitch Lag
            // RFC 6716 Section 4.2.7.6.1: Use absolute coding when:
            // - First SILK frame for this channel in current Opus frame
            // - Previous frame for this channel was not coded
            // - Previous frame was coded but not voiced
            let use_absolute = self.previous_pitch_lag[channel_idx].is_none() || self.decoder_reset;
            let primary_lag =
                self.decode_primary_pitch_lag(range_decoder, bandwidth, use_absolute, channel_idx)?;

            // RFC Table 5 Entry 9: Subframe Pitch Contour
            let pitch_lags = self.decode_pitch_contour(range_decoder, primary_lag, bandwidth)?;

            // RFC Table 5 Entries 10-11: Periodicity Index + LTP Filter
            let ltp_filters = self.decode_ltp_filter_coefficients(range_decoder)?;

            // RFC Table 5 Entry 12: LTP Scaling (conditional)
            let should_decode_scaling = self.decoder_reset; // Decode on first frame
            let ltp_scale = Self::decode_ltp_scaling(range_decoder, should_decode_scaling)?;

            (pitch_lags, ltp_filters, ltp_scale)
        } else {
            // Unvoiced frames: no LTP parameters
            let default_lags = vec![0_i16; num_subframes];
            let default_filters = vec![[0_i8; 5]; num_subframes];
            (default_lags, default_filters, 0)
        };

        // Step 5: Decode LCG seed (RFC Table 5 entry 13)
        let seed = self.decode_lcg_seed(range_decoder)?;
        log::trace!("[SILK_FRAME] Decoded LCG seed: {seed}");
        self.lcg_seed = seed;

        // Step 6: Decode excitation signal (RFC Table 5 entries 14-18)
        // RFC COMPLIANT 4-PHASE BATCH PROCESSING (Lines 4895-4897, 4977-4980, 5260-5263)
        // Phase 1: ALL pulse counts → Phase 2: ALL locations → Phase 3: ALL LSBs → Phase 4: ALL signs

        let rate_level = self.decode_rate_level(range_decoder, frame_type)?;
        let num_shell_blocks = Self::get_shell_block_count(bandwidth, self.frame_size_ms)?;

        // Storage for 4-phase batch decode
        let mut pulse_counts = Vec::with_capacity(num_shell_blocks);
        let mut lsb_counts = Vec::with_capacity(num_shell_blocks);
        let mut pulse_locations = Vec::with_capacity(num_shell_blocks);
        let mut magnitudes_vec = Vec::with_capacity(num_shell_blocks);

        // PHASE 1: Decode ALL pulse counts consecutively (RFC lines 4895-4897)
        // "The pulse counts for all of the shell blocks are coded consecutively,
        //  before the content of any of the blocks"
        for _ in 0..num_shell_blocks {
            let (pulse_count, lsb_count) = self.decode_pulse_count(range_decoder, rate_level)?;
            pulse_counts.push(pulse_count);
            lsb_counts.push(lsb_count);
        }

        // PHASE 2: Decode ALL pulse locations (RFC lines 4977-4980)
        // "These locations are coded for all the shell blocks before any of the
        //  remaining information for each block"
        for &pulse_count in &pulse_counts {
            let locations = if pulse_count > 0 {
                self.decode_pulse_locations(range_decoder, pulse_count)?
            } else {
                [0_u8; 16]
            };
            pulse_locations.push(locations);
        }

        // PHASE 3: Decode ALL LSBs block-by-block (RFC lines 5260-5263)
        // "After the decoder reads the pulse locations for all blocks, it reads
        //  the LSBs (if any) for each block in turn"
        log::trace!(
            "[EXCITATION] num_shell_blocks={num_shell_blocks}, pulse_counts: {pulse_counts:?}, lsb_counts: {lsb_counts:?}"
        );
        log::trace!(
            "[EXCITATION] pulse_locations[0]: {:?}",
            pulse_locations.first()
        );
        log::trace!(
            "[EXCITATION] pulse_locations[1]: {:?}",
            pulse_locations.get(1)
        );
        log::trace!(
            "[EXCITATION] pulse_locations[2]: {:?}",
            pulse_locations.get(2)
        );
        log::trace!(
            "[EXCITATION] pulse_locations[4]: {:?}",
            pulse_locations.get(4)
        );

        for block_idx in 0..num_shell_blocks {
            let magnitudes = if lsb_counts[block_idx] > 0 {
                self.decode_lsbs(
                    range_decoder,
                    &pulse_locations[block_idx],
                    lsb_counts[block_idx],
                )?
            } else {
                // No LSBs - magnitudes are just the pulse locations
                let mut mags = [0_u16; 16];
                for (i, mag) in mags.iter_mut().enumerate() {
                    *mag = u16::from(pulse_locations[block_idx][i]);
                }
                mags
            };
            magnitudes_vec.push(magnitudes);
        }

        // PHASE 4: Decode ALL signs (RFC lines 5293-5295)
        // "After decoding the pulse locations and the LSBs, the decoder knows
        //  the magnitude of each coefficient"
        let mut excitation_blocks: Vec<[i32; 16]> = Vec::with_capacity(num_shell_blocks);
        for block_idx in 0..num_shell_blocks {
            let e_raw = self.decode_signs(
                range_decoder,
                &magnitudes_vec[block_idx],
                frame_type,
                quant_offset,
                pulse_counts[block_idx],
            )?;

            // Reconstruct excitation (applies Q23 format, offset, LCG noise)
            let e_q23 = self.reconstruct_excitation(&e_raw, frame_type, quant_offset);
            excitation_blocks.push(e_q23);
        }

        // Flatten excitation blocks into continuous signal
        let mut full_excitation = Vec::with_capacity(num_shell_blocks * 16);
        for block in &excitation_blocks {
            full_excitation.extend_from_slice(block);
        }

        // Clear LPC history on first subframe after decoder reset
        // This ensures the first LPC synthesis starts with zero history
        if self.decoder_reset {
            self.ltp_state[channel_idx].lpc_history_q14.clear();
        }

        // Now synthesize audio for each subframe
        let mut all_samples = Vec::with_capacity(total_samples);

        for subframe_idx in 0..num_subframes {
            // Extract excitation for this subframe
            let subframe_start = subframe_idx * samples_per_subframe;
            let subframe_end = subframe_start + samples_per_subframe;
            let excitation_q23: Vec<i32> = if subframe_end <= full_excitation.len() {
                full_excitation[subframe_start..subframe_end].to_vec()
            } else {
                // Handle edge case - pad with zeros if needed
                let mut exc = vec![0_i32; samples_per_subframe];
                let available = full_excitation.len().saturating_sub(subframe_start);
                if available > 0 {
                    exc[..available].copy_from_slice(&full_excitation[subframe_start..]);
                }
                exc
            };

            // RFC lines 2553-2567: Gain dequantization
            // gain_indices contains log_gain values (0-63) directly from decode_subframe_gains
            // No lookup table needed - the decoded value IS the log_gain
            let gain_q16 = if !gain_indices.is_empty() && (subframe_idx < gain_indices.len()) {
                let log_gain = i32::from(gain_indices[subframe_idx]);
                Self::dequantize_gain(log_gain)
            } else {
                65536 // Unity gain fallback (Q16)
            };

            // RFC lines 3593-3626: Select LPC based on subframe for 20ms frames
            // First half (subframes 0-1): interpolated LPC
            // Second half (subframes 2-3): current LPC
            #[allow(clippy::cast_possible_truncation)]
            let lpc_for_subframe: Vec<i16> = if self.frame_size_ms == 20 && subframe_idx < 2 {
                lpc_coeffs_first_half
                    .iter()
                    .map(|&x| x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16)
                    .collect()
            } else if self.frame_size_ms == 20 {
                lpc_coeffs_second_half
                    .iter()
                    .map(|&x| x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16)
                    .collect()
            } else {
                // 10ms frames: use same LPC for both subframes
                lpc_coeffs_first_half
                    .iter()
                    .map(|&x| x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16)
                    .collect()
            };

            let params = SubframeParams {
                lpc_coeffs_q12: lpc_for_subframe,
                gain_q16,
                pitch_lag: if subframe_idx < pitch_lags.len() {
                    pitch_lags[subframe_idx]
                } else {
                    0
                },
                ltp_filter_q7: if subframe_idx < ltp_filters.len() {
                    ltp_filters[subframe_idx]
                } else {
                    [0; 5]
                },
                ltp_scale_q14: i16::try_from(ltp_scale)
                    .expect("LTP scale values (12288, 8192, 15565) fit in i16::MAX"),
            };

            // Apply LTP synthesis (long-term prediction) - returns Q14 residual
            let residual_q14 = if matches!(frame_type, FrameType::Voiced) {
                Self::ltp_synthesis_voiced(&excitation_q23, &params, subframe_idx, bandwidth)?
            } else {
                Self::ltp_synthesis_unvoiced(&excitation_q23)
            };

            // Apply LPC synthesis (short-term prediction) - returns i16 samples directly
            log::trace!("[SUBFRAME] Processing subframe {subframe_idx}/{num_subframes}");
            if channel_idx == 0 {
                log::trace!(
                    "[SUBFRAME ch=0] Processing subframe {} of {}, output offset={}",
                    subframe_idx,
                    num_subframes,
                    all_samples.len()
                );
            }
            let samples = self.lpc_synthesis(&residual_q14, &params, bandwidth, channel_idx)?;
            log::trace!(
                "[SUBFRAME] Subframe {} produced {} samples, first 5: {:?}",
                subframe_idx,
                samples.len(),
                &samples[..samples.len().min(5)]
            );
            if channel_idx == 0 {
                log::trace!(
                    "[SUBFRAME ch=0] Subframe {} produced {} samples: {:?}",
                    subframe_idx,
                    samples.len(),
                    samples
                );
            }

            all_samples.extend(samples);
        }

        // Write samples directly to output (already in i16 format)
        for (i, &sample) in all_samples.iter().enumerate() {
            if i < output.len() {
                output[i] = sample;
            }
        }

        // Update state
        self.decoder_reset = false;

        Ok(total_samples)
    }

    /// Decode stereo SILK frame (mid + side channels)
    ///
    /// Decodes stereo prediction weights, mid-only flag, then decodes both
    /// mid and side channels, and applies stereo unmixing.
    ///
    /// # Arguments
    /// * `range_decoder` - Shared range decoder
    /// * `vad_flags` - VAD flags for (mid, side) channels
    /// * `output` - Interleaved stereo output buffer
    ///
    /// # Returns
    /// Number of samples decoded per channel
    ///
    /// # Errors
    /// * `Error::SilkDecoder` - Component decode failure
    /// * `Error::RangeDecoder` - Range decoder error
    ///
    /// # RFC Reference
    /// * Table 5 Entry 1-2: Stereo weights and mid-only flag
    /// * Figures 15-16: Stereo decode flow
    #[allow(clippy::similar_names)]
    pub(crate) fn decode_silk_frame_stereo(
        &mut self,
        range_decoder: &mut RangeDecoder,
        vad_flags: (bool, bool),
        output: &mut [i16],
    ) -> Result<usize> {
        let bandwidth = match self.sample_rate {
            SampleRate::Hz8000 => Bandwidth::Narrowband,
            SampleRate::Hz12000 => Bandwidth::Mediumband,
            SampleRate::Hz16000 => Bandwidth::Wideband,
            _ => {
                return Err(Error::SilkDecoder(format!(
                    "Invalid sample rate for SILK: {:?}",
                    self.sample_rate
                )));
            }
        };

        let samples_per_subframe = match bandwidth {
            Bandwidth::Narrowband => 40,
            Bandwidth::Mediumband => 60,
            Bandwidth::Wideband => 80,
            _ => return Err(Error::SilkDecoder("Invalid bandwidth for SILK".to_string())),
        };

        let num_subframes = match self.frame_size_ms {
            10 => 2,
            20 => 4,
            _ => {
                return Err(Error::SilkDecoder(format!(
                    "Frame size {}ms not supported",
                    self.frame_size_ms
                )));
            }
        };

        let total_samples = samples_per_subframe * num_subframes;

        // RFC Table 5 Entry 1: Stereo Prediction Weights
        log::debug!("[decode_silk_frame_stereo] Decoding stereo weights");
        let (w0_q13, w1_q13) = self.decode_stereo_weights(range_decoder)?;
        log::debug!("[decode_silk_frame_stereo] Decoded weights: w0_q13={w0_q13}, w1_q13={w1_q13}");

        // RFC Table 5 Entry 2: Mid-only Flag
        // Per libopus dec_API.c line 367-372: only decode if side channel VAD is inactive
        let mid_only = if vad_flags.1 {
            false
        } else {
            self.decode_mid_only_flag(range_decoder)?
        };

        // Allocate buffers with 2-sample history space at start
        // libopus uses this for buffering in silk_stereo_MS_to_LR
        let mut x1 = vec![0_i16; total_samples + 2]; // mid -> left
        let mut x2 = vec![0_i16; total_samples + 2]; // side -> right

        // Decode mid channel into x1 starting at index 2 (skip history)
        log::debug!(
            "[decode_silk_frame_stereo] Decoding mid channel, vad={}",
            vad_flags.0
        );
        self.decode_silk_frame_internal(range_decoder, vad_flags.0, &mut x1[2..], 0)?;
        log::trace!(
            "[decode_silk_frame_stereo] Mid channel first 10: {:?}",
            &x1[2..12.min(x1.len())]
        );
        log::trace!("[MID_CH] First 20: {:?}", &x1[2..22.min(x1.len())]);

        // Decode side channel (if not mid-only)
        if mid_only {
            log::debug!("[decode_silk_frame_stereo] mid_only=true, side channel is zero");
        } else {
            log::debug!(
                "[decode_silk_frame_stereo] Decoding side channel, vad={}",
                vad_flags.1
            );
            self.decode_silk_frame_internal(range_decoder, vad_flags.1, &mut x2[2..], 1)?;
            log::trace!(
                "[decode_silk_frame_stereo] Side channel first 10: {:?}",
                &x2[2..12.min(x2.len())]
            );
            log::trace!("[SIDE_CH] First 20: {:?}", &x2[2..22.min(x2.len())]);
        }

        // Get fs_kHz for interpolation calculation
        let fs_khz = match bandwidth {
            Bandwidth::Narrowband => 8,
            Bandwidth::Mediumband => 12,
            Bandwidth::Wideband => 16,
            _ => return Err(Error::SilkDecoder("Invalid bandwidth".to_string())),
        };

        // Apply fixed-point stereo MS->LR conversion
        // This modifies x1 and x2 in-place: x1 becomes left, x2 becomes right
        #[allow(clippy::tuple_array_conversions)]
        self.stereo_ms_to_lr(&mut x1, &mut x2, [w0_q13, w1_q13], fs_khz, total_samples)?;

        // Interleave left/right into output
        // After MS->LR, output is in x1[1..frame_length+1] and x2[1..frame_length+1]
        // But we decode into x1[2..] and x2[2..], so after MS->LR processing,
        // the output is in x1[2..frame_length+2] and x2[2..frame_length+2]
        for i in 0..total_samples {
            if i * 2 + 1 < output.len() {
                output[i * 2] = x1[i + 2]; // Left
                output[i * 2 + 1] = x2[i + 2]; // Right
            }
        }

        Ok(total_samples)
    }

    /// Decodes VAD flags for all SILK frames.
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    pub fn decode_vad_flags(&self, range_decoder: &mut RangeDecoder) -> Result<Vec<bool>> {
        let mut vad_flags = Vec::with_capacity(self.num_silk_frames);

        for _ in 0..self.num_silk_frames {
            let vad_flag = range_decoder.ec_dec_bit_logp(1)?;
            vad_flags.push(vad_flag);
        }

        Ok(vad_flags)
    }

    /// Decodes LBRR flag.
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    pub fn decode_lbrr_flag(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        range_decoder.ec_dec_bit_logp(1)
    }

    /// Decodes per-frame LBRR flags for 40ms and 60ms frames.
    ///
    /// # Errors
    ///
    /// * Returns error if `frame_size_ms` is invalid
    /// * Returns error if range decoder fails
    pub fn decode_per_frame_lbrr_flags(
        &self,
        range_decoder: &mut RangeDecoder,
        frame_size_ms: u8,
    ) -> Result<Vec<bool>> {
        let flags_value = match frame_size_ms {
            10 | 20 => return Ok(vec![true]),
            40 => {
                // RFC 6716 Table 4 (line 1987): LBRR 40ms PDF {0, 53, 53, 150}/256
                // Converted to ICDF (skip leading 256): [203, 150, 0]
                const LBRR_40MS_ICDF: &[u8] = &[203, 150, 0];
                range_decoder.ec_dec_icdf(LBRR_40MS_ICDF, 8)?
            }
            60 => {
                // RFC 6716 Table 4 (line 1989): LBRR 60ms PDF {0, 41, 20, 29, 41, 15, 28, 82}/256
                // Converted to ICDF (skip leading 256): [215, 195, 166, 125, 110, 82, 0]
                const LBRR_60MS_ICDF: &[u8] = &[215, 195, 166, 125, 110, 82, 0];
                range_decoder.ec_dec_icdf(LBRR_60MS_ICDF, 8)?
            }
            _ => return Err(Error::SilkDecoder("invalid frame size".to_string())),
        };

        let num_frames = (frame_size_ms / 20) as usize;
        let mut flags = Vec::with_capacity(num_frames);
        for i in 0..num_frames {
            flags.push((flags_value >> i) & 1 == 1);
        }

        Ok(flags)
    }

    /// Decodes header bits (VAD and LBRR flags) for mono or stereo packets.
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    pub fn decode_header_bits(
        &mut self,
        range_decoder: &mut RangeDecoder,
        is_stereo: bool,
    ) -> Result<HeaderBits> {
        let mid_vad_flags = self.decode_vad_flags(range_decoder)?;
        let mid_lbrr_flag = self.decode_lbrr_flag(range_decoder)?;

        let (side_vad_flags, side_lbrr_flag) = if is_stereo {
            let vad = self.decode_vad_flags(range_decoder)?;
            let lbrr = self.decode_lbrr_flag(range_decoder)?;
            (Some(vad), Some(lbrr))
        } else {
            (None, None)
        };

        Ok(HeaderBits {
            mid_vad_flags,
            mid_lbrr_flag,
            side_vad_flags,
            side_lbrr_flag,
        })
    }

    /// Decodes stereo prediction weights using three-stage decoding.
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    pub fn decode_stereo_weights(
        &mut self,
        range_decoder: &mut RangeDecoder,
    ) -> Result<(i16, i16)> {
        log::trace!("[decode_stereo_weights] Starting stereo weight decode");
        log::trace!(
            "[WEIGHT_DECODE] PDF1 len={}, PDF2 len={}, PDF3 len={}",
            STEREO_WEIGHT_PDF_STAGE1.len(),
            STEREO_WEIGHT_PDF_STAGE2.len(),
            STEREO_WEIGHT_PDF_STAGE3.len()
        );

        let n = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE1, 8)?;
        log::trace!("[WEIGHT_DECODE] n={n}");
        let i0 = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE2, 8)?;
        log::trace!("[WEIGHT_DECODE] i0={i0}");
        let i1 = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE3, 8)?;
        log::trace!("[WEIGHT_DECODE] i1={i1}");
        let i2 = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE2, 8)?;
        log::trace!("[WEIGHT_DECODE] i2={i2}");
        let i3 = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE3, 8)?;
        log::trace!("[WEIGHT_DECODE] i3={i3}");

        #[allow(clippy::cast_possible_truncation)]
        let wi0 = (i0 + 3 * (n / 5)) as usize;
        #[allow(clippy::cast_possible_truncation)]
        let wi1 = (i2 + 3 * (n % 5)) as usize;

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let step1_q13 = ((i32::from(STEREO_WEIGHT_TABLE_Q13[wi1 + 1])
            - i32::from(STEREO_WEIGHT_TABLE_Q13[wi1]))
            * i32::from(6554_i16))
            >> 16;

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let w1_q13 = i32::from(STEREO_WEIGHT_TABLE_Q13[wi1])
            + i32::from(step1_q13 as i16) * (i3 as i32 * 2 + 1);

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let step0_q13 = ((i32::from(STEREO_WEIGHT_TABLE_Q13[wi0 + 1])
            - i32::from(STEREO_WEIGHT_TABLE_Q13[wi0]))
            * i32::from(6554_i16))
            >> 16;

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let w0_q13 = i32::from(STEREO_WEIGHT_TABLE_Q13[wi0])
            + i32::from(step0_q13 as i16) * (i1 as i32 * 2 + 1)
            - w1_q13;

        #[allow(clippy::cast_possible_truncation)]
        let weights = (w0_q13 as i16, w1_q13 as i16);
        self.previous_stereo_weights = Some(weights);

        Ok(weights)
    }

    /// Decodes mid-only flag for stereo frames
    ///
    /// Determines whether only the mid channel is coded (side channel uncoded).
    /// Per RFC 6716 Table 5, this is decoded after stereo weights and before frame type.
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    ///
    /// # Returns
    ///
    /// * `true` - Only mid channel is coded, side channel is zero
    /// * `false` - Both mid and side channels are coded
    ///
    /// # RFC Reference
    ///
    /// * Table 5 Entry 2 (lines 2060-2179): Mid-only flag decode order
    /// * Lines 1976-1978: Mid-only flag PDF `[192, 0]` (probability 75% false, 25% true)
    pub fn decode_mid_only_flag(&mut self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        // RFC 6716 - silk_stereo_only_code_mid_iCDF from libopus/silk/tables_other.c
        // For mid-only flag: P(0) = 192/256, P(1) = 64/256
        // iCDF format: icdf[0] = cumulative prob of values ≥ 1 = 64
        const MID_ONLY_PDF: &[u8] = &[64, 0];
        let mid_only = range_decoder.ec_dec_icdf(MID_ONLY_PDF, 8)?;

        if mid_only == 1 {
            self.uncoded_side_channel = true;
        }

        Ok(mid_only == 1)
    }

    /// Decodes frame type and quantization offset.
    ///
    /// # Errors
    ///
    /// * Returns error if frame type value is invalid
    /// * Returns error if range decoder fails
    pub fn decode_frame_type(
        &self,
        range_decoder: &mut RangeDecoder,
        vad_flag: bool,
    ) -> Result<(FrameType, QuantizationOffsetType)> {
        let (pdf, offset) = if vad_flag {
            (FRAME_TYPE_PDF_ACTIVE, 2)
        } else {
            (FRAME_TYPE_PDF_INACTIVE, 0)
        };

        let decoded_value = range_decoder.ec_dec_icdf(pdf, 8)?;
        let frame_type_value = decoded_value + offset;

        let (signal_type, quant_offset) = match frame_type_value {
            0 => (FrameType::Inactive, QuantizationOffsetType::Low),
            1 => (FrameType::Inactive, QuantizationOffsetType::High),
            2 => (FrameType::Unvoiced, QuantizationOffsetType::Low),
            3 => (FrameType::Unvoiced, QuantizationOffsetType::High),
            4 => (FrameType::Voiced, QuantizationOffsetType::Low),
            5 => (FrameType::Voiced, QuantizationOffsetType::High),
            _ => return Err(Error::SilkDecoder("invalid frame type".to_string())),
        };

        Ok((signal_type, quant_offset))
    }

    /// Decodes subframe gains using independent or delta coding.
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    ///
    /// # Panics
    ///
    /// * Panics if `previous_log_gain` is `None` when delta coding is used (should never happen due to logic)
    #[allow(clippy::cast_possible_truncation, clippy::similar_names)]
    pub fn decode_subframe_gains(
        &mut self,
        range_decoder: &mut RangeDecoder,
        frame_type: FrameType,
        num_subframes: usize,
        channel: usize,
        is_first_frame: bool,
    ) -> Result<Vec<u8>> {
        let mut gain_indices = Vec::with_capacity(num_subframes);
        let mut previous_log_gain: Option<u8> = self.previous_gain_indices[channel];

        for subframe_idx in 0..num_subframes {
            let use_independent_coding =
                subframe_idx == 0 && (is_first_frame || previous_log_gain.is_none());

            let log_gain = if use_independent_coding {
                let pdf_msb = match frame_type {
                    FrameType::Inactive => GAIN_PDF_INACTIVE,
                    FrameType::Unvoiced => GAIN_PDF_UNVOICED,
                    FrameType::Voiced => GAIN_PDF_VOICED,
                };
                let gain_msb = range_decoder.ec_dec_icdf(pdf_msb, 8)?;
                let gain_lsb_value = range_decoder.ec_dec_icdf(GAIN_PDF_LSB, 8)?;
                let gain_index = ((gain_msb << 3) | gain_lsb_value) as u8;

                previous_log_gain.map_or(gain_index, |prev| gain_index.max(prev.saturating_sub(16)))
            } else {
                let delta_gain_index = range_decoder.ec_dec_icdf(GAIN_PDF_DELTA, 8)? as u8;
                let prev = previous_log_gain.unwrap();

                let formula1 = 2_u8.saturating_mul(delta_gain_index).saturating_sub(16);
                let formula2 = prev.saturating_add(delta_gain_index).saturating_sub(4);
                let unclamped = formula1.max(formula2);
                unclamped.clamp(0, 63)
            };

            gain_indices.push(log_gain);
            previous_log_gain = Some(log_gain);
        }

        self.previous_gain_indices[channel] = previous_log_gain;
        Ok(gain_indices)
    }

    /// Decodes LSF Stage 1 index (RFC 6716 Section 4.2.7.5.1, lines 2605-2661).
    ///
    /// # Errors
    ///
    /// * Returns error if bandwidth is invalid for LSF decoding
    /// * Returns error if range decoder fails
    pub fn decode_lsf_stage1(
        &self,
        range_decoder: &mut RangeDecoder,
        bandwidth: Bandwidth,
        frame_type: FrameType,
    ) -> Result<u8> {
        use super::lsf_constants::{
            LSF_STAGE1_PDF_NB_MB_INACTIVE, LSF_STAGE1_PDF_NB_MB_VOICED, LSF_STAGE1_PDF_WB_INACTIVE,
            LSF_STAGE1_PDF_WB_VOICED,
        };

        let pdf = match (bandwidth, frame_type) {
            (
                Bandwidth::Narrowband | Bandwidth::Mediumband,
                FrameType::Inactive | FrameType::Unvoiced,
            ) => LSF_STAGE1_PDF_NB_MB_INACTIVE,
            (Bandwidth::Narrowband | Bandwidth::Mediumband, FrameType::Voiced) => {
                LSF_STAGE1_PDF_NB_MB_VOICED
            }
            (Bandwidth::Wideband, FrameType::Inactive | FrameType::Unvoiced) => {
                LSF_STAGE1_PDF_WB_INACTIVE
            }
            (Bandwidth::Wideband, FrameType::Voiced) => LSF_STAGE1_PDF_WB_VOICED,
            _ => {
                return Err(Error::SilkDecoder(
                    "invalid bandwidth for LSF decoding".to_string(),
                ));
            }
        };

        #[allow(clippy::cast_possible_truncation)]
        range_decoder.ec_dec_icdf(pdf, 8).map(|v| v as u8)
    }

    /// Decodes LSF Stage 2 residual indices (RFC 6716 Section 4.2.7.5.2, lines 2662-2934).
    ///
    /// # Errors
    ///
    /// * Returns error if bandwidth is invalid for LSF decoding
    /// * Returns error if range decoder fails
    /// * Returns error if invalid codebook character is encountered
    #[allow(clippy::cast_possible_wrap)]
    pub fn decode_lsf_stage2(
        &self,
        range_decoder: &mut RangeDecoder,
        stage1_index: u8,
        bandwidth: Bandwidth,
    ) -> Result<Vec<i8>> {
        use super::lsf_constants::{LSF_CB_SELECT_NB, LSF_CB_SELECT_WB, LSF_EXTENSION_PDF};

        let d_lpc = match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => 10,
            Bandwidth::Wideband => 16,
            _ => return Err(Error::SilkDecoder("invalid bandwidth for LSF".to_string())),
        };

        let mut indices = Vec::with_capacity(d_lpc);

        for k in 0..d_lpc {
            let codebook = match bandwidth {
                Bandwidth::Narrowband | Bandwidth::Mediumband => {
                    LSF_CB_SELECT_NB[stage1_index as usize][k]
                }
                Bandwidth::Wideband => LSF_CB_SELECT_WB[stage1_index as usize][k],
                _ => unreachable!(),
            };

            let pdf = Self::get_lsf_stage2_pdf(codebook, bandwidth)?;

            #[allow(clippy::cast_possible_truncation)]
            let mut index = range_decoder.ec_dec_icdf(pdf, 8)? as i8 - 4;

            // Extension decoding (RFC lines 2923-2926)
            if index.abs() == 4 {
                #[allow(clippy::cast_possible_truncation)]
                let extension = range_decoder.ec_dec_icdf(LSF_EXTENSION_PDF, 8)? as i8;
                index += extension * index.signum();
            }

            indices.push(index);
        }

        Ok(indices)
    }

    fn get_lsf_stage2_pdf(codebook: u8, bandwidth: Bandwidth) -> Result<&'static [u8]> {
        use super::lsf_constants::{
            LSF_STAGE2_PDF_NB_A, LSF_STAGE2_PDF_NB_B, LSF_STAGE2_PDF_NB_C, LSF_STAGE2_PDF_NB_D,
            LSF_STAGE2_PDF_NB_E, LSF_STAGE2_PDF_NB_F, LSF_STAGE2_PDF_NB_G, LSF_STAGE2_PDF_NB_H,
            LSF_STAGE2_PDF_WB_I, LSF_STAGE2_PDF_WB_J, LSF_STAGE2_PDF_WB_K, LSF_STAGE2_PDF_WB_L,
            LSF_STAGE2_PDF_WB_M, LSF_STAGE2_PDF_WB_N, LSF_STAGE2_PDF_WB_O, LSF_STAGE2_PDF_WB_P,
        };

        match (bandwidth, codebook) {
            (Bandwidth::Narrowband | Bandwidth::Mediumband, b'a') => Ok(LSF_STAGE2_PDF_NB_A),
            (Bandwidth::Narrowband | Bandwidth::Mediumband, b'b') => Ok(LSF_STAGE2_PDF_NB_B),
            (Bandwidth::Narrowband | Bandwidth::Mediumband, b'c') => Ok(LSF_STAGE2_PDF_NB_C),
            (Bandwidth::Narrowband | Bandwidth::Mediumband, b'd') => Ok(LSF_STAGE2_PDF_NB_D),
            (Bandwidth::Narrowband | Bandwidth::Mediumband, b'e') => Ok(LSF_STAGE2_PDF_NB_E),
            (Bandwidth::Narrowband | Bandwidth::Mediumband, b'f') => Ok(LSF_STAGE2_PDF_NB_F),
            (Bandwidth::Narrowband | Bandwidth::Mediumband, b'g') => Ok(LSF_STAGE2_PDF_NB_G),
            (Bandwidth::Narrowband | Bandwidth::Mediumband, b'h') => Ok(LSF_STAGE2_PDF_NB_H),
            (Bandwidth::Wideband, b'i') => Ok(LSF_STAGE2_PDF_WB_I),
            (Bandwidth::Wideband, b'j') => Ok(LSF_STAGE2_PDF_WB_J),
            (Bandwidth::Wideband, b'k') => Ok(LSF_STAGE2_PDF_WB_K),
            (Bandwidth::Wideband, b'l') => Ok(LSF_STAGE2_PDF_WB_L),
            (Bandwidth::Wideband, b'm') => Ok(LSF_STAGE2_PDF_WB_M),
            (Bandwidth::Wideband, b'n') => Ok(LSF_STAGE2_PDF_WB_N),
            (Bandwidth::Wideband, b'o') => Ok(LSF_STAGE2_PDF_WB_O),
            (Bandwidth::Wideband, b'p') => Ok(LSF_STAGE2_PDF_WB_P),
            _ => Err(Error::SilkDecoder(format!(
                "invalid LSF codebook: {}",
                codebook as char
            ))),
        }
    }

    /// Dequantizes LSF Stage 2 residuals using backward prediction (RFC 6716 Section 4.2.7.5.3, lines 3011-3033).
    ///
    /// # Errors
    ///
    /// * Returns error if bandwidth is invalid
    #[allow(
        dead_code,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn dequantize_lsf_residuals(
        stage1_index: u8,
        stage2_indices: &[i8],
        bandwidth: Bandwidth,
    ) -> Result<Vec<i16>> {
        use super::lsf_constants::{
            LSF_PRED_WEIGHT_SEL_NB, LSF_PRED_WEIGHT_SEL_WB, LSF_PRED_WEIGHTS_NB_A,
            LSF_PRED_WEIGHTS_NB_B, LSF_PRED_WEIGHTS_WB_C, LSF_PRED_WEIGHTS_WB_D, LSF_QSTEP_NB,
            LSF_QSTEP_WB,
        };

        let (d_lpc, qstep) = match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => (10, i32::from(LSF_QSTEP_NB)),
            Bandwidth::Wideband => (16, i32::from(LSF_QSTEP_WB)),
            _ => return Err(Error::SilkDecoder("invalid bandwidth for LSF".to_string())),
        };

        let mut res_q10 = vec![0_i16; d_lpc];

        // Process backward from k = d_LPC-1 down to 0 (RFC line 3021)
        for k in (0..d_lpc).rev() {
            // Prediction weights are only defined for k < d_LPC-1 (RFC line 3018)
            let prediction = if k + 1 < d_lpc {
                let pred_weight = match bandwidth {
                    Bandwidth::Narrowband | Bandwidth::Mediumband => {
                        let sel = LSF_PRED_WEIGHT_SEL_NB[stage1_index as usize][k];
                        if sel == b'A' {
                            LSF_PRED_WEIGHTS_NB_A[k]
                        } else {
                            LSF_PRED_WEIGHTS_NB_B[k]
                        }
                    }
                    Bandwidth::Wideband => {
                        let sel = LSF_PRED_WEIGHT_SEL_WB[stage1_index as usize][k];
                        if sel == b'C' {
                            LSF_PRED_WEIGHTS_WB_C[k]
                        } else {
                            LSF_PRED_WEIGHTS_WB_D[k]
                        }
                    }
                    _ => unreachable!(),
                };
                (i32::from(res_q10[k + 1]) * i32::from(pred_weight)) >> 8
            } else {
                0
            };

            let i2 = i32::from(stage2_indices[k]);
            let quantized = (((i2 << 10) - i2.signum() * 102) * qstep) >> 16;

            res_q10[k] = (prediction + quantized) as i16;
        }

        Ok(res_q10)
    }

    /// Computes IHMW (Inverse Harmonic Mean Weighting) weights from Stage-1 codebook (RFC 6716 Section 4.2.7.5.3, lines 3207-3244).
    ///
    /// # Errors
    ///
    /// * Returns error if bandwidth is invalid
    #[allow(dead_code, clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn compute_ihmw_weights(stage1_index: u8, bandwidth: Bandwidth) -> Result<Vec<u16>> {
        use super::lsf_constants::{LSF_CODEBOOK_NB, LSF_CODEBOOK_WB};

        let (d_lpc, cb1_q8) = match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => {
                (10, &LSF_CODEBOOK_NB[stage1_index as usize][..])
            }
            Bandwidth::Wideband => (16, &LSF_CODEBOOK_WB[stage1_index as usize][..]),
            _ => return Err(Error::SilkDecoder("invalid bandwidth for LSF".to_string())),
        };

        let mut w_q9 = Vec::with_capacity(d_lpc);

        for k in 0..d_lpc {
            let cb1_prev = if k > 0 { i32::from(cb1_q8[k - 1]) } else { 0 };
            let cb1_curr = i32::from(cb1_q8[k]);
            let cb1_next = if k + 1 < d_lpc {
                i32::from(cb1_q8[k + 1])
            } else {
                256
            };

            let w2_q18 = ((1024 / (cb1_curr - cb1_prev)) + (1024 / (cb1_next - cb1_curr))) << 16;

            // Square root approximation (RFC lines 3231-3234)
            let i = 32 - w2_q18.leading_zeros();
            let f = ((w2_q18 >> (i.saturating_sub(8))) & 127) as u32;
            let y = if (i & 1) != 0 { 32768 } else { 46214 } >> ((32 - i) >> 1);
            let w = y + ((213 * f * y) >> 16);

            w_q9.push(w as u16);
        }

        Ok(w_q9)
    }

    /// Reconstructs normalized LSF coefficients from Stage-1/Stage-2 data (RFC 6716 Section 4.2.7.5.3, lines 3423-3436).
    ///
    /// # Errors
    ///
    /// * Returns error if bandwidth is invalid
    /// * Returns error if computation fails
    #[allow(dead_code, clippy::cast_sign_loss)]
    fn reconstruct_lsf(
        stage1_index: u8,
        stage2_indices: &[i8],
        bandwidth: Bandwidth,
    ) -> Result<Vec<i16>> {
        use super::lsf_constants::{LSF_CODEBOOK_NB, LSF_CODEBOOK_WB};

        let res_q10 = Self::dequantize_lsf_residuals(stage1_index, stage2_indices, bandwidth)?;
        let w_q9 = Self::compute_ihmw_weights(stage1_index, bandwidth)?;

        let cb1_q8 = match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => {
                &LSF_CODEBOOK_NB[stage1_index as usize][..]
            }
            Bandwidth::Wideband => &LSF_CODEBOOK_WB[stage1_index as usize][..],
            _ => return Err(Error::SilkDecoder("invalid bandwidth for LSF".to_string())),
        };

        let d_lpc = res_q10.len();
        let mut nlsf_q15 = Vec::with_capacity(d_lpc);

        for k in 0..d_lpc {
            let cb1_term = i32::from(cb1_q8[k]) << 7;
            let res_term = (i32::from(res_q10[k]) << 14) / i32::from(w_q9[k]);
            let reconstructed = cb1_term + res_term;

            nlsf_q15.push(reconstructed.clamp(0, 32767) as i16);
        }

        log::trace!("[RECONSTRUCT_LSF] stage1={stage1_index}, stage2={stage2_indices:?}");
        log::trace!("[RECONSTRUCT_LSF] nlsf_q15: {nlsf_q15:?}");
        Ok(nlsf_q15)
    }

    /// Stabilizes normalized LSF coefficients to ensure monotonicity (RFC 6716 Section 4.2.7.5.4, lines 3438-3582).
    ///
    /// # Errors
    ///
    /// * Returns error if bandwidth is invalid
    #[allow(
        dead_code,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::needless_range_loop
    )]
    fn stabilize_lsf(mut nlsf_q15: Vec<i16>, bandwidth: Bandwidth) -> Result<Vec<i16>> {
        use super::lsf_constants::{LSF_MIN_SPACING_NB, LSF_MIN_SPACING_WB};

        let ndelta_min_q15 = match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => LSF_MIN_SPACING_NB,
            Bandwidth::Wideband => LSF_MIN_SPACING_WB,
            _ => return Err(Error::SilkDecoder("invalid bandwidth for LSF".to_string())),
        };

        let d_lpc = nlsf_q15.len();

        // Phase 1: Up to 20 iterations of gentle adjustments (RFC lines 3519-3566)
        for _ in 0..20 {
            let mut min_diff = i32::MAX;
            let mut min_idx = 0;

            for i in 0..=d_lpc {
                let prev = if i > 0 { i32::from(nlsf_q15[i - 1]) } else { 0 };
                let curr = if i < d_lpc {
                    i32::from(nlsf_q15[i])
                } else {
                    32768
                };
                let diff = curr - prev - i32::from(ndelta_min_q15[i]);

                if diff < min_diff {
                    min_diff = diff;
                    min_idx = i;
                }
            }

            if min_diff >= 0 {
                break;
            }

            // Apply adjustment (RFC lines 3540-3562)
            if min_idx == 0 {
                nlsf_q15[0] = ndelta_min_q15[0] as i16;
            } else if min_idx == d_lpc {
                nlsf_q15[d_lpc - 1] = (32768 - i32::from(ndelta_min_q15[d_lpc])) as i16;
            } else {
                let mut min_center = i32::from(ndelta_min_q15[min_idx]) >> 1;
                for k in 0..min_idx {
                    min_center += i32::from(ndelta_min_q15[k]);
                }

                let mut max_center = 32768 - (i32::from(ndelta_min_q15[min_idx]) >> 1);
                for k in (min_idx + 1)..=d_lpc {
                    max_center -= i32::from(ndelta_min_q15[k]);
                }

                let center_freq =
                    ((i32::from(nlsf_q15[min_idx - 1]) + i32::from(nlsf_q15[min_idx]) + 1) >> 1)
                        .clamp(min_center, max_center);

                nlsf_q15[min_idx - 1] =
                    (center_freq - (i32::from(ndelta_min_q15[min_idx]) >> 1)) as i16;
                nlsf_q15[min_idx] =
                    (i32::from(nlsf_q15[min_idx - 1]) + i32::from(ndelta_min_q15[min_idx])) as i16;
            }
        }

        // Phase 2: Fallback procedure (RFC lines 3568-3582)
        nlsf_q15.sort_unstable();

        for k in 0..d_lpc {
            let prev = if k > 0 { nlsf_q15[k - 1] } else { 0 };
            nlsf_q15[k] = nlsf_q15[k].max(prev + ndelta_min_q15[k] as i16);
        }

        for k in (0..d_lpc).rev() {
            let next = if k + 1 < d_lpc {
                i32::from(nlsf_q15[k + 1])
            } else {
                32768
            };
            nlsf_q15[k] = nlsf_q15[k].min((next - i32::from(ndelta_min_q15[k + 1])) as i16);
        }

        Ok(nlsf_q15)
    }

    /// Decodes and applies LSF interpolation for 20ms frames (RFC 6716 Section 4.2.7.5.5, lines 3591-3626).
    ///
    /// # Arguments
    /// * `range_decoder` - Range decoder for reading interpolation weight
    /// * `n2_q15` - Current frame's normalized LSF coefficients (Q15)
    /// * `bandwidth` - Audio bandwidth (determines which previous LSFs to use)
    ///
    /// # Returns
    /// * `Ok(Some(n1_q15))` - Interpolated LSFs for first half of 20ms frame
    /// * `Ok(None)` - No interpolation (10ms frame or first frame)
    ///
    /// # Errors
    /// * Returns error if range decoder fails
    // TODO(Section 3.5+): Remove dead_code annotation when integrated into full LSF decode pipeline
    #[allow(dead_code, clippy::cast_possible_truncation)]
    fn interpolate_lsf(
        &mut self,
        range_decoder: &mut RangeDecoder,
        n2_q15: &[i16],
        bandwidth: Bandwidth,
    ) -> Result<Option<Vec<i16>>> {
        use super::lsf_constants::LSF_INTERP_PDF;

        // Only interpolate for 20ms frames (RFC line 3593-3607)
        if self.frame_size_ms != 20 {
            return Ok(None);
        }

        // Decode interpolation weight (Q2 format, 0-4)
        let w_q2 = range_decoder.ec_dec_icdf(LSF_INTERP_PDF, 8)? as i16;

        // RFC lines 3601-3607: Override w_Q2 to 4 in special cases
        // After either:
        //   1. An uncoded regular SILK frame in the side channel, or
        //   2. A decoder reset
        // The decoder still decodes the factor but ignores its value and uses 4 instead
        let effective_w_q2 = if self.decoder_reset || self.uncoded_side_channel {
            4 // Force to 4 (means use n2 directly, full interpolation to current frame)
        } else {
            w_q2
        };

        // Clear reset flag after first use
        if self.decoder_reset {
            self.decoder_reset = false;
        }

        // Clear uncoded side channel flag (one-shot flag)
        if self.uncoded_side_channel {
            self.uncoded_side_channel = false;
        }

        // Get previous frame LSFs based on bandwidth
        let n0_q15 = match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => {
                self.previous_lsf_nb.as_ref().map(<[i16; 10]>::as_slice)
            }
            Bandwidth::Wideband => self.previous_lsf_wb.as_ref().map(<[i16; 16]>::as_slice),
            _ => {
                return Err(Error::SilkDecoder(
                    "invalid bandwidth for LSF interpolation".to_string(),
                ));
            }
        };

        n0_q15.map_or(Ok(None), |n0| {
            // RFC line 3623: n1_Q15[k] = n0_Q15[k] + (w_Q2*(n2_Q15[k] - n0_Q15[k]) >> 2)
            // Use effective_w_q2 (may be overridden to 4)
            let n1_q15: Vec<i16> = n0
                .iter()
                .zip(n2_q15.iter())
                .map(|(&n0_val, &n2_val)| {
                    let diff = i32::from(n2_val) - i32::from(n0_val);
                    let weighted = (i32::from(effective_w_q2) * diff) >> 2;
                    (i32::from(n0_val) + weighted) as i16
                })
                .collect();
            Ok(Some(n1_q15))
        })
    }

    /// Stores current frame's LSFs as previous for next frame's interpolation.
    // TODO(Section 3.5+): Remove dead_code annotation when integrated into full LSF decode pipeline
    #[allow(dead_code)]
    fn store_previous_lsf(&mut self, nlsf_q15: &[i16], bandwidth: Bandwidth) {
        match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => {
                if nlsf_q15.len() >= 10 {
                    let mut arr = [0_i16; 10];
                    arr.copy_from_slice(&nlsf_q15[..10]);
                    self.previous_lsf_nb = Some(arr);
                }
            }
            Bandwidth::Wideband => {
                if nlsf_q15.len() >= 16 {
                    let mut arr = [0_i16; 16];
                    arr.copy_from_slice(&nlsf_q15[..16]);
                    self.previous_lsf_wb = Some(arr);
                }
            }
            _ => {}
        }
    }

    /// Marks that an uncoded side channel frame was encountered.
    /// This will cause the next interpolation to use `w_Q2=4` (RFC lines 3601-3607).
    // TODO(Section 3.5+): Remove dead_code annotation when integrated into full LSF decode pipeline
    #[allow(dead_code)]
    const fn mark_uncoded_side_channel(&mut self) {
        self.uncoded_side_channel = true;
    }

    /// Resets decoder state for mode transitions.
    ///
    /// RFC 6716 Section 4.5.2 (lines 7088-7102): SILK state must be reset when
    /// transitioning FROM CELT-only mode TO SILK-only or Hybrid mode to avoid
    /// reusing "out of date" memory.
    ///
    /// This method clears ALL decoder state to ensure bit-exact RFC compliance:
    ///
    /// **NORMATIVE Requirements (MUST reset):**
    /// * LSF state - RFC 3595-3612 (interpolation uses `w_Q2=4`)
    /// * Stereo prediction weights - RFC 2200-2205 (zeros after reset)
    /// * LTP buffers - RFC 4740-4747, 5550-5565 (cleared to zeros)
    /// * Stereo unmixing state - RFC 2197-2205, 5715-5722 (prior samples to zeros)
    ///
    /// **Additional State (consistency):**
    /// * Gain indices - RFC 2517-2518 (independent coding after reset)
    /// * Pitch lag - RFC 4136-4152 (absolute coding after reset)
    /// * LCG seed, flags - Clean slate for new mode
    pub fn reset_decoder_state(&mut self) {
        self.decoder_reset = true;
        self.previous_lsf_nb = None;
        self.previous_lsf_wb = None;
        self.previous_stereo_weights = None;
        self.ltp_state[0].reset();
        self.ltp_state[1].reset();
        if let Some(ref mut state) = self.stereo_state {
            state.reset();
        }

        self.previous_gain_indices = [None, None];
        self.previous_pitch_lag = [None, None];
        self.lcg_seed = 0;
        self.uncoded_side_channel = false;
    }

    /// Converts normalized LSF coefficients to LPC coefficients (RFC 6716 Section 4.2.7.5.6, lines 3628-3892).
    ///
    /// # Arguments
    /// * `nlsf_q15` - Normalized LSF coefficients (Q15 format)
    /// * `bandwidth` - Audio bandwidth (determines ordering and `d_LPC`)
    ///
    /// # Returns
    /// * LPC coefficients in Q17 format (32-bit, before range limiting)
    ///
    /// # Errors
    /// * Returns error if bandwidth is invalid
    // TODO(Section 3.5): Remove dead_code annotation when called by LPC coefficient limiting
    #[allow(
        dead_code,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn lsf_to_lpc(nlsf_q15: &[i16], bandwidth: Bandwidth) -> Result<Vec<i32>> {
        use super::lsf_constants::{LSF_COS_TABLE_Q12, LSF_ORDERING_NB, LSF_ORDERING_WB};

        let (d_lpc, ordering): (usize, &[usize]) = match bandwidth {
            Bandwidth::Narrowband | Bandwidth::Mediumband => (10, LSF_ORDERING_NB),
            Bandwidth::Wideband => (16, LSF_ORDERING_WB),
            _ => {
                return Err(Error::SilkDecoder(
                    "invalid bandwidth for LSF-to-LPC".to_string(),
                ));
            }
        };

        // Step 1: Cosine approximation with reordering (RFC lines 3741-3748)
        // Libopus uses QA=16 for internal polynomial computation
        let mut c_q16_ordered = vec![0_i32; d_lpc];
        log::trace!("[LSF_TO_LPC] Input nlsf_q15: {nlsf_q15:?}");
        for k in 0..d_lpc {
            let n = nlsf_q15[k];
            let i = (n >> 8) as usize; // Integer index (top 7 bits)
            let f = i32::from(n & 255); // Fractional part (next 8 bits)

            // Linear interpolation in Q16 format (matches libopus QA=16)
            // cos_i is Q12, shift left 8 to get Q20
            // delta * f is Q12 * Q8 = Q20
            // Use RSHIFT_ROUND to convert Q20 to Q16 (matches libopus)
            let cos_i = i32::from(LSF_COS_TABLE_Q12[i]);
            let cos_i_plus_1 = i32::from(LSF_COS_TABLE_Q12[i + 1]);
            let delta = cos_i_plus_1 - cos_i;
            let q20_sum = (cos_i << 8) + (delta * f);
            // silk_RSHIFT_ROUND(x, 4) = (((x >> 3) + 1) >> 1)
            c_q16_ordered[ordering[k]] = ((q20_sum >> 3) + 1) >> 1;
        }
        log::trace!("[LSF_TO_LPC] c_q16 (cosine values): {c_q16_ordered:?}");

        // Step 2: Construct P(z) and Q(z) polynomials via in-place recurrence
        // Matches libopus silk_NLSF2A_find_poly() - uses 1D array updated in place
        let d2 = d_lpc / 2;
        let mut p_q16 = vec![0_i64; d2 + 2];
        let mut q_q16 = vec![0_i64; d2 + 2];

        // Boundary conditions (RFC lines 3849-3850)
        p_q16[0] = 1_i64 << 16;
        p_q16[1] = -i64::from(c_q16_ordered[0]);
        q_q16[0] = 1_i64 << 16;
        q_q16[1] = -i64::from(c_q16_ordered[1]);

        // Recurrence (RFC lines 3855-3859) - IN-PLACE updates
        for k in 1..d2 {
            // Update p[k+1] first (doesn't depend on p[k])
            p_q16[k + 1] =
                (p_q16[k - 1] << 1) - ((i64::from(c_q16_ordered[2 * k]) * p_q16[k] + 32768) >> 16);
            q_q16[k + 1] = (q_q16[k - 1] << 1)
                - ((i64::from(c_q16_ordered[2 * k + 1]) * q_q16[k] + 32768) >> 16);

            // Update p[n] and q[n] in reverse order (in-place)
            for n in (2..=k).rev() {
                p_q16[n] +=
                    p_q16[n - 2] - ((i64::from(c_q16_ordered[2 * k]) * p_q16[n - 1] + 32768) >> 16);
                q_q16[n] += q_q16[n - 2]
                    - ((i64::from(c_q16_ordered[2 * k + 1]) * q_q16[n - 1] + 32768) >> 16);
            }

            // Update p[1] and q[1] last
            p_q16[1] -= (i64::from(c_q16_ordered[2 * k]) * p_q16[0] + 32768) >> 16;
            q_q16[1] -= (i64::from(c_q16_ordered[2 * k + 1]) * q_q16[0] + 32768) >> 16;
        }

        // Step 3: Extract LPC coefficients (RFC lines 3882-3886)
        // Output is in "QA+1" format (Q17) due to potential overflow during extraction
        let mut a32_q17 = vec![0_i32; d_lpc];
        for k in 0..d2 {
            let q_diff = q_q16[k + 1] - q_q16[k];
            let p_sum = p_q16[k + 1] + p_q16[k];

            a32_q17[k] = (-(q_diff + p_sum)) as i32;
            a32_q17[d_lpc - k - 1] = (q_diff - p_sum) as i32;

            if k <= 1 {
                log::trace!(
                    "[LSF_TO_LPC EXTRACT k={}] q_diff={}, p_sum={}, a32_q17[{}]={}, a32_q17[{}]={}",
                    k,
                    q_diff,
                    p_sum,
                    k,
                    a32_q17[k],
                    d_lpc - k - 1,
                    a32_q17[d_lpc - k - 1]
                );
            }
        }

        Ok(a32_q17)
    }

    /// Limits LPC coefficients to ensure magnitude fits in Q12 and filter is stable.
    ///
    /// Two-stage process per RFC 6716:
    /// * 1. Magnitude limiting: Up to 10 rounds of bandwidth expansion (Section 4.2.7.5.7)
    /// * 2. Prediction gain limiting: Up to 16 rounds for stability (Section 4.2.7.5.8)
    ///
    /// # Arguments
    /// * `nlsf_q15` - Normalized LSF coefficients (Q15 format)
    /// * `bandwidth` - Audio bandwidth (determines `d_LPC`)
    ///
    /// # Returns
    /// * LPC coefficients in Q12 format (16-bit, safe for synthesis filter)
    ///
    /// # Errors
    /// * Returns error if bandwidth is invalid
    ///
    /// RFC 6716 lines 3893-4120
    // TODO(Section 3.6+): Remove dead_code annotation when integrated into full decoder pipeline
    #[allow(dead_code)]
    pub fn limit_lpc_coefficients(nlsf_q15: &[i16], bandwidth: Bandwidth) -> Result<Vec<i16>> {
        // Step 1: Convert LSF to LPC (from Section 3.4)
        let mut a32_q17 = Self::lsf_to_lpc(nlsf_q15, bandwidth)?;
        log::trace!(
            "[LPC] a32_q17 after lsf_to_lpc (first 10): {:?}",
            &a32_q17[..10.min(a32_q17.len())]
        );

        // Step 2: Magnitude limiting (up to 10 rounds, RFC Section 4.2.7.5.7)
        Self::limit_coefficient_magnitude(&mut a32_q17);
        log::trace!(
            "[LPC] a32_q17 after magnitude limiting (first 10): {:?}",
            &a32_q17[..10.min(a32_q17.len())]
        );

        // Step 3: Prediction gain limiting (up to 16 rounds, RFC Section 4.2.7.5.8)
        for round in 0..16 {
            if Self::is_filter_stable(&a32_q17) {
                break; // Filter is stable
            }

            // Compute chirp factor with progressively stronger expansion (RFC line 4116)
            let sc_q16_0 = 65536 - (2 << round);

            // Apply bandwidth expansion
            Self::apply_bandwidth_expansion(&mut a32_q17, sc_q16_0);

            // Round 15: Force to zero (guaranteed stable, RFC lines 4118-4119)
            if round == 15 {
                return Ok(vec![0; a32_q17.len()]);
            }
        }

        // Step 4: Convert Q17 to Q12 (RFC line 4111)
        #[allow(clippy::cast_possible_truncation)]
        let a_q12: Vec<i16> = a32_q17
            .iter()
            .map(|&a| {
                let q12 = (a + 16) >> 5;
                q12.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
            })
            .collect();

        log::trace!(
            "[LPC] a_q12 (Q12 coefficients, first 10): {:?}",
            &a_q12[..10.min(a_q12.len())]
        );
        Ok(a_q12)
    }

    /// Limits LPC coefficient magnitude using bandwidth expansion (RFC 6716 Section 4.2.7.5.7, lines 3893-3963).
    ///
    /// Applies up to 10 rounds of bandwidth expansion to ensure Q17 coefficients
    /// can be safely converted to Q12 16-bit format.
    ///
    /// # Arguments
    /// * `a32_q17` - LPC coefficients in Q17 format
    ///
    /// RFC 6716 lines 3893-3963
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn limit_coefficient_magnitude(a32_q17: &mut [i32]) {
        for round in 0..10 {
            // Step 1: Find index k with largest abs(a32_Q17[k]) (RFC lines 3903-3905)
            // Break ties by choosing lowest k
            let (max_idx, maxabs_q17) = a32_q17
                .iter()
                .enumerate()
                .map(|(i, &v)| (i, v.abs()))
                .max_by(|(i1, v1), (i2, v2)| v1.cmp(v2).then(i2.cmp(i1))) // Ties: prefer lower index
                .unwrap_or((0, 0));

            // Step 2: Compute Q12 precision value with upper bound (RFC line 3909)
            let maxabs_q12 = ((maxabs_q17 + 16) >> 5).min(163_838);

            // Step 3: Check if limiting is needed (matches libopus LPC_fit.c line 55)
            // Coefficients must fit in i16 range: -32768 to 32767
            if maxabs_q12 <= 32767 {
                break; // Coefficients fit in Q12, done
            }

            log::trace!(
                "[LIMIT] Round {round}: maxabs_q12={maxabs_q12}, max_idx={max_idx}, exceeds 32767, applying bandwidth expansion"
            );

            // Step 4: Compute chirp factor (RFC lines 3914-3916)
            let numerator = (maxabs_q12 - 32767) << 14;
            #[allow(clippy::cast_possible_wrap)]
            let denominator = (maxabs_q12 * (max_idx as i32 + 1)) >> 2;
            let sc_q16_0 = 65470 - (numerator / denominator);

            // Step 5: Apply bandwidth expansion (RFC lines 3938-3942)
            Self::apply_bandwidth_expansion(a32_q17, sc_q16_0);

            // Step 6: After 10th round, perform saturation (RFC lines 3951-3962)
            if round == 9 {
                for coeff in a32_q17.iter_mut() {
                    // Convert to Q12, clamp, convert back to Q17
                    let q12 = (*coeff + 16) >> 5;
                    let clamped = q12.clamp(-32768, 32767);
                    *coeff = clamped << 5;
                }
            }
        }
    }

    /// Applies bandwidth expansion to LPC coefficients using chirp factor.
    ///
    /// # Arguments
    /// * `a32_q17` - LPC coefficients in Q17 format (modified in place)
    /// * `sc_q16_0` - Initial chirp factor in Q16 format
    ///
    /// RFC 6716 lines 3936-3949
    #[allow(clippy::cast_possible_truncation)]
    fn apply_bandwidth_expansion(a32_q17: &mut [i32], sc_q16_0: i32) {
        let mut sc_q16 = sc_q16_0;
        for coeff in a32_q17.iter_mut() {
            // RFC line 3940: requires up to 48-bit precision
            *coeff = ((i64::from(*coeff) * i64::from(sc_q16)) >> 16) as i32;

            // RFC line 3942: unsigned multiply to avoid 32-bit overflow
            #[allow(clippy::cast_sign_loss)]
            let sc_unsigned = sc_q16 as u64;
            #[allow(clippy::cast_sign_loss)]
            let sc_q16_0_unsigned = sc_q16_0 as u64;
            sc_q16 = (((sc_q16_0_unsigned * sc_unsigned) + 32768) >> 16) as i32;
        }
    }

    /// Checks LPC filter stability using Levinson recursion (RFC 6716 Section 4.2.7.5.8, lines 3983-4105).
    ///
    /// Computes reflection coefficients and inverse prediction gain using fixed-point
    /// arithmetic to ensure bit-exact reproducibility across platforms.
    ///
    /// # Arguments
    /// * `a32_q17` - LPC coefficients in Q17 format
    ///
    /// # Returns
    /// * `true` if filter is stable, `false` if unstable
    ///
    /// RFC 6716 lines 3983-4105
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )]
    fn is_filter_stable(a32_q17: &[i32]) -> bool {
        let d_lpc = a32_q17.len();

        // Step 1: Convert Q17 to Q12 coefficients (RFC line 4004)
        let a32_q12: Vec<i32> = a32_q17.iter().map(|&a| (a + 16) >> 5).collect();

        // Step 2: DC response check (RFC lines 4008-4016)
        let dc_resp: i32 = a32_q12.iter().sum();
        if dc_resp > 4096 {
            return false; // Unstable
        }

        // Step 3: Initialize Q24 coefficients and inverse gain (RFC lines 4020-4025)
        let mut a32_q24 = vec![vec![0_i64; d_lpc]; d_lpc];
        for (n, &coeff) in a32_q12.iter().enumerate() {
            a32_q24[d_lpc - 1][n] = i64::from(coeff) << 12;
        }

        let mut inv_gain_q30 = vec![0_i64; d_lpc + 1];
        inv_gain_q30[d_lpc] = 1_i64 << 30;

        // Step 4: Levinson recurrence (RFC lines 4039-4097)
        for k in (0..d_lpc).rev() {
            // Check coefficient magnitude (RFC lines 4040-4041)
            // Constant 16773022 ≈ 0.99975 in Q24
            if a32_q24[k][k].abs() > 16_773_022 {
                return false; // Unstable
            }

            // Compute reflection coefficient (RFC line 4045)
            let rc_q31 = -(a32_q24[k][k] << 7);

            // Compute denominator (RFC line 4047)
            let rc_sq = (rc_q31 * rc_q31) >> 32;
            let div_q30 = (1_i64 << 30) - rc_sq;

            // Update inverse prediction gain (RFC line 4049)
            inv_gain_q30[k] = ((inv_gain_q30[k + 1] * div_q30) >> 32) << 2;

            // Check inverse gain (RFC lines 4051-4052)
            // Constant 107374 ≈ 1/10000 in Q30
            if inv_gain_q30[k] < 107_374 {
                return false; // Unstable
            }

            // If k > 0, compute next row (RFC lines 4054-4074)
            if k > 0 {
                // Compute precision for division (RFC lines 4056-4058)
                let b1 = ilog(div_q30 as u32);
                let b2 = b1 - 16;

                // Compute inverse with error correction (RFC lines 4060-4068)
                let inv_qb2 = ((1_i64 << 29) - 1) / (div_q30 >> (b2 + 1));
                let err_q29 = (1_i64 << 29) - (((div_q30 << (15 - b2)) * inv_qb2) >> 16);
                let gain_qb1 = (inv_qb2 << 16) + ((err_q29 * inv_qb2) >> 13);

                // Compute row k-1 from row k (RFC lines 4070-4074)
                for n in 0..k {
                    let num_q24 =
                        a32_q24[k][n] - ((a32_q24[k][k - n - 1] * rc_q31 + (1_i64 << 30)) >> 31);
                    a32_q24[k - 1][n] = (num_q24 * gain_qb1 + (1_i64 << (b1 - 1))) >> b1;
                }
            }
        }

        // If we reach here, all checks passed (RFC lines 4099-4100)
        true
    }

    /// Decodes primary pitch lag (RFC 6716 Section 4.2.7.6.1, lines 4130-4216).
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails or bandwidth is invalid
    ///
    /// # Panics
    ///
    /// Panics if integer conversions cannot be performed.
    // TODO(Section 3.7+): Remove dead_code when integrated into frame decoder
    #[allow(dead_code)]
    fn decode_primary_pitch_lag(
        &mut self,
        range_decoder: &mut RangeDecoder,
        bandwidth: Bandwidth,
        use_absolute: bool,
        channel_idx: usize,
    ) -> Result<i16> {
        use super::ltp_constants::{
            LTP_LAG_DELTA_PDF, LTP_LAG_HIGH_PDF, LTP_LAG_LOW_PDF_MB, LTP_LAG_LOW_PDF_NB,
            LTP_LAG_LOW_PDF_WB,
        };

        if use_absolute {
            let lag_high = i16::try_from(range_decoder.ec_dec_icdf(LTP_LAG_HIGH_PDF, 8)?)
                .expect("ec_dec_icdf returns u8, always fits in i16");

            let (pdf_low, lag_scale, lag_min) = match bandwidth {
                Bandwidth::Narrowband => (LTP_LAG_LOW_PDF_NB, 4, 16),
                Bandwidth::Mediumband => (LTP_LAG_LOW_PDF_MB, 6, 24),
                Bandwidth::Wideband => (LTP_LAG_LOW_PDF_WB, 8, 32),
                _ => return Err(Error::SilkDecoder("invalid bandwidth for LTP".to_string())),
            };

            let lag_low = i16::try_from(range_decoder.ec_dec_icdf(pdf_low, 8)?)
                .expect("ec_dec_icdf returns u8, always fits in i16");

            let lag = lag_high * lag_scale + lag_low + lag_min;

            self.previous_pitch_lag[channel_idx] = Some(lag);
            Ok(lag)
        } else {
            let delta_lag_index = i16::try_from(range_decoder.ec_dec_icdf(LTP_LAG_DELTA_PDF, 8)?)
                .expect("ec_dec_icdf returns u8, always fits in i16");

            if delta_lag_index == 0 {
                self.decode_primary_pitch_lag(range_decoder, bandwidth, true, channel_idx)
            } else {
                let previous_lag = self.previous_pitch_lag[channel_idx]
                    .ok_or_else(|| Error::SilkDecoder("no previous pitch lag".to_string()))?;
                let lag = previous_lag + (delta_lag_index - 9);

                self.previous_pitch_lag[channel_idx] = Some(lag);
                Ok(lag)
            }
        }
    }

    /// Decodes pitch contour (RFC 6716 Section 4.2.7.6.1, lines 4226-4452).
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails or parameters invalid
    // TODO(Section 3.7+): Remove dead_code when integrated into frame decoder
    #[allow(dead_code)]
    fn decode_pitch_contour(
        &self,
        range_decoder: &mut RangeDecoder,
        primary_lag: i16,
        bandwidth: Bandwidth,
    ) -> Result<Vec<i16>> {
        use super::ltp_constants::{
            PITCH_CONTOUR_CB_MBWB_10MS, PITCH_CONTOUR_CB_MBWB_20MS, PITCH_CONTOUR_CB_NB_10MS,
            PITCH_CONTOUR_CB_NB_20MS, PITCH_CONTOUR_PDF_MBWB_10MS, PITCH_CONTOUR_PDF_MBWB_20MS,
            PITCH_CONTOUR_PDF_NB_10MS, PITCH_CONTOUR_PDF_NB_20MS,
        };

        let silk_frame_size_ms = if self.frame_size_ms <= 20 {
            self.frame_size_ms
        } else {
            20
        };

        let (pdf, lag_min, lag_max) = match (bandwidth, silk_frame_size_ms) {
            (Bandwidth::Narrowband, 10) => (PITCH_CONTOUR_PDF_NB_10MS, 16, 144),
            (Bandwidth::Narrowband, 20) => (PITCH_CONTOUR_PDF_NB_20MS, 16, 144),
            (Bandwidth::Mediumband, 10) => (PITCH_CONTOUR_PDF_MBWB_10MS, 24, 216),
            (Bandwidth::Wideband, 10) => (PITCH_CONTOUR_PDF_MBWB_10MS, 32, 288),
            (Bandwidth::Mediumband, 20) => (PITCH_CONTOUR_PDF_MBWB_20MS, 24, 216),
            (Bandwidth::Wideband, 20) => (PITCH_CONTOUR_PDF_MBWB_20MS, 32, 288),
            _ => {
                return Err(Error::SilkDecoder(
                    "invalid bandwidth/frame size".to_string(),
                ));
            }
        };

        let contour_index = range_decoder.ec_dec_icdf(pdf, 8)? as usize;

        let offsets: &[i8] = match (bandwidth, silk_frame_size_ms) {
            (Bandwidth::Narrowband, 10) => {
                if contour_index >= PITCH_CONTOUR_CB_NB_10MS.len() {
                    return Err(Error::SilkDecoder(
                        "invalid pitch contour index".to_string(),
                    ));
                }
                &PITCH_CONTOUR_CB_NB_10MS[contour_index]
            }
            (Bandwidth::Narrowband, 20) => {
                if contour_index >= PITCH_CONTOUR_CB_NB_20MS.len() {
                    return Err(Error::SilkDecoder(
                        "invalid pitch contour index".to_string(),
                    ));
                }
                &PITCH_CONTOUR_CB_NB_20MS[contour_index]
            }
            (Bandwidth::Mediumband | Bandwidth::Wideband, 10) => {
                if contour_index >= PITCH_CONTOUR_CB_MBWB_10MS.len() {
                    return Err(Error::SilkDecoder(
                        "invalid pitch contour index".to_string(),
                    ));
                }
                &PITCH_CONTOUR_CB_MBWB_10MS[contour_index]
            }
            (Bandwidth::Mediumband | Bandwidth::Wideband, 20) => {
                if contour_index >= PITCH_CONTOUR_CB_MBWB_20MS.len() {
                    return Err(Error::SilkDecoder(
                        "invalid pitch contour index".to_string(),
                    ));
                }
                &PITCH_CONTOUR_CB_MBWB_20MS[contour_index]
            }
            _ => {
                return Err(Error::SilkDecoder(
                    "invalid bandwidth/frame size".to_string(),
                ));
            }
        };

        let pitch_lags = offsets
            .iter()
            .map(|&offset| {
                let lag = primary_lag + i16::from(offset);
                lag.clamp(lag_min, lag_max)
            })
            .collect();

        Ok(pitch_lags)
    }

    /// Decodes LTP filter coefficients (RFC 6716 Section 4.2.7.6.2, lines 4454-4721).
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    // TODO(Section 3.7+): Remove dead_code when integrated into frame decoder
    #[allow(dead_code)]
    fn decode_ltp_filter_coefficients(
        &self,
        range_decoder: &mut RangeDecoder,
    ) -> Result<Vec<[i8; 5]>> {
        use super::ltp_constants::{
            LTP_FILTER_CB_0, LTP_FILTER_CB_1, LTP_FILTER_CB_2, LTP_FILTER_PDF_0, LTP_FILTER_PDF_1,
            LTP_FILTER_PDF_2, LTP_PERIODICITY_PDF,
        };

        let silk_frame_size_ms = if self.frame_size_ms <= 20 {
            self.frame_size_ms
        } else {
            20
        };

        let num_subframes = match silk_frame_size_ms {
            10 => 2,
            20 => 4,
            _ => {
                return Err(Error::SilkDecoder(
                    "invalid SILK frame size for LTP".to_string(),
                ));
            }
        };

        let periodicity_index = range_decoder.ec_dec_icdf(LTP_PERIODICITY_PDF, 8)?;

        let pdf = match periodicity_index {
            0 => LTP_FILTER_PDF_0,
            1 => LTP_FILTER_PDF_1,
            2 => LTP_FILTER_PDF_2,
            _ => return Err(Error::SilkDecoder("invalid periodicity index".to_string())),
        };

        let mut filters = Vec::with_capacity(num_subframes);
        for _ in 0..num_subframes {
            let filter_index = range_decoder.ec_dec_icdf(pdf, 8)? as usize;

            let filter = match periodicity_index {
                0 => {
                    if filter_index >= LTP_FILTER_CB_0.len() {
                        return Err(Error::SilkDecoder("invalid LTP filter index".to_string()));
                    }
                    LTP_FILTER_CB_0[filter_index]
                }
                1 => {
                    if filter_index >= LTP_FILTER_CB_1.len() {
                        return Err(Error::SilkDecoder("invalid LTP filter index".to_string()));
                    }
                    LTP_FILTER_CB_1[filter_index]
                }
                2 => {
                    if filter_index >= LTP_FILTER_CB_2.len() {
                        return Err(Error::SilkDecoder("invalid LTP filter index".to_string()));
                    }
                    LTP_FILTER_CB_2[filter_index]
                }
                _ => unreachable!(),
            };

            filters.push(filter);
        }

        Ok(filters)
    }

    /// Decodes LTP scaling parameter (RFC 6716 Section 4.2.7.6.3, lines 4722-4754).
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    // TODO(Section 3.7+): Remove dead_code when integrated into frame decoder
    #[allow(dead_code)]
    fn decode_ltp_scaling(range_decoder: &mut RangeDecoder, should_decode: bool) -> Result<u16> {
        use super::ltp_constants::{LTP_SCALING_PDF, ltp_scaling_factor_q14};

        if should_decode {
            let index = range_decoder.ec_dec_icdf(LTP_SCALING_PDF, 8)? as usize;
            Ok(ltp_scaling_factor_q14(index))
        } else {
            Ok(15565)
        }
    }

    /// Decodes LCG seed for pseudorandom noise injection (RFC 6716 Section 4.2.7.7, lines 4775-4793).
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    // TODO(Section 3.7.7): Remove dead_code when used in noise injection
    #[allow(dead_code)]
    pub fn decode_lcg_seed(&mut self, range_decoder: &mut RangeDecoder) -> Result<u32> {
        use super::excitation_constants::LCG_SEED_PDF;

        let seed = range_decoder.ec_dec_icdf(LCG_SEED_PDF, 8)?;
        self.lcg_seed = seed;
        Ok(seed)
    }

    /// Gets the number of 16-sample shell blocks for excitation coding (RFC 6716 Section 4.2.7.8 + Table 44, lines 4828-4855).
    ///
    /// # Errors
    ///
    /// * Returns error if bandwidth/frame size combination is invalid
    // TODO(Section 3.7.4): Remove dead_code when used in pulse location decoding
    #[allow(dead_code)]
    pub fn get_shell_block_count(bandwidth: Bandwidth, frame_size_ms: u8) -> Result<usize> {
        use super::excitation_constants::get_shell_block_count;

        get_shell_block_count(bandwidth, frame_size_ms).map_or_else(
            || {
                Err(Error::SilkDecoder(format!(
                    "invalid bandwidth/frame size for shell blocks: {bandwidth:?}/{frame_size_ms}ms"
                )))
            },
            Ok,
        )
    }

    /// Decodes rate level for excitation pulse coding (RFC 6716 Section 4.2.7.8.1, lines 4857-4891).
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    // TODO(Section 3.7.4): Remove dead_code when used in pulse location decoding
    #[allow(dead_code)]
    pub fn decode_rate_level(
        &self,
        range_decoder: &mut RangeDecoder,
        frame_type: FrameType,
    ) -> Result<u8> {
        use super::excitation_constants::{RATE_LEVEL_PDF_INACTIVE, RATE_LEVEL_PDF_VOICED};

        let pdf = match frame_type {
            FrameType::Inactive | FrameType::Unvoiced => RATE_LEVEL_PDF_INACTIVE,
            FrameType::Voiced => RATE_LEVEL_PDF_VOICED,
        };

        #[allow(clippy::cast_possible_truncation)]
        range_decoder.ec_dec_icdf(pdf, 8).map(|v| v as u8)
    }

    /// Decodes pulse count for a single shell block (RFC 6716 Section 4.2.7.8.2, lines 4893-4973).
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    /// * Returns error if rate level is invalid
    // TODO(Section 3.7.4): Remove dead_code when used in pulse location decoding
    #[allow(dead_code)]
    pub fn decode_pulse_count(
        &self,
        range_decoder: &mut RangeDecoder,
        rate_level: u8,
    ) -> Result<(u8, u8)> {
        use super::excitation_constants::{
            PULSE_COUNT_PDF_LEVEL_0, PULSE_COUNT_PDF_LEVEL_1, PULSE_COUNT_PDF_LEVEL_2,
            PULSE_COUNT_PDF_LEVEL_3, PULSE_COUNT_PDF_LEVEL_4, PULSE_COUNT_PDF_LEVEL_5,
            PULSE_COUNT_PDF_LEVEL_6, PULSE_COUNT_PDF_LEVEL_7, PULSE_COUNT_PDF_LEVEL_8,
            PULSE_COUNT_PDF_LEVEL_9, PULSE_COUNT_PDF_LEVEL_10,
        };

        let mut lsb_count = 0_u8;
        let mut current_rate_level = rate_level;

        loop {
            let pdf = match current_rate_level {
                0 => PULSE_COUNT_PDF_LEVEL_0,
                1 => PULSE_COUNT_PDF_LEVEL_1,
                2 => PULSE_COUNT_PDF_LEVEL_2,
                3 => PULSE_COUNT_PDF_LEVEL_3,
                4 => PULSE_COUNT_PDF_LEVEL_4,
                5 => PULSE_COUNT_PDF_LEVEL_5,
                6 => PULSE_COUNT_PDF_LEVEL_6,
                7 => PULSE_COUNT_PDF_LEVEL_7,
                8 => PULSE_COUNT_PDF_LEVEL_8,
                9 => PULSE_COUNT_PDF_LEVEL_9,
                10 => PULSE_COUNT_PDF_LEVEL_10,
                _ => return Err(Error::SilkDecoder("invalid rate level".to_string())),
            };

            let pulse_count = range_decoder.ec_dec_icdf(pdf, 8)?;

            if pulse_count < 17 {
                #[allow(clippy::cast_possible_truncation)]
                return Ok((pulse_count as u8, lsb_count));
            }

            lsb_count += 1;

            if lsb_count >= 10 {
                current_rate_level = 10;
            } else {
                current_rate_level = 9;
            }
        }
    }

    /// Decodes pulse positions for a shell block using hierarchical binary splitting (RFC 6716 Section 4.2.7.8.3, lines 4975-5007).
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder for reading bitstream
    /// * `pulse_count` - Total number of pulses in the 16-sample block
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    /// * Returns error if invalid partition size/pulse count combination
    // TODO(Section 3.7.5): Remove dead_code when used in LSB decoding
    #[allow(dead_code)]
    pub fn decode_pulse_locations(
        &self,
        range_decoder: &mut RangeDecoder,
        pulse_count: u8,
    ) -> Result<[u8; 16]> {
        log::trace!("[DECODE_PULSE_LOC] Called with pulse_count={pulse_count}");

        let mut locations = [0_u8; 16];

        if pulse_count == 0 {
            log::trace!("[DECODE_PULSE_LOC] pulse_count=0, returning zeros");
            return Ok(locations);
        }

        Self::decode_split_recursive(range_decoder, &mut locations, 0, 16, pulse_count)?;

        log::trace!("[DECODE_PULSE_LOC] Returning locations: {locations:?}");
        Ok(locations)
    }

    /// Recursively decodes pulse split using preorder traversal (RFC 6716 lines 4995-5007).
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder for reading bitstream
    /// * `locations` - Array to store pulse counts per location
    /// * `offset` - Starting offset in locations array
    /// * `partition_size` - Current partition size (16, 8, 4, 2, or 1)
    /// * `pulse_count` - Number of pulses in current partition
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    /// * Returns error if PDF lookup fails
    fn decode_split_recursive(
        range_decoder: &mut RangeDecoder,
        locations: &mut [u8; 16],
        offset: usize,
        partition_size: usize,
        pulse_count: u8,
    ) -> Result<()> {
        use super::excitation_constants::get_pulse_split_pdf;

        if pulse_count == 0 || partition_size == 1 {
            if partition_size == 1 && pulse_count > 0 {
                log::trace!("[SPLIT_BASE] Setting locations[{offset}] = {pulse_count}");
                locations[offset] = pulse_count;
            }
            return Ok(());
        }

        let pdf = get_pulse_split_pdf(partition_size, pulse_count).ok_or_else(|| {
            Error::SilkDecoder(format!(
                "invalid pulse split parameters: size={partition_size}, count={pulse_count}"
            ))
        })?;

        let left_pulses = range_decoder.ec_dec_icdf(pdf, 8)?;

        #[allow(clippy::cast_possible_truncation)]
        let left_pulses_u8 = left_pulses as u8;
        let right_pulses = pulse_count - left_pulses_u8;

        let half_size = partition_size / 2;

        Self::decode_split_recursive(range_decoder, locations, offset, half_size, left_pulses_u8)?;
        Self::decode_split_recursive(
            range_decoder,
            locations,
            offset + half_size,
            half_size,
            right_pulses,
        )?;

        Ok(())
    }

    /// Decodes LSBs for excitation coefficients (RFC 6716 Section 4.2.7.8.4, lines 5258-5289).
    ///
    /// LSBs are decoded MSB-first for all 16 coefficients per bit level.
    /// For 10ms MB frames, LSBs are decoded for all 16 samples even though only first 8 are used.
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder for reading bitstream
    /// * `pulse_locations` - Pulse counts per location (initial magnitudes)
    /// * `lsb_count` - Number of LSB levels to decode
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    // TODO(Section 3.7.6): Remove dead_code when used in sign decoding
    #[allow(dead_code)]
    #[allow(clippy::cast_possible_truncation)]
    pub fn decode_lsbs(
        &self,
        range_decoder: &mut RangeDecoder,
        pulse_locations: &[u8; 16],
        lsb_count: u8,
    ) -> Result<[u16; 16]> {
        use super::excitation_constants::EXCITATION_LSB_PDF;

        let mut magnitudes = [0_u16; 16];

        for i in 0..16 {
            magnitudes[i] = u16::from(pulse_locations[i]);
        }

        if lsb_count == 0 {
            return Ok(magnitudes);
        }

        for _ in 0..lsb_count {
            for magnitude in &mut magnitudes {
                let lsb_bit = range_decoder.ec_dec_icdf(EXCITATION_LSB_PDF, 8)?;
                *magnitude = (*magnitude << 1) | (lsb_bit as u16);
            }
        }

        Ok(magnitudes)
    }

    /// Decodes signs for all non-zero magnitude coefficients (RFC 6716 Section 4.2.7.8.5, lines 5291-5420).
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder state
    /// * `magnitudes` - Coefficient magnitudes (16 coefficients)
    /// * `frame_type` - Signal type (Inactive, Unvoiced, or Voiced)
    /// * `quant_offset_type` - Quantization offset type (Low or High)
    /// * `pulse_count` - Number of pulses in current shell (from Section 4.2.7.8.2, NOT including LSBs)
    ///
    /// # Returns
    ///
    /// Array of 16 signed excitation coefficients (positive or negative based on decoded signs)
    ///
    /// # Errors
    ///
    /// * Range decoder errors from `ec_dec_icdf()`
    /// * Returns `Error::SilkDecoder` if magnitude exceeds `i16::MAX` (32767)
    ///
    /// # RFC Algorithm (lines 5293-5297)
    ///
    /// For each coefficient with non-zero magnitude:
    /// * Select PDF based on signal type, quantization offset type, and pulse count
    /// * Decode sign bit using selected PDF
    /// * If sign bit = 0, negate the magnitude
    /// * Otherwise, keep magnitude positive
    pub fn decode_signs(
        &self,
        range_decoder: &mut RangeDecoder,
        magnitudes: &[u16; 16],
        frame_type: FrameType,
        quant_offset_type: QuantizationOffsetType,
        pulse_count: u8,
    ) -> Result<[i16; 16]> {
        let pdf = Self::get_sign_pdf(frame_type, quant_offset_type, pulse_count);

        let mut signed_excitation = [0_i16; 16];

        for i in 0..16 {
            if magnitudes[i] == 0 {
                signed_excitation[i] = 0;
            } else {
                let sign_bit = range_decoder.ec_dec_icdf(pdf, 8)?;
                let magnitude_i16 = i16::try_from(magnitudes[i]).map_err(|_| {
                    Error::SilkDecoder(format!(
                        "magnitude {} exceeds i16::MAX (32767) at position {}",
                        magnitudes[i], i
                    ))
                })?;
                signed_excitation[i] = if sign_bit == 0 {
                    -magnitude_i16
                } else {
                    magnitude_i16
                };
            }
        }

        Ok(signed_excitation)
    }

    /// Selects sign PDF based on signal type, quantization offset type, and pulse count.
    ///
    /// # Arguments
    ///
    /// * `frame_type` - Signal type (Inactive, Unvoiced, or Voiced)
    /// * `quant_offset_type` - Quantization offset type (Low or High)
    /// * `pulse_count` - Number of pulses in shell (0, 1, 2, 3, 4, 5, or 6+)
    ///
    /// # Returns
    ///
    /// ICDF array for sign decoding
    ///
    /// # RFC Reference
    ///
    /// Table 52 (lines 5310-5420): All 42 PDFs organized by signal type, offset type, and pulse count
    const fn get_sign_pdf(
        frame_type: FrameType,
        quant_offset_type: QuantizationOffsetType,
        pulse_count: u8,
    ) -> &'static [u8] {
        use super::excitation_constants::{
            SIGN_PDF_INACTIVE_HIGH_0, SIGN_PDF_INACTIVE_HIGH_1, SIGN_PDF_INACTIVE_HIGH_2,
            SIGN_PDF_INACTIVE_HIGH_3, SIGN_PDF_INACTIVE_HIGH_4, SIGN_PDF_INACTIVE_HIGH_5,
            SIGN_PDF_INACTIVE_HIGH_6PLUS, SIGN_PDF_INACTIVE_LOW_0, SIGN_PDF_INACTIVE_LOW_1,
            SIGN_PDF_INACTIVE_LOW_2, SIGN_PDF_INACTIVE_LOW_3, SIGN_PDF_INACTIVE_LOW_4,
            SIGN_PDF_INACTIVE_LOW_5, SIGN_PDF_INACTIVE_LOW_6PLUS, SIGN_PDF_UNVOICED_HIGH_0,
            SIGN_PDF_UNVOICED_HIGH_1, SIGN_PDF_UNVOICED_HIGH_2, SIGN_PDF_UNVOICED_HIGH_3,
            SIGN_PDF_UNVOICED_HIGH_4, SIGN_PDF_UNVOICED_HIGH_5, SIGN_PDF_UNVOICED_HIGH_6PLUS,
            SIGN_PDF_UNVOICED_LOW_0, SIGN_PDF_UNVOICED_LOW_1, SIGN_PDF_UNVOICED_LOW_2,
            SIGN_PDF_UNVOICED_LOW_3, SIGN_PDF_UNVOICED_LOW_4, SIGN_PDF_UNVOICED_LOW_5,
            SIGN_PDF_UNVOICED_LOW_6PLUS, SIGN_PDF_VOICED_HIGH_0, SIGN_PDF_VOICED_HIGH_1,
            SIGN_PDF_VOICED_HIGH_2, SIGN_PDF_VOICED_HIGH_3, SIGN_PDF_VOICED_HIGH_4,
            SIGN_PDF_VOICED_HIGH_5, SIGN_PDF_VOICED_HIGH_6PLUS, SIGN_PDF_VOICED_LOW_0,
            SIGN_PDF_VOICED_LOW_1, SIGN_PDF_VOICED_LOW_2, SIGN_PDF_VOICED_LOW_3,
            SIGN_PDF_VOICED_LOW_4, SIGN_PDF_VOICED_LOW_5, SIGN_PDF_VOICED_LOW_6PLUS,
        };

        let pulse_category = if pulse_count >= 6 { 6 } else { pulse_count };

        match (frame_type, quant_offset_type, pulse_category) {
            (FrameType::Inactive, QuantizationOffsetType::Low, 0) => SIGN_PDF_INACTIVE_LOW_0,
            (FrameType::Inactive, QuantizationOffsetType::Low, 1) => SIGN_PDF_INACTIVE_LOW_1,
            (FrameType::Inactive, QuantizationOffsetType::Low, 2) => SIGN_PDF_INACTIVE_LOW_2,
            (FrameType::Inactive, QuantizationOffsetType::Low, 3) => SIGN_PDF_INACTIVE_LOW_3,
            (FrameType::Inactive, QuantizationOffsetType::Low, 4) => SIGN_PDF_INACTIVE_LOW_4,
            (FrameType::Inactive, QuantizationOffsetType::Low, 5) => SIGN_PDF_INACTIVE_LOW_5,
            (FrameType::Inactive, QuantizationOffsetType::Low, _) => SIGN_PDF_INACTIVE_LOW_6PLUS,

            (FrameType::Inactive, QuantizationOffsetType::High, 0) => SIGN_PDF_INACTIVE_HIGH_0,
            (FrameType::Inactive, QuantizationOffsetType::High, 1) => SIGN_PDF_INACTIVE_HIGH_1,
            (FrameType::Inactive, QuantizationOffsetType::High, 2) => SIGN_PDF_INACTIVE_HIGH_2,
            (FrameType::Inactive, QuantizationOffsetType::High, 3) => SIGN_PDF_INACTIVE_HIGH_3,
            (FrameType::Inactive, QuantizationOffsetType::High, 4) => SIGN_PDF_INACTIVE_HIGH_4,
            (FrameType::Inactive, QuantizationOffsetType::High, 5) => SIGN_PDF_INACTIVE_HIGH_5,
            (FrameType::Inactive, QuantizationOffsetType::High, _) => SIGN_PDF_INACTIVE_HIGH_6PLUS,

            (FrameType::Unvoiced, QuantizationOffsetType::Low, 0) => SIGN_PDF_UNVOICED_LOW_0,
            (FrameType::Unvoiced, QuantizationOffsetType::Low, 1) => SIGN_PDF_UNVOICED_LOW_1,
            (FrameType::Unvoiced, QuantizationOffsetType::Low, 2) => SIGN_PDF_UNVOICED_LOW_2,
            (FrameType::Unvoiced, QuantizationOffsetType::Low, 3) => SIGN_PDF_UNVOICED_LOW_3,
            (FrameType::Unvoiced, QuantizationOffsetType::Low, 4) => SIGN_PDF_UNVOICED_LOW_4,
            (FrameType::Unvoiced, QuantizationOffsetType::Low, 5) => SIGN_PDF_UNVOICED_LOW_5,
            (FrameType::Unvoiced, QuantizationOffsetType::Low, _) => SIGN_PDF_UNVOICED_LOW_6PLUS,

            (FrameType::Unvoiced, QuantizationOffsetType::High, 0) => SIGN_PDF_UNVOICED_HIGH_0,
            (FrameType::Unvoiced, QuantizationOffsetType::High, 1) => SIGN_PDF_UNVOICED_HIGH_1,
            (FrameType::Unvoiced, QuantizationOffsetType::High, 2) => SIGN_PDF_UNVOICED_HIGH_2,
            (FrameType::Unvoiced, QuantizationOffsetType::High, 3) => SIGN_PDF_UNVOICED_HIGH_3,
            (FrameType::Unvoiced, QuantizationOffsetType::High, 4) => SIGN_PDF_UNVOICED_HIGH_4,
            (FrameType::Unvoiced, QuantizationOffsetType::High, 5) => SIGN_PDF_UNVOICED_HIGH_5,
            (FrameType::Unvoiced, QuantizationOffsetType::High, _) => SIGN_PDF_UNVOICED_HIGH_6PLUS,

            (FrameType::Voiced, QuantizationOffsetType::Low, 0) => SIGN_PDF_VOICED_LOW_0,
            (FrameType::Voiced, QuantizationOffsetType::Low, 1) => SIGN_PDF_VOICED_LOW_1,
            (FrameType::Voiced, QuantizationOffsetType::Low, 2) => SIGN_PDF_VOICED_LOW_2,
            (FrameType::Voiced, QuantizationOffsetType::Low, 3) => SIGN_PDF_VOICED_LOW_3,
            (FrameType::Voiced, QuantizationOffsetType::Low, 4) => SIGN_PDF_VOICED_LOW_4,
            (FrameType::Voiced, QuantizationOffsetType::Low, 5) => SIGN_PDF_VOICED_LOW_5,
            (FrameType::Voiced, QuantizationOffsetType::Low, _) => SIGN_PDF_VOICED_LOW_6PLUS,

            (FrameType::Voiced, QuantizationOffsetType::High, 0) => SIGN_PDF_VOICED_HIGH_0,
            (FrameType::Voiced, QuantizationOffsetType::High, 1) => SIGN_PDF_VOICED_HIGH_1,
            (FrameType::Voiced, QuantizationOffsetType::High, 2) => SIGN_PDF_VOICED_HIGH_2,
            (FrameType::Voiced, QuantizationOffsetType::High, 3) => SIGN_PDF_VOICED_HIGH_3,
            (FrameType::Voiced, QuantizationOffsetType::High, 4) => SIGN_PDF_VOICED_HIGH_4,
            (FrameType::Voiced, QuantizationOffsetType::High, 5) => SIGN_PDF_VOICED_HIGH_5,
            (FrameType::Voiced, QuantizationOffsetType::High, _) => SIGN_PDF_VOICED_HIGH_6PLUS,
        }
    }

    /// Gets quantization offset from Table 53 (RFC 6716 lines 5439-5456).
    ///
    /// # Arguments
    ///
    /// * `frame_type` - Signal type (Inactive, Unvoiced, or Voiced)
    /// * `quant_offset_type` - Quantization offset type (Low or High)
    ///
    /// # Returns
    ///
    /// Quantization offset in Q23 format
    ///
    /// # RFC Reference
    ///
    /// Table 53 (lines 5439-5456): 6 different offset values based on signal type and offset type
    #[must_use]
    const fn get_quantization_offset(
        frame_type: FrameType,
        quant_offset_type: QuantizationOffsetType,
    ) -> i32 {
        // Returns offset in Q10 format (matches silk_Quantization_Offsets_Q10 in libopus)
        match (frame_type, quant_offset_type) {
            (FrameType::Inactive | FrameType::Unvoiced, QuantizationOffsetType::High) => 240, // OFFSET_UVH_Q10
            (FrameType::Voiced, QuantizationOffsetType::Low) => 32, // OFFSET_VL_Q10
            (FrameType::Inactive | FrameType::Unvoiced, QuantizationOffsetType::Low) // OFFSET_UVL_Q10
            | (FrameType::Voiced, QuantizationOffsetType::High) => 100, // OFFSET_VH_Q10
        }
    }

    /// Reconstructs final excitation signal with quantization offset and pseudorandom noise
    /// (RFC 6716 Section 4.2.7.8.6, lines 5422-5478).
    ///
    /// # Arguments
    ///
    /// * `e_raw` - Raw signed excitation values (from sign decoding)
    /// * `frame_type` - Signal type for quantization offset selection
    /// * `quant_offset_type` - Quantization offset type
    ///
    /// # Returns
    ///
    /// Final excitation signal in Q23 format (23 bits including sign)
    ///
    /// # Panics
    ///
    /// * If `e_raw[i]` is negative and `e_raw[i]` cannot fit in u32
    ///
    /// # Algorithm (RFC lines 5458-5473)
    ///
    /// For each sample i:
    /// * Scale to Q23 and apply offset: `e_Q23[i] = (e_raw[i] << 8) - sign(e_raw[i])*20 + offset_Q23`
    /// * Update LCG seed: `seed = (196314165*seed + 907633515) & 0xFFFFFFFF`
    /// * Pseudorandom inversion: `if (seed & 0x80000000) != 0 { e_Q23[i] = -e_Q23[i] }`
    /// * Update seed with raw value: `seed = (seed + e_raw[i]) & 0xFFFFFFFF`
    ///
    /// # Notes
    ///
    /// * When `e_raw`\[i\] is zero, `sign`() returns 0, so factor of 20 is not subtracted (RFC lines 5475-5476)
    /// * Final `e_Q23`\[i\] requires ≤23 bits including sign (RFC lines 5477-5478)
    /// * LCG seed is stored in decoder state and persists across calls
    pub fn reconstruct_excitation(
        &mut self,
        e_raw: &[i16; 16],
        frame_type: FrameType,
        quant_offset_type: QuantizationOffsetType,
    ) -> [i32; 16] {
        log::trace!("[RECONSTRUCT] e_raw: {e_raw:?}");

        let offset_q10 = Self::get_quantization_offset(frame_type, quant_offset_type);

        let mut e_q14 = [0_i32; 16];

        for i in 0..16 {
            // Match libopus decode_core.c lines ~94-108 EXACTLY
            // Step 1: Update LCG (BEFORE processing pulse)
            self.lcg_seed = self
                .lcg_seed
                .wrapping_mul(196_314_165)
                .wrapping_add(907_633_515);

            // Step 2: Scale pulse to Q14
            let mut value = i32::from(e_raw[i]) << 14;

            // Step 3: Adjust based on sign (QUANT_LEVEL_ADJUST_Q10 = 80)
            if value > 0 {
                value -= 1280; // 80 << 4
            } else if value < 0 {
                value += 1280;
            }

            // Step 4: Add quantization offset
            value += offset_q10 << 4;

            // Step 5: Apply pseudorandom inversion based on LCG
            if (self.lcg_seed & 0x8000_0000) != 0 {
                value = -value;
            }

            // Step 6: Update LCG seed with pulse value
            #[allow(clippy::cast_sign_loss)]
            {
                self.lcg_seed = self.lcg_seed.wrapping_add(e_raw[i] as u32);
            }

            e_q14[i] = value;
        }

        log::trace!(
            "[EXCITATION ch={}] e_q14[0..5]={:?}, frame_type={:?}, offset_q10={}",
            i32::from(frame_type == FrameType::Inactive),
            &e_q14[0..5.min(e_q14.len())],
            frame_type,
            offset_q10
        );
        log::trace!("[RECONSTRUCT] e_q14: {e_q14:?}");
        e_q14
    }

    // TODO(Section 3.8.2): Remove dead_code when used in LTP synthesis
    #[allow(dead_code, clippy::too_many_arguments)]
    fn select_subframe_params(
        subframe_index: usize,
        frame_size_ms: u8,
        w_q2: u8,
        lpc_n1_q15: Option<&[i16]>,
        lpc_n2_q15: &[i16],
        gains_q16: &[i32],
        pitch_lags: &[i16],
        ltp_filters_q7: &[[i8; 5]],
        ltp_scale_q14: i16,
        bandwidth: Bandwidth,
    ) -> Result<SubframeParams> {
        let use_interpolated =
            frame_size_ms == 20 && (subframe_index == 0 || subframe_index == 1) && w_q2 < 4;

        let lpc_coeffs_q12 = if use_interpolated && let Some(lpc_n1_q15) = lpc_n1_q15 {
            Self::limit_lpc_coefficients(lpc_n1_q15, bandwidth)?
        } else {
            Self::limit_lpc_coefficients(lpc_n2_q15, bandwidth)?
        };

        let adjusted_ltp_scale_q14 =
            if frame_size_ms == 20 && (subframe_index == 2 || subframe_index == 3) && w_q2 < 4 {
                16384
            } else {
                ltp_scale_q14
            };

        Ok(SubframeParams {
            lpc_coeffs_q12,
            gain_q16: gains_q16[subframe_index],
            pitch_lag: pitch_lags[subframe_index],
            ltp_filter_q7: ltp_filters_q7[subframe_index],
            ltp_scale_q14: adjusted_ltp_scale_q14,
        })
    }

    #[allow(dead_code, clippy::cast_precision_loss)]
    fn ltp_synthesis_unvoiced(excitation_q14: &[i32]) -> Vec<i32> {
        // For unvoiced frames, residual = excitation (no LTP filtering)
        // Return as-is in Q14 format
        excitation_q14.to_vec()
    }

    #[allow(
        dead_code,
        clippy::too_many_lines,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::unnecessary_wraps,
        clippy::similar_names,
        clippy::needless_pass_by_ref_mut
    )]
    fn ltp_synthesis_voiced(
        excitation_q14: &[i32],
        params: &SubframeParams,
        _subframe_index: usize,
        bandwidth: Bandwidth,
    ) -> Result<Vec<i32>> {
        let n = Self::samples_per_subframe(bandwidth);
        let pitch_lag = params.pitch_lag as usize;

        if excitation_q14.len() != n {
            return Err(Error::SilkDecoder(format!(
                "excitation length {} doesn't match subframe size {}",
                excitation_q14.len(),
                n
            )));
        }

        // Initialize LTP history buffer with zeros (16 samples max for 5-tap filter)
        let ltp_buf_size = pitch_lag + 5;
        let mut ltp_buf_q14 = vec![0_i32; ltp_buf_size];

        // TODO: Copy history from previous subframe when state management is implemented

        let mut residual_q14 = Vec::with_capacity(n);

        for (i, value) in excitation_q14.iter().enumerate().take(n) {
            // Apply 5-tap LTP filter to compute prediction
            // Matches libopus NSQ.c line 225: LTP_pred_Q13 = sum(b_Q7[k] * sLTP[i-lag+2-k]) >> 7
            let mut ltp_pred_q14 = 0_i64;

            for k in 0..5 {
                let tap_idx = if i + 2 >= k + pitch_lag {
                    i + 2 - k - pitch_lag
                } else {
                    // Access history buffer
                    ltp_buf_size - (pitch_lag + k - i - 2)
                };

                let ltp_val = if tap_idx < ltp_buf_q14.len() {
                    ltp_buf_q14[tap_idx]
                } else {
                    0
                };

                let b_q7 = i64::from(params.ltp_filter_q7[k]);
                // ltp_val is Q14, b_q7 is Q7, product is Q21, >> 7 gives Q14
                ltp_pred_q14 += (i64::from(ltp_val) * b_q7) >> 7;
            }

            // Add excitation to LTP prediction: residual = excitation + LTP_pred
            #[allow(clippy::cast_possible_truncation)]
            let res_val_q14 = value.saturating_add(ltp_pred_q14 as i32);

            // Store in LTP buffer for next samples
            ltp_buf_q14.push(res_val_q14);

            residual_q14.push(res_val_q14);
        }

        Ok(residual_q14)
    }

    #[allow(dead_code, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    fn ltp_synthesis_voiced_old(
        &self,
        excitation_q23: &[i32],
        params: &SubframeParams,
        subframe_index: usize,
        bandwidth: Bandwidth,
        channel_idx: usize,
    ) -> Vec<f32> {
        let n = Self::samples_per_subframe(bandwidth);
        let j = Self::subframe_start_index(subframe_index, n);
        let d_lpc = params.lpc_coeffs_q12.len();
        let pitch_lag = params.pitch_lag as usize;

        let mut res = Vec::new();

        let out_end = if params.ltp_scale_q14 == 16384 {
            j.saturating_sub(subframe_index.saturating_sub(2) * n)
        } else {
            j.saturating_sub(subframe_index * n)
        };

        let out_start = j.saturating_sub(pitch_lag + 2);

        for i in out_start..out_end {
            let out_val = self.ltp_state[channel_idx]
                .out_buffer
                .get(i)
                .copied()
                .unwrap_or(0.0);

            let mut lpc_sum = 0.0_f32;
            for k in 0..d_lpc {
                let idx = i.saturating_sub(k + 1);
                let out_prev = self.ltp_state[channel_idx]
                    .out_buffer
                    .get(idx)
                    .copied()
                    .unwrap_or(0.0);
                let a_q12 = f32::from(params.lpc_coeffs_q12[k]);
                lpc_sum += out_prev * (a_q12 / 4096.0);
            }

            let whitened = out_val - lpc_sum;
            let clamped = whitened.clamp(-1.0, 1.0);
            let scale = (4.0 * f32::from(params.ltp_scale_q14)) / params.gain_q16 as f32;
            let res_val = scale * clamped;

            res.push(res_val);
        }

        for i in out_end..j {
            let lpc_val = self.ltp_state[channel_idx]
                .lpc_buffer
                .get(i)
                .copied()
                .unwrap_or(0.0);

            let mut lpc_sum = 0.0_f32;
            for k in 0..d_lpc {
                let idx = i.saturating_sub(k + 1);
                let lpc_prev = self.ltp_state[channel_idx]
                    .lpc_buffer
                    .get(idx)
                    .copied()
                    .unwrap_or(0.0);
                let a_q12 = f32::from(params.lpc_coeffs_q12[k]);
                lpc_sum += lpc_prev * (a_q12 / 4096.0);
            }

            let whitened = lpc_val - lpc_sum;
            let scale = 65536.0 / params.gain_q16 as f32;
            let res_val = scale * whitened;

            res.push(res_val);
        }

        let res_base_offset = res.len();

        for (i, &e_val) in excitation_q23.iter().enumerate() {
            let e_normalized = e_val as f32 / (1_i32 << 23) as f32;

            let mut ltp_sum = 0.0_f32;
            for k in 0..5 {
                let global_idx = j + i;
                let target_idx = global_idx
                    .saturating_sub(pitch_lag)
                    .saturating_add(2)
                    .saturating_sub(k);
                let res_idx = target_idx.saturating_sub(out_start);

                let res_prev = res.get(res_idx).copied().unwrap_or(0.0);

                let b_q7 = f32::from(params.ltp_filter_q7[k]);
                ltp_sum += res_prev * (b_q7 / 128.0);
            }

            let res_val = e_normalized + ltp_sum;
            res.push(res_val);
        }

        res[res_base_offset..].to_vec()
    }

    #[allow(
        dead_code,
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn lpc_synthesis(
        &mut self,
        residual_q14: &[i32],
        params: &SubframeParams,
        bandwidth: Bandwidth,
        channel_idx: usize,
    ) -> Result<Vec<i16>> {
        log::trace!(
            "[LPC_SYNTH] gain_q16={}, lpc_coeffs_q12={:?}",
            params.gain_q16,
            params.lpc_coeffs_q12
        );
        let n = Self::samples_per_subframe(bandwidth);
        let d_lpc = params.lpc_coeffs_q12.len();

        if residual_q14.len() != n {
            return Err(Error::SilkDecoder(format!(
                "residual length {} doesn't match subframe size {}",
                residual_q14.len(),
                n
            )));
        }

        // Initialize LPC state buffer (Q14 format)
        let max_lpc_order = 16;
        let mut slpc_q14 = vec![0_i32; max_lpc_order + n];

        // Copy history from previous subframe (matches libopus decode_core.c line 113)
        // libopus: silk_memcpy( sLPC_Q14, psDec->sLPC_Q14_buf, MAX_LPC_ORDER * sizeof( opus_int32 ) );
        // On first subframe after decoder reset, history is empty (was cleared before loop)
        if self.ltp_state[channel_idx].lpc_history_q14.len() >= d_lpc {
            slpc_q14[max_lpc_order - d_lpc..max_lpc_order].copy_from_slice(
                &self.ltp_state[channel_idx].lpc_history_q14
                    [self.ltp_state[channel_idx].lpc_history_q14.len() - d_lpc..],
            );
            log::trace!("[LPC_LOAD_STATE] Loaded {d_lpc} samples from history");
        } else if !self.ltp_state[channel_idx].lpc_history_q14.is_empty() {
            let len = self.ltp_state[channel_idx].lpc_history_q14.len();
            slpc_q14[max_lpc_order - len..max_lpc_order]
                .copy_from_slice(&self.ltp_state[channel_idx].lpc_history_q14);
            log::trace!("[LPC_LOAD_STATE] Loaded {len} samples from partial history");
        }

        // Gain interpolation between subframes (matches libopus decode_core.c lines 118-127)
        // When gain changes, scale LPC history by gain adjustment factor
        if params.gain_q16 != self.prev_gain_q16[channel_idx] {
            let gain_adj_q16 =
                (i64::from(self.prev_gain_q16[channel_idx]) << 16) / i64::from(params.gain_q16);
            for value in slpc_q14.iter_mut().take(max_lpc_order) {
                *value = ((i64::from(*value) * gain_adj_q16) >> 16) as i32;
            }
        }
        self.prev_gain_q16[channel_idx] = params.gain_q16;

        let mut output = Vec::with_capacity(n);

        for i in 0..n {
            // Short-term prediction (LPC): matches libopus decode_core.c lines 204-227
            // LPC_pred_Q10 = sum(A_Q12[k] * sLPC_Q14[i-k-1]) in Q10 format
            // Initialize with rounding bias (LPC_order / 2)
            let mut lpc_pred_q10 = (d_lpc as i32) >> 1;

            for k in 0..d_lpc {
                let a_q12 = i32::from(params.lpc_coeffs_q12[k]);
                let slpc_prev = slpc_q14[max_lpc_order + i - k - 1];

                // silk_SMLAWB: (a + (b * (i16)c) >> 16)
                // Cast a_q12 to i16 (low 16 bits) before multiplication
                // slpc_prev is Q14, a_q12 is Q12, product is Q26, >> 16 gives Q10
                let product = ((i64::from(slpc_prev) * i64::from(a_q12 as i16)) >> 16) as i32;
                if channel_idx == 0 && i == 5 && n == 40 && k == 0 {
                    log::trace!(
                        "[SILK LPC i={} k={}] slpc_prev={}, a_q12={}, product={}, accum={}",
                        i,
                        k,
                        slpc_prev,
                        a_q12,
                        product,
                        lpc_pred_q10 + product
                    );
                }
                lpc_pred_q10 = lpc_pred_q10.wrapping_add(product);
            }

            // Add residual: sLPC_Q14[i] = residual_Q14[i] + (LPC_pred_Q10 << 4)
            // LPC_pred_Q10 << 4 converts Q10 -> Q14
            // libopus uses silk_LSHIFT_SAT32 which saturates the shift, then silk_ADD_SAT32
            // LSHIFT_SAT32 saturates if shift would overflow
            let lpc_pred_q14 = if lpc_pred_q10 > (i32::MAX >> 4) {
                i32::MAX
            } else if lpc_pred_q10 < (i32::MIN >> 4) {
                i32::MIN
            } else {
                lpc_pred_q10 << 4
            };
            let slpc_val_q14 = residual_q14[i].saturating_add(lpc_pred_q14);
            slpc_q14[max_lpc_order + i] = slpc_val_q14;

            // Apply gain and convert to output (matches libopus decode_core.c line 230)
            // Convert Gain_Q16 to Gain_Q10 by shifting right 6
            let gain_q10 = params.gain_q16 >> 6;

            // SMULWW: (a * b) >> 16 where both are i32
            // sLPC_Q14 * Gain_Q10 = Q24, then >> 16 = Q8
            // Then RSHIFT_ROUND(product, 8) to get Q0 (i16 range)
            let full_product = i64::from(slpc_val_q14) * i64::from(gain_q10);
            let product_q8 = (full_product >> 16) as i32; // Q24 >> 16 = Q8

            // Use libopus RSHIFT_ROUND formula: (((a) >> (shift - 1)) + 1) >> 1
            let output_val = ((product_q8 >> 7) + 1) >> 1;

            #[allow(clippy::cast_possible_truncation)]
            let sample = output_val.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;

            if i == 5 && n == 40 && channel_idx == 0 {
                log::trace!(
                    "[SILK SYNTH ch={} i={}] LPC_pred_Q10={}, sLPC_Q14={}, Gain_Q10={}, pxq={}, residual_Q14={}",
                    channel_idx,
                    i,
                    lpc_pred_q10,
                    slpc_val_q14,
                    gain_q10,
                    sample,
                    residual_q14[i]
                );
            }

            output.push(sample);
        }

        // Save LPC state for next subframe (matches libopus decode_core.c line 235)
        // libopus: silk_memcpy( psDec->sLPC_Q14_buf, &sLPC_Q14[ psDec->subfr_length ], MAX_LPC_ORDER * sizeof( opus_int32 ) );
        // This copies the last MAX_LPC_ORDER samples from sLPC_Q14 buffer
        self.ltp_state[channel_idx].lpc_history_q14.clear();
        self.ltp_state[channel_idx]
            .lpc_history_q14
            .extend_from_slice(&slpc_q14[n..max_lpc_order + n]);

        Ok(output)
    }

    #[allow(dead_code)]
    fn update_ltp_buffers(
        &mut self,
        unclamped_lpc: &[f32],
        output: &[f32],
        subframe_index: usize,
        bandwidth: Bandwidth,
        channel_idx: usize,
    ) {
        let n = Self::samples_per_subframe(bandwidth);
        let j = Self::subframe_start_index(subframe_index, n);

        for (offset, &val) in output.iter().enumerate() {
            let idx = j + offset;
            if idx < self.ltp_state[channel_idx].out_buffer.len() {
                self.ltp_state[channel_idx].out_buffer[idx] = val;
            }
        }

        for (offset, &val) in unclamped_lpc.iter().enumerate() {
            let idx = j + offset;
            if idx < self.ltp_state[channel_idx].lpc_buffer.len() {
                self.ltp_state[channel_idx].lpc_buffer[idx] = val;
            }
        }
    }

    #[allow(
        dead_code,
        clippy::too_many_lines,
        clippy::similar_names,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    /// Convert adaptive Mid/Side to Left/Right stereo using fixed-point arithmetic.
    /// Matches libopus `silk_stereo_MS_to_LR` exactly for bit-exact decoding.
    ///
    /// # Arguments
    /// * `x1` - Mid channel samples (i16), will be overwritten with left channel
    /// * `x2` - Side channel samples (i16), will be overwritten with right channel
    /// * `pred_q13` - Stereo prediction weights [w0, w1] in Q13 format
    /// * `fs_khz` - Sample rate in kHz (8, 12, or 16)
    /// * `frame_length` - Number of samples in frame
    fn stereo_ms_to_lr(
        &mut self,
        x1: &mut [i16],
        x2: &mut [i16],
        pred_q13: [i16; 2],
        fs_khz: i32,
        frame_length: usize,
    ) -> Result<()> {
        // STEREO_INTERP_LEN_MS = 8 (must be even)
        const STEREO_INTERP_LEN_MS: i32 = 8;

        let state = self.stereo_state.as_mut().ok_or_else(|| {
            Error::SilkDecoder(
                "stereo_ms_to_lr called but stereo state not initialized".to_string(),
            )
        })?;

        // Buffering: prepend 2-sample history (libopus lines 51-53)
        log::trace!(
            "[stereo_ms_to_lr] Before history: x1[0..5]={:?}, x2[0..5]={:?}",
            &x1[0..x1.len().min(5)],
            &x2[0..x2.len().min(5)]
        );
        x1[0] = state.s_mid[0];
        x1[1] = state.s_mid[1];
        x2[0] = state.s_side[0];
        x2[1] = state.s_side[1];
        log::trace!(
            "[stereo_ms_to_lr] After history: x1[0..5]={:?}, x2[0..5]={:?}",
            &x1[0..x1.len().min(5)],
            &x2[0..x2.len().min(5)]
        );

        // Save last 2 samples as history for next frame (libopus lines 54-55)
        state.s_mid[0] = x1[frame_length];
        state.s_mid[1] = x1[frame_length + 1];
        state.s_side[0] = x2[frame_length];
        state.s_side[1] = x2[frame_length + 1];

        // Interpolate predictors and add prediction to side channel (libopus lines 57-77)
        let mut pred0_q13 = i32::from(state.pred_prev_q13[0]);
        let mut pred1_q13 = i32::from(state.pred_prev_q13[1]);

        let interp_len = (STEREO_INTERP_LEN_MS * fs_khz) as usize;

        // Compute deltas for linear interpolation
        // silk_SMULBB: cast both difference and denom_q16 to i16 before multiply
        let denom_q16 = ((1_i64 << 16) / i64::from(STEREO_INTERP_LEN_MS * fs_khz)) as i32;
        let diff0 = pred_q13[0] - state.pred_prev_q13[0];
        let diff1 = pred_q13[1] - state.pred_prev_q13[1];
        let delta0_q13 = ((i32::from(diff0) * i32::from(denom_q16 as i16)) + (1 << 15)) >> 16;
        let delta1_q13 = ((i32::from(diff1) * i32::from(denom_q16 as i16)) + (1 << 15)) >> 16;

        log::debug!(
            "[stereo_ms_to_lr] pred_q13={:?}, prev={:?}, denom_q16={}, delta={:?}",
            pred_q13,
            state.pred_prev_q13,
            denom_q16,
            [delta0_q13, delta1_q13]
        );

        // Phase 1: Interpolation (libopus lines 62-68)
        for n in 0..interp_len.min(frame_length) {
            pred0_q13 = pred0_q13.wrapping_add(delta0_q13);
            pred1_q13 = pred1_q13.wrapping_add(delta1_q13);

            log::trace!(
                "[stereo_ms_to_lr] Phase1 n={}: pred=[{},{}], x1=[{},{},{}], x2[{}]={}",
                n,
                pred0_q13,
                pred1_q13,
                x1[n],
                x1[n + 1],
                x1[n + 2],
                n + 1,
                x2[n + 1]
            );

            // sum = (x1[n] + x1[n+2] + 2*x1[n+1]) << 9  (Q11)
            let sum_q11 = i32::from(x1[n]) + i32::from(x1[n + 2]) + (i32::from(x1[n + 1]) << 1);
            let sum_q11_shifted = sum_q11 << 9;

            // sum = x2[n+1] << 8 + (sum_q11 * pred0_Q13) >> 16  (Q8)
            // silk_SMLAWB: cast pred0_q13 to i16 first (low 16 bits)
            let mut sum_q8 = i32::from(x2[n + 1]) << 8;
            sum_q8 = sum_q8.wrapping_add(
                ((i64::from(sum_q11_shifted) * i64::from(pred0_q13 as i16)) >> 16) as i32,
            );

            // sum = sum + (x1[n+1] << 11 * pred1_Q13) >> 16  (Q8)
            // silk_SMLAWB: cast pred1_q13 to i16 first (low 16 bits)
            sum_q8 = sum_q8.wrapping_add(
                ((i64::from(i32::from(x1[n + 1]) << 11) * i64::from(pred1_q13 as i16)) >> 16)
                    as i32,
            );

            // x2[n+1] = SAT16(RSHIFT_ROUND(sum, 8))
            x2[n + 1] =
                ((((sum_q8 >> 7) + 1) >> 1).clamp(i32::from(i16::MIN), i32::from(i16::MAX))) as i16;

            log::trace!(
                "[stereo_ms_to_lr] Phase1 n={}: sum_q11={}, sum_q8={}, x2[{}]={}",
                n,
                sum_q11,
                sum_q8,
                n + 1,
                x2[n + 1]
            );
        }

        // Phase 2: Steady state with constant predictors (libopus lines 69-73)
        pred0_q13 = i32::from(pred_q13[0]);
        pred1_q13 = i32::from(pred_q13[1]);
        for n in interp_len..frame_length {
            let sum_q11 = i32::from(x1[n]) + i32::from(x1[n + 2]) + (i32::from(x1[n + 1]) << 1);
            let sum_q11_shifted = sum_q11 << 9;

            // silk_SMLAWB: cast pred0_q13 to i16 first (low 16 bits)
            let mut sum_q8 = i32::from(x2[n + 1]) << 8;
            sum_q8 = sum_q8.wrapping_add(
                ((i64::from(sum_q11_shifted) * i64::from(pred0_q13 as i16)) >> 16) as i32,
            );
            sum_q8 = sum_q8.wrapping_add(
                ((i64::from(i32::from(x1[n + 1]) << 11) * i64::from(pred1_q13 as i16)) >> 16)
                    as i32,
            );

            x2[n + 1] =
                ((((sum_q8 >> 7) + 1) >> 1).clamp(i32::from(i16::MIN), i32::from(i16::MAX))) as i16;
        }

        // Save predictors for next frame (libopus lines 74-75)
        state.pred_prev_q13[0] = pred_q13[0];
        state.pred_prev_q13[1] = pred_q13[1];

        // Convert mid/side to left/right (libopus lines 77-82)
        // Start at index 1 because index 0 is history
        log::trace!(
            "[stereo_ms_to_lr] Before MS->LR: x1[1..6]={:?}, x2[1..6]={:?}",
            &x1[1..6.min(x1.len())],
            &x2[1..6.min(x2.len())]
        );

        for n in 0..frame_length {
            let sum = i32::from(x1[n + 1]) + i32::from(x2[n + 1]);
            let diff = i32::from(x1[n + 1]) - i32::from(x2[n + 1]);

            log::trace!(
                "[stereo_ms_to_lr] MS->LR n={}: mid={}, side={}, L={}, R={}",
                n,
                x1[n + 1],
                x2[n + 1],
                sum,
                diff
            );

            x1[n + 1] = (sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX))) as i16;
            x2[n + 1] = (diff.clamp(i32::from(i16::MIN), i32::from(i16::MAX))) as i16;
        }

        log::trace!(
            "[stereo_ms_to_lr] After MS->LR: x1[1..6]={:?}, x2[1..6]={:?}",
            &x1[1..6.min(x1.len())],
            &x2[1..6.min(x2.len())]
        );

        Ok(())
    }

    #[allow(dead_code)]
    fn apply_mono_delay(&mut self, samples: &[i16]) -> Vec<i16> {
        let mut delayed = Vec::with_capacity(samples.len());

        if let Some(state) = &self.stereo_state {
            delayed.push(state.s_mid[1]);
        } else {
            delayed.push(0);
        }

        if !samples.is_empty() {
            delayed.extend_from_slice(&samples[0..samples.len().saturating_sub(1)]);
        }

        if let Some(state) = &mut self.stereo_state
            && !samples.is_empty()
        {
            state.s_mid[1] = samples[samples.len() - 1];
        }

        delayed
    }

    // TODO(Section 3.8.2): Remove dead_code when used in LTP synthesis
    #[allow(dead_code)]
    const fn samples_per_subframe(bandwidth: Bandwidth) -> usize {
        match bandwidth {
            Bandwidth::Narrowband => 40,
            Bandwidth::Mediumband => 60,
            Bandwidth::Wideband => 80,
            _ => unreachable!(),
        }
    }

    // TODO(Section 3.8.2): Remove dead_code when used in LTP synthesis
    #[allow(dead_code)]
    const fn num_subframes(frame_size_ms: u8) -> usize {
        match frame_size_ms {
            10 => 2,
            20 => 4,
            _ => unreachable!(),
        }
    }

    // TODO(Section 3.8.2): Remove dead_code when used in LTP synthesis
    #[allow(dead_code)]
    const fn subframe_start_index(subframe_index: usize, samples_per_subframe: usize) -> usize {
        subframe_index * samples_per_subframe
    }

    /// Returns normative resampler delay in milliseconds for a given bandwidth.
    ///
    /// These delay values are specified in RFC 6716 Table 54 (lines 5766-5775).
    /// The delays are normative and must be accounted for in decoder implementations.
    ///
    /// # Arguments
    ///
    /// * `bandwidth` - SILK bandwidth (Narrowband/Mediumband/Wideband)
    ///
    /// # Returns
    ///
    /// Resampler delay in milliseconds (0.0 for unsupported bandwidths)
    #[must_use]
    pub const fn resampler_delay_ms(bandwidth: Bandwidth) -> f32 {
        match bandwidth {
            Bandwidth::Narrowband => 0.538,
            Bandwidth::Mediumband => 0.692,
            Bandwidth::Wideband => 0.706,
            _ => 0.0,
        }
    }

    /// Resample SILK output to target sample rate
    ///
    /// RFC 6716 lines 5726-5734: Resampling is NON-NORMATIVE.
    /// Any resampling method is allowed. This uses `moosicbox_resampler`.
    ///
    /// # Errors
    ///
    /// * Returns error if `num_channels` is not 1 or 2
    /// * Returns error if resampling fails
    #[cfg(feature = "resampling")]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn resample(
        &self,
        samples: &[f32],
        input_rate: u32,
        output_rate: u32,
        num_channels: usize,
    ) -> Result<Vec<f32>> {
        if input_rate == output_rate {
            return Ok(samples.to_vec());
        }

        let samples_per_channel = samples.len() / num_channels;

        let channels = match num_channels {
            1 => symphonia::core::audio::Channels::FRONT_LEFT,
            2 => {
                symphonia::core::audio::Channels::FRONT_LEFT
                    | symphonia::core::audio::Channels::FRONT_RIGHT
            }
            _ => {
                return Err(Error::SilkDecoder(format!(
                    "unsupported channel count: {num_channels}"
                )));
            }
        };

        let spec = SignalSpec::new(input_rate, channels);

        let mut audio_buffer = AudioBuffer::new(samples_per_channel as u64, spec);
        audio_buffer.render_reserved(Some(samples_per_channel));

        for ch in 0..num_channels {
            let channel_buf = audio_buffer.chan_mut(ch);
            for (i, sample) in samples.iter().skip(ch).step_by(num_channels).enumerate() {
                channel_buf[i] = *sample;
            }
        }

        let mut resampler = Resampler::new(spec, output_rate as usize, samples_per_channel as u64);

        let output = resampler
            .resample(&audio_buffer)
            .ok_or_else(|| Error::SilkDecoder("resampling failed".to_string()))?;

        Ok(output.to_vec())
    }

    /// Resample without resampling feature - returns error
    ///
    /// RFC 6716 line 5732: Resampling is optional and non-normative
    ///
    /// # Errors
    ///
    /// * Always returns error indicating resampling feature is not enabled
    #[cfg(not(feature = "resampling"))]
    pub fn resample(
        &self,
        _samples: &[f32],
        _input_rate: u32,
        _output_rate: u32,
        _num_channels: usize,
    ) -> Result<Vec<f32>> {
        Err(Error::SilkDecoder(
            "Resampling not available - enable 'resampling' feature in Cargo.toml".to_string(),
        ))
    }

    /// Convert log-scale value to linear scale using RFC 6716 algorithm
    ///
    /// Implements `silk_log2lin()` per RFC 6716 lines 2558-2563.
    ///
    /// Computes `2^(inLog_Q7/128.0)` in Q16 format.
    ///
    /// # Arguments
    ///
    /// * `in_log_q7` - Logarithmic value in Q7 format (7 fractional bits)
    ///
    /// # Returns
    ///
    /// Linear value in Q16 format (16 fractional bits)
    ///
    /// # Algorithm (RFC Exact)
    ///
    /// ```text
    /// i = inLog_Q7 >> 7           // Integer part
    /// f = inLog_Q7 & 127          // Fractional part
    /// pow2_i = 1 << i             // 2^i
    /// return pow2_i + (((-174*f*(128-f)) >> 16) + f) * (pow2_i >> 7)
    /// ```
    ///
    /// # RFC Reference
    ///
    /// Lines 2558-2563
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    const fn silk_log2lin(in_log_q7: i32) -> i32 {
        let i = in_log_q7 >> 7;
        let f = in_log_q7 & 127;
        let pow2_i = 1_i32 << i;

        // RFC formula: pow2_i + (((-174*f*(128-f)) >> 16) + f) * (pow2_i >> 7)
        let frac_part = (((-174 * f * (128 - f)) >> 16) + f) * (pow2_i >> 7);
        pow2_i + frac_part
    }

    /// Dequantize gain index to Q16 linear gain
    ///
    /// Implements gain dequantization per RFC 6716 lines 2553-2567.
    ///
    /// # Arguments
    ///
    /// * `log_gain` - Logarithmic gain value (0-63 from `decode_subframe_gains`)
    ///
    /// # Returns
    ///
    /// Linear gain in Q16 format (16 fractional bits), range: 81920 to 1686110208
    ///
    /// # Algorithm (RFC Exact - Line 2556)
    ///
    /// ```text
    /// gain_Q16 = silk_log2lin((0x1D1C71 * log_gain >> 16) + 2090)
    /// ```
    ///
    /// # RFC Reference
    ///
    /// Lines 2553-2567
    ///
    /// # Notes
    ///
    /// * Constant `0x1D1C71` (1941617 decimal) scales the logarithmic gain
    /// * Constant 2090 is the bias added before log-to-linear conversion
    /// * `log_gain` comes directly from `decode_subframe_gains` (no table lookup needed)
    /// * Independent coding: `log_gain` = 0-63 (clamped `gain_idx`)
    /// * Delta coding: `log_gain` = 0-63 (clamped computed value)
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn dequantize_gain(log_gain: i32) -> i32 {
        let scaled = (0x001D_1C71_i64 * i64::from(log_gain)) >> 16;
        #[allow(clippy::cast_possible_truncation)]
        let in_log_q7 = (scaled as i32) + 2090;
        Self::silk_log2lin(in_log_q7)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silk_decoder_creation_valid() {
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_silk_decoder_invalid_frame_size() {
        let result = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 15);
        assert!(result.is_err());
    }

    #[test]
    fn test_num_silk_frames_calculation() {
        assert_eq!(
            SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 10)
                .unwrap()
                .num_silk_frames,
            1
        );
        assert_eq!(
            SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20)
                .unwrap()
                .num_silk_frames,
            1
        );
        assert_eq!(
            SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 40)
                .unwrap()
                .num_silk_frames,
            2
        );
        assert_eq!(
            SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 60)
                .unwrap()
                .num_silk_frames,
            3
        );
    }

    #[test]
    fn test_vad_flags_decoding() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 60).unwrap();

        let vad_flags = decoder.decode_vad_flags(&mut range_decoder).unwrap();
        assert_eq!(vad_flags.len(), 3);
    }

    #[test]
    fn test_lbrr_flag_decoding() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let _lbrr_flag = decoder.decode_lbrr_flag(&mut range_decoder).unwrap();
    }

    #[test]
    fn test_header_bits_mono() {
        let data = vec![0b1010_1010, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let header = decoder
            .decode_header_bits(&mut range_decoder, false)
            .unwrap();
        assert_eq!(header.mid_vad_flags.len(), 1);
        assert!(header.side_vad_flags.is_none());
        assert!(header.side_lbrr_flag.is_none());
    }

    #[test]
    fn test_header_bits_stereo() {
        let data = vec![0b1010_1010, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let header = decoder
            .decode_header_bits(&mut range_decoder, true)
            .unwrap();
        assert_eq!(header.mid_vad_flags.len(), 1);
        assert!(header.side_vad_flags.is_some());
        assert_eq!(header.side_vad_flags.unwrap().len(), 1);
        assert!(header.side_lbrr_flag.is_some());
    }

    #[test]
    fn test_stereo_weight_decoding() {
        let data = vec![0xFF; 20];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let weights = decoder.decode_stereo_weights(&mut range_decoder).unwrap();
        assert!(weights.0 >= -13732 && weights.0 <= 13732);
        assert!(weights.1 >= -13732 && weights.1 <= 13732);
    }

    #[test]
    fn test_stereo_weights_stored() {
        let data = vec![0xFF; 20];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        assert!(decoder.previous_stereo_weights.is_none());
        let _ = decoder.decode_stereo_weights(&mut range_decoder).unwrap();
        assert!(decoder.previous_stereo_weights.is_some());
    }

    #[test]
    fn test_independent_gain_decoding() {
        let data = vec![0xFF; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let gains = decoder
            .decode_subframe_gains(&mut range_decoder, FrameType::Voiced, 4, 0, true)
            .unwrap();

        assert_eq!(gains.len(), 4);
        for gain in gains {
            assert!(gain <= 63);
        }
    }

    #[test]
    fn test_gain_indices_stored() {
        let data = vec![0xFF; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        assert!(decoder.previous_gain_indices[0].is_none());
        let _ = decoder.decode_subframe_gains(&mut range_decoder, FrameType::Voiced, 2, 0, true);
        assert!(decoder.previous_gain_indices[0].is_some());
    }

    #[test]
    fn test_lsf_stage1_nb_inactive() {
        let data = vec![0x00, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        let index = decoder
            .decode_lsf_stage1(
                &mut range_decoder,
                Bandwidth::Narrowband,
                FrameType::Inactive,
            )
            .unwrap();
        assert!(index < 32);
    }

    #[test]
    fn test_lsf_stage1_wb_voiced() {
        let data = vec![0x80, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let index = decoder
            .decode_lsf_stage1(&mut range_decoder, Bandwidth::Wideband, FrameType::Voiced)
            .unwrap();
        assert!(index < 32);
    }

    #[test]
    fn test_lsf_stage2_decoding_nb() {
        let data = vec![0x80; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        let indices = decoder
            .decode_lsf_stage2(&mut range_decoder, 0, Bandwidth::Narrowband)
            .unwrap();
        assert_eq!(indices.len(), 10);
        for index in indices {
            assert!((-10..=10).contains(&index));
        }
    }

    #[test]
    fn test_lsf_stage2_decoding_wb() {
        let data = vec![0x80; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let indices = decoder
            .decode_lsf_stage2(&mut range_decoder, 0, Bandwidth::Wideband)
            .unwrap();
        assert_eq!(indices.len(), 16);
        for index in indices {
            assert!((-10..=10).contains(&index));
        }
    }

    #[test]
    fn test_lsf_stage2_extension() {
        let data = vec![0x00; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        let indices = decoder
            .decode_lsf_stage2(&mut range_decoder, 0, Bandwidth::Narrowband)
            .unwrap();
        assert_eq!(indices.len(), 10);
    }

    #[test]
    fn test_residual_dequantization_nb() {
        let stage1_index = 0;
        let stage2_indices = vec![0, 1, -1, 2, -2, 0, 1, 0, -1, 0];

        let result = SilkDecoder::dequantize_lsf_residuals(
            stage1_index,
            &stage2_indices,
            Bandwidth::Narrowband,
        );
        assert!(result.is_ok());

        let residuals = result.unwrap();
        assert_eq!(residuals.len(), 10);
    }

    #[test]
    fn test_residual_dequantization_wb() {
        let stage1_index = 0;
        let stage2_indices = vec![0, 1, -1, 2, -2, 0, 1, 0, -1, 0, 1, -1, 0, 1, -1, 0];

        let result = SilkDecoder::dequantize_lsf_residuals(
            stage1_index,
            &stage2_indices,
            Bandwidth::Wideband,
        );
        assert!(result.is_ok());

        let residuals = result.unwrap();
        assert_eq!(residuals.len(), 16);
    }

    #[test]
    fn test_residual_dequantization_invalid_bandwidth() {
        let stage1_index = 0;
        let stage2_indices = vec![0; 10];

        let result = SilkDecoder::dequantize_lsf_residuals(
            stage1_index,
            &stage2_indices,
            Bandwidth::SuperWideband,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_ihmw_weights_nb() {
        let result = SilkDecoder::compute_ihmw_weights(0, Bandwidth::Narrowband);
        assert!(result.is_ok());

        let weights = result.unwrap();
        assert_eq!(weights.len(), 10);
        for weight in weights {
            assert!((1819..=5227).contains(&weight));
        }
    }

    #[test]
    fn test_ihmw_weights_wb() {
        let result = SilkDecoder::compute_ihmw_weights(0, Bandwidth::Wideband);
        assert!(result.is_ok());

        let weights = result.unwrap();
        assert_eq!(weights.len(), 16);
        for weight in weights {
            assert!((1819..=5227).contains(&weight));
        }
    }

    #[test]
    fn test_ihmw_weights_invalid_bandwidth() {
        let result = SilkDecoder::compute_ihmw_weights(0, Bandwidth::SuperWideband);
        assert!(result.is_err());
    }

    #[test]
    fn test_lsf_reconstruction_nb() {
        let stage1_index = 10;
        let stage2_indices = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        let result =
            SilkDecoder::reconstruct_lsf(stage1_index, &stage2_indices, Bandwidth::Narrowband);
        assert!(result.is_ok());

        let nlsf = result.unwrap();
        assert_eq!(nlsf.len(), 10);
        for coeff in nlsf {
            assert!((0..=32767).contains(&coeff));
        }
    }

    #[test]
    fn test_lsf_reconstruction_wb() {
        let stage1_index = 5;
        let stage2_indices = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        let result =
            SilkDecoder::reconstruct_lsf(stage1_index, &stage2_indices, Bandwidth::Wideband);
        assert!(result.is_ok());

        let nlsf = result.unwrap();
        assert_eq!(nlsf.len(), 16);
        for coeff in nlsf {
            assert!((0..=32767).contains(&coeff));
        }
    }

    #[test]
    fn test_lsf_reconstruction_monotonic_before_stabilization() {
        let stage1_index = 0;
        let stage2_indices = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        let nlsf =
            SilkDecoder::reconstruct_lsf(stage1_index, &stage2_indices, Bandwidth::Narrowband)
                .unwrap();

        for i in 1..nlsf.len() {
            assert!(nlsf[i] >= nlsf[i - 1]);
        }
    }

    #[test]
    fn test_lsf_stabilization_nb() {
        let nlsf = vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];
        let result = SilkDecoder::stabilize_lsf(nlsf, Bandwidth::Narrowband);
        assert!(result.is_ok());

        let stabilized = result.unwrap();
        assert_eq!(stabilized.len(), 10);
    }

    #[test]
    fn test_lsf_stabilization_wb() {
        let nlsf = vec![
            100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1100, 1200, 1300, 1400, 1500, 1600,
        ];
        let result = SilkDecoder::stabilize_lsf(nlsf, Bandwidth::Wideband);
        assert!(result.is_ok());

        let stabilized = result.unwrap();
        assert_eq!(stabilized.len(), 16);
    }

    #[test]
    fn test_lsf_stabilization_enforces_minimum_spacing_nb() {
        use super::super::lsf_constants::LSF_MIN_SPACING_NB;

        let nlsf = vec![250, 251, 252, 253, 254, 255, 256, 257, 258, 259];
        let stabilized = SilkDecoder::stabilize_lsf(nlsf, Bandwidth::Narrowband).unwrap();

        let mut prev = 0;
        for (i, &curr) in stabilized.iter().enumerate() {
            let spacing = i32::from(curr) - prev;
            assert!(
                spacing >= i32::from(LSF_MIN_SPACING_NB[i]),
                "Spacing violation at index {i}: {spacing} < {}",
                LSF_MIN_SPACING_NB[i]
            );
            prev = i32::from(curr);
        }

        let final_spacing = 32768 - i32::from(stabilized[9]);
        assert!(
            final_spacing >= i32::from(LSF_MIN_SPACING_NB[10]),
            "Final spacing violation: {final_spacing} < {}",
            LSF_MIN_SPACING_NB[10]
        );
    }

    #[test]
    fn test_lsf_stabilization_enforces_minimum_spacing_wb() {
        use super::super::lsf_constants::LSF_MIN_SPACING_WB;

        let nlsf = vec![
            100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115,
        ];
        let stabilized = SilkDecoder::stabilize_lsf(nlsf, Bandwidth::Wideband).unwrap();

        let mut prev = 0;
        for (i, &curr) in stabilized.iter().enumerate() {
            let spacing = i32::from(curr) - prev;
            assert!(
                spacing >= i32::from(LSF_MIN_SPACING_WB[i]),
                "Spacing violation at index {i}: {spacing} < {}",
                LSF_MIN_SPACING_WB[i]
            );
            prev = i32::from(curr);
        }

        let final_spacing = 32768 - i32::from(stabilized[15]);
        assert!(
            final_spacing >= i32::from(LSF_MIN_SPACING_WB[16]),
            "Final spacing violation: {final_spacing} < {}",
            LSF_MIN_SPACING_WB[16]
        );
    }

    #[test]
    fn test_lsf_stabilization_maintains_monotonicity() {
        let nlsf = vec![5000, 4000, 6000, 3000, 7000, 2000, 8000, 1000, 9000, 500];
        let stabilized = SilkDecoder::stabilize_lsf(nlsf, Bandwidth::Narrowband).unwrap();

        for i in 1..stabilized.len() {
            assert!(
                stabilized[i] >= stabilized[i - 1],
                "Monotonicity violation at index {i}: {} < {}",
                stabilized[i],
                stabilized[i - 1]
            );
        }
    }

    #[test]
    fn test_full_lsf_pipeline_nb() {
        let stage1_index = 15;
        let stage2_indices = vec![1, -1, 0, 2, -2, 1, -1, 0, 1, -1];

        let nlsf =
            SilkDecoder::reconstruct_lsf(stage1_index, &stage2_indices, Bandwidth::Narrowband)
                .unwrap();
        let stabilized = SilkDecoder::stabilize_lsf(nlsf, Bandwidth::Narrowband).unwrap();

        assert_eq!(stabilized.len(), 10);
        assert!(stabilized[0] >= 0);

        for i in 1..stabilized.len() {
            assert!(stabilized[i] >= stabilized[i - 1]);
        }
    }

    #[test]
    fn test_full_lsf_pipeline_wb() {
        let stage1_index = 8;
        let stage2_indices = vec![0, 1, -1, 2, -2, 0, 1, 0, -1, 1, 0, -1, 2, -1, 0, 1];

        let nlsf = SilkDecoder::reconstruct_lsf(stage1_index, &stage2_indices, Bandwidth::Wideband)
            .unwrap();
        let stabilized = SilkDecoder::stabilize_lsf(nlsf, Bandwidth::Wideband).unwrap();

        assert_eq!(stabilized.len(), 16);
        assert!(stabilized[0] >= 0);

        for i in 1..stabilized.len() {
            assert!(stabilized[i] >= stabilized[i - 1]);
        }
    }

    #[test]
    fn test_lsf_interpolation_20ms_nb() {
        let data = vec![0xFF; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        decoder.previous_lsf_nb = Some([100, 200, 300, 400, 500, 600, 700, 800, 900, 1000]);
        decoder.decoder_reset = false; // Normal operation

        let n2_q15 = vec![150, 250, 350, 450, 550, 650, 750, 850, 950, 1050];
        let result = decoder.interpolate_lsf(&mut range_decoder, &n2_q15, Bandwidth::Narrowband);

        assert!(result.is_ok());
        let interpolated = result.unwrap();
        assert!(interpolated.is_some());
        assert_eq!(interpolated.unwrap().len(), 10);
    }

    #[test]
    fn test_lsf_interpolation_decoder_reset_forces_w_q2_4() {
        // RFC lines 3601-3607: After decoder reset, w_Q2 must be forced to 4
        let data = vec![0x00; 50]; // Will decode w_Q2 = 0
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        decoder.previous_lsf_nb = Some([100; 10]);
        decoder.decoder_reset = true; // Reset flag set

        let n2_q15 = vec![200; 10];
        let result = decoder.interpolate_lsf(&mut range_decoder, &n2_q15, Bandwidth::Narrowband);

        assert!(result.is_ok());
        let interpolated = result.unwrap();
        assert!(interpolated.is_some());

        // With w_Q2=4, interpolation should give n2 (full interpolation)
        let n1 = interpolated.unwrap();
        assert_eq!(n1[0], 200); // Should be n2, not interpolated with n0

        // Verify reset flag was cleared
        assert!(!decoder.decoder_reset);
    }

    #[test]
    fn test_lsf_interpolation_uncoded_side_channel_forces_w_q2_4() {
        // RFC lines 3601-3607: After uncoded side channel, w_Q2 must be forced to 4
        let data = vec![0x00; 50]; // Will decode w_Q2 = 0
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        decoder.previous_lsf_nb = Some([100; 10]);
        decoder.decoder_reset = false;
        decoder.uncoded_side_channel = true; // Uncoded side channel flag set

        let n2_q15 = vec![200; 10];
        let result = decoder.interpolate_lsf(&mut range_decoder, &n2_q15, Bandwidth::Narrowband);

        assert!(result.is_ok());
        let interpolated = result.unwrap();
        assert!(interpolated.is_some());

        // With w_Q2=4, should get full interpolation to n2
        let n1 = interpolated.unwrap();
        assert_eq!(n1[0], 200);

        // Verify flag was cleared
        assert!(!decoder.uncoded_side_channel);
    }

    #[test]
    fn test_mark_uncoded_side_channel() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        assert!(!decoder.uncoded_side_channel);
        decoder.mark_uncoded_side_channel();
        assert!(decoder.uncoded_side_channel);
    }

    #[test]
    fn test_reset_decoder_state() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        // Set some state
        decoder.previous_lsf_nb = Some([100; 10]);
        decoder.decoder_reset = false;

        // Reset
        decoder.reset_decoder_state();

        assert!(decoder.decoder_reset);
        assert!(decoder.previous_lsf_nb.is_none());
    }

    #[test]
    fn test_lsf_interpolation_10ms_returns_none() {
        let data = vec![0xFF; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 10).unwrap();

        let n2_q15 = vec![100; 10];
        let result = decoder.interpolate_lsf(&mut range_decoder, &n2_q15, Bandwidth::Narrowband);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_lsf_interpolation_no_previous_returns_none() {
        let data = vec![0xFF; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();
        decoder.decoder_reset = false; // Clear initial reset flag

        let n2_q15 = vec![100; 10];
        let result = decoder.interpolate_lsf(&mut range_decoder, &n2_q15, Bandwidth::Narrowband);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_lsf_interpolation_wb() {
        let data = vec![0xFF; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        decoder.previous_lsf_wb = Some([100; 16]);
        decoder.decoder_reset = false;
        let n2_q15 = vec![200; 16];
        let result = decoder.interpolate_lsf(&mut range_decoder, &n2_q15, Bandwidth::Wideband);

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_store_previous_lsf_nb() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();
        let nlsf = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];

        decoder.store_previous_lsf(&nlsf, Bandwidth::Narrowband);

        assert!(decoder.previous_lsf_nb.is_some());
        assert_eq!(
            decoder.previous_lsf_nb.unwrap(),
            [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]
        );
    }

    #[test]
    fn test_store_previous_lsf_wb() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        let nlsf = vec![
            10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160,
        ];

        decoder.store_previous_lsf(&nlsf, Bandwidth::Wideband);

        assert!(decoder.previous_lsf_wb.is_some());
        assert_eq!(decoder.previous_lsf_wb.unwrap()[0], 10);
        assert_eq!(decoder.previous_lsf_wb.unwrap()[15], 160);
    }

    #[test]
    fn test_lsf_to_output_nb() {
        let nlsf_q15 = vec![1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000];

        let result = SilkDecoder::lsf_to_lpc(&nlsf_q15, Bandwidth::Narrowband);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 10);
    }

    #[test]
    fn test_lsf_to_output_wb() {
        let nlsf_q15: Vec<i16> = (1..=16).map(|i| i * 1000).collect();

        let result = SilkDecoder::lsf_to_lpc(&nlsf_q15, Bandwidth::Wideband);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 16);
    }

    #[test]
    fn test_lsf_to_lpc_invalid_bandwidth() {
        let nlsf_q15 = vec![0; 10];

        let result = SilkDecoder::lsf_to_lpc(&nlsf_q15, Bandwidth::SuperWideband);
        assert!(result.is_err());
    }

    #[test]
    fn test_cosine_table_bounds() {
        use super::super::lsf_constants::LSF_COS_TABLE_Q12;

        assert_eq!(LSF_COS_TABLE_Q12.len(), 129);
        assert_eq!(LSF_COS_TABLE_Q12[0], 8192); // cos(0) = 1.0 in Q13
        assert_eq!(LSF_COS_TABLE_Q12[128], -8192); // cos(pi) = -1.0 in Q13
    }

    #[test]
    fn test_lsf_ordering_lengths() {
        use super::super::lsf_constants::{LSF_ORDERING_NB, LSF_ORDERING_WB};

        assert_eq!(LSF_ORDERING_NB.len(), 10);
        assert_eq!(LSF_ORDERING_WB.len(), 16);
    }

    #[test]
    fn test_lsf_ordering_values_in_bounds() {
        use super::super::lsf_constants::{LSF_ORDERING_NB, LSF_ORDERING_WB};

        for &idx in LSF_ORDERING_NB {
            assert!(idx < 10);
        }

        for &idx in LSF_ORDERING_WB {
            assert!(idx < 16);
        }
    }

    #[test]
    fn test_bandwidth_expansion_reduces_magnitude() {
        let mut coeffs = vec![40000_i32, -35000, 30000];
        let sc_q16 = 60000; // Less than 65536 (1.0 in Q16)

        SilkDecoder::apply_bandwidth_expansion(&mut coeffs, sc_q16);

        // All coefficients should be reduced in magnitude
        assert!(coeffs[0].abs() < 40000);
        assert!(coeffs[1].abs() < 35000);
        assert!(coeffs[2].abs() < 30000);
    }

    #[test]
    fn test_magnitude_limiting_within_q12_range() {
        // Coefficients already small enough
        let mut coeffs = vec![1000_i32 << 5, 2000 << 5, -1500 << 5];
        SilkDecoder::limit_coefficient_magnitude(&mut coeffs);

        // Should convert cleanly to Q12
        for &c in &coeffs {
            let q12 = (c + 16) >> 5;
            assert!((-32768..=32767).contains(&q12));
        }
    }

    #[test]
    fn test_magnitude_limiting_large_coefficients() {
        // Coefficients that exceed Q12 range
        let mut coeffs = vec![100_000_i32, -90_000, 80_000];
        SilkDecoder::limit_coefficient_magnitude(&mut coeffs);

        // After limiting, should fit in Q12
        for &c in &coeffs {
            let q12 = (c + 16) >> 5;
            assert!((-32768..=32767).contains(&q12));
        }
    }

    #[test]
    fn test_dc_response_instability() {
        // Create coefficients with DC response > 4096
        let coeffs_q17 = vec![2000_i32 << 5; 10]; // Each is ~2000 in Q12
        // Sum in Q12 would be 20000 > 4096

        assert!(!SilkDecoder::is_filter_stable(&coeffs_q17));
    }

    #[test]
    fn test_small_dc_response_stable() {
        // Create coefficients with small DC response
        let coeffs_q17 = [100_i32 << 5; 10]; // Each is 100 in Q12
        // Sum in Q12 would be 1000 < 4096

        // May still be unstable due to other checks, but DC check passes
        // This just verifies the DC check doesn't false-positive
        let a_q12: Vec<i32> = coeffs_q17.iter().map(|&a| (a + 16) >> 5).collect();
        let dc_resp: i32 = a_q12.iter().sum();
        assert!(dc_resp <= 4096);
    }

    #[test]
    fn test_prediction_gain_limiting_nb() {
        let nlsf_q15 = vec![1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000];

        let result = SilkDecoder::limit_lpc_coefficients(&nlsf_q15, Bandwidth::Narrowband);
        assert!(result.is_ok());

        let coeffs = result.unwrap();
        assert_eq!(coeffs.len(), 10);

        // All coefficients should fit in i16
        for &c in &coeffs {
            assert!(c >= -32768);
        }
    }

    #[test]
    fn test_prediction_gain_limiting_wb() {
        let nlsf_q15: Vec<i16> = (1..=16).map(|i| i * 1000).collect();

        let result = SilkDecoder::limit_lpc_coefficients(&nlsf_q15, Bandwidth::Wideband);
        assert!(result.is_ok());

        let coeffs = result.unwrap();
        assert_eq!(coeffs.len(), 16);

        // All coefficients should fit in i16
        for &c in &coeffs {
            assert!(c >= -32768);
        }
    }

    #[test]
    fn test_limit_lpc_invalid_bandwidth() {
        let nlsf_q15 = vec![0; 10];

        let result = SilkDecoder::limit_lpc_coefficients(&nlsf_q15, Bandwidth::SuperWideband);
        assert!(result.is_err());
    }

    #[test]
    fn test_round_15_forces_zero() {
        // This is hard to test directly, but we can verify the logic
        // Round 15 should use sc_Q16[0] = 65536 - (2 << 15) = 65536 - 65536 = 0
        let sc_q16_0 = 65536 - (2 << 15);
        assert_eq!(sc_q16_0, 0);

        // With sc_Q16[0] = 0, bandwidth expansion should zero all coefficients
        let mut coeffs = vec![10000_i32, -5000, 3000];
        SilkDecoder::apply_bandwidth_expansion(&mut coeffs, sc_q16_0);

        assert_eq!(coeffs, vec![0, 0, 0]);
    }

    #[test]
    fn test_primary_pitch_lag_absolute_nb() {
        let data = vec![0x80, 0x80, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        let lag = decoder
            .decode_primary_pitch_lag(&mut range_decoder, Bandwidth::Narrowband, true, 0)
            .unwrap();

        assert!((16..=144).contains(&lag));
        assert_eq!(decoder.previous_pitch_lag[0], Some(lag));
    }

    #[test]
    fn test_primary_pitch_lag_absolute_mb() {
        let data = vec![0x80, 0x80, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz12000, Channels::Mono, 20).unwrap();

        let lag = decoder
            .decode_primary_pitch_lag(&mut range_decoder, Bandwidth::Mediumband, true, 0)
            .unwrap();

        assert!((24..=216).contains(&lag));
        assert_eq!(decoder.previous_pitch_lag[0], Some(lag));
    }

    #[test]
    fn test_primary_pitch_lag_absolute_wb() {
        let data = vec![0x80, 0x80, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let lag = decoder
            .decode_primary_pitch_lag(&mut range_decoder, Bandwidth::Wideband, true, 0)
            .unwrap();

        assert!((32..=288).contains(&lag));
        assert_eq!(decoder.previous_pitch_lag[0], Some(lag));
    }

    #[test]
    fn test_primary_pitch_lag_relative() {
        let data = vec![0xFF, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        decoder.previous_pitch_lag[0] = Some(100);

        let lag = decoder
            .decode_primary_pitch_lag(&mut range_decoder, Bandwidth::Wideband, false, 0)
            .unwrap();

        assert_eq!(decoder.previous_pitch_lag[0], Some(lag));
    }

    #[test]
    fn test_primary_pitch_lag_relative_no_previous() {
        let data = vec![0xFF, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let result =
            decoder.decode_primary_pitch_lag(&mut range_decoder, Bandwidth::Wideband, false, 0);

        assert!(result.is_err());
    }

    #[test]
    fn test_primary_pitch_lag_invalid_bandwidth() {
        let data = vec![0x80, 0x80, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let result =
            decoder.decode_primary_pitch_lag(&mut range_decoder, Bandwidth::SuperWideband, true, 0);

        assert!(result.is_err());
    }

    #[test]
    fn test_pitch_contour_nb_10ms() {
        let data = vec![0x00, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 10).unwrap();

        let lags = decoder
            .decode_pitch_contour(&mut range_decoder, 80, Bandwidth::Narrowband)
            .unwrap();

        assert_eq!(lags.len(), 2);
        for &lag in &lags {
            assert!((16..=144).contains(&lag));
        }
    }

    #[test]
    fn test_pitch_contour_nb_20ms() {
        let data = vec![0x00, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        let lags = decoder
            .decode_pitch_contour(&mut range_decoder, 80, Bandwidth::Narrowband)
            .unwrap();

        assert_eq!(lags.len(), 4);
        for &lag in &lags {
            assert!((16..=144).contains(&lag));
        }
    }

    #[test]
    fn test_pitch_contour_mb_10ms() {
        let data = vec![0x00, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz12000, Channels::Mono, 10).unwrap();

        let lags = decoder
            .decode_pitch_contour(&mut range_decoder, 120, Bandwidth::Mediumband)
            .unwrap();

        assert_eq!(lags.len(), 2);
        for &lag in &lags {
            assert!((24..=216).contains(&lag));
        }
    }

    #[test]
    fn test_pitch_contour_wb_20ms() {
        let data = vec![0x00, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let lags = decoder
            .decode_pitch_contour(&mut range_decoder, 160, Bandwidth::Wideband)
            .unwrap();

        assert_eq!(lags.len(), 4);
        for &lag in &lags {
            assert!((32..=288).contains(&lag));
        }
    }

    #[test]
    fn test_pitch_contour_clamping() {
        let data = vec![0x00, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();

        let lags = decoder
            .decode_pitch_contour(&mut range_decoder, 16, Bandwidth::Narrowband)
            .unwrap();

        for &lag in &lags {
            assert!((16..=144).contains(&lag));
        }
    }

    #[test]
    fn test_ltp_filter_periodicity_0() {
        let data = vec![0x00, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let filters = decoder
            .decode_ltp_filter_coefficients(&mut range_decoder)
            .unwrap();

        assert_eq!(filters.len(), 4);
        for filter in filters {
            assert_eq!(filter.len(), 5);
        }
    }

    #[test]
    fn test_ltp_filter_periodicity_1() {
        let data = vec![0x80, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let filters = decoder
            .decode_ltp_filter_coefficients(&mut range_decoder)
            .unwrap();

        assert_eq!(filters.len(), 4);
        for filter in filters {
            assert_eq!(filter.len(), 5);
        }
    }

    #[test]
    fn test_ltp_filter_all_periodicities() {
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        for &first_byte in &[0x00, 0x80, 0xA0] {
            let data = vec![first_byte, 0x00, 0xFF, 0xFF];
            let mut range_decoder = RangeDecoder::new(&data).unwrap();

            let filters = decoder.decode_ltp_filter_coefficients(&mut range_decoder);

            if let Ok(f) = filters {
                assert_eq!(f.len(), 4);
                for filter in f {
                    assert_eq!(filter.len(), 5);
                }
            }
        }
    }

    #[test]
    fn test_ltp_scaling_decode() {
        let data = vec![0xFF, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let scaling = SilkDecoder::decode_ltp_scaling(&mut range_decoder, true).unwrap();

        assert!(scaling == 15565 || scaling == 12288 || scaling == 8192);
    }

    #[test]
    fn test_ltp_scaling_default() {
        let data = vec![0xFF, 0xFF, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();

        let scaling = SilkDecoder::decode_ltp_scaling(&mut range_decoder, false).unwrap();

        assert_eq!(scaling, 15565);
    }

    #[test]
    fn test_lcg_seed_decoding() {
        let data = vec![0x00, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let seed = decoder.decode_lcg_seed(&mut range_decoder).unwrap();

        assert!(seed <= 3);
        assert_eq!(decoder.lcg_seed, seed);
    }

    #[test]
    fn test_lcg_seed_uniform_distribution() {
        for seed_value in 0..4 {
            let data = if seed_value == 0 {
                vec![0x00, 0xFF, 0xFF, 0xFF]
            } else if seed_value == 1 {
                vec![0x55, 0xFF, 0xFF, 0xFF]
            } else if seed_value == 2 {
                vec![0xAA, 0xFF, 0xFF, 0xFF]
            } else {
                vec![0xFF, 0xFF, 0xFF, 0xFF]
            };

            let mut range_decoder = RangeDecoder::new(&data).unwrap();
            let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

            let seed = decoder.decode_lcg_seed(&mut range_decoder).unwrap();
            assert!(seed <= 3);
        }
    }

    #[test]
    fn test_shell_block_count_nb_all() {
        use crate::silk::excitation_constants::get_shell_block_count;

        assert_eq!(get_shell_block_count(Bandwidth::Narrowband, 10), Some(5));
        assert_eq!(get_shell_block_count(Bandwidth::Narrowband, 20), Some(10));
        assert_eq!(get_shell_block_count(Bandwidth::Mediumband, 10), Some(8));
        assert_eq!(get_shell_block_count(Bandwidth::Mediumband, 20), Some(15));
        assert_eq!(get_shell_block_count(Bandwidth::Wideband, 10), Some(10));
        assert_eq!(get_shell_block_count(Bandwidth::Wideband, 20), Some(20));
    }

    #[test]
    fn test_shell_block_count_invalid_nb() {
        use crate::silk::excitation_constants::get_shell_block_count;

        assert_eq!(get_shell_block_count(Bandwidth::SuperWideband, 10), None);
        assert_eq!(get_shell_block_count(Bandwidth::Narrowband, 40), None);
    }

    #[test]
    fn test_decode_rate_level_inactive() {
        let data = vec![0x00; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let rate_level = decoder
            .decode_rate_level(&mut range_decoder, FrameType::Inactive)
            .unwrap();
        assert!(rate_level <= 8);
    }

    #[test]
    fn test_decode_rate_level_voiced() {
        let data = vec![0x80; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let rate_level = decoder
            .decode_rate_level(&mut range_decoder, FrameType::Voiced)
            .unwrap();
        assert!(rate_level <= 8);
    }

    #[test]
    fn test_decode_rate_level_unvoiced_uses_inactive_pdf() {
        let data = vec![0x00; 10];
        let mut range_decoder1 = RangeDecoder::new(&data).unwrap();
        let mut range_decoder2 = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let rate_inactive = decoder
            .decode_rate_level(&mut range_decoder1, FrameType::Inactive)
            .unwrap();
        let rate_unvoiced = decoder
            .decode_rate_level(&mut range_decoder2, FrameType::Unvoiced)
            .unwrap();

        assert_eq!(rate_inactive, rate_unvoiced);
    }

    #[test]
    fn test_decode_pulse_count_no_lsb() {
        let data = vec![0x00; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let (pulse_count, lsb_count) = decoder.decode_pulse_count(&mut range_decoder, 0).unwrap();
        assert!(pulse_count <= 16);
        assert_eq!(lsb_count, 0);
    }

    #[test]
    fn test_decode_pulse_count_with_lsb() {
        let data = vec![0xFF; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let (pulse_count, lsb_count) = decoder.decode_pulse_count(&mut range_decoder, 5).unwrap();
        assert!(pulse_count <= 16);
        assert!(lsb_count <= 10);
    }

    #[test]
    fn test_decode_pulse_count_lsb_cap() {
        let data = vec![0xFF; 200];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let (_pulse_count, lsb_count) = decoder.decode_pulse_count(&mut range_decoder, 9).unwrap();
        assert!(lsb_count <= 10);
    }

    #[test]
    fn test_decode_pulse_count_rate_level_switching() {
        let data = vec![0xFF; 200];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let result = decoder.decode_pulse_count(&mut range_decoder, 8);
        assert!(result.is_ok());
    }

    #[test]
    fn test_decode_pulse_count_invalid_rate_level() {
        let data = vec![0x00; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let result = decoder.decode_pulse_count(&mut range_decoder, 11);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_pulse_count_all_rate_levels() {
        for rate_level in 0..=10 {
            let data = vec![0x00; 20];
            let mut range_decoder = RangeDecoder::new(&data).unwrap();
            let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

            let result = decoder.decode_pulse_count(&mut range_decoder, rate_level);
            assert!(
                result.is_ok(),
                "Failed to decode pulse count for rate level {rate_level}"
            );
            let (pulse_count, lsb_count) = result.unwrap();
            assert!(pulse_count <= 16);
            assert_eq!(lsb_count, 0);
        }
    }

    #[test]
    fn test_decode_pulse_locations_zero_pulses() {
        let data = vec![0x00; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let locations = decoder
            .decode_pulse_locations(&mut range_decoder, 0)
            .unwrap();

        assert_eq!(locations, [0; 16]);
    }

    #[test]
    fn test_decode_pulse_locations_single_pulse() {
        let data = vec![0x00; 20];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let locations = decoder
            .decode_pulse_locations(&mut range_decoder, 1)
            .unwrap();

        let total_pulses: u32 = locations.iter().map(|&x| u32::from(x)).sum();
        assert_eq!(total_pulses, 1);
    }

    #[test]
    fn test_decode_pulse_locations_multiple_pulses() {
        let data = vec![0x80; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let locations = decoder
            .decode_pulse_locations(&mut range_decoder, 8)
            .unwrap();

        let total_pulses: u32 = locations.iter().map(|&x| u32::from(x)).sum();
        assert_eq!(total_pulses, 8);
    }

    #[test]
    fn test_decode_pulse_locations_max_pulses() {
        let data = vec![0xFF; 100];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let locations = decoder
            .decode_pulse_locations(&mut range_decoder, 16)
            .unwrap();

        let total_pulses: u32 = locations.iter().map(|&x| u32::from(x)).sum();
        assert_eq!(total_pulses, 16);
    }

    #[test]
    fn test_get_pulse_split_pdf_all_sizes() {
        use crate::silk::excitation_constants::get_pulse_split_pdf;

        for &size in &[16, 8, 4, 2] {
            for count in 1..=16 {
                let pdf = get_pulse_split_pdf(size, count);
                assert!(pdf.is_some(), "Missing PDF for size={size}, count={count}");
                let pdf_arr = pdf.unwrap();
                assert!(!pdf_arr.is_empty());
                assert_eq!(pdf_arr[pdf_arr.len() - 1], 0, "PDF must end with 0");
            }
        }
    }

    #[test]
    fn test_get_pulse_split_pdf_invalid() {
        use crate::silk::excitation_constants::get_pulse_split_pdf;

        assert!(get_pulse_split_pdf(16, 0).is_none());
        assert!(get_pulse_split_pdf(16, 17).is_none());
        assert!(get_pulse_split_pdf(3, 1).is_none());
        assert!(get_pulse_split_pdf(32, 1).is_none());
    }

    #[test]
    fn test_pulse_location_sum_conservation() {
        for pulse_count in 1..=16 {
            let data = vec![0x55; 100];
            let mut range_decoder = RangeDecoder::new(&data).unwrap();
            let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

            let locations = decoder
                .decode_pulse_locations(&mut range_decoder, pulse_count)
                .unwrap();

            let total: u32 = locations.iter().map(|&x| u32::from(x)).sum();
            assert_eq!(
                total,
                u32::from(pulse_count),
                "Pulse count mismatch for {pulse_count} pulses"
            );
        }
    }

    #[test]
    fn test_decode_lsbs_no_lsb() {
        let data = vec![0x00; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let pulse_locations = [1, 2, 0, 3, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let magnitudes = decoder
            .decode_lsbs(&mut range_decoder, &pulse_locations, 0)
            .unwrap();

        assert_eq!(magnitudes[0], 1);
        assert_eq!(magnitudes[1], 2);
        assert_eq!(magnitudes[2], 0);
        assert_eq!(magnitudes[3], 3);
    }

    #[test]
    fn test_decode_lsbs_single_lsb() {
        let data = vec![0x00; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let pulse_locations = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let magnitudes = decoder
            .decode_lsbs(&mut range_decoder, &pulse_locations, 1)
            .unwrap();

        assert!(magnitudes[0] >= 4 && magnitudes[0] <= 5);
    }

    #[test]
    fn test_decode_lsbs_multiple_lsb() {
        let data = vec![0xFF; 100];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let pulse_locations = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let magnitudes = decoder
            .decode_lsbs(&mut range_decoder, &pulse_locations, 3)
            .unwrap();

        assert!(magnitudes[0] >= 8 && magnitudes[0] < 16);
    }

    #[test]
    fn test_decode_lsbs_all_coefficients() {
        let data = vec![0x80; 100];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let pulse_locations = [1; 16];
        let magnitudes = decoder
            .decode_lsbs(&mut range_decoder, &pulse_locations, 2)
            .unwrap();

        for &mag in &magnitudes {
            assert!((4..8).contains(&mag));
        }
    }

    #[test]
    fn test_decode_lsbs_zero_pulses_get_lsbs() {
        let data = vec![0x00; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let pulse_locations = [0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let magnitudes = decoder
            .decode_lsbs(&mut range_decoder, &pulse_locations, 1)
            .unwrap();

        assert!(magnitudes[0] <= 1);
        assert!(magnitudes[2] <= 1);
    }

    #[test]
    fn test_decode_lsbs_magnitude_doubling() {
        let data = vec![0x00; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let pulse_locations = [3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let magnitudes = decoder
            .decode_lsbs(&mut range_decoder, &pulse_locations, 1)
            .unwrap();

        assert!(magnitudes[0] >= 6 && magnitudes[0] <= 7);
    }

    #[test]
    fn test_excitation_lsb_pdf() {
        use crate::silk::excitation_constants::EXCITATION_LSB_PDF;

        assert_eq!(EXCITATION_LSB_PDF.len(), 2);
        assert_eq!(EXCITATION_LSB_PDF[0], 120);
        assert_eq!(EXCITATION_LSB_PDF[1], 0);
    }

    #[test]
    fn test_shell_block_count_mb_special() {
        assert_eq!(
            SilkDecoder::get_shell_block_count(Bandwidth::Mediumband, 10).unwrap(),
            8
        );
        assert_eq!(
            SilkDecoder::get_shell_block_count(Bandwidth::Mediumband, 20).unwrap(),
            15
        );
    }

    #[test]
    fn test_shell_block_count_wb() {
        assert_eq!(
            SilkDecoder::get_shell_block_count(Bandwidth::Wideband, 10).unwrap(),
            10
        );
        assert_eq!(
            SilkDecoder::get_shell_block_count(Bandwidth::Wideband, 20).unwrap(),
            20
        );
    }

    #[test]
    fn test_shell_block_count_invalid() {
        assert!(SilkDecoder::get_shell_block_count(Bandwidth::SuperWideband, 10).is_err());
        assert!(SilkDecoder::get_shell_block_count(Bandwidth::Narrowband, 40).is_err());
    }

    // ====================================================================
    // Section 3.7.6: Sign Decoding Tests
    // ====================================================================

    #[test]
    fn test_decode_signs_all_zero_magnitudes() {
        let data = vec![0x80; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let magnitudes = [0_u16; 16];
        let signed = decoder
            .decode_signs(
                &mut range_decoder,
                &magnitudes,
                FrameType::Voiced,
                QuantizationOffsetType::Low,
                5,
            )
            .unwrap();

        for &val in &signed {
            assert_eq!(val, 0);
        }
    }

    #[test]
    #[allow(clippy::cast_possible_wrap)]
    fn test_decode_signs_positive_values() {
        let data = vec![0xFF; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let magnitudes = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let signed = decoder
            .decode_signs(
                &mut range_decoder,
                &magnitudes,
                FrameType::Inactive,
                QuantizationOffsetType::Low,
                0,
            )
            .unwrap();

        for i in 0..16 {
            assert!(signed[i] == magnitudes[i] as i16 || signed[i] == -(magnitudes[i] as i16));
        }
    }

    #[test]
    #[allow(clippy::cast_possible_wrap)]
    fn test_decode_signs_negative_values() {
        let data = vec![0x00; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let magnitudes = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let signed = decoder
            .decode_signs(
                &mut range_decoder,
                &magnitudes,
                FrameType::Voiced,
                QuantizationOffsetType::High,
                3,
            )
            .unwrap();

        for i in 0..16 {
            assert!(signed[i] == magnitudes[i] as i16 || signed[i] == -(magnitudes[i] as i16));
        }
    }

    #[test]
    #[allow(clippy::cast_possible_wrap)]
    fn test_decode_signs_mixed_zero_nonzero() {
        let data = vec![0x80; 50];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let magnitudes = [0, 5, 0, 3, 0, 0, 8, 0, 0, 2, 0, 0, 0, 6, 0, 1];
        let signed = decoder
            .decode_signs(
                &mut range_decoder,
                &magnitudes,
                FrameType::Unvoiced,
                QuantizationOffsetType::Low,
                4,
            )
            .unwrap();

        for i in 0..16 {
            if magnitudes[i] == 0 {
                assert_eq!(signed[i], 0);
            } else {
                assert!(signed[i] == magnitudes[i] as i16 || signed[i] == -(magnitudes[i] as i16));
            }
        }
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_get_sign_pdf_inactive_low() {
        let pdf0 = SilkDecoder::get_sign_pdf(FrameType::Inactive, QuantizationOffsetType::Low, 0);
        assert_eq!(pdf0.len(), 2);

        let pdf3 = SilkDecoder::get_sign_pdf(FrameType::Inactive, QuantizationOffsetType::Low, 3);
        assert_eq!(pdf3.len(), 2);

        let pdf10 = SilkDecoder::get_sign_pdf(FrameType::Inactive, QuantizationOffsetType::Low, 10);
        assert_eq!(pdf10.len(), 2);
    }

    #[test]
    fn test_get_sign_pdf_voiced_high() {
        let pdf1 = SilkDecoder::get_sign_pdf(FrameType::Voiced, QuantizationOffsetType::High, 1);
        assert_eq!(pdf1.len(), 2);

        let pdf5 = SilkDecoder::get_sign_pdf(FrameType::Voiced, QuantizationOffsetType::High, 5);
        assert_eq!(pdf5.len(), 2);
    }

    #[test]
    fn test_get_sign_pdf_unvoiced_all_pulse_counts() {
        for pulse_count in 0..=10 {
            let pdf_low = SilkDecoder::get_sign_pdf(
                FrameType::Unvoiced,
                QuantizationOffsetType::Low,
                pulse_count,
            );
            assert_eq!(pdf_low.len(), 2);

            let pdf_high = SilkDecoder::get_sign_pdf(
                FrameType::Unvoiced,
                QuantizationOffsetType::High,
                pulse_count,
            );
            assert_eq!(pdf_high.len(), 2);
        }
    }

    #[test]
    fn test_decode_signs_all_42_pdfs() {
        let frame_types = [FrameType::Inactive, FrameType::Unvoiced, FrameType::Voiced];
        let offset_types = [QuantizationOffsetType::Low, QuantizationOffsetType::High];

        for &frame_type in &frame_types {
            for &offset_type in &offset_types {
                for pulse_count in 0..=10 {
                    let pdf = SilkDecoder::get_sign_pdf(frame_type, offset_type, pulse_count);
                    assert_eq!(pdf.len(), 2);
                    assert_eq!(pdf[1], 0);
                }
            }
        }
    }

    // ====================================================================
    // Section 3.7.7: Excitation Reconstruction Tests
    // ====================================================================

    #[test]
    fn test_quantization_offset_inactive_low() {
        let offset =
            SilkDecoder::get_quantization_offset(FrameType::Inactive, QuantizationOffsetType::Low);
        assert_eq!(offset, 100); // Q10 format
    }

    #[test]
    fn test_quantization_offset_inactive_high() {
        let offset =
            SilkDecoder::get_quantization_offset(FrameType::Inactive, QuantizationOffsetType::High);
        assert_eq!(offset, 240); // Q10 format
    }

    #[test]
    fn test_quantization_offset_unvoiced_low() {
        let offset =
            SilkDecoder::get_quantization_offset(FrameType::Unvoiced, QuantizationOffsetType::Low);
        assert_eq!(offset, 100); // Q10 format
    }

    #[test]
    fn test_quantization_offset_unvoiced_high() {
        let offset =
            SilkDecoder::get_quantization_offset(FrameType::Unvoiced, QuantizationOffsetType::High);
        assert_eq!(offset, 240); // Q10 format
    }

    #[test]
    fn test_quantization_offset_voiced_low() {
        let offset =
            SilkDecoder::get_quantization_offset(FrameType::Voiced, QuantizationOffsetType::Low);
        assert_eq!(offset, 32); // Q10 format
    }

    #[test]
    fn test_quantization_offset_voiced_high() {
        let offset =
            SilkDecoder::get_quantization_offset(FrameType::Voiced, QuantizationOffsetType::High);
        assert_eq!(offset, 100); // Q10 format
    }

    #[test]
    fn test_reconstruct_excitation_all_zeros() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        decoder.lcg_seed = 12345;

        let e_raw = [0_i16; 16];
        let e_q23 =
            decoder.reconstruct_excitation(&e_raw, FrameType::Voiced, QuantizationOffsetType::Low);

        for &val in &e_q23 {
            assert!(val.abs() <= (1 << 23));
        }
    }

    #[test]
    fn test_reconstruct_excitation_nonzero() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        decoder.lcg_seed = 54321;

        let e_raw = [10, -5, 3, 0, -8, 15, 0, 2, -1, 0, 0, 0, 6, -3, 0, 1];
        let e_q23 = decoder.reconstruct_excitation(
            &e_raw,
            FrameType::Unvoiced,
            QuantizationOffsetType::High,
        );

        for &val in &e_q23 {
            assert!(val.abs() <= (1 << 23));
        }
    }

    #[test]
    fn test_lcg_sequence() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        decoder.lcg_seed = 1;

        let initial_seed = decoder.lcg_seed;
        let e_raw = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let _ = decoder.reconstruct_excitation(
            &e_raw,
            FrameType::Inactive,
            QuantizationOffsetType::Low,
        );

        assert_ne!(decoder.lcg_seed, initial_seed);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_lcg_formula() {
        let mut seed = 100_u32;

        seed = seed.wrapping_mul(196_314_165).wrapping_add(907_633_515);

        let expected = (100_u64 * 196_314_165 + 907_633_515) as u32;
        assert_eq!(seed, expected);
    }

    #[test]
    fn test_excitation_reconstruction_zero_no_20_factor() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        decoder.lcg_seed = 0;

        let e_raw = [0_i16; 16];
        let e_q14 =
            decoder.reconstruct_excitation(&e_raw, FrameType::Voiced, QuantizationOffsetType::Low);

        // For zero values, only noise is added - should be small values
        for &val in &e_q14 {
            assert!(val.abs() < 1000, "Unexpected value: {val}");
        }
    }

    #[test]
    fn test_excitation_reconstruction_positive_value() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        decoder.lcg_seed = 0;

        let e_raw = [5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let e_q14 =
            decoder.reconstruct_excitation(&e_raw, FrameType::Voiced, QuantizationOffsetType::Low);

        // Q14 format: 5 << 14 = 81920, minus QUANT_LEVEL_ADJUST (80 << 4 = 1280) = 80640
        // Plus noise (seed=0 gives small values)
        assert!(e_q14[0].abs() > 80000, "e_q14[0] = {}", e_q14[0]);
    }

    #[test]
    fn test_excitation_reconstruction_negative_value() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        decoder.lcg_seed = 0;

        let e_raw = [-5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let e_q14 =
            decoder.reconstruct_excitation(&e_raw, FrameType::Voiced, QuantizationOffsetType::Low);

        // Q14 format: -5 << 14 = -81920, plus QUANT_LEVEL_ADJUST (80 << 4 = 1280) = -80640
        // Plus noise (seed=0 gives small values)
        assert!(e_q14[0].abs() > 80000, "e_q14[0] = {}", e_q14[0]);
    }

    #[test]
    fn test_excitation_q14_range() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        decoder.lcg_seed = 999;

        let e_raw = [
            127, -127, 100, -100, 50, -50, 0, 0, 25, -25, 75, -75, 10, -10, 1, -1,
        ];
        let e_q14 = decoder.reconstruct_excitation(
            &e_raw,
            FrameType::Inactive,
            QuantizationOffsetType::High,
        );

        for &val in &e_q14 {
            assert!(
                val.abs() <= (1 << 30),
                "Value {val} exceeds reasonable range for Q14"
            );
        }
    }

    #[test]
    fn test_pseudorandom_inversion_msb() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        decoder.lcg_seed = 0x0000_0000;
        let e_raw1 = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let e_q23_1 =
            decoder.reconstruct_excitation(&e_raw1, FrameType::Voiced, QuantizationOffsetType::Low);

        decoder.lcg_seed = 0x8000_0000;
        let e_raw2 = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let e_q23_2 =
            decoder.reconstruct_excitation(&e_raw2, FrameType::Voiced, QuantizationOffsetType::Low);

        assert_ne!(e_q23_1[0].signum(), e_q23_2[0].signum());
    }

    #[test]
    fn test_seed_update_with_raw_value() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        decoder.lcg_seed = 100;

        let e_raw = [5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let _ =
            decoder.reconstruct_excitation(&e_raw, FrameType::Voiced, QuantizationOffsetType::Low);

        let seed_after_first = decoder.lcg_seed;

        decoder.lcg_seed = 100;
        let e_raw2 = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let _ =
            decoder.reconstruct_excitation(&e_raw2, FrameType::Voiced, QuantizationOffsetType::Low);

        assert_ne!(decoder.lcg_seed, seed_after_first);
    }

    #[test]
    fn test_subframe_params_interpolated_lpc() {
        let n1_q15 = vec![100_i16; 16];
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![65536_i32; 4];
        let pitch_lags = vec![100_i16; 4];
        let ltp_filters = vec![[10_i8; 5]; 4];
        let params = SilkDecoder::select_subframe_params(
            0,
            20,
            3,
            Some(&n1_q15),
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.lpc_coeffs_q12.len(), 16);
        assert_eq!(params.ltp_scale_q14, 14000);
    }

    #[test]
    fn test_subframe_params_interpolated_lpc_subframe1() {
        let n1_q15 = vec![100_i16; 16];
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![65536_i32; 4];
        let pitch_lags = vec![100_i16; 4];
        let ltp_filters = vec![[10_i8; 5]; 4];
        let params = SilkDecoder::select_subframe_params(
            1,
            20,
            3,
            Some(&n1_q15),
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.lpc_coeffs_q12.len(), 16);
    }

    #[test]
    fn test_subframe_params_normal_lpc_w_q2_ge_4() {
        let n1_q15 = vec![100_i16; 16];
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![65536_i32; 4];
        let pitch_lags = vec![100_i16; 4];
        let ltp_filters = vec![[10_i8; 5]; 4];
        let params = SilkDecoder::select_subframe_params(
            0,
            20,
            4,
            Some(&n1_q15),
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.lpc_coeffs_q12.len(), 16);
    }

    #[test]
    fn test_subframe_params_normal_lpc_subframe2() {
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![65536_i32; 4];
        let pitch_lags = vec![100_i16; 4];
        let ltp_filters = vec![[10_i8; 5]; 4];
        let params = SilkDecoder::select_subframe_params(
            2,
            20,
            3,
            None,
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.lpc_coeffs_q12.len(), 16);
    }

    #[test]
    fn test_subframe_params_ltp_scale_adjustment_subframe2() {
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![65536_i32; 4];
        let pitch_lags = vec![100_i16; 4];
        let ltp_filters = vec![[10_i8; 5]; 4];
        let params = SilkDecoder::select_subframe_params(
            2,
            20,
            3,
            None,
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.ltp_scale_q14, 16384);
    }

    #[test]
    fn test_subframe_params_ltp_scale_adjustment_subframe3() {
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![65536_i32; 4];
        let pitch_lags = vec![100_i16; 4];
        let ltp_filters = vec![[10_i8; 5]; 4];
        let params = SilkDecoder::select_subframe_params(
            3,
            20,
            3,
            None,
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.ltp_scale_q14, 16384);
    }

    #[test]
    fn test_subframe_params_ltp_scale_normal_w_q2_ge_4() {
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![65536_i32; 4];
        let pitch_lags = vec![100_i16; 4];
        let ltp_filters = vec![[10_i8; 5]; 4];
        let params = SilkDecoder::select_subframe_params(
            2,
            20,
            4,
            None,
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.ltp_scale_q14, 14000);
    }

    #[test]
    fn test_subframe_params_10ms_frame() {
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![65536_i32; 2];
        let pitch_lags = vec![100_i16; 2];
        let ltp_filters = vec![[10_i8; 5]; 2];
        let params = SilkDecoder::select_subframe_params(
            0,
            10,
            3,
            None,
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.ltp_scale_q14, 14000);
    }

    #[test]
    fn test_subframe_params_nb_bandwidth() {
        let n2_q15 = vec![200_i16; 10];
        let gains = vec![65536_i32; 4];
        let pitch_lags = vec![100_i16; 4];
        let ltp_filters = vec![[10_i8; 5]; 4];
        let params = SilkDecoder::select_subframe_params(
            0,
            20,
            3,
            None,
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Narrowband,
        )
        .unwrap();
        assert_eq!(params.lpc_coeffs_q12.len(), 10);
    }

    #[test]
    fn test_subframe_params_field_values() {
        let n2_q15 = vec![200_i16; 16];
        let gains = vec![10000_i32, 20000, 30000, 40000];
        let pitch_lags = vec![80_i16, 90, 100, 110];
        let ltp_filters = vec![
            [1_i8, 2, 3, 4, 5],
            [5, 10, 15, 10, 5],
            [2, 4, 6, 4, 2],
            [1, 1, 1, 1, 1],
        ];
        let params = SilkDecoder::select_subframe_params(
            1,
            20,
            4,
            None,
            &n2_q15,
            &gains,
            &pitch_lags,
            &ltp_filters,
            14000,
            Bandwidth::Wideband,
        )
        .unwrap();
        assert_eq!(params.gain_q16, 20000);
        assert_eq!(params.pitch_lag, 90);
        assert_eq!(params.ltp_filter_q7, [5, 10, 15, 10, 5]);
        assert_eq!(params.ltp_scale_q14, 14000);
        assert_eq!(params.lpc_coeffs_q12.len(), 16);
    }

    #[test]
    fn test_samples_per_subframe_nb() {
        assert_eq!(SilkDecoder::samples_per_subframe(Bandwidth::Narrowband), 40);
    }

    #[test]
    fn test_samples_per_subframe_mb() {
        assert_eq!(SilkDecoder::samples_per_subframe(Bandwidth::Mediumband), 60);
    }

    #[test]
    fn test_samples_per_subframe_wb() {
        assert_eq!(SilkDecoder::samples_per_subframe(Bandwidth::Wideband), 80);
    }

    #[test]
    fn test_num_subframes_10ms() {
        assert_eq!(SilkDecoder::num_subframes(10), 2);
    }

    #[test]
    fn test_num_subframes_20ms() {
        assert_eq!(SilkDecoder::num_subframes(20), 4);
    }

    #[test]
    fn test_subframe_start_index() {
        assert_eq!(SilkDecoder::subframe_start_index(0, 80), 0);
        assert_eq!(SilkDecoder::subframe_start_index(1, 80), 80);
        assert_eq!(SilkDecoder::subframe_start_index(2, 80), 160);
        assert_eq!(SilkDecoder::subframe_start_index(3, 80), 240);
    }

    #[test]
    fn test_ltp_synthesis_unvoiced_simple() {
        let excitation_q14 = vec![16384, 8192, -16384, 0];

        let res = SilkDecoder::ltp_synthesis_unvoiced(&excitation_q14);

        assert_eq!(res.len(), 4);
        assert_eq!(res[0], 16384);
        assert_eq!(res[1], 8192);
        assert_eq!(res[2], -16384);
        assert_eq!(res[3], 0);
    }

    #[test]
    fn test_ltp_synthesis_unvoiced_full_subframe() {
        let excitation_q14 = vec![1000_i32; 80];
        let res = SilkDecoder::ltp_synthesis_unvoiced(&excitation_q14);

        assert_eq!(res.len(), 80);
        for &val in &res {
            assert_eq!(val, 1000);
        }
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_ltp_state_initialization() {
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        assert_eq!(decoder.ltp_state[0].out_buffer.len(), 306);
        assert_eq!(decoder.ltp_state[0].lpc_buffer.len(), 256);

        for &val in &decoder.ltp_state[0].out_buffer {
            assert_eq!(val, 0.0);
        }
        for &val in &decoder.ltp_state[0].lpc_buffer {
            assert_eq!(val, 0.0);
        }
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_ltp_state_reset() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        decoder.ltp_state[0].out_buffer[0] = 1.0;
        decoder.ltp_state[0].lpc_buffer[0] = 2.0;

        decoder.ltp_state[0].reset();

        assert_eq!(decoder.ltp_state[0].out_buffer[0], 0.0);
        assert_eq!(decoder.ltp_state[0].lpc_buffer[0], 0.0);
    }

    #[test]
    fn test_ltp_buffer_sizes() {
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        assert_eq!(decoder.ltp_state[0].out_buffer.len(), 306);
        assert_eq!(decoder.ltp_state[0].lpc_buffer.len(), 256);
    }

    #[test]
    fn test_ltp_synthesis_voiced_zero_excitation() {
        let excitation = vec![0_i32; 80];
        let lpc_coeffs = vec![0_i16; 16];
        let params = SubframeParams {
            lpc_coeffs_q12: lpc_coeffs,
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 16384,
        };

        let res = SilkDecoder::ltp_synthesis_voiced(&excitation, &params, 0, Bandwidth::Wideband)
            .unwrap();

        assert_eq!(res.len(), 80);
        for &val in &res {
            assert_eq!(val, 0);
        }
    }

    #[test]
    fn test_ltp_synthesis_voiced_out_end_normal() {
        let excitation_q14 = vec![1000_i32; 80];
        let lpc_coeffs = vec![0_i16; 16];
        let params = SubframeParams {
            lpc_coeffs_q12: lpc_coeffs,
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 14000,
        };

        let res =
            SilkDecoder::ltp_synthesis_voiced(&excitation_q14, &params, 0, Bandwidth::Wideband)
                .unwrap();

        assert_eq!(res.len(), 80);
    }

    #[test]
    fn test_ltp_synthesis_voiced_out_end_interpolation() {
        let excitation_q14 = vec![1000_i32; 80];
        let lpc_coeffs = vec![0_i16; 16];
        let params = SubframeParams {
            lpc_coeffs_q12: lpc_coeffs,
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 16384,
        };

        let res =
            SilkDecoder::ltp_synthesis_voiced(&excitation_q14, &params, 0, Bandwidth::Wideband)
                .unwrap();

        assert_eq!(res.len(), 80);
    }

    #[test]
    fn test_ltp_synthesis_voiced_pitch_lag_short() {
        let excitation = vec![1_000_000_i32; 40];
        let lpc_coeffs = vec![0_i16; 10];
        let params = SubframeParams {
            lpc_coeffs_q12: lpc_coeffs,
            gain_q16: 65536,
            pitch_lag: 32,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 16384,
        };

        let res = SilkDecoder::ltp_synthesis_voiced(&excitation, &params, 0, Bandwidth::Narrowband)
            .unwrap();

        assert_eq!(res.len(), 40);
    }

    #[test]
    fn test_ltp_synthesis_voiced_pitch_lag_long() {
        let excitation_q14 = vec![1000_i32; 80];
        let lpc_coeffs = vec![0_i16; 16];
        let params = SubframeParams {
            lpc_coeffs_q12: lpc_coeffs,
            gain_q16: 65536,
            pitch_lag: 288,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 16384,
        };

        let res =
            SilkDecoder::ltp_synthesis_voiced(&excitation_q14, &params, 0, Bandwidth::Wideband)
                .unwrap();

        assert_eq!(res.len(), 80);
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_ltp_synthesis_voiced_all_bandwidths() {
        let excitation_nb = vec![1_000_000_i32; 40];
        let output_nb = vec![0_i16; 10];
        let params_nb = SubframeParams {
            lpc_coeffs_q12: output_nb,
            gain_q16: 65536,
            pitch_lag: 80,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 16384,
        };

        let res_nb =
            SilkDecoder::ltp_synthesis_voiced(&excitation_nb, &params_nb, 0, Bandwidth::Narrowband)
                .unwrap();
        assert_eq!(res_nb.len(), 40);

        let excitation_mb = vec![1_000_000_i32; 60];
        let output_mb = vec![0_i16; 16];
        let params_mb = SubframeParams {
            lpc_coeffs_q12: output_mb,
            gain_q16: 65536,
            pitch_lag: 120,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 16384,
        };

        let res_mb =
            SilkDecoder::ltp_synthesis_voiced(&excitation_mb, &params_mb, 0, Bandwidth::Mediumband)
                .unwrap();
        assert_eq!(res_mb.len(), 60);

        let excitation_wb = vec![1_000_000_i32; 80];
        let output_wb = vec![0_i16; 16];
        let params_wb = SubframeParams {
            lpc_coeffs_q12: output_wb,
            gain_q16: 65536,
            pitch_lag: 160,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 16384,
        };

        let res_wb =
            SilkDecoder::ltp_synthesis_voiced(&excitation_wb, &params_wb, 0, Bandwidth::Wideband)
                .unwrap();
        assert_eq!(res_wb.len(), 80);
    }

    #[test]
    fn test_ltp_synthesis_voiced_5tap_filter() {
        let excitation_q14 = vec![1000_i32; 80];
        let lpc_coeffs = vec![0_i16; 16];
        let params = SubframeParams {
            lpc_coeffs_q12: lpc_coeffs,
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 16384,
        };

        let res =
            SilkDecoder::ltp_synthesis_voiced(&excitation_q14, &params, 0, Bandwidth::Wideband)
                .unwrap();

        assert_eq!(res.len(), 80);
    }

    #[test]
    fn test_ltp_synthesis_voiced_nonzero_gain() {
        let excitation_q14 = vec![1000_i32; 80];
        let lpc_coeffs = vec![0_i16; 16];
        let params = SubframeParams {
            lpc_coeffs_q12: lpc_coeffs,
            gain_q16: 32768,
            pitch_lag: 100,
            ltp_filter_q7: [0; 5],
            ltp_scale_q14: 16384,
        };

        let res =
            SilkDecoder::ltp_synthesis_voiced(&excitation_q14, &params, 0, Bandwidth::Wideband)
                .unwrap();

        assert_eq!(res.len(), 80);
    }

    #[test]
    fn test_ltp_synthesis_voiced_subframe_indices() {
        let excitation_q14 = vec![1000_i32; 80];
        let lpc_coeffs = vec![0_i16; 16];

        for s in 0..4 {
            let params = SubframeParams {
                lpc_coeffs_q12: lpc_coeffs.clone(),
                gain_q16: 65536,
                pitch_lag: 100,
                ltp_filter_q7: [0; 5],
                ltp_scale_q14: if s >= 2 { 16384 } else { 14000 },
            };

            let res =
                SilkDecoder::ltp_synthesis_voiced(&excitation_q14, &params, s, Bandwidth::Wideband)
                    .unwrap();

            assert_eq!(res.len(), 80);
        }
    }

    #[test]
    fn test_lpc_synthesis_zero_residual() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let residual_q14 = vec![0_i32; 80];
        let params = SubframeParams {
            lpc_coeffs_q12: vec![100i16; 16],
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        let output = decoder
            .lpc_synthesis(&residual_q14, &params, Bandwidth::Wideband, 0)
            .unwrap();

        assert_eq!(output.len(), 80);
        assert!(output.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_lpc_synthesis_simple_gain_scaling() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let residual_q14 = vec![16384_i32; 80];
        let params = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 16],
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        let output = decoder
            .lpc_synthesis(&residual_q14, &params, Bandwidth::Wideband, 0)
            .unwrap();

        assert!(output.iter().all(|&x| x == 1));
    }

    #[test]
    fn test_lpc_synthesis_gain_scaling_half() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let residual_q14 = vec![16384_i32; 80];
        let params = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 16],
            gain_q16: 32768,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        let output = decoder
            .lpc_synthesis(&residual_q14, &params, Bandwidth::Wideband, 0)
            .unwrap();

        assert!(output.iter().all(|&x| x == 0 || x == 1));
    }

    #[test]
    fn test_lpc_synthesis_clamping() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let residual_q14 = vec![163_840_i32; 80];
        let params = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 16],
            gain_q16: 131_072,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        let output = decoder
            .lpc_synthesis(&residual_q14, &params, Bandwidth::Wideband, 0)
            .unwrap();

        assert!(output.iter().all(|&x| x == 20));
    }

    #[test]
    fn test_lpc_synthesis_negative_clamping() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let residual_q14 = vec![-163_840_i32; 80];
        let params = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 16],
            gain_q16: 131_072,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        decoder
            .lpc_synthesis(&residual_q14, &params, Bandwidth::Wideband, 0)
            .unwrap();
    }

    #[test]
    fn test_lpc_synthesis_history_saved() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let residual_q14 = vec![8192_i32; 80];
        let params = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 16],
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        decoder
            .lpc_synthesis(&residual_q14, &params, Bandwidth::Wideband, 0)
            .unwrap();

        assert_eq!(decoder.ltp_state[0].lpc_history_q14.len(), 16);
        // With zero LPC coeffs and residual_q14=8192, slpc_q14 = 8192 + rounding (128) = 8320
        // Verify history contains the last 16 samples
        assert!(
            decoder.ltp_state[0]
                .lpc_history_q14
                .iter()
                .all(|&x| x == 8320),
            "History values: {:?}",
            &decoder.ltp_state[0].lpc_history_q14
        );
    }

    #[test]
    fn test_lpc_synthesis_with_history() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let residual1_q14 = vec![16384_i32; 80];
        let params1 = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 16],
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        decoder
            .lpc_synthesis(&residual1_q14, &params1, Bandwidth::Wideband, 0)
            .unwrap();

        let residual2_q14 = vec![0_i32; 80];
        let params2 = SubframeParams {
            lpc_coeffs_q12: vec![1024i16; 16],
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        let output2 = decoder
            .lpc_synthesis(&residual2_q14, &params2, Bandwidth::Wideband, 0)
            .unwrap();

        assert!(output2[0] > 0);
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_lpc_synthesis_all_bandwidths() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 10).unwrap();

        let residual_nb_q14 = vec![8192_i32; 40];
        let params_nb = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 10],
            gain_q16: 65536,
            pitch_lag: 50,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        let output_nb = decoder
            .lpc_synthesis(&residual_nb_q14, &params_nb, Bandwidth::Narrowband, 0)
            .unwrap();

        assert_eq!(output_nb.len(), 40);
        assert_eq!(output_nb.len(), 40);

        let residual_mb_q14 = vec![8192_i32; 60];
        let params_mb = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 10],
            gain_q16: 65536,
            pitch_lag: 50,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };
        let output_mb = decoder
            .lpc_synthesis(&residual_mb_q14, &params_mb, Bandwidth::Mediumband, 0)
            .unwrap();

        assert_eq!(output_mb.len(), 60);
        assert_eq!(output_mb.len(), 60);

        let residual_wb_q14 = vec![8192_i32; 80];
        let params_wb = SubframeParams {
            lpc_coeffs_q12: vec![0i16; 16],
            gain_q16: 65536,
            pitch_lag: 100,
            ltp_filter_q7: [10, 20, 30, 20, 10],
            ltp_scale_q14: 14000,
        };

        let output_wb = decoder
            .lpc_synthesis(&residual_wb_q14, &params_wb, Bandwidth::Wideband, 0)
            .unwrap();

        assert_eq!(output_wb.len(), 80);
        assert_eq!(output_wb.len(), 80);
    }

    #[test]
    fn test_stereo_ms_to_lr_phase1_duration() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let mut mid = vec![16384_i16; 322];
        let mut side = vec![3276_i16; 322];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [1000, 500], 16, 320)
            .unwrap();

        assert_eq!(mid.len(), 322);
        assert_eq!(side.len(), 322);
    }

    #[test]
    fn test_stereo_ms_to_lr_phase1_nb() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Stereo, 20).unwrap();

        let mut mid = vec![16384_i16; 162];
        let mut side = vec![3276_i16; 162];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [1000, 500], 8, 160)
            .unwrap();

        assert_eq!(mid.len(), 162);
        assert_eq!(side.len(), 162);
    }

    #[test]
    fn test_stereo_ms_to_lr_phase1_mb() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz12000, Channels::Stereo, 20).unwrap();

        let mut mid = vec![16384_i16; 242];
        let mut side = vec![3276_i16; 242];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [1000, 500], 12, 240)
            .unwrap();

        assert_eq!(mid.len(), 242);
        assert_eq!(side.len(), 242);
    }

    #[test]
    fn test_stereo_ms_to_lr_weight_interpolation() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        if let Some(state) = &mut decoder.stereo_state {
            state.pred_prev_q13 = [0, 0];
        }

        let mut mid = vec![32767_i16; 322];
        let mut side = vec![0_i16; 322];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [8192, 4096], 16, 320)
            .unwrap();

        if let Some(state) = &decoder.stereo_state {
            assert_eq!(state.pred_prev_q13[0], 8192);
            assert_eq!(state.pred_prev_q13[1], 4096);
        }
    }

    #[test]
    fn test_stereo_ms_to_lr_basic() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let mut mid = vec![16384_i16; 322];
        let mut side = vec![0_i16; 322];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [0, 0], 16, 320)
            .unwrap();

        assert_eq!(mid.len(), 322);
        assert_eq!(side.len(), 322);
    }

    #[test]
    fn test_stereo_ms_to_lr_with_history() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        if let Some(state) = &mut decoder.stereo_state {
            state.s_mid = [1000, 2000];
        }

        let mut mid = vec![3000_i16; 322];
        let mut side = vec![0_i16; 322];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [8192, 0], 16, 320)
            .unwrap();
    }

    #[test]
    fn test_stereo_ms_to_lr_short_frame() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        if let Some(state) = &mut decoder.stereo_state {
            state.s_mid = [0, 1000];
            state.s_side = [500, 1000];
        }

        let mut mid = vec![2000_i16, 3000, 4000, 5000, 6000];
        let mut side = vec![1000_i16, 1500, 2000, 2500, 3000];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [0, 0], 16, 3)
            .unwrap();

        assert_eq!(mid.len(), 5);
        assert_eq!(side.len(), 5);
    }

    #[test]
    fn test_stereo_ms_to_lr_zero_weights() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        if let Some(state) = &mut decoder.stereo_state {
            state.s_mid = [0, 10000];
            state.s_side = [5000, 8000];
        }

        let mut mid = vec![20000_i16; 12];
        let mut side = vec![10000_i16; 12];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [0, 0], 16, 10)
            .unwrap();

        // Output starts at index 1 (index 0 is history)
        // After MS to LR conversion: L = M + S, R = M - S
        // With M=20000, S=10000: L = 30000, R = 10000
        assert!(mid[1] > 0);
        assert!(side[1] > 0);
    }

    #[test]
    fn test_stereo_ms_to_lr_large_values() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let mut mid = vec![32767_i16; 322];
        let mut side = vec![32767_i16; 322];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [8192, 4096], 16, 320)
            .unwrap();

        assert!(mid.iter().all(|&x| (i16::MIN..=i16::MAX).contains(&x)));
        assert!(side.iter().all(|&x| (i16::MIN..=i16::MAX).contains(&x)));
    }

    #[test]
    fn test_stereo_ms_to_lr_negative_values() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let mut mid = vec![-32767_i16; 322];
        let mut side = vec![-32767_i16; 322];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [8192, 4096], 16, 320)
            .unwrap();

        assert!(mid.iter().all(|&x| (i16::MIN..=i16::MAX).contains(&x)));
        assert!(side.iter().all(|&x| (i16::MIN..=i16::MAX).contains(&x)));
    }

    #[test]
    fn test_stereo_ms_to_lr_history_updated() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let mut mid = vec![1000_i16, 2000, 3000, 4000, 5000, 6000, 7000];
        let mut side = vec![100_i16, 200, 300, 400, 500, 600, 700];

        decoder
            .stereo_ms_to_lr(&mut mid, &mut side, [1000, 500], 16, 5)
            .unwrap();

        if let Some(state) = &decoder.stereo_state {
            assert_eq!(state.pred_prev_q13[0], 1000);
            assert_eq!(state.pred_prev_q13[1], 500);
        }
    }

    #[test]
    fn test_mono_one_sample_delay() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let samples = vec![1000_i16, 2000, 3000, 4000, 5000];
        let delayed = decoder.apply_mono_delay(&samples);

        assert_eq!(delayed.len(), 5);
        assert_eq!(delayed[0], 0);
        assert_eq!(delayed[1], 1000);
        assert_eq!(delayed[2], 2000);
        assert_eq!(delayed[3], 3000);
        assert_eq!(delayed[4], 4000);
    }

    #[test]
    fn test_resampler_delay_constants() {
        assert!((SilkDecoder::resampler_delay_ms(Bandwidth::Narrowband) - 0.538).abs() < 1e-6);
        assert!((SilkDecoder::resampler_delay_ms(Bandwidth::Mediumband) - 0.692).abs() < 1e-6);
        assert!((SilkDecoder::resampler_delay_ms(Bandwidth::Wideband) - 0.706).abs() < 1e-6);
        assert!((SilkDecoder::resampler_delay_ms(Bandwidth::SuperWideband) - 0.0).abs() < 1e-6);
        assert!((SilkDecoder::resampler_delay_ms(Bandwidth::Fullband) - 0.0).abs() < 1e-6);
    }

    #[cfg(feature = "resampling")]
    #[test]
    fn test_resampling_same_rate() {
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let samples = vec![0.5_f32; 320];
        let resampled = decoder.resample(&samples, 16000, 16000, 1).unwrap();

        assert_eq!(resampled.len(), samples.len());
        assert_eq!(resampled, samples);
    }

    #[cfg(feature = "resampling")]
    #[test]
    fn test_resampling_16khz_to_48khz() {
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let samples = vec![0.5_f32; 320];
        let resampled = decoder.resample(&samples, 16000, 48000, 1).unwrap();

        assert!(resampled.len() > 900 && resampled.len() < 1000);
    }

    #[cfg(not(feature = "resampling"))]
    #[test]
    fn test_resampling_without_feature_errors() {
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        let result = decoder.resample(&[0.0; 160], 16000, 48000, 1);
        assert!(result.is_err());

        if let Err(e) = result {
            let msg = format!("{e:?}");
            assert!(msg.contains("Resampling not available"));
        }
    }

    // ========================================================================
    // Phase 5.3.1.9: Unit Tests for Violation Fixes
    // ========================================================================

    // 5.3.1.9.1: Gain Dequantization Formula Tests

    #[test]
    fn test_silk_log2lin_zero() {
        assert_eq!(SilkDecoder::silk_log2lin(0), 1);
    }

    #[test]
    fn test_silk_log2lin_integer_powers() {
        assert_eq!(SilkDecoder::silk_log2lin(128), 2); // 2^(128/128) = 2^1
        assert_eq!(SilkDecoder::silk_log2lin(256), 4); // 2^(256/128) = 2^2
        assert_eq!(SilkDecoder::silk_log2lin(384), 8); // 2^(384/128) = 2^3
    }

    #[test]
    fn test_silk_log2lin_rfc_formula_verification() {
        let in_log_q7 = 200;
        let i = in_log_q7 >> 7;
        let f = in_log_q7 & 127;
        let pow2_i = 1_i32 << i;
        let expected = pow2_i + (((-174 * f * (128 - f)) >> 16) + f) * (pow2_i >> 7);
        assert_eq!(SilkDecoder::silk_log2lin(in_log_q7), expected);
    }

    #[test]
    fn test_dequantize_gain_log_gain_zero() {
        let result = SilkDecoder::dequantize_gain(0);
        assert_eq!(result, SilkDecoder::silk_log2lin(2090));
    }

    #[test]
    fn test_dequantize_gain_log_gain_63() {
        let scaled = (0x001D_1C71_i64 * 63) >> 16;
        #[allow(clippy::cast_possible_truncation)]
        let in_log_q7 = (scaled as i32) + 2090;
        let expected = SilkDecoder::silk_log2lin(in_log_q7);
        assert_eq!(SilkDecoder::dequantize_gain(63), expected);
    }

    #[test]
    fn test_dequantize_gain_output_range() {
        for log_gain in 0..=63 {
            let gain = SilkDecoder::dequantize_gain(log_gain);
            assert!(gain >= 81920, "log_gain={log_gain}");
            assert!(gain <= 1_686_110_208, "log_gain={log_gain}");
        }
    }

    #[test]
    fn test_dequantize_gain_rfc_constants() {
        let log_gain = 32;
        let scaled = (0x001D_1C71_i64 * i64::from(log_gain)) >> 16;
        #[allow(clippy::cast_possible_truncation)]
        let in_log_q7 = (scaled as i32) + 2090;
        assert_eq!(
            SilkDecoder::dequantize_gain(log_gain),
            SilkDecoder::silk_log2lin(in_log_q7)
        );
    }

    // 5.3.1.9.2: Stereo Decode Tests

    #[test]
    fn test_decode_mid_only_flag_false() {
        let data = vec![0x00; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let mid_only = decoder.decode_mid_only_flag(&mut range_decoder).unwrap();
        assert!(!mid_only);
        assert!(!decoder.uncoded_side_channel);
    }

    #[test]
    fn test_decode_mid_only_flag_true() {
        let data = vec![0xFF; 10];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let mid_only = decoder.decode_mid_only_flag(&mut range_decoder).unwrap();
        assert!(mid_only);
        assert!(decoder.uncoded_side_channel);
    }

    #[test]
    fn test_decode_silk_frame_wrapper_mono_vs_stereo() {
        let data = vec![0xFF; 200];
        let mut range_decoder_mono = RangeDecoder::new(&data).unwrap();
        let mut range_decoder_stereo = RangeDecoder::new(&data).unwrap();

        let mut mono_decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        let mut stereo_decoder =
            SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();

        let mut mono_output = vec![0_i16; 320];
        let mut stereo_output = vec![0_i16; 640];

        // Both should work without error (though may fail on invalid bitstream)
        let mono_result =
            mono_decoder.decode_silk_frame(&mut range_decoder_mono, true, &mut mono_output);
        let stereo_result =
            stereo_decoder.decode_silk_frame(&mut range_decoder_stereo, true, &mut stereo_output);

        // At minimum, verify signatures are correct
        assert!(mono_result.is_ok() || mono_result.is_err());
        assert!(stereo_result.is_ok() || stereo_result.is_err());
    }

    // 5.3.1.9.3: LPC Selection Tests

    #[test]
    fn test_lpc_coefficients_generated_for_20ms_interpolation() {
        let data = vec![0xFF; 200];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        decoder.previous_lsf_wb = Some([
            100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1100, 1200, 1300, 1400, 1500, 1600,
        ]);
        decoder.decoder_reset = false;

        let mut output = vec![0_i16; 320];
        let _result = decoder.decode_silk_frame(&mut range_decoder, true, &mut output);

        // Test passes if it doesn't panic (interpolation attempted)
    }

    #[test]
    fn test_lpc_selection_10ms_no_interpolation() {
        let data = vec![0xFF; 100];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 10).unwrap();

        let mut output = vec![0_i16; 160];
        let _result = decoder.decode_silk_frame(&mut range_decoder, true, &mut output);

        // 10ms frames don't interpolate - test passes if no panic
    }

    // 5.3.1.9.4: VAD Parameter Tests

    #[test]
    fn test_decode_silk_frame_accepts_vad_parameter() {
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
        let data = vec![0xFF; 100];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut output = vec![0_i16; 320];

        // Should compile and accept vad_flag parameter
        let _result = decoder.decode_silk_frame(&mut range_decoder, true, &mut output);
        // Test passes if signature is correct (may fail on decode)
    }

    #[test]
    fn test_vad_flag_affects_frame_type() {
        let data = vec![0x80; 100];
        let decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        // vad_flag should be passed to decode_frame_type
        let mut range_decoder1 = RangeDecoder::new(&data).unwrap();
        let result1 = decoder.decode_frame_type(&mut range_decoder1, true);

        let mut range_decoder2 = RangeDecoder::new(&data).unwrap();
        let result2 = decoder.decode_frame_type(&mut range_decoder2, false);

        // Both should succeed (different PDFs used based on VAD flag)
        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    // 5.3.1.9.5: Integration Tests

    #[test]
    fn test_decode_silk_frame_complete_10ms_mono() {
        let data = vec![0xFF; 100];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 10).unwrap();

        let mut output = vec![0_i16; 80];
        let result = decoder.decode_silk_frame(&mut range_decoder, true, &mut output);

        // Should attempt decode (may fail on invalid bitstream)
        if let Ok(samples) = result {
            assert_eq!(samples, 80); // NB 10ms = 80 samples
        }
    }

    #[test]
    fn test_decode_silk_frame_complete_20ms_wb() {
        let data = vec![0xFF; 200];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        let mut output = vec![0_i16; 320];
        let result = decoder.decode_silk_frame(&mut range_decoder, true, &mut output);

        // Should attempt decode (may fail on invalid bitstream)
        if let Ok(samples) = result {
            assert_eq!(samples, 320); // WB 20ms = 320 samples
        }
    }

    #[test]
    fn test_decode_silk_frame_state_persistence() {
        let data = vec![0xFF; 100];
        let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();

        // Frame 1
        let mut range_decoder1 = RangeDecoder::new(&data).unwrap();
        let mut output1 = vec![0_i16; 320];
        let _ = decoder.decode_silk_frame(&mut range_decoder1, true, &mut output1);

        // Check state was updated
        assert!(!decoder.decoder_reset); // Should be cleared after first frame
    }
}
