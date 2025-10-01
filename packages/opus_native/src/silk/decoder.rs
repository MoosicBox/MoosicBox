use crate::error::{Error, Result};
use crate::range::RangeDecoder;
use crate::{Channels, SampleRate};

use super::frame::{FrameType, QuantizationOffsetType};

// RFC 6716 Table 6: Stereo weight PDFs (lines 2225-2238)
// NOTE: All ICDF tables MUST end with 0 per RFC 6716 Section 4.1.3.3 (line 1534):
//       "the table is terminated by a value of 0 (where fh[k] == ft)."
//       The RFC tables show PDF values; ICDF format requires this terminating zero.
const STEREO_WEIGHT_PDF_STAGE1: &[u8] = &[
    7, 2, 1, 1, 1, 10, 24, 8, 1, 1, 3, 23, 92, 23, 3, 1, 1, 8, 24, 10, 1, 1, 1, 2, 7, 0,
];

const STEREO_WEIGHT_PDF_STAGE2: &[u8] = &[85, 86, 85, 0];

const STEREO_WEIGHT_PDF_STAGE3: &[u8] = &[51, 51, 52, 51, 51, 0];

const STEREO_WEIGHT_TABLE_Q13: &[i16] = &[
    -13732, -10050, -8266, -7526, -6500, -5000, -2950, -820, 820, 2950, 5000, 6500, 7526, 8266,
    10050, 13732,
];

// RFC 6716 Tables 11-13: Gain PDFs (lines 2485-2545)
// NOTE: All ICDF tables MUST end with 0 per RFC 6716 Section 4.1.3.3 (line 1534):
//       "the table is terminated by a value of 0 (where fh[k] == ft)."
//       The RFC tables show PDF values; ICDF format requires this terminating zero.
const GAIN_PDF_INACTIVE: &[u8] = &[32, 112, 68, 29, 12, 1, 1, 1, 0];
const GAIN_PDF_UNVOICED: &[u8] = &[2, 17, 45, 60, 62, 47, 19, 4, 0];
const GAIN_PDF_VOICED: &[u8] = &[1, 3, 26, 71, 94, 50, 9, 2, 0];
const GAIN_PDF_LSB: &[u8] = &[32, 32, 32, 32, 32, 32, 32, 32, 0];
const GAIN_PDF_DELTA: &[u8] = &[
    6, 5, 11, 31, 132, 21, 8, 4, 3, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0,
];

