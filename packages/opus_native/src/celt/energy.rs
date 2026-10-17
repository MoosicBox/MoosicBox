//! Full-precision CELT log-energy pipeline. Q8 APIs in decoder.rs are legacy only.
use super::{
    CELT_NUM_BANDS, CeltDecoder, CeltNorm, CeltSig, Channels, Error, RangeDecoder, Result,
    mult16_32_q15,
};

const ONE: i32 = 1 << 24;
const FLOOR: i32 = -28 * ONE;
const MEANS_Q4: [i8; CELT_NUM_BANDS] = [
    103, 100, 92, 85, 81, 77, 72, 70, 78, 75, 73, 71, 78, 74, 69, 72, 70, 74, 76, 71, 60,
];

/// Persistent per-channel histories, with two slots also for mono as in libopus.
pub(super) struct EnergyHistory {
    pub band: [i32; 2 * CELT_NUM_BANDS],
    pub log: [i32; 2 * CELT_NUM_BANDS],
    pub log2: [i32; 2 * CELT_NUM_BANDS],
}

impl EnergyHistory {
    pub const fn new() -> Self {
        Self {
            band: [0; 2 * CELT_NUM_BANDS],
            log: [FLOOR; 2 * CELT_NUM_BANDS],
            log2: [FLOOR; 2 * CELT_NUM_BANDS],
        }
    }

    pub fn update(
        &mut self,
        mut energy: Vec<i32>,
        channels: usize,
        transient: bool,
        start: usize,
        end: usize,
    ) {
        if channels == 1 {
            energy.extend_from_within(..CELT_NUM_BANDS);
        }
        self.band.copy_from_slice(&energy);
        if transient {
            for (log, &value) in self.log.iter_mut().zip(&energy) {
                *log = (*log).min(value);
            }
        } else {
            self.log2 = self.log;
            self.log.copy_from_slice(&energy);
        }
        for channel in 0..2 {
            for band in 0..CELT_NUM_BANDS {
                if !(start..end).contains(&band) {
                    let index = channel * CELT_NUM_BANDS + band;
                    self.band[index] = 0;
                    self.log[index] = FLOOR;
                    self.log2[index] = FLOOR;
                }
            }
        }
    }
}

impl CeltDecoder {
    fn validate_energy_q24(&self, energy: &[i32], fine: &[u8; CELT_NUM_BANDS]) -> Result<usize> {
        let channels = if self.channels == Channels::Stereo {
            2
        } else {
            1
        };
        if self.start_band > self.end_band
            || self.end_band > CELT_NUM_BANDS
            || energy.len() != channels * CELT_NUM_BANDS
            || fine.iter().any(|&bits| bits > 8)
        {
            return Err(Error::CeltDecoder(
                "invalid Q24 energy bands, history, or fine precision".into(),
            ));
        }
        Ok(channels)
    }

    /// Refines channel-major Q24 energies with raw fine-energy bits.
    ///
    /// # Errors
    /// * Returns an error for invalid dimensions, bands, precision (>8), or entropy failure.
    pub fn decode_fine_energy_q24(
        &self,
        rd: &mut RangeDecoder,
        energy: &mut [i32],
        fine: &[u8; CELT_NUM_BANDS],
    ) -> Result<()> {
        let channels = self.validate_energy_q24(energy, fine)?;
        for (band, &bits) in fine
            .iter()
            .enumerate()
            .take(self.end_band)
            .skip(self.start_band)
        {
            if bits == 0 {
                continue;
            }
            for channel in 0..channels {
                let value = rd.ec_dec_bits(u32::from(bits))? as i32;
                let offset = (value << (24 - bits)) + (ONE >> (bits + 1));
                energy[channel * CELT_NUM_BANDS + band] += offset - ONE / 2;
            }
        }
        Ok(())
    }

    /// Allocates remaining raw bits to priority bands, preserving Q24 energies.
    ///
    /// # Errors
    /// * Returns an error for invalid dimensions, bands, precision (>8), or entropy failure.
    pub fn decode_final_energy_q24(
        &self,
        rd: &mut RangeDecoder,
        energy: &mut [i32],
        fine: &[u8; CELT_NUM_BANDS],
        priority: &[u8; CELT_NUM_BANDS],
        mut bits_left: usize,
    ) -> Result<()> {
        let channels = self.validate_energy_q24(energy, fine)?;
        for prio in 0..2 {
            for band in self.start_band..self.end_band {
                if bits_left < channels {
                    break;
                }
                if fine[band] >= 8 || priority[band] != prio {
                    continue;
                }
                for channel in 0..channels {
                    let value = i32::from(rd.ec_dec_bits(1)? != 0);
                    let offset = ((value << 24) - ONE / 2) >> (fine[band] + 1);
                    energy[channel * CELT_NUM_BANDS + band] += offset;
                    bits_left -= 1;
                }
            }
        }
        Ok(())
    }

