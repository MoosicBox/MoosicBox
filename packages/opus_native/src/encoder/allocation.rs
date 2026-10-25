//! Integer CELT bit allocation, corresponding to libopus `rate.c`.
use super::entropy;
use crate::Result;
use crate::celt::constants::{ALLOCATION_TABLE, CACHE_CAPS50, CELT_BINS_2_5MS, LOG2_FRAC_TABLE};
use crate::range::RangeEncoder;

pub(super) struct Allocation {
    pub bits: [i32; 21],
    pub fine: [u8; 21],
    pub priority: [u8; 21],
    pub coded: usize,
    pub balance: i32,
    pub intensity: usize,
    pub dual: bool,
}

/// Keep independently pitched stereo bands out of intensity coding. The
/// baseline boundary is a rate decision; raise it past bands whose normalized
/// squared correlation is below 3/4. Anti-correlated shapes remain eligible:
/// intensity coding carries their inversion flag. Quiet bands do not force a
/// boundary increase. Decisions use scaled integer powers to avoid overflow.
#[must_use]
pub(super) fn stereo_intensity(spectrum: &[Vec<i32>], lm: usize, baseline: usize) -> usize {
    if spectrum.len() != 2 {
        return 21;
    }
    let floor = spectrum
        .iter()
        .map(|channel| {
            channel
                .iter()
                .map(|&value| {
                    let value = i128::from(value >> 8);
                    value * value
                })
                .sum::<i128>()
        })
        .max()
        .unwrap_or(0)
        / 1024;
    let mut boundary = baseline;
    let mut offset = CELT_BINS_2_5MS[..baseline]
        .iter()
        .map(|&width| usize::from(width) << lm)
        .sum::<usize>();
    for (band, &width) in CELT_BINS_2_5MS.iter().enumerate().skip(baseline) {
        let n = usize::from(width) << lm;
        let left = &spectrum[0][offset..offset + n];
        let right = &spectrum[1][offset..offset + n];
        let mut powers = [0_i128; 2];
        let mut dot = 0_i128;
        for (&left, &right) in left.iter().zip(right) {
            let left = i128::from(left >> 8);
            let right = i128::from(right >> 8);
            powers[0] += left * left;
            powers[1] += right * right;
            dot += left * right;
        }
        // Bands below -30 dB of frame energy do not determine stereo shape.
        if powers[0].min(powers[1]) > floor.max(256) && 4 * dot * dot < 3 * powers[0] * powers[1] {
            boundary = band + 1;
        }
        offset += n;
    }
    boundary
}

// Long-window CELT, neutral trim, no dynamic boosts. The legacy entry point
// retains independent stereo throughout for existing hybrid callers.
#[allow(clippy::too_many_lines)]
pub(super) fn allocate(
    enc: &mut RangeEncoder,
    total: i32,
    channels: usize,
    lm: i32,
    start: usize,
) -> Result<Allocation> {
    allocate_celt(enc, total, channels, lm, start, 21)
}

/// Allocate neutral-trim CELT bits with independent low bands and a shared
/// intensity shape above `intensity`. All budgets use eighth-bit units.
#[allow(clippy::too_many_lines)]
pub(super) fn allocate_celt(
    enc: &mut RangeEncoder,
    total: i32,
    channels: usize,
    lm: i32,
    start: usize,
    intensity: usize,
) -> Result<Allocation> {
    allocate_celt_end(enc, total, channels, lm, start, 21, intensity)
}

/// Allocate a CELT band range, including wideband redundancy (`end = 17`).
/// Bands outside `start..end` receive no shape or fine-energy bits.
///
/// # Errors
/// * Returns an error for invalid codec dimensions or entropy coding failure.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn allocate_celt_end(
    enc: &mut RangeEncoder,
    total: i32,
    channels: usize,
    lm: i32,
    start: usize,
    end: usize,
    intensity: usize,
) -> Result<Allocation> {
    allocate_stereo(enc, total, channels, lm, start, end, intensity, true)
}

