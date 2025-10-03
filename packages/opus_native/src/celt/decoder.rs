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

    /// Decodes transient flag (RFC Table 56)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_transient(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        let value = range_decoder.ec_dec_icdf(CELT_TRANSIENT_PDF, 8)?;
        Ok(value == 1)
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
    /// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/celt.c#L2473-2505>
    pub fn decode_band_boost(
        &self,
        range_decoder: &mut RangeDecoder,
        total_bits: i32,
        caps: &[i32; CELT_NUM_BANDS],
    ) -> Result<([i32; CELT_NUM_BANDS], i32)> {
        let bins = self.bins_per_band();
        let mut boosts = [0_i32; CELT_NUM_BANDS];
        let mut total_boost = 0_i32;
        let mut dynalloc_logp = 6; // Initial cost: 6 bits

        for band in 0..CELT_NUM_BANDS {
            let n = i32::from(bins[band]);
            // Boost quanta: min(8*N, max(48, N)) in 1/8 bit units
            let quanta = n.min(8 * n).max(48);

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
                dynalloc_loop_logp = 1; // Subsequent bits cost only 1 bit
            }

            boosts[band] = boost;

            // Reduce initial cost if we used this band (minimum 2 bits)
            if boost > 0 && dynalloc_logp > 2 {
                dynalloc_logp -= 1;
            }
        }

        Ok((boosts, total_boost))
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
    ) -> Result<Allocation> {
        let bins = self.bins_per_band();
        let mut shape_bits = [0_i32; CELT_NUM_BANDS];
        let mut fine_energy_bits = [0_u8; CELT_NUM_BANDS];
        let mut fine_priority = [0_u8; CELT_NUM_BANDS];

        let c = i32::try_from(channels).unwrap_or(1);
        let lm_i32 = i32::from(lm);
        let alloc_trim = i32::from(trim);

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

            if psum > total_bits * 8 {
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

            if psum > total_bits * 8 {
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

        let left = (total_bits * 8).saturating_sub(psum);
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
    fn test_transient_flag_decoding() {
        let data = vec![0x80, 0x00, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_transient(&mut range_decoder);
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
        let (_boosts, total) = result.unwrap();
        assert!(total >= 0);
    }

    #[test]
    fn test_decode_band_boost_with_budget() {
        let data = vec![0xFF; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let caps = [10000_i32; CELT_NUM_BANDS];
        let result = decoder.decode_band_boost(&mut range_decoder, 5000, &caps);
        assert!(result.is_ok());
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
        let result = decoder.compute_allocation(1000, 2, 1, &boosts, 5, 0, 21);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert_eq!(alloc.coded_bands, 21);
        assert!(alloc.shape_bits.iter().any(|&b| b > 0));
    }

    #[test]
    fn test_compute_allocation_stereo() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(2000, 2, 2, &boosts, 5, 0, 21);
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

        let result = decoder.compute_allocation(1500, 2, 1, &boosts, 5, 0, 21);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert!(alloc.shape_bits[5] > 0 || alloc.shape_bits[10] > 0);
    }

    #[test]
    fn test_compute_allocation_low_rate() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(100, 2, 1, &boosts, 5, 0, 21);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert_eq!(alloc.coded_bands, 21);
    }

    #[test]
    fn test_compute_allocation_high_rate() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(10000, 2, 1, &boosts, 5, 0, 21);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert!(alloc.shape_bits.iter().all(|&b| b >= 0));
        assert!(alloc.shape_bits.iter().any(|&b| b > 100));
    }

    #[test]
    fn test_compute_allocation_trim_low() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(1000, 2, 1, &boosts, 0, 0, 21);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compute_allocation_trim_high() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(1000, 2, 1, &boosts, 10, 0, 21);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compute_allocation_partial_bands() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(500, 2, 1, &boosts, 5, 0, 15);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert_eq!(alloc.coded_bands, 15);
        assert!(alloc.shape_bits[15..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_compute_allocation_fine_energy_extraction() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let boosts = [0_i32; CELT_NUM_BANDS];
        let result = decoder.compute_allocation(2000, 2, 1, &boosts, 5, 0, 21);
        assert!(result.is_ok());

        let alloc = result.unwrap();
        assert!(alloc.fine_energy_bits.iter().any(|&b| b > 0));
        assert!(alloc.fine_priority.iter().all(|&p| p <= 1));
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
}
