//! Mono residual state machine from the pinned fixed-point `bands.c`.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
// All dimensions, indices and shifts are bounded by the CELT mode. Explicit casts
// preserve the fixed-width reference arithmetic rather than saturating it.
use super::{Allocation, CeltDecoder, RangeDecoder, Result, residual};
use crate::celt::{constants::CELT_BINS_2_5MS, fixed_point::vshr32, pvq};

const LOGN: [i32; 21] = [
    0, 0, 0, 0, 0, 0, 0, 0, 8, 8, 8, 8, 16, 16, 16, 21, 21, 24, 29, 34, 36,
];
pub(super) fn mul(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 31) as i32
}
pub(super) fn frac(a: i32, b: i32) -> i32 {
    (16384 + i32::from(a as i16) * i32::from(b as i16)) >> 15
}
pub(super) fn normalize(x: &mut [i16], energy: i32, gain: i32) {
    let k = (31 - energy.leading_zeros()) as i32 >> 1;
    let g = mul(
        i32::from(residual::residual_rsqrt_norm(vshr32(energy, 2 * (k - 7)))),
        gain,
    ) as i16;
    for value in x {
        *value = ((i32::from(g) * i32::from(*value) + (1 << k)) >> (k + 1)) as i16;
    }
}
fn sqrt(x: i32) -> i32 {
    if x == 0 {
        return 0;
    }
    if x >= 1_073_741_824 {
        return 32767;
    }
    let k = (((31 - x.leading_zeros()) >> 1) as i32) - 7;
    let n = (vshr32(x, 2 * k) - 32768) as i16;
    let q = |a: i16, b: i16| ((i32::from(a) * i32::from(b)) >> 15) as i16;
    let mut tail = 336_i16;
    for coefficient in [-1002_i16, 1592, -2901, 11574] {
        tail = coefficient.wrapping_add(q(n, tail));
    }
    let rt = 23171 + i32::from(q(n, tail));
    vshr32(rt, 7 - k)
}

