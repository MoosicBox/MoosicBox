//! CELT energy and algebraic pulse quantization.
use super::entropy;
use crate::Result;
use crate::celt::constants::CELT_BINS_2_5MS;
use crate::celt::{CACHE_BITS50, CACHE_INDEX50};
use crate::range::RangeEncoder;

const ONE: i32 = 1 << 16;

// Binary logarithm with a Q31 mantissa and repeated integer squaring.
fn log2_q16(value: u64) -> i32 {
    if value == 0 {
        return -80 * ONE;
    }
    let exponent = value.ilog2();
    let mut normalized = if exponent > 31 {
        value >> (exponent - 31)
    } else {
        value << (31 - exponent)
    };
    let mut fraction = 0;
    for bit in (0..16).rev() {
        normalized = ((u128::from(normalized) * u128::from(normalized)) >> 31) as u64;
        if normalized >= 1_u64 << 32 {
            normalized >>= 1;
            fraction |= 1 << bit;
        }
    }
    exponent as i32 * ONE + fraction
}

pub(super) fn energies(spectrum: &[Vec<i32>], lm: usize) -> (Vec<i32>, Vec<i32>) {
    let means = [
        103, 100, 92, 85, 81, 77, 72, 70, 78, 75, 73, 71, 78, 74, 69, 72, 70, 74, 76, 71, 60,
    ];
    let mut target = Vec::with_capacity(21 * spectrum.len());
    for channel in spectrum {
        let mut offset = 0;
        for (band, &width) in CELT_BINS_2_5MS.iter().enumerate() {
            let n = usize::from(width) << lm;
            let power = channel[offset..offset + n]
                .iter()
                .map(|&x| (i64::from(x) * i64::from(x)) as u64)
                .sum();
            target.push(
                ((log2_q16(power) - 24 * ONE) / 2 - means[band] * (ONE / 16))
                    .clamp(-28 * ONE, 28 * ONE),
            );
            offset += n;
        }
    }
    let energy = vec![0; target.len()];
    (target, energy)
}

pub(super) fn laplace(
    enc: &mut RangeEncoder,
    value: &mut i32,
    mut fs: u32,
    decay: u32,
) -> Result<()> {
    let mut low = 0;
    if *value != 0 {
        let negative = *value < 0;
        let val = value.unsigned_abs();
        low = fs;
        fs = ((32768 - 32 - fs) * (16384 - decay)) >> 15;
        let mut i = 1;
        while fs > 0 && i < val {
            fs *= 2;
            low += fs + 2;
            fs = (fs * decay) >> 15;
            i += 1;
        }
        if fs == 0 {
            let max = (32768 - low).midpoint(u32::from(negative));
            let delta = (val - i).min(max - 1);
            low += 2 * delta + u32::from(!negative);
            fs = 1.min(32768 - low);
            *value = (i + delta) as i32 * if negative { -1 } else { 1 };
        } else {
            fs += 1;
            if !negative {
                low += fs;
            }
        }
    }
    enc.ec_encode_bin(low, low + fs, 15).map_err(entropy)
}