    /// Converts a single channel's normalized Q14 spectrum and Q24 log energies
    /// into Q12 synthesis coefficients, including band means and output bound.
    ///
    /// # Errors
    /// * Returns an error if dimensions or configured band bounds are invalid.
    pub fn denormalize_bands_q24(
        &self,
        spectrum: &[i16],
        energy: &[i32],
        silence: bool,
    ) -> Result<Vec<CeltSig>> {
        const EDGES: [usize; 22] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 20, 24, 28, 34, 40, 48, 60, 78, 100,
        ];
        let m = 1 << self.frame_duration_index();
        let n = 120 * m;
        if spectrum.len() != n
            || energy.len() != CELT_NUM_BANDS
            || self.start_band > self.end_band
            || self.end_band > CELT_NUM_BANDS
        {
            return Err(Error::CeltDecoder(
                "invalid Q24 denormalization dimensions or bands".into(),
            ));
        }
        let mut freq = vec![0; n];
        if silence {
            return Ok(freq);
        }
        for band in self.start_band..self.end_band {
            let lg = energy[band].wrapping_add(i32::from(MEANS_Q4[band]) << 20);
            let mut shift = 15 - (lg >> 24);
            let gain = if shift > 31 {
                shift = 0;
                0
            } else {
                exp2_fraction_q24(lg & (ONE - 1))
            };
            let gain = if shift <= -2 {
                shift = -2;
                16384 * 32768
            } else {
                gain
            };
            for bin in m * EDGES[band]..m * EDGES[band + 1] {
                let value = mult16_32_q15(spectrum[bin], gain);
                freq[bin] = if shift < 0 {
                    value.wrapping_shl(shift.unsigned_abs())
                } else {
                    value >> shift
                };
            }
        }
        let bound = (m * EDGES[self.end_band]).min(n / self.downsample as usize);
        freq[bound..].fill(0);
        Ok(freq)
    }
}

// Bundled non-QEXT mathops.h: celt_exp2_db_frac(x) expands to Q10-rounded
// celt_exp2_frac(x) << 14. Keeping Q24 energy up to this specified operation is
// essential: pre-rounding to Q8 changes both the fraction and integer boundary.
fn exp2_fraction_q24(x: i32) -> i32 {
    let frac = (((x + (1 << 13)) >> 14) << 4) as i16;
    let mul = |a: i16, b: i16| ((i32::from(a) * i32::from(b)) >> 15) as i16;
    let value = 16383_i16.wrapping_add(mul(
        frac,
        22804_i16.wrapping_add(mul(frac, 14819_i16.wrapping_add(mul(10204, frac)))),
    ));
    i32::from(value) << 14
}

