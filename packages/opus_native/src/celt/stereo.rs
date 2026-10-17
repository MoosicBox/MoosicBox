//! Coupled, dual and intensity stereo residual reconstruction from `bands.c`.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::type_complexity,
    clippy::cast_sign_loss
)]
use super::{
    Allocation, CeltDecoder, RangeDecoder, Result,
    mono::{Context, frac, mul},
    residual,
};
use crate::celt::{constants::CELT_BINS_2_5MS, fixed_point::vshr32, pvq};
const LOGN: [i32; 21] = [
    0, 0, 0, 0, 0, 0, 0, 0, 8, 8, 8, 8, 16, 16, 16, 21, 21, 24, 29, 34, 36,
];
fn merge(x: &mut [i16], y: &mut [i16], mid: i32) {
    let dot: i32 = x
        .iter()
        .zip(y.iter())
        .map(|(&a, &b)| i32::from(a) * i32::from(b))
        .sum();
    let side: i32 = y.iter().map(|&v| i32::from(v) * i32::from(v)).sum();
    let cross = mul(mid, dot);
    let left = (mul(mid, mid) >> 3) + side - 2 * cross;
    let right = (mul(mid, mid) >> 3) + side + 2 * cross;
    if left < 161_061 || right < 161_061 {
        y.copy_from_slice(x);
        return;
    }
    let kl = ((31 - left.leading_zeros()) >> 1) as i32;
    let kr = ((31 - right.leading_zeros()) >> 1) as i32;
    let lg = i32::from(residual::residual_rsqrt_norm(vshr32(left, (kl - 7) * 2)));
    let rg = i32::from(residual::residual_rsqrt_norm(vshr32(right, (kr - 7) * 2)));
    let kl = kl.max(7);
    let kr = kr.max(7);
    for (x, y) in x.iter_mut().zip(y) {
        let l = mul(mid, i32::from(*x)) as i16;
        let r = *y;
        *x = ((lg * i32::from(l.wrapping_sub(r)) + (1 << kl)) >> (kl + 1)) as i16;
        *y = ((rg * i32::from(l.wrapping_add(r)) + (1 << kr)) >> (kr + 1)) as i16;
    }
}
impl Context<'_> {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn stereo_band(
        &mut self,
        x: &mut [i16],
        y: &mut [i16],
        mut bits: i32,
        blocks: usize,
        low: Option<&[i16]>,
        lm: i32,
        resolution: i8,
        mut fill: u32,
        intensity: usize,
        disable_inversion: bool,
    ) -> Result<(u32, Vec<i16>)> {
        let n = x.len();
        if n == 1 {
            let (_, norm) = self.band(x, bits, blocks, low, lm, resolution, fill, 2_147_483_647)?;
            self.band(y, bits, blocks, None, lm, resolution, fill, 2_147_483_647)?;
            return Ok((1, norm));
        }
        let original_fill = fill;
        let cap = LOGN[self.band] + lm * 8;
        let offset = (cap >> 1) - if n == 2 { 16 } else { 4 };
        let dimensions = 2 * n as i32 - 1 - i32::from(n == 2);
        let qb = ((bits + dimensions * offset) / dimensions)
            .min(bits - cap - 32)
            .min(64);
        let qn = if qb < 4 || self.band >= intensity {
            1
        } else {
            let exp = [16384, 17866, 19483, 21247, 23170, 25267, 27554, 30048];
            ((exp[(qb & 7) as usize] >> (14 - (qb >> 3))) + 1) >> 1 << 1
        };
        let before = self.rd.ec_tell_frac();
        let mut invert = false;
        let theta = if qn == 1 {
            if bits > 16 && self.remaining > 16 {
                invert = self.rd.ec_dec_bit_logp(2)?;
            }
            0
        } else {
            let symbol = if n > 2 {
                let half = qn / 2;
                let total = 3 * (half + 1) + half;
                let frequency = self.rd.ec_decode(total as u32)? as i32;
                let value = if frequency < (half + 1) * 3 {
                    frequency / 3
                } else {
                    half + 1 + frequency - (half + 1) * 3
                };
                let low = if value <= half {
                    3 * value
                } else {
                    value - 1 - half + (half + 1) * 3
                };
                let high = if value <= half {
                    3 * (value + 1)
                } else {
                    value - half + (half + 1) * 3
                };
                self.rd
                    .ec_dec_update(low as u32, high as u32, total as u32)?;
                value
            } else {
                self.rd.ec_dec_uint((qn + 1) as u32)? as i32
            };
            symbol * 16384 / qn
        };
        if disable_inversion {
            invert = false;
        }
        let cost = (self.rd.ec_tell_frac() - before) as i32;
        bits -= cost;
        let (mid, side, delta) = if theta == 0 {
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
                frac((n as i32 - 1) << 7, pvq::bitexact_log2tan(side, mid)),
            )
        };
        let norm;
        let mask;
        if n == 2 {
            let sbits = if theta != 0 && theta != 16384 { 8 } else { 0 };
            let mbits = bits - sbits;
            self.remaining -= cost + sbits;
            let sign = if sbits != 0 {
                1 - 2 * self.rd.ec_dec_bits(1)? as i32
            } else {
                1
            };
            if theta > 8192 {
                let (cm, out) = self.band(
                    y,
                    mbits,
                    blocks,
                    low,
                    lm,
                    resolution,
                    original_fill,
                    2_147_483_647,
                )?;
                mask = cm;
                norm = out;
                x[0] = (-sign * i32::from(y[1])) as i16;
                x[1] = (sign * i32::from(y[0])) as i16;
            } else {
                let (cm, out) = self.band(
                    x,
                    mbits,
                    blocks,
                    low,
                    lm,
                    resolution,
                    original_fill,
                    2_147_483_647,
                )?;
                mask = cm;
                norm = out;
                y[0] = (-sign * i32::from(x[1])) as i16;
                y[1] = (sign * i32::from(x[0])) as i16;
            }
            for (x, y) in x.iter_mut().zip(y.iter_mut()) {
                let a = mul(mid << 16, i32::from(*x)) as i16;
                let b = mul(side << 16, i32::from(*y)) as i16;
                *x = a.wrapping_sub(b);
                *y = a.wrapping_add(b);
            }
        } else {
            let mut mbits = 0.max(bits.min((bits - delta) / 2));
            let mut sbits = bits - mbits;
            self.remaining -= cost;
            let prior = self.remaining;
            if mbits >= sbits {
                let (cm, out) =
                    self.band(x, mbits, blocks, low, lm, resolution, fill, 2_147_483_647)?;
                norm = out;
                let rebalance = mbits - (prior - self.remaining);
                if rebalance > 24 && theta != 0 {
                    sbits += rebalance - 24;
                }
                let (other, _) = self.band(
                    y,
                    sbits,
                    blocks,
                    None,
                    lm,
                    resolution,
                    fill >> blocks,
                    side << 16,
                )?;
                mask = cm | other;
            } else {
                let (cm, _) = self.band(
                    y,
                    sbits,
                    blocks,
                    None,
                    lm,
                    resolution,
                    fill >> blocks,
                    side << 16,
                )?;
                let rebalance = sbits - (prior - self.remaining);
                if rebalance > 24 && theta != 16384 {
                    mbits += rebalance - 24;
                }
                let (other, out) =
                    self.band(x, mbits, blocks, low, lm, resolution, fill, 2_147_483_647)?;
                norm = out;
                mask = cm | other;
            }
            merge(x, y, mid << 16);
        }
        if invert {
            for value in y {
                *value = value.wrapping_neg();
            }
        }
        Ok((mask, norm))
    }
}