pub(super) fn fine_energy(
    enc: &mut RangeEncoder,
    target: &[i32],
    energy: &mut [i32],
    fine: &[u8; 21],
    channels: usize,
) -> Result<()> {
    for (band, &bits) in fine.iter().enumerate() {
        if bits == 0 {
            continue;
        }
        let scale = 1_u32 << bits;
        for channel in 0..channels {
            let i = channel * 21 + band;
            let q = ((i64::from(target[i] - energy[i] + ONE / 2) * i64::from(scale)) >> 16)
                .clamp(0, i64::from(scale - 1)) as u32;
            enc.ec_enc_bits(q, u32::from(bits)).map_err(entropy)?;
            energy[i] += ((2 * q + 1) as i32 * ONE) / (2 * scale as i32) - ONE / 2;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Entropy stage with explicit band range and packet budget.
pub(super) fn final_energy(
    enc: &mut RangeEncoder,
    target: &[i32],
    energy: &mut [i32],
    fine: &[u8; 21],
    priority: &[u8; 21],
    channels: usize,
    total: usize,
    start: usize,
) -> Result<()> {
    for pass in 0..2 {
        for band in start..21 {
            if fine[band] >= 8
                || priority[band] != pass
                || total.saturating_sub(enc.tell() as usize) < channels
            {
                continue;
            }
            for channel in 0..channels {
                let i = channel * 21 + band;
                let q = u32::from(target[i] >= energy[i]);
                enc.ec_enc_bits(q, 1).map_err(entropy)?;
                energy[i] += (2 * q as i32 - 1) * (ONE >> (fine[band] + 2));
            }
        }
    }
    Ok(())
}

const fn frac(a: i32, b: i32) -> i32 {
    (16384 + a * b) >> 15
}
fn cos(x: i32) -> i32 {
    let x2 = ((4096 + x * x) >> 13) as i16;
    let x = i32::from(x2);
    i32::from((32767 - x + frac(x, -7651 + frac(x, 8277 + frac(x, -626)))) as i16).wrapping_add(1)
}
#[allow(clippy::many_single_char_names)] // Q15 trigonometric notation.
const fn logtan(s: i32, c: i32) -> i32 {
    let ls = 32 - s.leading_zeros();
    let lc = 32 - c.leading_zeros();
    let s = s << (15 - ls);
    let c = c << (15 - lc);
    (ls as i32 - lc as i32) * 2048 + frac(s, frac(s, -2597) + 7932) - frac(c, frac(c, -2597) + 7932)
}

/// Recursively split overlarge pulse codebooks, quantize their angle, and emit
/// a signed integer pyramid index for each leaf.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)] // Recursive RFC N/K/theta notation.
pub(super) fn partition(
    enc: &mut RangeEncoder,
    x: &[i32],
    mut bits: i32,
    band: usize,
    mut lm: i32,
    remaining: &mut i32,
) -> Result<()> {
    let n = x.len();
    if n == 1 {
        if *remaining >= 8 {
            enc.ec_enc_bits(u32::from(x[0] < 0), 1).map_err(entropy)?;
            *remaining -= 8;
        }
        return Ok(());
    }
    let index = CACHE_INDEX50[((lm + 1) * 21) as usize + band] as usize;
    let cache = &CACHE_BITS50[index..];
    if lm != -1 && bits > i32::from(cache[usize::from(cache[0])]) + 12 && n > 2 {
        let half = n / 2;
        lm -= 1;
        let logn = [
            0, 0, 0, 0, 0, 0, 0, 0, 8, 8, 8, 8, 16, 16, 16, 21, 21, 24, 29, 34, 36,
        ];
        let pulse_cap = logn[band] + lm * 8;
        let offset = (pulse_cap >> 1) - 4;
        let dim = 2 * half as i32 - 1;
        let qb = ((bits + dim * offset) / dim)
            .min(bits - pulse_cap - 32)
            .min(64);
        let qn = if qb < 4 {
            1
        } else {
            let exp = [16384, 17866, 19483, 21247, 23170, 25267, 27554, 30048];
            ((exp[(qb & 7) as usize] >> (14 - (qb >> 3))) + 1) >> 1 << 1
        };
        let a = x[..half]
            .iter()
            .map(|&v| i128::from(v) * i128::from(v))
            .sum::<i128>();
        let b = x[half..]
            .iter()
            .map(|&v| i128::from(v) * i128::from(v))
            .sum::<i128>();
        let symbol = if qn == 1 {
            0
        } else {
            // Compare squared tan(theta) against the power ratio. All angle
            // decisions use integer cosine coefficients, including endpoints.
            (0..=qn)
                .min_by_key(|&q| {
                    let phase = q * 32768 / qn;
                    let c = i128::from(crate::celt::fixed_point::celt_cos_norm(phase));
                    let s = i128::from(crate::celt::fixed_point::celt_cos_norm(32768 - phase));
                    (b * c * c - a * s * s).abs()
                })
                .unwrap_or(0)
        };
        let before = enc.tell_frac() as i32;
        if qn > 1 {
            let total = ((qn >> 1) + 1) * ((qn >> 1) + 1);
            let mut cumulative = 0;
            for candidate in 0..=qn {
                let weight = if candidate <= qn / 2 {
                    candidate + 1
                } else {
                    qn + 1 - candidate
                };
                if candidate == symbol {
                    enc.ec_encode(
                        cumulative as u32,
                        (cumulative + weight) as u32,
                        total as u32,
                    )
                    .map_err(entropy)?;
                    break;
                }
                cumulative += weight;
            }
        }
        let theta = symbol * 16384 / qn;
        let cost = enc.tell_frac() as i32 - before;
        bits -= cost;
        let delta = if theta == 0 {
            -16384
        } else if theta == 16384 {
            16384
        } else {
            frac(
                (half as i32 - 1) << 7,
                logtan(cos(16384 - theta), cos(theta)),
            )
        };
        let mut mbits = (bits - delta) / 2;
        mbits = mbits.clamp(0, bits);
        let mut sbits = bits - mbits;
        *remaining -= cost;
        let prior = *remaining;
        if mbits >= sbits {
            partition(enc, &x[..half], mbits, band, lm, remaining)?;
            let rebalance = mbits - (prior - *remaining);
            if rebalance > 24 && theta != 0 {
                sbits += rebalance - 24;
            }
            partition(enc, &x[half..], sbits, band, lm, remaining)?;
        } else {
            partition(enc, &x[half..], sbits, band, lm, remaining)?;
            let rebalance = sbits - (prior - *remaining);
            if rebalance > 24 && theta != 16384 {
                mbits += rebalance - 24;
            }
            partition(enc, &x[..half], mbits, band, lm, remaining)?;
        }
        return Ok(());
    }
    let target = bits - 1;
    let mut lo = 0;
    let mut hi = usize::from(cache[0]);
    for _ in 0..6 {
        let mid = (lo + hi).div_ceil(2);
        if i32::from(cache[mid]) >= target {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let mut q = if target - if lo == 0 { -1 } else { i32::from(cache[lo]) }
        <= i32::from(cache[hi]) - target
    {
        lo
    } else {
        hi
    };
    let cost = |q: usize| if q == 0 { 0 } else { i32::from(cache[q]) + 1 };
    *remaining -= cost(q);
    while *remaining < 0 && q > 0 {
        *remaining += cost(q);
        q -= 1;
        *remaining -= cost(q);
    }
    if q == 0 {
        return Ok(());
    }
    let k = if q < 8 {
        q
    } else {
        (8 + (q & 7)) << ((q >> 3) - 1)
    };
    let pulses = search(x, k);
    let (index, total) = rank(&pulses, k);
    enc.ec_enc_uint(index, total).map_err(|error| {
        crate::Error::Unsupported(format!(
            "PVQ n={n} k={k} index={index} total={total}: {error}"
        ))
    })
}

#[allow(clippy::many_single_char_names)] // Integer normalized-correlation pulse search.
fn search(x: &[i32], k: usize) -> Vec<i32> {
    let mut y = vec![0_i32; x.len()];
    let mut dot = 0_i128;
    let mut norm = 0_i128;
    for _ in 0..k {
        let mut best = 0;
        let mut best_num = -1;
        let mut best_den = 1;
        for (j, &value) in x.iter().enumerate() {
            let d = dot + i128::from(value.abs());
            let e = norm + 2 * i128::from(y[j].abs()) + 1;
            if d * d * best_den > best_num * e {
                best = j;
                best_num = d * d;
                best_den = e;
            }
        }
        norm += 2 * i128::from(y[best].abs()) + 1;
        dot += i128::from(x[best].abs());
        y[best] += if x[best] < 0 { -1 } else { 1 };
    }
    y
}

// Reverse the RFC signed-pyramid unranking recurrence. The complete V table is
// bounded by the pulse cache to fit the 32-bit entropy coder at each leaf.
#[allow(clippy::many_single_char_names)] // RFC pyramid dimensions N and pulse count K.
fn rank(pulses: &[i32], k: usize) -> (u32, u32) {
    let n = pulses.len();
    let mut v = vec![vec![0_u32; k + 1]; n + 1];
    v[0][0] = 1;
    for i in 1..=n {
        v[i][0] = 1;
        for j in 1..=k {
            v[i][j] = v[i - 1][j]
                .saturating_add(v[i][j - 1])
                .saturating_add(v[i - 1][j - 1]);
        }
    }
    let mut remaining = k;
    let mut offsets = Vec::with_capacity(n);
    for (j, &pulse) in pulses.iter().enumerate() {
        let dims = n - j;
        let mut p = if dims > 1 {
            v[dims - 1][remaining].midpoint(v[dims][remaining])
        } else {
            v[dims][remaining] / 2
        };
        let sign = if pulse < 0 { p } else { 0 };
        if dims > 1 {
            p = p.saturating_sub(v[dims - 1][remaining]);
        }
        for _ in 0..pulse.unsigned_abs() {
            remaining -= 1;
            if dims > 1 {
                p = p.saturating_sub(v[dims - 1][remaining]);
            }
        }
        offsets.push(sign + if dims == 1 { 0 } else { p });
    }
    (offsets.iter().sum(), v[n][k])
}