impl CeltDecoder {
    pub(super) fn apply_anti_collapse_q24(
        &mut self,
        bands: &mut [Vec<CeltNorm>],
        current_energy: &[i32],
        channel: usize,
        collapse_masks: &[u8],
        pulses: &[u16; CELT_NUM_BANDS],
        anti_collapse_on: bool,
    ) -> Result<()> {
        use crate::celt::fixed_point::{mult16_16, mult16_32_q15, qconst16, shl32, shr16, shr32};

        if !anti_collapse_on {
            return Ok(());
        }

        let lm = self.compute_lm();
        let num_mdcts = 1_usize << lm; // 2^LM MDCTs per band

        log::trace!("apply_anti_collapse: lm={lm}, num_mdcts={num_mdcts}");

        // Process only coded bands [start_band, end_band)
        for band_idx in self.start_band..self.end_band {
            if band_idx >= CELT_NUM_BANDS {
                break;
            }

            let band = &mut bands[band_idx];
            if band.is_empty() {
                continue;
            }

            // N0 = bins per single MDCT (from bins_per_band table)
            // libopus: N0 = m->eBands[i+1] - m->eBands[i]
            let n0 = usize::from(crate::celt::constants::CELT_BINS_2_5MS[band_idx]);

            // Total band size must be N0 << LM
            let expected_size = n0 << lm;
            if band.len() != expected_size {
                return Err(Error::CeltDecoder(format!(
                    "Band {} size mismatch: expected {} (N0={} << LM={}), got {}",
                    band_idx,
                    expected_size,
                    n0,
                    lm,
                    band.len()
                )));
            }

            let collapse_mask = collapse_masks[band_idx];

            // Compute depth: (1 + pulses[i]) / N0 >> LM
            // libopus bands.c:301: depth = celt_udiv(1+pulses[i], N0)>>LM
            let depth = ((1 + u32::from(pulses[band_idx])) / (n0 as u32)) >> lm;

            // Threshold computation (libopus bands.c:304-305 FIXED_POINT path)
            // thresh32 = SHR32(celt_exp2(-SHL16(depth, 10-BITRES)),1)
            // BITRES=3, so depth/8 in the Q10 exponent.
            #[allow(clippy::cast_possible_truncation)]
            let depth_scaled = -(depth as i16).wrapping_shl(7); // BITRES=3, Q10 input
            let thresh32 = shr32(exp2_q10(depth_scaled), 1);

            // thresh = MULT16_32_Q15(QCONST16(0.5f, 15), MIN32(32767,thresh32))
            let half_q15 = qconst16(0.5, 15);
            let thresh32_clamped = thresh32.min(32767);
            let thresh = mult16_32_q15(half_q15, thresh32_clamped) as i16;

            // Compute sqrt_1 = 1/sqrt(N0<<LM) (libopus bands.c:307-312)
            let t = (n0 << lm) as i32;
            let shift = i32::from(crate::celt::fixed_point::celt_ilog2(t) >> 1);
            let t_scaled = shl32(t, (7 - shift) << 1);
            let sqrt_1 = super::residual::residual_rsqrt_norm(t_scaled);

            log::trace!(
                "Band {band_idx}: depth={depth}, thresh={thresh}, sqrt_1={sqrt_1}, shift={shift}"
            );

            // Get previous energies (Q24 format)
            let prev1 = self.energy_q24.log[channel * CELT_NUM_BANDS + band_idx];
            let prev2 = self.energy_q24.log2[channel * CELT_NUM_BANDS + band_idx];

            // Energy difference: current - MIN(prev1, prev2)
            // libopus bands.c:333: Ediff = logE[c*m->nbEBands+i]-MING(prev1,prev2)
            let current_q24 = current_energy[channel * CELT_NUM_BANDS + band_idx];
            let min_prev_q24 = prev1.min(prev2);
            let ediff_q24 = current_q24 - min_prev_q24;
            let ediff_q24 = ediff_q24.max(0); // libopus bands.c:334: MAX32(0, Ediff)

            // Compute r (libopus bands.c:336-347 FIXED_POINT path)
            let r = if ediff_q24 < 16 * ONE {
                // r32 = SHR32(celt_exp2_db(-Ediff),1)
                let exponent = ((-ediff_q24 + (1 << 13)) >> 14) as i16;
                let r32 = shr32(exp2_q10(exponent), 1);
                // r = 2*MIN16(16383,r32)
                let mut r_val = 2 * (r32 as i16).min(16383);

                // if (LM==3) r = MULT16_16_Q14(23170, MIN32(23169, r))
                // 23170 in Q14 = sqrt(2), 23169 is just below that
                if lm == 3 {
                    let sqrt2_q14 = 23170_i16;
                    let r_clamped = r_val.min(23169);
                    // MULT16_16_Q14: (a * b) >> 14
                    r_val = ((i32::from(sqrt2_q14) * i32::from(r_clamped)) >> 14) as i16;
                }

                // r = SHR16(MIN16(thresh, r),1)
                r_val = shr16(r_val.min(thresh), 1);

                // r = SHR32(MULT16_16_Q15(sqrt_1, r),shift)
                let r_scaled = mult16_16(sqrt_1, r_val) >> 15;
                shr32(r_scaled, shift) as i16
            } else {
                0 // Energy difference too large, no injection
            };

            log::trace!("Band {band_idx}: ediff_q24={ediff_q24}, r={r}");

            let mut renormalize = false;

            // RFC line 6717: "For each band of each MDCT"
            // libopus bands.c:358-371: for (k=0;k<1<<LM;k++)
            for k in 0..num_mdcts {
                // Check bit k of collapse mask
                // libopus bands.c:361: if (!(collapse_masks[i*C+c]&1<<k))
                if (collapse_mask & (1_u8 << k)) == 0 {
                    // MDCT k collapsed - inject pseudo-random noise
                    log::trace!("Band {band_idx}, MDCT {k}: collapsed, injecting noise");

                    // Fill only this MDCT with noise
                    // libopus bands.c:364-368: for (j=0;j<N0;j++) X[(j<<LM)+k] = ...
                    for j in 0..n0 {
                        // Interleaved index: (j<<LM) + k
                        let idx = (j << lm) + k;

                        // Use anti-collapse PRNG
                        let random = self.state.anti_collapse_state.next_random();

                        // libopus bands.c:367: X[(j<<LM)+k] = (seed & 0x8000 ? r : -r)
                        band[idx] = if (random & 0x8000) != 0 { r } else { -r };
                    }

                    renormalize = true;
                }
            }

            // Renormalize band to preserve total energy
            // libopus bands.c:374: if (renormalize) renormalise_vector(X, N0<<LM, Q31ONE, arch)
            // Note: LibOpus passes Q31ONE but operates on Q15 data - the gain is in Q31 for precision
            if renormalize {
                log::trace!("Band {band_idx}: renormalizing after anti-collapse");
                let energy = 1 + band
                    .iter()
                    .map(|&v| i32::from(v) * i32::from(v))
                    .sum::<i32>();
                super::mono::normalize(band, energy, 0x7FFF_FFFF);
            }
        }

        Ok(())
    }
}

fn exp2_q10(x: i16) -> i32 {
    let integer = i32::from(x >> 10);
    if integer > 14 {
        return 0x7f00_0000;
    }
    if integer < -15 {
        return 0;
    }
    let frac = i32::from(x) - integer * 1024;
    let polynomial = exp2_fraction_q24(frac << 14) >> 14;
    crate::celt::fixed_point::vshr32(polynomial, -integer - 2)
}
