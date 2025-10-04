use crate::error::{Error, Result};
use crate::range::RangeDecoder;
use crate::{Channels, SampleRate};

use super::constants::{
    ALLOCATION_TABLE, CACHE_CAPS, CELT_BINS_2_5MS, CELT_BINS_5MS, CELT_BINS_10MS, CELT_BINS_20MS,
    CELT_INTRA_PDF, CELT_NUM_BANDS, CELT_SILENCE_PDF, CELT_TRANSIENT_PDF, LOG2_FRAC_TABLE,
    TRIM_PDF,
};

/// Result of bit allocation computation
#[derive(Debug, Clone)]
pub struct Allocation {
    /// Shape bits per band in 1/8 bit units (for PVQ)
    pub shape_bits: [i32; CELT_NUM_BANDS],

    /// Fine energy bits per band per channel
    pub fine_energy_bits: [u8; CELT_NUM_BANDS],

    /// Priority flags for final bit allocation (0 or 1)
    pub fine_priority: [u8; CELT_NUM_BANDS],

    /// Number of bands actually coded
    pub coded_bands: usize,

    /// Remaining bits for rebalancing
    pub balance: i32,
}

/// CELT decoder state (RFC Section 4.3)
pub struct CeltState {
    /// Previous frame's final energy per band (Q8 format)
    pub prev_energy: [i16; CELT_NUM_BANDS],

    /// Post-filter state (if enabled)
    pub post_filter_state: Option<PostFilterState>,

    /// Previous frame's MDCT output for overlap-add
    pub overlap_buffer: Vec<f32>,

    /// Anti-collapse processing state
    pub anti_collapse_state: AntiCollapseState,
}

/// Post-filter state (RFC Section 4.3.7.1)
#[derive(Debug, Clone)]
pub struct PostFilterState {
    /// Previous pitch period
    #[allow(dead_code)]
    pub prev_period: u16,

    /// Previous pitch gain
    #[allow(dead_code)]
    pub prev_gain: u8,

    /// Filter memory
    #[allow(dead_code)]
    pub memory: Vec<f32>,
}

/// Anti-collapse state (RFC Section 4.3.5)
#[derive(Debug, Clone)]
pub struct AntiCollapseState {
    /// Seed for random number generator
    pub seed: u32,
}

impl CeltState {
    #[must_use]
    pub fn new(frame_size: usize, channels: usize) -> Self {
        Self {
            prev_energy: [0; CELT_NUM_BANDS],
            post_filter_state: None,
            overlap_buffer: vec![0.0; frame_size * channels],
            anti_collapse_state: AntiCollapseState { seed: 0 },
        }
    }

    /// Resets decoder state (for packet loss recovery)
    pub fn reset(&mut self) {
        self.prev_energy.fill(0);
        self.post_filter_state = None;
        self.overlap_buffer.fill(0.0);
        self.anti_collapse_state.seed = 0;
    }
}

pub struct CeltDecoder {
    sample_rate: SampleRate,
    #[allow(dead_code)]
    channels: Channels,
    frame_size: usize, // In samples
    state: CeltState,

    // Band configuration (matching libopus st->start and st->end)
    /// Starting band index (usually 0, can be 17 for narrowband)
    ///
    /// **CRITICAL TODO (Phase 4.6):** Remove `#[allow(dead_code)]` and use in `decode_celt_frame()`
    ///
    /// This field MUST be consumed by the main orchestration function:
    /// ```ignore
    /// self.decode_tf_changes(range_decoder, self.start_band, self.end_band)?;
    /// self.compute_allocation(..., self.start_band, self.end_band, ...)?;
    /// ```
    ///
    /// Will be SET by:
    /// - Phase 5: Mode detection (narrowband sets `start_band = 17`)
    /// - Phase 7: CTL commands (`CELT_SET_START_BAND_REQUEST`)
    #[allow(dead_code)]
    start_band: usize,
    /// Ending band index (usually `CELT_NUM_BANDS`, can vary by bandwidth)
    ///
    /// **CRITICAL TODO (Phase 4.6):** Remove `#[allow(dead_code)]` and use in `decode_celt_frame()`
    ///
    /// This field MUST be consumed by the main orchestration function.
    ///
    /// Will be SET by:
    /// - Phase 5: Custom mode detection via TOC byte
    /// - Phase 7: CTL commands (`CELT_SET_END_BAND_REQUEST`)
    #[allow(dead_code)]
    end_band: usize,

    // Transient state (RFC Section 4.3.1)
    /// Global transient flag (RFC line 6011)
    transient: bool,
    /// TF select index (RFC line 6020)
    tf_select: Option<u8>,
    /// Per-band TF change flags (RFC line 6016)
    tf_change: Vec<bool>,
    /// Computed TF resolution per band
    tf_resolution: Vec<u8>,
}

impl CeltDecoder {
    /// Creates a new CELT decoder.
    ///
    /// # Errors
    ///
    /// * Returns an error if `frame_size` is invalid for the given `sample_rate`.
    pub fn new(sample_rate: SampleRate, channels: Channels, frame_size: usize) -> Result<Self> {
        // Validate frame size based on sample rate (RFC Section 2)
        // CELT supports 2.5/5/10/20 ms frames
        let valid_frame_sizes = match sample_rate {
            SampleRate::Hz8000 => vec![20, 40, 80, 160],
            SampleRate::Hz12000 => vec![30, 60, 120, 240],
            SampleRate::Hz16000 => vec![40, 80, 160, 320],
            SampleRate::Hz24000 => vec![60, 120, 240, 480],
            SampleRate::Hz48000 => vec![120, 240, 480, 960],
        };

        if !valid_frame_sizes.contains(&frame_size) {
            return Err(Error::CeltDecoder(format!(
                "invalid frame size {frame_size} for sample rate {sample_rate:?}"
            )));
        }

        let num_channels = match channels {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        };

        Ok(Self {
            sample_rate,
            channels,
            frame_size,
            state: CeltState::new(frame_size, num_channels),
            start_band: 0,
            end_band: CELT_NUM_BANDS,
            transient: false,
            tf_select: None,
            tf_change: Vec::new(),
            tf_resolution: Vec::new(),
        })
    }

    /// Resets decoder state
    pub fn reset(&mut self) {
        self.state.reset();
    }

