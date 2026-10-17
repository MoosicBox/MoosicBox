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
}

// Long-window CELT, neutral trim, no dynamic boosts. Choose independent
// stereo shapes throughout the coded range; intensity folding is unnecessary.
#[allow(clippy::too_many_lines)]
pub(super) fn allocate(
    enc: &mut RangeEncoder,
    mut total: i32,
    channels: usize,
    lm: i32,
    start: usize,
) -> Result<Allocation> {
    let c = channels as i32;
    let stereo = c - 1;
    let end = 21;
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
        ((c * widths[j] * (-lm) * (end - j - 1) as i32 * (1 << (lm + 3))) >> 6)
            - if widths[j] << lm == 1 { c * 8 } else { 0 }
    });
    let curve = |j: usize, q: usize| {
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
        enc.ec_enc_uint((coded - start) as u32, (coded + 1 - start) as u32)
            .map_err(entropy)?;
        coded
    } else {
        0
    };
    if intensity <= start {
        total += dual_rsv;
        dual_rsv = 0;
    }
    let dual = dual_rsv > 0;
    if dual {
        enc.ec_enc_bit_logp(true, 1).map_err(entropy)?;
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
    })
}