impl CeltDecoder {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(super) fn stereo_residual(
        &mut self,
        rd: &mut RangeDecoder,
        allocation: &Allocation,
        spread: u8,
        total: i32,
        intensity: usize,
        mut dual: bool,
    ) -> Result<(Vec<Vec<Vec<i16>>>, Vec<Vec<u8>>)> {
        let lm = self.frame_duration_index() as i32;
        let widths = CELT_BINS_2_5MS.map(|v| usize::from(v) << lm);
        let mut edges = [0; 22];
        for j in 0..21 {
            edges[j + 1] = edges[j] + widths[j];
        }
        let blocks = if self.transient { 1 << lm } else { 1 };
        let mut norm = vec![vec![0_i16; edges[21]]; 2];
        let mut masks = vec![vec![0_u8; 21]; 2];
        let mut shapes = vec![Vec::new(), Vec::new()];
        let mut balance = allocation.balance;
        let mut lowband_offset = 0;
        let mut update = true;
        let mut ctx = Context {
            rd,
            band: 0,
            spread,
            remaining: 0,
            seed: self.state.anti_collapse_state.seed,
            avoid_split_noise: blocks > 1,
        };
        for i in 0..21 {
            if i < self.start_band || i >= self.end_band {
                shapes[0].push(vec![0; widths[i]]);
                shapes[1].push(vec![0; widths[i]]);
                continue;
            }
            let n = widths[i];
            let tell = ctx.rd.ec_tell_frac() as i32;
            if i != self.start_band {
                balance -= tell;
            }
            ctx.remaining = total - tell - 1;
            let b = if i < allocation.coded_bands {
                (allocation.shape_bits[i] + balance / (3.min(allocation.coded_bands - i) as i32))
                    .min(ctx.remaining + 1)
                    .clamp(0, 16383)
            } else {
                0
            };
            if (edges[i] >= edges[self.start_band] + n || i == self.start_band + 1)
                && (update || lowband_offset == 0)
            {
                lowband_offset = i;
            }
            if i == self.start_band + 1 {
                let first = widths[self.start_band];
                let second = widths[self.start_band + 1];
                for values in norm.iter_mut().take(if dual { 2 } else { 1 }) {
                    values.copy_within(
                        edges[self.start_band] + 2 * first - second..edges[self.start_band] + first,
                        edges[self.start_band] + first,
                    );
                }
            }
            let mut effective = None;
            let mut fills = [(1 << blocks) - 1; 2];
            if lowband_offset != 0
                && (spread != 3 || blocks > 1 || self.tf_resolution_signed[i] < 0)
            {
                let offset = (edges[lowband_offset] - edges[self.start_band]).saturating_sub(n)
                    + edges[self.start_band];
                let mut from = lowband_offset - 1;
                while edges[from] > offset {
                    from -= 1;
                }
                let mut to = lowband_offset;
                while to < i && edges[to] < offset + n {
                    to += 1;
                }
                for channel in 0..2 {
                    fills[channel] = masks[channel][from..to]
                        .iter()
                        .fold(0, |a, &v| a | u32::from(v));
                }
                effective = Some(offset);
            }
            if dual && i == intensity {
                dual = false;
                let (left, right) = norm.split_at_mut(1);
                for (left, &right) in left[0][..edges[i]].iter_mut().zip(&right[0]) {
                    *left = ((i32::from(*left) + i32::from(right)) >> 1) as i16;
                }
            }
            ctx.band = i;
            let mut x = vec![0; n];
            let mut y = vec![0; n];
            let lows: Vec<Option<Vec<i16>>> = norm
                .iter()
                .map(|values| effective.map(|o| values[o..o + n].to_vec()))
                .collect();
            if dual {
                let (cm, out) = ctx.band(
                    &mut x,
                    b / 2,
                    blocks,
                    lows[0].as_deref(),
                    lm,
                    self.tf_resolution_signed[i],
                    fills[0],
                    2_147_483_647,
                )?;
                let (cm2, out2) = ctx.band(
                    &mut y,
                    b / 2,
                    blocks,
                    lows[1].as_deref(),
                    lm,
                    self.tf_resolution_signed[i],
                    fills[1],
                    2_147_483_647,
                )?;
                masks[0][i] = cm as u8;
                masks[1][i] = cm2 as u8;
                norm[0][edges[i]..edges[i + 1]].copy_from_slice(&out);
                norm[1][edges[i]..edges[i + 1]].copy_from_slice(&out2);
            } else {
                let (cm, out) = ctx.stereo_band(
                    &mut x,
                    &mut y,
                    b,
                    blocks,
                    lows[0].as_deref(),
                    lm,
                    self.tf_resolution_signed[i],
                    fills[0] | fills[1],
                    intensity,
                    self.synthesis_channels == crate::Channels::Mono,
                )?;
                masks[0][i] = cm as u8;
                masks[1][i] = cm as u8;
                norm[0][edges[i]..edges[i + 1]].copy_from_slice(&out);
            }
            shapes[0].push(x);
            shapes[1].push(y);
            balance += allocation.shape_bits[i] + tell;
            update = b > n as i32 * 8;
            ctx.avoid_split_noise = false;
        }
        self.state.anti_collapse_state.seed = ctx.seed;
        Ok((shapes, masks))
    }
}