// RFC 6716 Tables 9-10: Frame type PDFs (lines 2419-2445)
// NOTE: All ICDF tables MUST end with 0 per RFC 6716 Section 4.1.3.3 (line 1534):
//       "the table is terminated by a value of 0 (where fh[k] == ft)."
//       The RFC tables show PDF values; ICDF format requires this terminating zero.
const FRAME_TYPE_PDF_INACTIVE: &[u8] = &[26, 230, 0, 0, 0, 0, 0];
const FRAME_TYPE_PDF_ACTIVE: &[u8] = &[0, 0, 24, 74, 148, 10, 0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub struct TocInfo {
    pub config: u8,
    pub is_stereo: bool,
    pub frame_count_code: u8,
}

impl TocInfo {
    #[must_use]
    #[allow(dead_code)]
    pub const fn parse(toc_byte: u8) -> Self {
        Self {
            config: toc_byte >> 3,
            is_stereo: (toc_byte >> 2) & 0x1 == 1,
            frame_count_code: toc_byte & 0x3,
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub const fn uses_silk(self) -> bool {
        self.config < 16
    }

    #[must_use]
    #[allow(dead_code)]
    pub const fn is_hybrid(self) -> bool {
        matches!(self.config, 12..=15)
    }

    #[must_use]
    #[allow(dead_code)]
    pub const fn bandwidth(self) -> Bandwidth {
        match self.config {
            0..=3 | 16..=19 => Bandwidth::Narrowband,
            4..=7 => Bandwidth::Mediumband,
            8..=11 | 20..=23 => Bandwidth::Wideband,
            12..=13 | 24..=27 => Bandwidth::SuperWideband,
            14..=15 | 28..=31 => Bandwidth::Fullband,
            _ => unreachable!(),
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub const fn frame_size_ms(self) -> u8 {
        let index = (self.config % 4) as usize;
        match self.config {
            0..=11 => [10, 20, 40, 60][index],
            12..=15 => [10, 20, 10, 20][index],
            16..=31 => [2, 5, 10, 20][index],
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Bandwidth {
    Narrowband,
    Mediumband,
    Wideband,
    SuperWideband,
    Fullband,
}

#[derive(Debug, Clone)]
pub struct HeaderBits {
    pub mid_vad_flags: Vec<bool>,
    pub mid_lbrr_flag: bool,
    pub side_vad_flags: Option<Vec<bool>>,
    pub side_lbrr_flag: Option<bool>,
}

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

        Ok(Self {
            sample_rate,
            channels,
            frame_size_ms,
            num_silk_frames,
            previous_stereo_weights: None,
            previous_gain_indices: [None, None],
        })
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
                const PDF_40MS: &[u8] = &[0, 53, 53, 150];
                range_decoder.ec_dec_icdf(PDF_40MS, 8)?
            }
            60 => {
                const PDF_60MS: &[u8] = &[0, 41, 20, 29, 41, 15, 28, 82];
                range_decoder.ec_dec_icdf(PDF_60MS, 8)?
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
        let n = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE1, 8)?;
        let i0 = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE2, 8)?;
        let i1 = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE3, 8)?;
        let i2 = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE2, 8)?;
        let i3 = range_decoder.ec_dec_icdf(STEREO_WEIGHT_PDF_STAGE3, 8)?;

        #[allow(clippy::cast_possible_truncation)]
        let wi0 = (i0 + 3 * (n / 5)) as usize;
        #[allow(clippy::cast_possible_truncation)]
        let wi1 = (i2 + 3 * (n % 5)) as usize;

        #[allow(clippy::cast_possible_wrap)]
        let w1_q13 = i32::from(STEREO_WEIGHT_TABLE_Q13[wi1])
            + (((i32::from(STEREO_WEIGHT_TABLE_Q13[wi1 + 1])
                - i32::from(STEREO_WEIGHT_TABLE_Q13[wi1]))
                * 6554)
                >> 16)
                * (i3 as i32 * 2 + 1);

        #[allow(clippy::cast_possible_wrap)]
        let w0_q13 = i32::from(STEREO_WEIGHT_TABLE_Q13[wi0])
            + (((i32::from(STEREO_WEIGHT_TABLE_Q13[wi0 + 1])
                - i32::from(STEREO_WEIGHT_TABLE_Q13[wi0]))
                * 6554)
                >> 16)
                * (i1 as i32 * 2 + 1)
            - w1_q13;

        #[allow(clippy::cast_possible_truncation)]
        let weights = (w0_q13 as i16, w1_q13 as i16);
        self.previous_stereo_weights = Some(weights);

        Ok(weights)
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
        let pdf = if vad_flag {
            FRAME_TYPE_PDF_ACTIVE
        } else {
            FRAME_TYPE_PDF_INACTIVE
        };

        let frame_type_value = range_decoder.ec_dec_icdf(pdf, 8)?;

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

                let unclamped = if delta_gain_index < 16 {
                    prev.saturating_add(delta_gain_index).saturating_sub(4)
                } else {
                    prev.saturating_add(2_u8.saturating_mul(delta_gain_index).saturating_sub(16))
                };

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
    fn test_toc_parsing_silk_nb() {
        let toc = TocInfo::parse(0b0000_0000);
        assert_eq!(toc.config, 0);
        assert!(!toc.is_stereo);
        assert_eq!(toc.frame_count_code, 0);
        assert!(toc.uses_silk());
        assert!(!toc.is_hybrid());
        assert_eq!(toc.bandwidth(), Bandwidth::Narrowband);
        assert_eq!(toc.frame_size_ms(), 10);
    }

    #[test]
    fn test_toc_parsing_hybrid_swb() {
        let toc = TocInfo::parse(0b0110_0101);
        assert_eq!(toc.config, 12);
        assert!(toc.is_stereo);
        assert!(toc.uses_silk());
        assert!(toc.is_hybrid());
        assert_eq!(toc.bandwidth(), Bandwidth::SuperWideband);
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
}