pub(super) struct Context<'a> {
    pub(super) rd: &'a mut RangeDecoder,
    pub(super) band: usize,
    pub(super) spread: u8,
    pub(super) remaining: i32,
    pub(super) seed: u32,
    pub(super) avoid_split_noise: bool,
}
impl Context<'_> {
    const fn random(&mut self) -> u32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        self.seed
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(super) fn partition(
        &mut self,
        x: &mut [i16],
        mut bits: i32,
        mut blocks: usize,
        low: Option<&[i16]>,
        mut lm: i32,
        gain: i32,
        mut fill: u32,
    ) -> Result<u32> {
        let n = x.len();
        let cache = &residual::CACHE_BITS50
            [residual::CACHE_INDEX50[((lm + 1) as usize) * 21 + self.band] as usize..];
        if lm != -1 && bits > i32::from(cache[usize::from(cache[0])]) + 12 && n > 2 {
            let half = n / 2;
            let original_blocks = blocks;
            lm -= 1;
            if blocks == 1 {
                fill = (fill & 1) | (fill << 1);
            }
            blocks = (blocks + 1) >> 1;
            let pulse_cap = LOGN[self.band] + lm * 8;
            let offset = (pulse_cap >> 1) - 4;
            let dimensions = 2 * half as i32 - 1;
            let qb = ((bits + dimensions * offset) / dimensions)
                .min(bits - pulse_cap - 32)
                .min(64);
            let qn = if qb < 4 {
                1
            } else {
                let exp = [16384, 17866, 19483, 21247, 23170, 25267, 27554, 30048];
                ((exp[(qb & 7) as usize] >> (14 - (qb >> 3))) + 1) >> 1 << 1
            };
            let before = self.rd.ec_tell_frac();
            let theta = if qn == 1 {
                0
            } else {
                let symbol = if original_blocks > 1 {
                    self.rd.ec_dec_uint((qn + 1) as u32)? as i32
                } else {
                    let total = ((qn >> 1) + 1) * ((qn >> 1) + 1);
                    let frequency = self.rd.ec_decode(total as u32)? as i32;
                    let mut cumulative = 0;
                    let mut chosen = 0;
                    for candidate in 0..=qn {
                        let weight = if candidate <= qn / 2 {
                            candidate + 1
                        } else {
                            qn + 1 - candidate
                        };
                        if frequency < cumulative + weight {
                            self.rd.ec_dec_update(
                                cumulative as u32,
                                (cumulative + weight) as u32,
                                total as u32,
                            )?;
                            chosen = candidate;
                            break;
                        }
                        cumulative += weight;
                    }
                    chosen
                };
                symbol * 16384 / qn
            };
            let cost = (self.rd.ec_tell_frac() - before) as i32;
            bits -= cost;
            let (mid, side, mut delta) = if theta == 0 {
                fill &= (1 << blocks) - 1;
                (32767, 0, -16384)
            } else if theta == 16384 {
                fill &= ((1 << blocks) - 1) << blocks;
                (0, 32767, 16384)
            } else {
                let mid = pvq::bitexact_cos(theta);
                let side = pvq::bitexact_cos(16384 - theta);
                (
                    mid,
                    side,
                    frac((half as i32 - 1) << 7, pvq::bitexact_log2tan(side, mid)),
                )
            };
            if original_blocks > 1 && (theta & 0x3fff) != 0 {
                if theta > 8192 {
                    delta -= delta >> (4 - lm);
                } else {
                    delta = 0.min(delta + ((half as i32 * 8) >> (5 - lm)));
                }
            }
            let mut mbits = 0.max(bits.min((bits - delta) / 2));
            let mut sbits = bits - mbits;
            self.remaining -= cost;
            let prior = self.remaining;
            let (left, right) = x.split_at_mut(half);
            let low_left = low.map(|v| &v[..half]);
            let low_right = low.map(|v| &v[half..]);
            let mask = if mbits >= sbits {
                let cm = self.partition(
                    left,
                    mbits,
                    blocks,
                    low_left,
                    lm,
                    mul(gain, mid << 16),
                    fill,
                )?;
                let rebalance = mbits - (prior - self.remaining);
                if rebalance > 24 && theta != 0 {
                    sbits += rebalance - 24;
                }
                cm | (self.partition(
                    right,
                    sbits,
                    blocks,
                    low_right,
                    lm,
                    mul(gain, side << 16),
                    fill >> blocks,
                )? << (original_blocks >> 1))
            } else {
                let cm = self.partition(
                    right,
                    sbits,
                    blocks,
                    low_right,
                    lm,
                    mul(gain, side << 16),
                    fill >> blocks,
                )? << (original_blocks >> 1);
                let rebalance = sbits - (prior - self.remaining);
                if rebalance > 24 && theta != 16384 {
                    mbits += rebalance - 24;
                }
                cm | self.partition(
                    left,
                    mbits,
                    blocks,
                    low_left,
                    lm,
                    mul(gain, mid << 16),
                    fill,
                )?
            };
            return Ok(mask);
        }
        let target = bits - 1;
        let mut lo = 0_usize;
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
        self.remaining -= cost(q);
        while self.remaining < 0 && q > 0 {
            self.remaining += cost(q);
            q -= 1;
            self.remaining -= cost(q);
        }
        if q > 0 {
            let k = if q < 8 {
                q
            } else {
                (8 + (q & 7)) << ((q >> 3) - 1)
            };
            let pulses = pvq::decode_pvq_vector(self.rd, n as u32, k as u32)?;
            let energy = pulses.iter().map(|&p| p * p).sum();
            for (value, pulse) in x.iter_mut().zip(&pulses) {
                *value = *pulse as i16;
            }
            normalize(x, energy, gain);
            residual::inverse_spread(x, k, blocks, self.spread);
            let mut mask = 0;
            for (i, block) in pulses.chunks_exact(n / blocks).enumerate() {
                if block.iter().any(|&p| p != 0) {
                    mask |= 1 << i;
                }
            }
            Ok(mask)
        } else {
            fill &= (1 << blocks) - 1;
            if fill == 0 {
                x.fill(0);
                return Ok(0);
            }
            let mask = if let Some(low) = low {
                for (value, &source) in x.iter_mut().zip(low) {
                    *value = source.wrapping_add(if self.random() & 0x8000 != 0 { 4 } else { -4 });
                }
                fill
            } else {
                for value in x.iter_mut() {
                    *value = ((self.random() as i32) >> 20) as i16;
                }
                (1 << blocks) - 1
            };
            let energy = 1 + x.iter().map(|&p| i32::from(p) * i32::from(p)).sum::<i32>();
            normalize(x, energy, gain);
            Ok(mask)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn band(
        &mut self,
        x: &mut [i16],
        bits: i32,
        blocks: usize,
        low: Option<&[i16]>,
        lm: i32,
        resolution: i8,
        fill: u32,
        gain: i32,
    ) -> Result<(u32, Vec<i16>)> {
        let n = x.len();
        if n == 1 {
            let sign = if self.remaining >= 8 {
                self.remaining -= 8;
                self.rd.ec_dec_bits(1)?
            } else {
                0
            };
            x[0] = if sign != 0 { -16384 } else { 16384 };
            return Ok((1, vec![x[0] >> 4]));
        }
        let (coded_blocks, recombine, divide) = residual::tf_blocks(n, blocks, resolution);
        let mut fill = fill;
        let mut low = low.map(<[i16]>::to_vec);
        for k in 0..recombine {
            if let Some(v) = &mut low {
                residual::haar(v, n >> k, 1 << k);
            }
            let table = [0, 1, 1, 1, 2, 3, 3, 3, 2, 3, 3, 3, 2, 3, 3, 3];
            fill = table[(fill & 15) as usize] | (table[(fill >> 4) as usize] << 2);
        }
        let mut b = blocks >> recombine;
        let mut width = n / b;
        for _ in 0..divide {
            if let Some(v) = &mut low {
                residual::haar(v, width, b);
            }
            fill |= fill << b;
            b <<= 1;
            width >>= 1;
        }
        if coded_blocks > 1
            && let Some(v) = &mut low
        {
            deinterleave(
                v,
                width >> recombine,
                coded_blocks << recombine,
                blocks == 1,
            );
        }
        let mut cm = self.partition(x, bits, coded_blocks, low.as_deref(), lm, gain, fill)?;
        residual::undo_tf(x, blocks, resolution);
        b = coded_blocks;
        for _ in 0..divide {
            b >>= 1;
            cm |= cm >> b;
        }
        let table = [
            0, 3, 12, 15, 48, 51, 60, 63, 192, 195, 204, 207, 240, 243, 252, 255,
        ];
        for _ in 0..recombine {
            cm = table[cm as usize];
        }
        b <<= recombine;
        let scale = sqrt((n as i32) << 22) as i16;
        let out = x
            .iter()
            .map(|&v| ((i32::from(v) * i32::from(scale)) >> 15) as i16)
            .collect();
        Ok((cm & ((1 << b) - 1), out))
    }
}

fn deinterleave(x: &mut [i16], width: usize, blocks: usize, hadamard: bool) {
    let order = [
        1, 0, 3, 0, 2, 1, 7, 0, 4, 3, 6, 1, 5, 2, 15, 0, 8, 7, 12, 3, 11, 4, 14, 1, 9, 6, 13, 2,
        10, 5,
    ];
    let src = x.to_vec();
    for lane in 0..blocks {
        let dest = if hadamard {
            order[blocks - 2 + lane]
        } else {
            lane
        };
        for j in 0..width {
            x[dest * width + j] = src[j * blocks + lane];
        }
    }
}

#[derive(Debug)]
pub(super) struct Trace {
    #[cfg(test)]
    pub bits: i32,
    #[cfg(test)]
    pub entropy: (u32, u32, u32),
    pub mask: u32,
}
impl CeltDecoder {
    #[allow(clippy::too_many_lines)]
    pub(super) fn mono_residual(
        &mut self,
        rd: &mut RangeDecoder,
        allocation: &Allocation,
        spread: u8,
        total_bits: i32,
    ) -> Result<(Vec<Vec<i16>>, Vec<Trace>)> {
        let lm = self.frame_duration_index() as i32;
        let widths = CELT_BINS_2_5MS.map(|n| usize::from(n) << lm);
        let mut edges = [0; 22];
        for j in 0..21 {
            edges[j + 1] = edges[j] + widths[j];
        }
        let start = self.start_band;
        let end = self.end_band;
        let blocks = if self.transient { 1 << lm } else { 1 };
        let mut norm = vec![0_i16; edges[21]];
        let mut masks = [0_u32; 21];
        let mut shapes = Vec::new();
        let mut trace = Vec::new();
        let mut balance = allocation.balance;
        let mut lowband_offset = 0;
        let mut update = true;
        let mut ctx = Context {
            rd,
            band: start,
            spread,
            remaining: 0,
            seed: self.state.anti_collapse_state.seed,
            avoid_split_noise: blocks > 1,
        };
        for i in 0..21 {
            let n = widths[i];
            let mut x = vec![0_i16; n];
            if i < start || i >= end {
                shapes.push(x);
                continue;
            }
            let tell = ctx.rd.ec_tell_frac() as i32;
            if i != start {
                balance -= tell;
            }
            ctx.remaining = total_bits - tell - 1;
            let b = if i < allocation.coded_bands {
                (allocation.shape_bits[i] + balance / (3.min(allocation.coded_bands - i) as i32))
                    .min(ctx.remaining + 1)
                    .clamp(0, 16383)
            } else {
                0
            };
            if (edges[i] >= edges[start] + n || i == start + 1) && (update || lowband_offset == 0) {
                lowband_offset = i;
            }
            if i == start + 1 {
                let first = widths[start];
                let second = widths[start + 1];
                norm.copy_within(
                    edges[start] + 2 * first - second..edges[start] + first,
                    edges[start] + first,
                );
            }
            let mut effective = None;
            let mut fill = (1 << blocks) - 1;
            if lowband_offset != 0
                && (spread != 3 || blocks > 1 || self.tf_resolution_signed[i] < 0)
            {
                let offset = (edges[lowband_offset] - edges[start]).saturating_sub(n);
                let mut from = lowband_offset - 1;
                while edges[from] > offset + edges[start] {
                    from -= 1;
                }
                let mut to = lowband_offset;
                while to < i && edges[to] < offset + edges[start] + n {
                    to += 1;
                }
                fill = 0;
                for &mask in &masks[from..to] {
                    fill |= mask;
                }
                effective = Some(offset + edges[start]);
            }
            ctx.band = i;
            let low = effective.map(|offset| norm[offset..offset + n].to_vec());
            let (mask, out) = ctx.band(
                &mut x,
                b,
                blocks,
                low.as_deref(),
                lm,
                self.tf_resolution_signed[i],
                fill,
                2_147_483_647,
            )?;
            masks[i] = mask;
            if i != end - 1 {
                norm[edges[i]..edges[i + 1]].copy_from_slice(&out);
            }
            trace.push(Trace {
                #[cfg(test)]
                bits: b,
                #[cfg(test)]
                entropy: (ctx.rd.get_range(), ctx.rd.get_value(), ctx.rd.ec_tell()),
                mask,
            });
            shapes.push(x);
            balance += allocation.shape_bits[i] + tell;
            update = b > n as i32 * 8;
            ctx.avoid_split_noise = false;
        }
        self.state.anti_collapse_state.seed = ctx.seed;
        Ok((shapes, trace))
    }
}