    /// Decodes silence flag (RFC Table 56)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_silence(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        let value = range_decoder.ec_dec_icdf_u16(CELT_SILENCE_PDF, 15)?;
        Ok(value == 1)
    }

    /// Decodes post-filter flag (RFC Table 56)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_post_filter(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        range_decoder.ec_dec_bit_logp(1)
    }

    /// Decodes transient flag (RFC Section 4.3.1, lines 6011-6015)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_transient_flag(&mut self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        let value = range_decoder.ec_dec_icdf(CELT_TRANSIENT_PDF, 8)?;
        let transient = value == 1;
        self.transient = transient;
        Ok(transient)
    }

    /// Decodes intra flag (RFC Table 56)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_intra(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        let value = range_decoder.ec_dec_icdf(CELT_INTRA_PDF, 8)?;
        Ok(value == 1)
    }

    /// Returns frame duration in milliseconds
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn frame_duration_ms(&self) -> f32 {
        let sample_rate_f32 = self.sample_rate as u32 as f32;
        (self.frame_size as f32 * 1000.0) / sample_rate_f32
    }

    /// Returns MDCT bins per band for this frame size
    #[must_use]
    pub fn bins_per_band(&self) -> &'static [u8; CELT_NUM_BANDS] {
        let duration_ms = self.frame_duration_ms();
        if (duration_ms - 2.5).abs() < 0.1 {
            &CELT_BINS_2_5MS
        } else if (duration_ms - 5.0).abs() < 0.1 {
            &CELT_BINS_5MS
        } else if (duration_ms - 10.0).abs() < 0.1 {
            &CELT_BINS_10MS
        } else {
            &CELT_BINS_20MS
        }
    }

    /// Returns frame duration index (0=2.5ms, 1=5ms, 2=10ms, 3=20ms)
    #[must_use]
    fn frame_duration_index(&self) -> usize {
        let duration = self.frame_duration_ms();
        if (duration - 2.5).abs() < 0.1 {
            0
        } else if (duration - 5.0).abs() < 0.1 {
            1
        } else if (duration - 10.0).abs() < 0.1 {
            2
        } else {
            3
        }
    }

    /// Decodes coarse energy for all bands
    ///
    /// Implements 2-D prediction filter for energy envelope decoding using
    /// time-domain (inter-frame) and frequency-domain (intra-frame) prediction.
    ///
    /// ## RFC References
    ///
    /// * RFC 6716 Section 4.3.2.1 (lines 6034-6077)
    /// * Prediction filter: RFC lines 6055-6063
    ///   - `A(z_l, z_b) = (1 - alpha*z_l^-1)*(1 - z_b^-1) / (1 - beta*z_b^-1)`
    ///
    /// ## Reference Implementation
    ///
    /// * Function: `unquant_coarse_energy()` in `celt/quant_bands.c`
    /// * URL: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L427-490>
    /// * IIR filter (L487): `prev[c] = prev[c] + q - MULT16_32_Q15(beta,q)`
    ///
    /// ## Critical Implementation Note
    ///
    /// The frequency prediction uses an IIR filter state update:
    /// ```c
    /// prev[c] = prev[c] + q - MULT16_32_Q15(beta,q);  // L487
    /// ```
    /// Equivalent to: `prev = prev + q*(1 - beta)`
    ///
    /// **Bug Fixed in Phase 4.2**: Initial implementation incorrectly used
    /// `prev = beta * energy[band]` which violated RFC specification.
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder state
    /// * `intra_flag` - Whether this is an intra frame (from `decode_intra()`)
    ///
    /// # Returns
    ///
    /// Array of coarse energy values (Q8 format, base-2 log domain)
    ///
    /// # Errors
    ///
    /// * Returns error if Laplace decoding fails
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn decode_coarse_energy(
        &mut self,
        range_decoder: &mut RangeDecoder,
        intra_flag: bool,
    ) -> Result<[i16; CELT_NUM_BANDS]> {
        use super::constants::{
            ENERGY_ALPHA_INTER, ENERGY_BETA_INTER, ENERGY_BETA_INTRA, ENERGY_PROB_MODEL,
        };

        let mut coarse_energy = [0_i16; CELT_NUM_BANDS];
        let mut prev = 0.0_f32;

        let (alpha, beta) = if intra_flag {
            (0.0, ENERGY_BETA_INTRA)
        } else {
            let frame_idx = self.frame_duration_index();
            (ENERGY_ALPHA_INTER[frame_idx], ENERGY_BETA_INTER[frame_idx])
        };

        let frame_idx = self.frame_duration_index();
        let prob_model = &ENERGY_PROB_MODEL[frame_idx][usize::from(intra_flag)];

        #[allow(clippy::needless_range_loop)]
        for band in 0..CELT_NUM_BANDS {
            let time_pred = if intra_flag || self.state.prev_energy[band] == 0 {
                0.0
            } else {
                alpha * f32::from(self.state.prev_energy[band])
            };

            let freq_pred = prev;

            let prediction = time_pred + freq_pred;

            let pi = 2 * band.min(20);
            let fs = u32::from(prob_model[pi]) << 7;
            let decay = u32::from(prob_model[pi + 1]) << 6;

            let error = range_decoder.ec_laplace_decode(fs, decay)?;

            #[allow(clippy::cast_precision_loss)]
            let q = error as f32 * 6.0;
            let raw_energy = prediction + q;

            coarse_energy[band] = raw_energy.clamp(-128.0, 127.0) as i16;

            prev = beta.mul_add(-q, prev + q);
        }

        Ok(coarse_energy)
    }

    /// Decodes fine energy quantization
    ///
    /// RFC 6716 Section 4.3.2.2 (lines 6079-6087)
    ///
    /// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L492-510>
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder state
    /// * `coarse_energy` - Coarse energy from `decode_coarse_energy()`
    /// * `fine_bits` - Bits allocated per band (from Section 4.3.3)
    ///
    /// # Returns
    ///
    /// Refined energy values (Q8 format)
    ///
    /// # Errors
    ///
    /// * Returns error if range decoding fails
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss
    )]
    pub fn decode_fine_energy(
        &self,
        range_decoder: &mut RangeDecoder,
        coarse_energy: &[i16; CELT_NUM_BANDS],
        fine_bits: &[u8; CELT_NUM_BANDS],
    ) -> Result<[i16; CELT_NUM_BANDS]> {
        let mut refined_energy = *coarse_energy;

        for band in 0..CELT_NUM_BANDS {
            let bits = fine_bits[band];

            if bits == 0 {
                continue;
            }

            let ft = 1_u32 << bits;
            let f = range_decoder.ec_dec_uint(ft)?;

            let correction = ((f as f32 + 0.5) / ft as f32) - 0.5;

            let correction_q8 = (correction * 256.0) as i16;

            refined_energy[band] = refined_energy[band].saturating_add(correction_q8);
        }

        Ok(refined_energy)
    }

    /// Decodes final fine energy allocation from unused bits
    ///
    /// RFC 6716 Section 4.3.2.2 (lines 6089-6099)
    ///
    /// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L512-539>
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder state
    /// * `fine_energy` - Energy after fine quantization
    /// * `priorities` - Priority (0 or 1) per band (from allocation)
    /// * `unused_bits` - Remaining bits after all decoding
    ///
    /// # Returns
    ///
    /// Final energy values with extra refinement
    ///
    /// # Errors
    ///
    /// * Returns error if range decoding fails
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn decode_final_energy(
        &self,
        range_decoder: &mut RangeDecoder,
        fine_energy: &[i16; CELT_NUM_BANDS],
        priorities: &[u8; CELT_NUM_BANDS],
        mut unused_bits: u32,
    ) -> Result<[i16; CELT_NUM_BANDS]> {
        let mut final_energy = *fine_energy;
        let channels = match self.channels {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        };

        for band in 0..CELT_NUM_BANDS {
            if priorities[band] == 0 && unused_bits >= channels {
                for _ in 0..channels {
                    if unused_bits == 0 {
                        break;
                    }

                    let bit = range_decoder.ec_dec_bit_logp(1)?;
                    let correction = if bit { 0.5 } else { -0.5 };
                    final_energy[band] =
                        final_energy[band].saturating_add((correction * 256.0) as i16);

                    unused_bits -= 1;
                }
            }
        }

        for band in 0..CELT_NUM_BANDS {
            if priorities[band] == 1 && unused_bits >= channels {
                for _ in 0..channels {
                    if unused_bits == 0 {
                        break;
                    }

                    let bit = range_decoder.ec_dec_bit_logp(1)?;
                    let correction = if bit { 0.5 } else { -0.5 };
                    final_energy[band] =
                        final_energy[band].saturating_add((correction * 256.0) as i16);

                    unused_bits -= 1;
                }
            }
        }

        Ok(final_energy)
    }

    /// Decodes band boost values (RFC lines 6310-6360)
    ///
    /// Band boost allows encoder to increase allocation for specific bands.
    /// Uses dynamic probability coding: starts at 6 bits, decreases to 2 bits minimum.
    ///
    /// # Errors
    ///
    /// * Returns an error if range decoder fails
    ///
    /// # Returns
    ///
    /// * Tuple of (boosts per band, total boost, bits consumed)
    ///
    /// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/celt.c#L2473-2505>
    pub fn decode_band_boost(
        &self,
        range_decoder: &mut RangeDecoder,
        total_bits: i32,
        caps: &[i32; CELT_NUM_BANDS],
    ) -> Result<([i32; CELT_NUM_BANDS], i32, i32)> {
        let bins = self.bins_per_band();
        let mut boosts = [0_i32; CELT_NUM_BANDS];
        let mut total_boost = 0_i32;
        let mut bits_consumed = 0_i32;
        let mut dynalloc_logp = 6; // Initial cost: 6 bits

        for band in 0..CELT_NUM_BANDS {
            let n = i32::from(bins[band]);
            // RFC line 6346: quanta = min(8*N, max(48, N))
            let quanta = (8 * n).min(48.max(n));

            let mut boost = 0_i32;
            let mut dynalloc_loop_logp = dynalloc_logp;

            // Decode boost symbols while we have budget
            while dynalloc_loop_logp * 8
                + i32::try_from(range_decoder.ec_tell_frac()).unwrap_or(i32::MAX)
                < total_bits * 8 + total_boost
                && boost < caps[band]
            {
                let bit = range_decoder
                    .ec_dec_bit_logp(u32::try_from(dynalloc_loop_logp).unwrap_or(31))?;
                if !bit {
                    break;
                }
                boost += quanta;
                total_boost += quanta;
                bits_consumed += quanta; // RFC line 6355: subtract quanta from total_bits
                dynalloc_loop_logp = 1; // Subsequent bits cost only 1 bit
            }

            boosts[band] = boost;

            // Reduce initial cost if we used this band (minimum 2 bits)
            if boost > 0 && dynalloc_logp > 2 {
                dynalloc_logp -= 1;
            }
        }

        Ok((boosts, total_boost, bits_consumed))
    }

    /// Decodes allocation trim parameter (RFC lines 6370-6397)
    ///
    /// Trim biases allocation towards low (trim<5) or high (trim>5) frequencies.
    /// Default value is 5 (no bias).
    ///
    /// # Errors
    ///
    /// * Returns an error if range decoder fails
    ///
    /// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/celt.c#L2565-2572>
    pub fn decode_allocation_trim(
        &self,
        range_decoder: &mut RangeDecoder,
        total_bits: i32,
        total_boost: i32,
    ) -> Result<u8> {
        let mut trim = 5_u8; // Default: no bias

        // Only decode if we have enough bits (6 bits = 48 eighth-bits)
        let tell = i32::try_from(range_decoder.ec_tell_frac()).unwrap_or(i32::MAX);
        if tell + 48 <= total_bits * 8 - total_boost {
            trim = range_decoder.ec_dec_icdf_u16(&TRIM_PDF, 7)?; // ftb=7 (2^7=128)
        }

        Ok(trim)
    }

    /// Decodes intensity stereo and dual stereo parameters (RFC lines 6400-6420)
    ///
    /// Only used in stereo mode:
    /// * Intensity: band index where side channel becomes zero
    /// * Dual stereo: flag to deactivate joint coding
    ///
    /// # Errors
    ///
    /// * Returns an error if range decoder fails
    ///
    /// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/celt.c#L2611-2629>
    #[allow(clippy::similar_names)]
    pub fn decode_stereo_params(
        &self,
        range_decoder: &mut RangeDecoder,
        num_bands: usize,
        total_bits: &mut i32,
    ) -> Result<(u8, bool)> {
        let mut intensity = 0_u8;
        let mut dual_stereo = false;

        // Reserve bits for intensity stereo (conservative log2 of num_bands)
        let intensity_rsv = if num_bands > 0 && num_bands <= LOG2_FRAC_TABLE.len() {
            i32::from(LOG2_FRAC_TABLE[num_bands - 1])
        } else {
            0
        };

        if intensity_rsv > 0 && intensity_rsv <= *total_bits * 8 {
            *total_bits = total_bits.saturating_sub((intensity_rsv + 7) / 8);

            // Decode intensity parameter
            intensity = u8::try_from(
                range_decoder.ec_dec_uint(u32::try_from(num_bands + 1).unwrap_or(u32::MAX))?,
            )
            .unwrap_or(0);

            // Reserve bit for dual stereo if available
            if *total_bits > 0 {
                *total_bits -= 1;
                dual_stereo = range_decoder.ec_dec_bit_logp(1)?;
            }
        }

        Ok((intensity, dual_stereo))
    }

    /// Computes bit allocation for all bands (RFC Section 4.3.3)
    ///
    /// This is the main allocation entry point that:
    /// * Applies conservative subtraction (RFC line 6413-6414)
    /// * Reserves anti-collapse bit if transient (RFC line 6415-6418)
    /// * Reserves skip bit if available (RFC line 6419-6421)
    /// * Computes base allocation from interpolated quality table
    /// * Applies boost, trim, and skip adjustments
    /// * Splits bits between shape (PVQ) and fine energy
    /// * Tracks balance for rebalancing
    ///
    /// # Errors
    ///
    /// * Returns an error if allocation computation fails
    ///
    /// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/rate.c#L555-720>
    #[allow(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        clippy::cognitive_complexity
    )]
    pub fn compute_allocation(
        &self,
        total_bits: i32,
        lm: u8,
        channels: usize,
        boosts: &[i32; CELT_NUM_BANDS],
        trim: u8,
        start_band: usize,
        end_band: usize,
        is_transient: bool,
    ) -> Result<Allocation> {
        let bins = self.bins_per_band();
        let mut shape_bits = [0_i32; CELT_NUM_BANDS];
        let mut fine_energy_bits = [0_u8; CELT_NUM_BANDS];
        let mut fine_priority = [0_u8; CELT_NUM_BANDS];

        let c = i32::try_from(channels).unwrap_or(1);
        let lm_i32 = i32::from(lm);
        let alloc_trim = i32::from(trim);

        // RFC line 6411-6414: Conservative allocation (subtract 1 eighth-bit)
        let mut total = (total_bits * 8).saturating_sub(1);

        // RFC line 6415-6418: Anti-collapse reservation
        let anti_collapse_rsv = if is_transient && lm > 1 && total >= (lm_i32 + 2) * 8 {
            8
        } else {
            0
        };
        total = total.saturating_sub(anti_collapse_rsv).max(0);

        // RFC line 6419-6421: Skip band reservation
        let skip_rsv = if total > 8 { 8 } else { 0 };
        total = total.saturating_sub(skip_rsv);

        let cap_index = 21 * (2 * usize::from(lm) + (channels - 1));

        let mut thresh = [0_i32; CELT_NUM_BANDS];
        for band in start_band..end_band {
            let n = i32::from(bins[band]);
            thresh[band] = (c << 3).max(((3 * n * c) << lm_i32 << 3) >> 4);
        }

        let mut trim_offset = [0_i32; CELT_NUM_BANDS];
        for band in start_band..end_band {
            let n = i32::from(bins[band]);
            let remaining = i32::try_from(end_band - band - 1).unwrap_or(0);
            trim_offset[band] =
                c * n * (alloc_trim - 5 - lm_i32) * remaining * (1 << (lm_i32 + 3)) / 64;

            if (n << lm_i32) == 1 {
                trim_offset[band] -= c << 3;
            }
        }

        let mut lo = 1_usize;
        let mut hi = ALLOCATION_TABLE[0].len() - 1;

        loop {
            let mid = (lo + hi) >> 1;
            let mut psum = 0_i32;
            let mut done = false;

            for band in (start_band..end_band).rev() {
                let n = i32::from(bins[band]);
                let mut bits_j = (c * n * i32::from(ALLOCATION_TABLE[band][mid])) << lm_i32 >> 2;

                if bits_j > 0 {
                    bits_j = bits_j.saturating_add(trim_offset[band]).max(0);
                }
                bits_j = bits_j.saturating_add(boosts[band]);

                if bits_j >= thresh[band] || done {
                    done = true;
                    let cap = if band + cap_index < CACHE_CAPS.len() {
                        i32::from(CACHE_CAPS[band + cap_index])
                    } else {
                        i32::MAX
                    };
                    psum = psum.saturating_add(bits_j.min(cap));
                } else if bits_j >= c << 3 {
                    psum = psum.saturating_add(c << 3);
                }
            }

            if psum > total {
                if mid == 0 {
                    break;
                }
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }

            if lo > hi {
                break;
            }
        }

        let quality_lo = if lo > 0 { lo - 1 } else { 0 };
        let quality_hi = lo.min(ALLOCATION_TABLE[0].len() - 1);

        let mut bits1 = [0_i32; CELT_NUM_BANDS];
        let mut bits2 = [0_i32; CELT_NUM_BANDS];
        let mut _skip_start = start_band;

        for band in start_band..end_band {
            let n = i32::from(bins[band]);

            let mut bits1_j =
                (c * n * i32::from(ALLOCATION_TABLE[band][quality_lo])) << lm_i32 >> 2;
            if bits1_j > 0 {
                bits1_j = bits1_j.saturating_add(trim_offset[band]).max(0);
            }
            if quality_lo > 0 {
                bits1_j = bits1_j.saturating_add(boosts[band]);
            }
            bits1[band] = bits1_j;

            let mut bits2_j = if quality_hi >= ALLOCATION_TABLE[0].len() {
                if band + cap_index < CACHE_CAPS.len() {
                    i32::from(CACHE_CAPS[band + cap_index])
                } else {
                    i32::MAX
                }
            } else {
                (c * n * i32::from(ALLOCATION_TABLE[band][quality_hi])) << lm_i32 >> 2
            };

            if bits2_j > 0 {
                bits2_j = bits2_j.saturating_add(trim_offset[band]).max(0);
            }
            bits2_j = bits2_j.saturating_add(boosts[band]);
            bits2[band] = bits2_j.saturating_sub(bits1_j).max(0);

            if boosts[band] > 0 {
                _skip_start = band;
            }
        }

        let alloc_steps = 6_i32;
        let mut lo_interp = 0_i32;
        let mut hi_interp = 1 << alloc_steps;

        for _ in 0..alloc_steps {
            let mid = (lo_interp + hi_interp) >> 1;
            let mut psum = 0_i32;
            let mut done = false;

            for band in (start_band..end_band).rev() {
                let tmp = bits1[band].saturating_add((mid * bits2[band]) >> alloc_steps);

                if tmp >= thresh[band] || done {
                    done = true;
                    let cap = if band + cap_index < CACHE_CAPS.len() {
                        i32::from(CACHE_CAPS[band + cap_index])
                    } else {
                        i32::MAX
                    };
                    psum = psum.saturating_add(tmp.min(cap));
                } else if tmp >= c << 3 {
                    psum = psum.saturating_add(c << 3);
                }
            }

            if psum > total {
                hi_interp = mid;
            } else {
                lo_interp = mid;
            }
        }

        let mut psum = 0_i32;
        let mut done = false;
        for band in (start_band..end_band).rev() {
            let tmp = bits1[band].saturating_add((lo_interp * bits2[band]) >> alloc_steps);

            let allocated = if tmp < thresh[band] && !done {
                if tmp >= c << 3 { c << 3 } else { 0 }
            } else {
                done = true;
                let cap = if band + cap_index < CACHE_CAPS.len() {
                    i32::from(CACHE_CAPS[band + cap_index])
                } else {
                    i32::MAX
                };
                tmp.min(cap)
            };

            shape_bits[band] = allocated;
            psum = psum.saturating_add(allocated);
        }

        let left = total.saturating_sub(psum);
        if left > 0 {
            let total_band_bins: i32 = (start_band..end_band).map(|b| i32::from(bins[b])).sum();

            let percoeff = if total_band_bins > 0 {
                left / total_band_bins
            } else {
                0
            };

            let mut remainder = left - percoeff * total_band_bins;

            for band in start_band..end_band {
                shape_bits[band] =
                    shape_bits[band].saturating_add(percoeff * i32::from(bins[band]));
                let add = remainder.min(i32::from(bins[band]));
                shape_bits[band] = shape_bits[band].saturating_add(add);
                remainder -= add;
            }
        }

        let mut balance = 0_i32;
        let logm = lm_i32 << 3;

        for band in start_band..end_band {
            let n = i32::from(bins[band]);
            let bit = shape_bits[band].saturating_add(balance);

            if n > 1 {
                let cap = if band + cap_index < CACHE_CAPS.len() {
                    i32::from(CACHE_CAPS[band + cap_index])
                } else {
                    i32::MAX
                };

                let excess = bit.saturating_sub(cap).max(0);
                shape_bits[band] = bit.saturating_sub(excess);

                let den = c * n;
                let offset = (den * (logm - (6 << 3))) >> 1;

                let ebits = if den > 0 {
                    ((shape_bits[band] + offset + (den << 2)) / den) >> 3
                } else {
                    0
                };

                #[allow(clippy::cast_sign_loss)]
                let ebits_u8 = ebits.clamp(0, 7) as u8;
                fine_energy_bits[band] = ebits_u8;

                fine_priority[band] =
                    u8::from((i32::from(ebits_u8) * den) << 3 >= shape_bits[band] + offset);

                shape_bits[band] = shape_bits[band].saturating_sub((c * i32::from(ebits_u8)) << 3);

                balance = excess;
            } else {
                let excess = bit.saturating_sub(c << 3).max(0);
                shape_bits[band] = bit.saturating_sub(excess);
                fine_energy_bits[band] = 0;
                fine_priority[band] = 1;
                balance = excess;
            }
        }

        Ok(Allocation {
            shape_bits,
            fine_energy_bits,
            fine_priority,
            coded_bands: end_band,
            balance,
        })
    }

    /// Compute LM (log2 of frame size relative to shortest)
    ///
    /// Helper for `TF_SELECT_TABLE` indexing
    ///
    /// # Returns
    ///
    /// LM value: 0=2.5ms, 1=5ms, 2=10ms, 3=20ms
    #[must_use]
    fn compute_lm(&self) -> u8 {
        // LM = log2(frame_size / 120) for 48kHz
        match self.frame_size {
            120 => 0, // 2.5ms @ 48kHz
            240 => 1, // 5ms @ 48kHz
            480 => 2, // 10ms @ 48kHz
            960 => 3, // 20ms @ 48kHz
            _ => {
                // For other sample rates, compute from duration
                let duration_ms = self.frame_duration_ms();
                if (duration_ms - 2.5).abs() < 0.1 {
                    0
                } else if (duration_ms - 5.0).abs() < 0.1 {
                    1
                } else if (duration_ms - 10.0).abs() < 0.1 {
                    2
                } else {
                    3
                }
            }
        }
    }

    /// Check if `tf_select` flag can affect decoding result
    ///
    /// RFC 6716 lines 6020-6023: "The `tf_select` flag uses a 1/2 probability,
    /// but is only decoded if it can have an impact on the result knowing
    /// the value of all per-band `tf_change` flags."
    ///
    /// # Arguments
    ///
    /// * `start` - First band index that was decoded
    /// * `end` - Last band index that was decoded (exclusive)
    ///
    /// # Returns
    ///
    /// `true` if `tf_select` should be decoded, `false` if it has no effect
    #[must_use]
    fn should_decode_tf_select(&self, start: usize, end: usize) -> bool {
        use super::constants::TF_SELECT_TABLE;

        let lm = self.compute_lm();
        let lm_idx = lm as usize;
        let is_transient_idx = usize::from(self.transient);

        // Check only the coded bands [start, end)
        for band in start..end {
            let tf_change_idx = usize::from(self.tf_change[band]);

            // Get TF values for both tf_select options using direct table lookup
            let tf_0 = TF_SELECT_TABLE[lm_idx][is_transient_idx][0][tf_change_idx];
            let tf_1 = TF_SELECT_TABLE[lm_idx][is_transient_idx][1][tf_change_idx];

            // Apply clamping to both values
            #[allow(clippy::cast_possible_wrap)]
            let lm_i8 = lm as i8;
            let tf_0_clamped = tf_0.max(0).min(lm_i8);
            let tf_1_clamped = tf_1.max(0).min(lm_i8);

            if tf_0_clamped != tf_1_clamped {
                return true; // tf_select affects at least this band
            }
        }

        false // tf_select has no effect on any band
    }

    /// Decode `tf_select` flag if it affects outcome
    ///
    /// RFC 6716 Section 4.3.1 (lines 6020-6023)
    ///
    /// **CRITICAL:** Must be called AFTER `decode_tf_changes()` per RFC Table 56.
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder instance
    /// * `start` - First band index that was decoded
    /// * `end` - Last band index that was decoded (exclusive)
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    /// * Returns error if `tf_change` not yet decoded
    pub fn decode_tf_select(
        &mut self,
        range_decoder: &mut RangeDecoder,
        start: usize,
        end: usize,
    ) -> Result<Option<u8>> {
        // Validate that tf_change was decoded first (RFC Table 56 ordering)
        if self.tf_change.is_empty() {
            return Err(Error::CeltDecoder(
                "decode_tf_changes() must be called before decode_tf_select()".to_string(),
            ));
        }

        // Only decode if it can impact result (RFC lines 6021-6023)
        // Decision is based on actual tf_change values that were decoded
        if self.should_decode_tf_select(start, end) {
            let tf_select = u8::from(range_decoder.ec_dec_bit_logp(1)?);
            self.tf_select = Some(tf_select);
            Ok(Some(tf_select))
        } else {
            self.tf_select = None;
            Ok(None)
        }
    }

    /// Decode per-band `tf_change` flags for coded bands
    ///
    /// RFC 6716 Section 4.3.4.5 (lines 6625-6631)
    ///
    /// Only decodes flags for bands in the range `[start, end)`. Bands outside
    /// this range retain their default value of `false`.
    ///
    /// Uses relative coding: first CODED band uses absolute PDF, subsequent
    /// bands decode deltas from the previous band using XOR.
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder instance
    /// * `start` - First band index to decode (inclusive)
    /// * `end` - Last band index to decode (exclusive)
    ///
    /// # Returns
    ///
    /// Vector of length `CELT_NUM_BANDS` where:
    /// * `tf_change[band]` for `band in [start, end)` contains decoded values
    /// * `tf_change[band]` for `band < start or band >= end` is `false`
    ///
    /// # Errors
    ///
    /// * Returns error if range decoder fails
    /// * Returns error if `start >= end`
    /// * Returns error if `end > CELT_NUM_BANDS`
    pub fn decode_tf_changes(
        &mut self,
        range_decoder: &mut RangeDecoder,
        start: usize,
        end: usize,
    ) -> Result<Vec<bool>> {
        // Validate parameters
        if start >= end {
            return Err(Error::CeltDecoder(format!(
                "start ({start}) must be less than end ({end})"
            )));
        }

        if end > CELT_NUM_BANDS {
            return Err(Error::CeltDecoder(format!(
                "end ({end}) must not exceed CELT_NUM_BANDS ({CELT_NUM_BANDS})"
            )));
        }

        // Initialize all bands to false (default for uncoded bands)
        let mut tf_change = vec![false; CELT_NUM_BANDS];
        let mut prev_change = false;

        // Only decode for bands in range [start, end)
        for (band, item) in tf_change.iter_mut().enumerate().take(end).skip(start) {
            let is_first = band == start; // First CODED band, not band 0
            let pdf = Self::compute_tf_change_pdf(is_first, self.transient);

            let decoded = range_decoder.ec_dec_icdf(&pdf, 8)? == 1;

            let change = if is_first {
                decoded // First band: absolute value
            } else {
                prev_change ^ decoded // Subsequent bands: XOR for relative coding
            };

            *item = change;
            prev_change = change;
        }

        self.tf_change.clone_from(&tf_change);
        Ok(tf_change)
    }

    /// Compute PDF for `tf_change` flag in given band
    ///
    /// RFC 6716 Section 4.3.4.5 (lines 6625-6631)
    ///
    /// # Arguments
    ///
    /// * `is_first_band` - True if this is the first coded band
    /// * `is_transient` - True if frame marked as transient
    ///
    /// # Returns
    ///
    /// ICDF table for this band's `tf_change` flag
    ///
    /// # Probabilities (RFC lines 6625-6631)
    ///
    /// * First band, transient: {3,1}/4 → ICDF [4, 1, 0]
    /// * First band, non-transient: {15,1}/16 → ICDF [16, 1, 0]
    /// * Subsequent bands, transient: {15,1}/16 → ICDF [16, 1, 0] (for delta)
    /// * Subsequent bands, non-transient: {31,1}/32 → ICDF [32, 1, 0] (for delta)
    #[must_use]
    fn compute_tf_change_pdf(is_first_band: bool, is_transient: bool) -> Vec<u8> {
        if is_first_band {
            // First band: absolute coding
            if is_transient {
                vec![4, 1, 0] // {3,1}/4 probability
            } else {
                vec![16, 1, 0] // {15,1}/16 probability
            }
        } else {
            // Subsequent bands: relative coding (delta from previous)
            if is_transient {
                vec![16, 1, 0] // {15,1}/16 probability for delta
            } else {
                vec![32, 1, 0] // {31,1}/32 probability for delta
            }
        }
    }

    /// Compute time-frequency resolution for each band
    ///
    /// RFC 6716 Section 4.3.4.5 (lines 6633-6697, Tables 60-63)
    ///
    /// Uses direct table lookup based on frame parameters and per-band `tf_change` flags.
    ///
    /// Computes resolution for ALL bands (`0..CELT_NUM_BANDS`). Bands outside the
    /// range that was decoded in `decode_tf_changes()` will use their default
    /// `tf_change=false` value from initialization.
    ///
    /// # Errors
    ///
    /// * Returns error if `tf_change` not yet decoded
    pub fn compute_tf_resolution(&mut self) -> Result<Vec<u8>> {
        use super::constants::TF_SELECT_TABLE;

        let lm = self.compute_lm();
        let num_bands = self.tf_change.len();

        if num_bands == 0 {
            return Err(Error::CeltDecoder(
                "tf_change must be decoded before computing tf_resolution".to_string(),
            ));
        }

        let mut tf_resolution = Vec::with_capacity(CELT_NUM_BANDS);

        let is_transient_idx = usize::from(self.transient);
        let tf_select_idx = usize::from(self.tf_select.unwrap_or(0));
        let lm_idx = lm as usize;

        // Compute resolution for ALL bands
        // Bands outside [start_band, end_band) use tf_change=false (default)
        for band in 0..CELT_NUM_BANDS {
            let tf_change_idx = usize::from(self.tf_change[band]);

            // Direct table lookup from RFC Tables 60-63
            // No arithmetic operations - just index into the 4D table
            let tf = TF_SELECT_TABLE[lm_idx][is_transient_idx][tf_select_idx][tf_change_idx];

            // Clamp to valid range [0, LM]
            #[allow(clippy::cast_possible_wrap)]
            let lm_i8 = lm as i8;
            let tf_clamped = tf.max(0).min(lm_i8);
            #[allow(clippy::cast_sign_loss)]
            tf_resolution.push(tf_clamped as u8);
        }

        self.tf_resolution.clone_from(&tf_resolution);
        Ok(tf_resolution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_celt_decoder_creation() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_frame_size_validation_48khz() {
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 120).is_ok()); // 2.5ms
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 240).is_ok()); // 5ms
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).is_ok()); // 10ms
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).is_ok()); // 20ms
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 100).is_err()); // invalid
    }

    #[test]
    fn test_frame_duration_calculation() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        assert!((decoder.frame_duration_ms() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_bins_per_band_10ms() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        let bins = decoder.bins_per_band();
        assert_eq!(bins[0], 4); // Band 0: 4 bins for 10ms
        assert_eq!(bins[20], 88); // Band 20: 88 bins for 10ms
    }

    #[test]
    fn test_state_initialization() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 480).unwrap();
        assert_eq!(decoder.state.prev_energy.len(), CELT_NUM_BANDS);
        assert_eq!(decoder.state.overlap_buffer.len(), 480 * 2); // stereo
        assert!(decoder.state.post_filter_state.is_none());
    }

    #[test]
    fn test_state_reset() {
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // Modify state
        decoder.state.prev_energy[0] = 100;
        decoder.state.overlap_buffer[0] = 1.5;
        decoder.state.anti_collapse_state.seed = 42;

        // Reset
        decoder.reset();

        // Verify reset
        assert_eq!(decoder.state.prev_energy[0], 0);
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(decoder.state.overlap_buffer[0], 0.0);
        }
        assert_eq!(decoder.state.anti_collapse_state.seed, 0);
    }

    #[test]
    fn test_silence_flag_decoding() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_silence(&mut range_decoder);
        assert!(result.is_ok());
    }

    #[test]
    fn test_transient_flag_decoding_basic() {
        let data = vec![0x80, 0x00, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_transient_flag(&mut range_decoder);
        assert!(result.is_ok());
        let _ = result.unwrap();
    }

    #[test]
    fn test_coarse_energy_intra() {
        let data = vec![0x55; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_coarse_energy(&mut range_decoder, true);
        assert!(result.is_ok());
        let energy = result.unwrap();
        assert_eq!(energy.len(), CELT_NUM_BANDS);
    }

    #[test]
    fn test_coarse_energy_inter() {
        let data = vec![0xAA; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        decoder.state.prev_energy[0] = 50;
        decoder.state.prev_energy[10] = 60;

        let result = decoder.decode_coarse_energy(&mut range_decoder, false);
        assert!(result.is_ok());
        let energy = result.unwrap();
        assert_eq!(energy.len(), CELT_NUM_BANDS);
    }

    #[test]
    fn test_coarse_energy_clamping() {
        let data = vec![0xFF; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_coarse_energy(&mut range_decoder, true);
        assert!(result.is_ok());
        let energy = result.unwrap();

        for &e in &energy {
            assert!((-128..=127).contains(&e));
        }
    }

    #[test]
    fn test_fine_energy_no_bits() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let coarse_energy = [10_i16; CELT_NUM_BANDS];
        let fine_bits = [0_u8; CELT_NUM_BANDS];

        let result = decoder.decode_fine_energy(&mut range_decoder, &coarse_energy, &fine_bits);
        assert!(result.is_ok());
        let energy = result.unwrap();

        assert_eq!(energy, coarse_energy);
    }

    #[test]
    fn test_fine_energy_single_bit() {
        let data = vec![0xAA; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let coarse_energy = [10_i16; CELT_NUM_BANDS];
        let mut fine_bits = [0_u8; CELT_NUM_BANDS];
        fine_bits[0] = 1;

        let result = decoder.decode_fine_energy(&mut range_decoder, &coarse_energy, &fine_bits);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fine_energy_multiple_bits() {
        let data = vec![0x55; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let coarse_energy = [10_i16; CELT_NUM_BANDS];
        let fine_bits = [2_u8; CELT_NUM_BANDS];

        let result = decoder.decode_fine_energy(&mut range_decoder, &coarse_energy, &fine_bits);
        assert!(result.is_ok());
    }

    #[test]
    fn test_final_energy_priority_0() {
        let data = vec![0xFF; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let fine_energy = [10_i16; CELT_NUM_BANDS];
        let priorities = [0_u8; CELT_NUM_BANDS];

        let result = decoder.decode_final_energy(&mut range_decoder, &fine_energy, &priorities, 21);
        assert!(result.is_ok());
    }

    #[test]
    fn test_final_energy_both_priorities() {
        let data = vec![0xAA; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let fine_energy = [10_i16; CELT_NUM_BANDS];
        let mut priorities = [0_u8; CELT_NUM_BANDS];
        priorities[10] = 1;
        priorities[11] = 1;

        let result = decoder.decode_final_energy(&mut range_decoder, &fine_energy, &priorities, 30);
        assert!(result.is_ok());
    }

    #[test]
    fn test_final_energy_unused_bits_left() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let fine_energy = [10_i16; CELT_NUM_BANDS];
        let priorities = [0_u8; CELT_NUM_BANDS];

        let result = decoder.decode_final_energy(&mut range_decoder, &fine_energy, &priorities, 5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_decode_band_boost_no_budget() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let caps = [100_i32; CELT_NUM_BANDS];
        let result = decoder.decode_band_boost(&mut range_decoder, 10, &caps);
        assert!(result.is_ok());
        let (_boosts, total, consumed) = result.unwrap();
        assert!(total >= 0);
        assert!(consumed >= 0);
    }

    #[test]
    fn test_decode_band_boost_with_budget() {
        let data = vec![0xFF; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let caps = [10000_i32; CELT_NUM_BANDS];
        let result = decoder.decode_band_boost(&mut range_decoder, 5000, &caps);
        assert!(result.is_ok());
        let (_boosts, _total, consumed) = result.unwrap();
        assert!(consumed >= 0);
    }

    #[test]
    fn test_band_boost_quanta_formula() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        let bins = decoder.bins_per_band();

        // RFC line 6346: quanta = min(8*N, max(48, N))
        // Small N (N < 6): should be min(8*N, 48) = 8*N
        let n4 = i32::from(bins[0]); // N=4 for band 0 at 10ms
        if n4 == 4 {
            let quanta = (8 * n4).min(48.max(n4));
            assert_eq!(quanta, 32); // min(32, max(48,4)) = min(32,48) = 32
        } else {
            panic!(
                "expected duration 10ms, got {}ms",
                decoder.frame_duration_ms()
            );
        }
    }

    #[test]
    fn test_decode_allocation_trim_default() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_allocation_trim(&mut range_decoder, 5, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
    }

    #[test]
    fn test_decode_allocation_trim_with_bits() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_allocation_trim(&mut range_decoder, 100, 0);
        assert!(result.is_ok());
        let trim = result.unwrap();
        assert!(trim <= 10);
    }

    #[test]
    fn test_decode_stereo_params_mono() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let mut total_bits = 100;
        let result = decoder.decode_stereo_params(&mut range_decoder, 21, &mut total_bits);
        assert!(result.is_ok());
        let (intensity, dual) = result.unwrap();
        assert_eq!(intensity, 0);
        assert!(!dual);
    }

    #[test]
    fn test_decode_stereo_params_insufficient_bits() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 480).unwrap();

        let mut total_bits = 1;
        let result = decoder.decode_stereo_params(&mut range_decoder, 21, &mut total_bits);
        assert!(result.is_ok());
        let (intensity, dual) = result.unwrap();
        assert_eq!(intensity, 0);
        assert!(!dual);
    }

    #[test]
    fn test_compute_allocation_mono_basic() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(1000, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert_eq!(alloc.coded_bands, 21);
        assert!(alloc.shape_bits.iter().any(|&b| b > 0));
    }

    #[test]
    fn test_compute_allocation_stereo() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(2000, 2, 2, &boosts, 5, 0, 21, false);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert_eq!(alloc.coded_bands, 21);
        assert!(alloc.shape_bits.iter().sum::<i32>() > 0);
    }

    #[test]
    fn test_compute_allocation_with_boost() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let mut boosts = [0_i32; CELT_NUM_BANDS];
        boosts[5] = 100;
        boosts[10] = 200;

        let result = decoder.compute_allocation(1500, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert!(alloc.shape_bits[5] > 0 || alloc.shape_bits[10] > 0);
    }

    #[test]
    fn test_compute_allocation_low_rate() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(100, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert_eq!(alloc.coded_bands, 21);
    }

    #[test]
    fn test_compute_allocation_high_rate() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(10000, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert!(alloc.shape_bits.iter().all(|&b| b >= 0));
        assert!(alloc.shape_bits.iter().any(|&b| b > 100));
    }

    #[test]
    fn test_compute_allocation_trim_low() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(1000, 2, 1, &boosts, 0, 0, 21, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compute_allocation_trim_high() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(1000, 2, 1, &boosts, 10, 0, 21, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compute_allocation_partial_bands() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(500, 2, 1, &boosts, 5, 0, 15, false);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert_eq!(alloc.coded_bands, 15);
        assert!(alloc.shape_bits[15..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_compute_allocation_fine_energy_extraction() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(2000, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert!(alloc.fine_energy_bits.iter().any(|&b| b > 0));
        assert!(alloc.fine_priority.iter().all(|&p| p <= 1));
    }

    #[test]
    fn test_compute_allocation_transient_reservation() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        let boosts = [0_i32; CELT_NUM_BANDS];

        // Non-transient: no anti-collapse reservation
        let result1 = decoder.compute_allocation(1000, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result1.is_ok());
        let alloc1 = result1.unwrap();

        // Transient with LM>1: should reserve 8 eighth-bits (1 bit)
        let result2 = decoder.compute_allocation(1000, 2, 1, &boosts, 5, 0, 21, true);
        assert!(result2.is_ok());
        let alloc2 = result2.unwrap();

        // Both should succeed but transient has less allocation
        let total1: i32 = alloc1.shape_bits.iter().sum();
        let total2: i32 = alloc2.shape_bits.iter().sum();
        assert!(total1 >= total2);
    }

    #[test]
    fn test_compute_allocation_skip_reservation() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        let boosts = [0_i32; CELT_NUM_BANDS];

        // Low bitrate (total <= 8 eighth-bits after conservative subtraction): no skip reservation
        let result1 = decoder.compute_allocation(1, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result1.is_ok());

        // Normal bitrate (total > 8 eighth-bits): should reserve 8 eighth-bits
        let result2 = decoder.compute_allocation(10, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result2.is_ok());

        // Both should succeed
        let alloc1 = result1.unwrap();
        let alloc2 = result2.unwrap();
        assert_eq!(alloc1.coded_bands, 21);
        assert_eq!(alloc2.coded_bands, 21);
    }

    #[test]
    fn test_compute_allocation_conservative_subtraction() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        let boosts = [0_i32; CELT_NUM_BANDS];

        // The conservative subtraction removes 1 eighth-bit
        // This should still succeed and produce valid allocation
        let result = decoder.compute_allocation(1000, 2, 1, &boosts, 5, 0, 21, false);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        // Verify allocation is reasonable
        assert!(alloc.shape_bits.iter().sum::<i32>() > 0);
        assert!(alloc.shape_bits.iter().sum::<i32>() < 1000 * 8);
    }

    #[test]
    fn test_allocation_struct_creation() {
        let alloc = Allocation {
            shape_bits: [0; CELT_NUM_BANDS],
            fine_energy_bits: [0; CELT_NUM_BANDS],
            fine_priority: [0; CELT_NUM_BANDS],
            coded_bands: 21,
            balance: 0,
        };

        assert_eq!(alloc.coded_bands, 21);
        assert_eq!(alloc.balance, 0);
    }

    // Phase 4.5: Transient Processing Tests

    #[test]
    fn test_transient_flag_state_update() {
        let data = vec![0x00, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        assert!(!decoder.transient);
        let transient = decoder.decode_transient_flag(&mut range_decoder).unwrap();
        // State should be updated to match decoded value
        assert_eq!(decoder.transient, transient);
    }

    #[test]
    fn test_compute_lm() {
        let decoder_2_5ms = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 120).unwrap();
        assert_eq!(decoder_2_5ms.compute_lm(), 0);

        let decoder_5ms = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 240).unwrap();
        assert_eq!(decoder_5ms.compute_lm(), 1);

        let decoder_10ms = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        assert_eq!(decoder_10ms.compute_lm(), 2);

        let decoder_20ms = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
        assert_eq!(decoder_20ms.compute_lm(), 3);
    }

    #[test]
    fn test_should_decode_tf_select_with_actual_tf_change() {
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // LM=2 (10ms), transient=true
        // base_tf_0 = 2 (config 4), base_tf_1 = 1 (config 6)
        decoder.transient = true;

        // All tf_change=false: tf_0=2, tf_1=1 → different, should decode
        decoder.tf_change = vec![false; CELT_NUM_BANDS];
        assert!(decoder.should_decode_tf_select(0, CELT_NUM_BANDS));

        // All tf_change=true: tf_0=3→clamped to 2, tf_1=2 → SAME after clamping, should NOT decode
        decoder.tf_change = vec![true; CELT_NUM_BANDS];
        assert!(!decoder.should_decode_tf_select(0, CELT_NUM_BANDS));

        // Mixed tf_change: at least one band differs → should decode
        let mut mixed = vec![true; CELT_NUM_BANDS];
        mixed[0] = false; // Band 0: tf_change=false → tf_0=2, tf_1=1 → different
        decoder.tf_change = mixed;
        assert!(decoder.should_decode_tf_select(0, CELT_NUM_BANDS));

        // LM=2, transient=false
        // base_tf_0 = 0 (config 0), base_tf_1 = 0 (config 2) → same
        decoder.transient = false;
        decoder.tf_change = vec![false; CELT_NUM_BANDS];
        assert!(!decoder.should_decode_tf_select(0, CELT_NUM_BANDS));

        decoder.tf_change = vec![true; CELT_NUM_BANDS];
        assert!(!decoder.should_decode_tf_select(0, CELT_NUM_BANDS));
    }

    #[test]
    fn test_tf_select_conditional_decoding() {
        let data = vec![0xFF; 64];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // Must decode tf_change first per RFC Table 56
        decoder.transient = true; // LM=2, transient=true
        decoder
            .decode_tf_changes(&mut range_decoder, 0, CELT_NUM_BANDS)
            .unwrap();

        // Now decode tf_select - should decode since it affects result for LM=2, transient=true
        let result = decoder
            .decode_tf_select(&mut range_decoder, 0, CELT_NUM_BANDS)
            .unwrap();
        assert!(result.is_some());
        assert_eq!(decoder.tf_select, result);
    }

    #[test]
    fn test_tf_select_error_without_tf_change() {
        let data = vec![0xFF; 64];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // Attempt to decode tf_select without decoding tf_change first
        let result = decoder.decode_tf_select(&mut range_decoder, 0, CELT_NUM_BANDS);
        assert!(result.is_err());
    }

    #[test]
    fn test_tf_change_decoding() {
        // Need enough data for 21 bands
        let data = vec![0xFF; 64];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let tf_changes = decoder
            .decode_tf_changes(&mut range_decoder, 0, CELT_NUM_BANDS)
            .unwrap();

        assert_eq!(tf_changes.len(), CELT_NUM_BANDS);
        assert_eq!(decoder.tf_change.len(), CELT_NUM_BANDS);
    }

    #[test]
    fn test_tf_change_pdf_first_band_transient() {
        let pdf = CeltDecoder::compute_tf_change_pdf(true, true);
        assert_eq!(pdf, vec![4, 1, 0]); // {3,1}/4 from RFC
    }

    #[test]
    fn test_tf_change_pdf_first_band_normal() {
        let pdf = CeltDecoder::compute_tf_change_pdf(true, false);
        assert_eq!(pdf, vec![16, 1, 0]); // {15,1}/16 from RFC
    }

    #[test]
    fn test_tf_change_pdf_subsequent_transient() {
        let pdf = CeltDecoder::compute_tf_change_pdf(false, true);
        assert_eq!(pdf, vec![16, 1, 0]); // {15,1}/16 for delta
    }

    #[test]
    fn test_tf_change_pdf_subsequent_normal() {
        let pdf = CeltDecoder::compute_tf_change_pdf(false, false);
        assert_eq!(pdf, vec![32, 1, 0]); // {31,1}/32 for delta
    }

    #[test]
    fn test_tf_resolution_computation() {
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // LM=2 (10ms), transient=0, tf_select=0
        decoder.transient = false;
        decoder.tf_select = Some(0);
        decoder.tf_change = vec![false; CELT_NUM_BANDS];

        let tf_res = decoder.compute_tf_resolution().unwrap();

        assert_eq!(tf_res.len(), CELT_NUM_BANDS);

        // Base TF for LM=2, normal, tf_select=0 is 0 (from TF_SELECT_TABLE)
        assert_eq!(tf_res[0], 0);

        // All bands should have same resolution (no tf_change)
        assert!(tf_res.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_tf_resolution_with_changes() {
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        decoder.transient = false;
        decoder.tf_select = Some(0);

        // Set tf_change for first band
        let mut tf_change = vec![false; CELT_NUM_BANDS];
        tf_change[0] = true;
        decoder.tf_change = tf_change;

        let tf_res = decoder.compute_tf_resolution().unwrap();

        // LM=2 (10ms), non-transient, tf_select=0
        // tf_change=0 → 0, tf_change=1 → -2 (clamped to 0)
        assert_eq!(tf_res[0], 0);
        assert_eq!(tf_res[1], 0);
    }

    #[test]
    fn test_tf_resolution_clamping() {
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // LM=2, max resolution is 2
        decoder.transient = false;
        decoder.tf_select = Some(0);
        decoder.tf_change = vec![true; CELT_NUM_BANDS];

        let tf_res = decoder.compute_tf_resolution().unwrap();

        // All resolutions should be clamped to [0, 2]
        assert!(tf_res.iter().all(|&x| x <= 2));
    }

    #[test]
    fn test_tf_resolution_error_without_tf_change() {
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // Try to compute resolution without decoding tf_change first
        let result = decoder.compute_tf_resolution();
        assert!(result.is_err());
    }

    #[test]
    fn test_tf_select_table_dimensions() {
        use crate::celt::constants::TF_SELECT_TABLE;

        // Verify 4D table structure
        assert_eq!(TF_SELECT_TABLE.len(), 4); // 4 LM values
        for lm_table in &TF_SELECT_TABLE {
            assert_eq!(lm_table.len(), 2); // 2 transient modes
            for trans_table in lm_table {
                assert_eq!(trans_table.len(), 2); // 2 tf_select values
                for sel_table in trans_table {
                    assert_eq!(sel_table.len(), 2); // 2 tf_change values
                }
            }
        }
    }

    #[test]
    fn test_rfc_table_60_non_transient_tf_select_0() {
        use crate::celt::constants::TF_SELECT_TABLE;

        // Table 60: Non-transient, tf_select=0
        assert_eq!(TF_SELECT_TABLE[0][0][0], [0, -1]); // 2.5ms
        assert_eq!(TF_SELECT_TABLE[1][0][0], [0, -1]); // 5ms
        assert_eq!(TF_SELECT_TABLE[2][0][0], [0, -2]); // 10ms
        assert_eq!(TF_SELECT_TABLE[3][0][0], [0, -2]); // 20ms
    }

    #[test]
    fn test_rfc_table_61_non_transient_tf_select_1() {
        use crate::celt::constants::TF_SELECT_TABLE;

        // Table 61: Non-transient, tf_select=1
        assert_eq!(TF_SELECT_TABLE[0][0][1], [0, -1]); // 2.5ms
        assert_eq!(TF_SELECT_TABLE[1][0][1], [0, -2]); // 5ms
        assert_eq!(TF_SELECT_TABLE[2][0][1], [0, -3]); // 10ms
        assert_eq!(TF_SELECT_TABLE[3][0][1], [0, -3]); // 20ms
    }

    #[test]
    fn test_rfc_table_62_transient_tf_select_0() {
        use crate::celt::constants::TF_SELECT_TABLE;

        // Table 62: Transient, tf_select=0
        assert_eq!(TF_SELECT_TABLE[0][1][0], [0, -1]); // 2.5ms
        assert_eq!(TF_SELECT_TABLE[1][1][0], [1, 0]); // 5ms
        assert_eq!(TF_SELECT_TABLE[2][1][0], [2, 0]); // 10ms
        assert_eq!(TF_SELECT_TABLE[3][1][0], [3, 0]); // 20ms
    }

    #[test]
    fn test_tf_change_partial_bands() {
        // Test decoding only bands 0..15 (narrowband cutoff)
        let data = vec![0xFF; 64];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        decoder.transient = false;

        // Decode only first 15 bands
        let tf_changes = decoder
            .decode_tf_changes(&mut range_decoder, 0, 15)
            .unwrap();

        // All bands should be initialized
        assert_eq!(tf_changes.len(), CELT_NUM_BANDS);
        assert_eq!(decoder.tf_change.len(), CELT_NUM_BANDS);

        // Bands 15..21 should be false (not decoded)
        for band in tf_changes.iter().take(CELT_NUM_BANDS).skip(15) {
            assert!(!band);
        }
    }

    #[test]
    fn test_tf_change_with_start_offset() {
        // Test decoding bands 17..21 (narrowband mode where start=17)
        let data = vec![0xFF; 64];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        decoder.transient = true;

        // Decode only bands 17..21
        let tf_changes = decoder
            .decode_tf_changes(&mut range_decoder, 17, 21)
            .unwrap();

        // Bands 0..17 should be false (not decoded)
        for band in tf_changes.iter().take(17) {
            assert!(!band);
        }
    }

    #[test]
    fn test_tf_change_validation_start_gte_end() {
        let data = vec![0xFF; 64];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // start >= end should fail
        let result = decoder.decode_tf_changes(&mut range_decoder, 10, 10);
        assert!(result.is_err());

        let result = decoder.decode_tf_changes(&mut range_decoder, 15, 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_tf_change_validation_end_exceeds_num_bands() {
        let data = vec![0xFF; 64];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // end > CELT_NUM_BANDS should fail
        let result = decoder.decode_tf_changes(&mut range_decoder, 0, 25);
        assert!(result.is_err());
    }

    #[test]
    fn test_rfc_table_63_transient_tf_select_1() {
        use crate::celt::constants::TF_SELECT_TABLE;

        // Table 63: Transient, tf_select=1
        assert_eq!(TF_SELECT_TABLE[0][1][1], [0, -1]); // 2.5ms
        assert_eq!(TF_SELECT_TABLE[1][1][1], [1, -1]); // 5ms
        assert_eq!(TF_SELECT_TABLE[2][1][1], [1, -1]); // 10ms
        assert_eq!(TF_SELECT_TABLE[3][1][1], [1, -1]); // 20ms
    }

    #[test]
    fn test_tf_resolution_rfc_compliance() {
        // Test that compute_tf_resolution uses table lookup correctly
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 240).unwrap();

        // LM=1 (5ms), transient, tf_select=0, tf_change=[false, true, ...]
        decoder.transient = true;
        decoder.tf_select = Some(0);
        decoder.tf_change = vec![false; CELT_NUM_BANDS];
        decoder.tf_change[1] = true;

        let tf_res = decoder.compute_tf_resolution().unwrap();

        // From RFC Table 62: LM=1, transient, tf_select=0 → [1, 0]
        assert_eq!(tf_res[0], 1); // tf_change=0 → 1
        assert_eq!(tf_res[1], 0); // tf_change=1 → 0
    }
}