/// Joint mid/side allocation. Shapes must be encoded with `quantize::stereo_band`
/// below intensity and `quantize::intensity_band` above it.
#[allow(clippy::too_many_arguments)]
pub(super) fn allocate_joint(
    enc: &mut RangeEncoder,
    total: i32,
    channels: usize,
    lm: i32,
    start: usize,
    end: usize,
    intensity: usize,
) -> Result<Allocation> {
    allocate_stereo(enc, total, channels, lm, start, end, intensity, false)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn allocate_stereo(
    enc: &mut RangeEncoder,
    mut total: i32,
    channels: usize,
    lm: i32,
    start: usize,
    end: usize,
    intensity: usize,
    independent: bool,
) -> Result<Allocation> {
    if start >= end || end > 21 || !(1..=2).contains(&channels) || !(0..=3).contains(&lm) {
        return Err(crate::Error::Unsupported(
            "invalid CELT allocation dimensions".into(),
        ));
    }
    let c = channels as i32;
    total = total.max(0);
    let stereo = c - 1;
    let skip = i32::from(total >= 8) * 8;
    total -= skip;
    let mut intensity_rsv = 0;
    let mut dual_rsv = 0;
    if c == 2 {
        intensity_rsv = i32::from(LOG2_FRAC_TABLE[end - start]);
        if intensity_rsv > total {
            intensity_rsv = 0;
        } else {
            total -= intensity_rsv;
            dual_rsv = i32::from(total >= 8) * 8;
            total -= dual_rsv;
        }
    }
    let widths = CELT_BINS_2_5MS.map(i32::from);
    let caps: [i32; 21] = std::array::from_fn(|j| {
        ((i32::from(CACHE_CAPS50[21 * (2 * lm as usize + channels - 1) + j]) + 64)
            * c
            * (widths[j] << lm))
            >> 2
    });
    let mut edges = [0; 22];
    for j in 0..21 {
        edges[j + 1] = edges[j] + widths[j];
    }
    let thresh: [i32; 21] = std::array::from_fn(|j| (c * 8).max(((3 * widths[j]) << lm << 3) >> 4));
    let tilt: [i32; 21] = std::array::from_fn(|j| {
        if j < start || j >= end {
            return 0;
        }
        ((c * widths[j] * (-lm) * (end - j - 1) as i32 * (1 << (lm + 3))) >> 6)
            - if widths[j] << lm == 1 { c * 8 } else { 0 }
    });
    let curve = |j: usize, q: usize| {
        if j < start || j >= end {
            return 0;
        }
        let bits = (c * widths[j] * i32::from(ALLOCATION_TABLE[j][q])) << lm >> 2;
        if bits > 0 {
            (bits + tilt[j]).max(0)
        } else {
            bits
        }
    };
    let mut low = 1_i32;
    let mut high = 10_i32;
    while low <= high {
        let mid = low.midpoint(high);
        let mut sum = 0;
        let mut done = false;
        for j in (start..end).rev() {
            let bits = curve(j, mid as usize);
            if bits >= thresh[j] || done {
                done = true;
                sum += bits.min(caps[j]);
            } else if bits >= c * 8 {
                sum += c * 8;
            }
        }
        if sum > total {
            high = mid - 1;
        } else {
            low = mid + 1;
        }
    }
    let high_quality = low as usize;
    let low_quality = (low - 1) as usize;
    let a: [i32; 21] = std::array::from_fn(|j| curve(j, low_quality));
    let delta: [i32; 21] = std::array::from_fn(|j| {
        let upper = if high_quality >= 11 {
            (caps[j] + tilt[j]).max(0)
        } else {
            curve(j, high_quality)
        };
        (upper - a[j]).max(0)
    });
    low = 0;
    high = 64;
    for _ in 0..6 {
        let mid = low.midpoint(high);
        let mut sum = 0;
        let mut done = false;
        for j in (start..end).rev() {
            let bits = a[j] + ((mid * delta[j]) >> 6);
            if bits >= thresh[j] || done {
                done = true;
                sum += bits.min(caps[j]);
            } else if bits >= c * 8 {
                sum += c * 8;
            }
        }
        if sum > total {
            high = mid;
        } else {
            low = mid;
        }
    }
    let mut bits = [0; 21];
    let mut sum = 0;
    let mut done = false;
    for j in (start..end).rev() {
        let mut value = a[j] + ((low * delta[j]) >> 6);
        if value < thresh[j] && !done {
            value = if value >= c * 8 { c * 8 } else { 0 };
        } else {
            done = true;
        }
        bits[j] = value.min(caps[j]);
        sum += bits[j];
    }
    let mut coded = end;
    loop {
        let j = coded - 1;
        if j == start {
            total += skip;
            break;
        }
        let left = total - sum;
        let per = left / (edges[coded] - edges[start]);
        let rem = (left - (edges[coded] - edges[start]) * per - (edges[j] - edges[start])).max(0);
        let band_bits = bits[j] + per * (edges[coded] - edges[j]) + rem;
        if band_bits >= thresh[j].max(c * 8 + 8) {
            enc.ec_enc_bit_logp(true, 1).map_err(entropy)?;
            break;
        }
        sum -= bits[j] + intensity_rsv;
        if intensity_rsv > 0 {
            intensity_rsv = i32::from(LOG2_FRAC_TABLE[j - start]);
        }
        sum += intensity_rsv;
        if band_bits >= c * 8 {
            sum += c * 8;
            bits[j] = c * 8;
        } else {
            bits[j] = 0;
        }
        // No entropy skip symbol is present for a band below threshold.
        coded -= 1;
    }
    let intensity = if intensity_rsv > 0 {
        let intensity = intensity.clamp(start, coded);
        enc.ec_enc_uint((intensity - start) as u32, (coded + 1 - start) as u32)
            .map_err(entropy)?;
        intensity
    } else {
        0
    };
    if intensity <= start {
        total += dual_rsv;
        dual_rsv = 0;
    }
    let dual = dual_rsv > 0 && independent;
    if dual_rsv > 0 {
        enc.ec_enc_bit_logp(dual, 1).map_err(entropy)?;
    }
    let mut left = total - sum;
    let per = left / (edges[coded] - edges[start]);
    left -= (edges[coded] - edges[start]) * per;
    for j in start..coded {
        bits[j] += per * widths[j];
    }
    for j in start..coded {
        let extra = left.min(widths[j]);
        bits[j] += extra;
        left -= extra;
    }
    let logn = [
        0, 0, 0, 0, 0, 0, 0, 0, 8, 8, 8, 8, 16, 16, 16, 21, 21, 24, 29, 34, 36,
    ];
    let mut fine = [0; 21];
    let mut priority = [0; 21];
    let mut balance = 0;
    for j in start..coded {
        let n = widths[j] << lm;
        let bit = bits[j] + balance;
        let mut excess;
        if n > 1 {
            excess = (bit - caps[j]).max(0);
            bits[j] = bit - excess;
            let den = c * n + i32::from(c == 2 && n > 2 && !dual && j < intensity);
            let nlogn = den * (logn[j] + lm * 8);
            let mut offset = (nlogn >> 1) - den * 21;
            if n == 2 {
                offset += den * 2;
            }
            if bits[j] + offset < den * 16 {
                offset += nlogn >> 2;
            } else if bits[j] + offset < den * 24 {
                offset += nlogn >> 3;
            }
            let mut ebits = ((bits[j] + offset + den * 4).max(0) / den) >> 3;
            if c * ebits > bits[j] >> 3 {
                ebits = bits[j] >> stereo >> 3;
            }
            ebits = ebits.min(8);
            priority[j] = u8::from(ebits * (den * 8) >= bits[j] + offset);
            fine[j] = ebits as u8;
            bits[j] -= c * ebits * 8;
        } else {
            excess = (bit - c * 8).max(0);
            bits[j] = bit - excess;
            priority[j] = 1;
        }
        if excess > 0 {
            let extra = (excess >> (stereo + 3)).min(8 - i32::from(fine[j]));
            fine[j] += extra as u8;
            let extra_bits = extra * c * 8;
            priority[j] = u8::from(extra_bits >= excess - balance);
            excess -= extra_bits;
        }
        balance = excess;
    }
    for j in coded..end {
        fine[j] = (bits[j] >> stereo >> 3) as u8;
        bits[j] = 0;
        priority[j] = u8::from(fine[j] < 1);
    }
    Ok(Allocation {
        bits,
        fine,
        priority,
        coded,
        balance,
        intensity,
        dual,
    })
}

#[cfg(test)]
mod tests {
    use super::{allocate_celt, allocate_celt_end, allocate_joint, stereo_intensity};
    use crate::range::{RangeDecoder, RangeEncoder};

    #[test]
    fn hybrid_swb_allocation_respects_end_and_entropy_ranges() {
        // Hybrid SWB codes only bands 17 and 18. FB's end=21 changes the
        // intensity alphabet and allocation, even with identical input budget.
        for channels in [1, 2] {
            for lm in [2, 3] {
                for total in [64, 256, 1024, 4096] {
                    let mut encoder = RangeEncoder::new(128).unwrap();
                    let allocation =
                        allocate_celt_end(&mut encoder, total, channels, lm, 17, 19, 19).unwrap();
                    assert!((18..=19).contains(&allocation.coded));
                    for band in (0..17).chain(19..21) {
                        assert_eq!(allocation.bits[band], 0);
                        assert_eq!(allocation.fine[band], 0);
                        assert_eq!(allocation.priority[band], 0);
                    }
                    let tell = encoder.tell_frac();
                    encoder.ec_enc_uint(11, 23).unwrap();
                    let packet = encoder.finish().unwrap();
                    let mut decoder = RangeDecoder::new(&packet).unwrap();
                    // Mirror the production decoder's entropy sequence: a
                    // forced skip emits nothing; retaining band 18 emits one.
                    if allocation.coded == 19 {
                        assert!(decoder.ec_dec_bit_logp(1).unwrap());
                    }
                    if channels == 2 {
                        assert_eq!(allocation.intensity, allocation.coded);
                        let intensity = 17
                            + usize::try_from(
                                decoder
                                    .ec_dec_uint((allocation.coded + 1 - 17) as u32)
                                    .unwrap(),
                            )
                            .unwrap();
                        assert_eq!(intensity, allocation.intensity);
                        assert!(allocation.dual);
                        assert!(decoder.ec_dec_bit_logp(1).unwrap());
                    } else {
                        assert_eq!(allocation.intensity, 0);
                        assert!(!allocation.dual);
                    }
                    assert_eq!(u64::from(decoder.ec_tell_frac()), tell);
                    assert_eq!(decoder.ec_dec_uint(23).unwrap(), 11);
                }
            }
        }
    }

    #[test]
    fn joint_and_independent_allocation_signal_distinct_dual_flags() {
        // At this budget, fullband allocation force-skips down to 16 bands.
        // Intensity is bounded by coded bands, not by the requested end=21.
        // A single active band separately guarantees intensity=17 survives.
        for (start, end, coded, intensity) in [(0, 21, 16, 16), (16, 17, 17, 17)] {
            let mut independent = RangeEncoder::new(128).unwrap();
            let mut joint = RangeEncoder::new(128).unwrap();
            let a = allocate_celt_end(&mut independent, 4096, 2, 3, start, end, 17).unwrap();
            let b = allocate_joint(&mut joint, 4096, 2, 3, start, end, 17).unwrap();
            assert!(a.dual);
            assert!(!b.dual);
            assert_eq!(a.coded, coded);
            assert_eq!(b.coded, coded);
            assert_eq!(a.intensity, intensity);
            assert_eq!(b.intensity, intensity);
            for (packet, expected_dual) in [
                (independent.finish().unwrap(), true),
                (joint.finish().unwrap(), false),
            ] {
                let mut decoder = RangeDecoder::new(&packet).unwrap();
                if coded > start + 1 {
                    // Force-skipped bands emit no symbols; the retained band does.
                    assert!(decoder.ec_dec_bit_logp(1).unwrap());
                }
                let decoded_intensity = start
                    + usize::try_from(decoder.ec_dec_uint((coded + 1 - start) as u32).unwrap())
                        .unwrap();
                assert_eq!(decoded_intensity, intensity);
                assert_eq!(decoder.ec_dec_bit_logp(1).unwrap(), expected_dual);
            }
        }
    }

    #[test]
    fn intensity_preserves_independent_tonal_bands_and_shares_correlated_attacks() {
        for lm in 0..=3 {
            let mut spectrum = vec![vec![0; 120 << lm]; 2];
            let offset = crate::celt::constants::CELT_BINS_2_5MS[..13]
                .iter()
                .map(|&width| usize::from(width) << lm)
                .sum::<usize>();
            spectrum[0][offset] = 1 << 24;
            spectrum[1][offset + 1] = 1 << 24;
            assert_eq!(stereo_intensity(&spectrum, lm, 13), 14);
            spectrum[1][offset + 1] = 0;
            spectrum[1][offset] = 1 << 24;
            assert_eq!(stereo_intensity(&spectrum, lm, 13), 13);
            spectrum[1][offset] = -(1 << 24);
            assert_eq!(stereo_intensity(&spectrum, lm, 13), 13);
        }
    }

    #[test]
    fn fullband_wrapper_is_byte_identical() {
        for channels in [1, 2] {
            for lm in 0..=3 {
                for total in [0, 64, 512, 4096] {
                    let mut wrapper = RangeEncoder::new(128).unwrap();
                    let mut explicit = RangeEncoder::new(128).unwrap();
                    let a = allocate_celt(&mut wrapper, total, channels, lm, 0, 17).unwrap();
                    let b =
                        allocate_celt_end(&mut explicit, total, channels, lm, 0, 21, 17).unwrap();
                    assert_eq!(a.bits, b.bits);
                    assert_eq!(a.fine, b.fine);
                    assert_eq!(a.priority, b.priority);
                    assert_eq!(
                        (a.coded, a.balance, a.intensity, a.dual),
                        (b.coded, b.balance, b.intensity, b.dual)
                    );
                    assert_eq!(wrapper.finish().unwrap(), explicit.finish().unwrap());
                }
            }
        }
    }

    #[test]
    fn wideband_allocation_keeps_excluded_bands_empty() {
        for channels in [1, 2] {
            for lm in 0..=3 {
                for total in [0, 64, 512, 4096] {
                    let mut encoder = RangeEncoder::new(128).unwrap();
                    let allocation =
                        allocate_celt_end(&mut encoder, total, channels, lm, 0, 17, 21).unwrap();
                    assert!(allocation.coded <= 17);
                    assert!(allocation.intensity <= 17);
                    assert_eq!(&allocation.bits[17..], &[0; 4]);
                    assert_eq!(&allocation.fine[17..], &[0; 4]);
                    encoder.finish().unwrap();
                }
            }
        }
    }

    #[test]
    fn invalid_band_ranges_return_errors() {
        for (start, end) in [(0, 0), (17, 17), (18, 17), (0, 22)] {
            let mut encoder = RangeEncoder::new(64).unwrap();
            assert!(allocate_celt_end(&mut encoder, 512, 2, 1, start, end, 17).is_err());
        }
    }
}
