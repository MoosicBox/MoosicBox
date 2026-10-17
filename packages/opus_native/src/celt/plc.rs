//! Fixed-point CELT packet-loss prediction primitives, ported from the pinned
//! libopus `celt_lpc.c`, `pitch.c`, and `celt_decoder.c` (34bba701).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names
)]
use super::transform;
use crate::celt::fixed_point;

pub(super) const HISTORY: usize = 2048;
const ORDER: usize = 24;
const PERIOD: usize = 1024;

pub(super) struct LossState {
    pub memory: Vec<Vec<i32>>,
    pub lpc: Vec<[i16; ORDER]>,
    pub duration: usize,
    pub skip: bool,
    pub fold: bool,
    pub pitch: usize,
    pub background: [i32; 42],
}
impl LossState {
    pub const fn new() -> Self {
        Self {
            memory: Vec::new(),
            lpc: Vec::new(),
            duration: 0,
            skip: true,
            fold: false,
            pitch: 0,
            background: [0; 42],
        }
    }
    pub fn resize(&mut self, channels: usize) {
        self.memory.resize_with(channels, || vec![0; HISTORY + 120]);
        self.lpc.resize(channels, [0; ORDER]);
    }
    pub fn good(&mut self, samples: &[Vec<i32>], tails: &[Vec<i32>], energies: &[i32], lm: u8) {
        self.resize(samples.len());
        for (c, pcm) in samples.iter().enumerate() {
            self.memory[c].copy_within(pcm.len()..HISTORY, 0);
            self.memory[c][HISTORY - pcm.len()..HISTORY].copy_from_slice(pcm);
            self.memory[c][HISTORY..].fill(0);
            self.memory[c][HISTORY..HISTORY + 60].copy_from_slice(&tails[c]);
        }
        let increase = i32::try_from((self.duration + (1_usize << lm)).min(160)).unwrap() * 16777;
        for (i, background) in self.background.iter_mut().enumerate() {
            let energy = energies[i % energies.len()];
            *background = (*background + increase).min(energy);
        }
        if self.duration == 0 {
            self.skip = false;
        }
        self.duration = 0;
        self.fold = false;
    }
    pub fn pitch_conceal(&mut self, n: usize) -> Vec<Vec<i32>> {
        if self.duration == 0 {
            self.pitch = pitch(&self.memory);
        }
        let pitch = self.pitch;
        let length = (2 * pitch).min(PERIOD);
        let mut result = Vec::new();
        for c in 0..self.memory.len() {
            let buf = &mut self.memory[c];
            let mut exc: Vec<i16> = buf[HISTORY - PERIOD - ORDER..HISTORY]
                .iter()
                .map(|&x| round(x))
                .collect();
            if self.duration == 0 {
                fit_prediction(&exc[ORDER..], &mut self.lpc[c]);
            }
            let coeff = &self.lpc[c];
            let filtered: Vec<_> = (ORDER + PERIOD - length..ORDER + PERIOD)
                .map(|i| {
                    let sum = coeff
                        .iter()
                        .enumerate()
                        .fold(i32::from(exc[i]) << 12, |s, (j, &a)| {
                            s.wrapping_add(i32::from(a) * i32::from(exc[i - j - 1]))
                        });
                    round(sum)
                })
                .collect();
            exc[ORDER + PERIOD - length..].copy_from_slice(&filtered);
            let max = filtered
                .iter()
                .map(|&x| i32::from(x).abs())
                .max()
                .unwrap_or(0);
            let shift = (2 * ilog(max) - 20).max(0) as u32;
            let half = length / 2;
            let e1 = 1 + exc[ORDER + PERIOD - half..]
                .iter()
                .map(|&x| (i32::from(x) * i32::from(x)) >> shift)
                .sum::<i32>();
            let e2 = 1 + exc[ORDER + PERIOD - 2 * half..ORDER + PERIOD - half]
                .iter()
                .map(|&x| (i32::from(x) * i32::from(x)) >> shift)
                .sum::<i32>();
            let decay = fixed_point::celt_sqrt(frac_div(e1.min(e2) >> 1, e2));
            buf.copy_within(n..HISTORY, 0);
            let offset = HISTORY - n;
            let mut attenuation = mul(if self.duration == 0 { 32767 } else { 26214 }, decay);
            let mut s1 = 0_i32;
            for i in 0..n + 120 {
                let j = i % pitch;
                if i != 0 && j == 0 {
                    attenuation = mul(attenuation, decay);
                }
                buf[offset + i] =
                    mul(attenuation, i32::from(exc[ORDER + PERIOD - pitch + j])) << 12;
                let tmp = i32::from(round(buf[HISTORY - PERIOD - n + PERIOD - pitch + j]));
                s1 = s1.wrapping_add((tmp * tmp) >> 10);
            }
            let mut mem = [0_i16; ORDER];
            for i in 0..ORDER {
                mem[i] = round(buf[offset - 1 - i]);
            }
            for i in 0..n + 120 {
                let sum = coeff.iter().zip(mem).fold(buf[offset + i], |s, (&a, b)| {
                    s.wrapping_sub(i32::from(a) * i32::from(b))
                });
                mem.copy_within(..ORDER - 1, 1);
                mem[0] = round(sum);
                buf[offset + i] = sum.clamp(-536_870_911, 536_870_911);
            }
            let s2 = buf[offset..offset + n + 120].iter().fold(0_i32, |s, &x| {
                let v = i32::from(round(x));
                s.wrapping_add((v * v) >> 10)
            });
            if s1 <= s2 >> 2 {
                buf[offset..offset + n + 120].fill(0);
            } else if s1 < s2 {
                let ratio = fixed_point::celt_sqrt(frac_div((s1 >> 1) + 1, s2 + 1));
                for i in 0..n + 120 {
                    let gain = if i < 120 {
                        32767 - mul(i32::from(transform::window(i)), 32767 - ratio)
                    } else {
                        ratio
                    };
                    buf[offset + i] = mul16_32(gain, buf[offset + i]);
                }
            }
            result.push(buf[offset..HISTORY].to_vec());
        }
        self.fold = true;
        result
    }
}
pub(super) fn normalize(x: &mut [i16]) {
    let energy = 1 + x.iter().map(|&v| i32::from(v) * i32::from(v)).sum::<i32>();
    let k = ilog(energy) >> 1;
    let t = shift(energy, 2 * (k - 7));
    let g = mul32(i32::from(super::residual::residual_rsqrt_norm(t)), i32::MAX) as i16;
    for v in x {
        *v = ((i32::from(g) * i32::from(*v) + (1 << k)) >> (k + 1)) as i16;
    }
}
const fn ilog(x: i32) -> i32 {
    if x <= 0 {
        0
    } else {
        31 - x.leading_zeros() as i32
    }
}
const fn shift(x: i32, n: i32) -> i32 {
    if n >= 0 {
        x >> n
    } else {
        x.wrapping_shl((-n) as u32)
    }
}
const fn mul(a: i32, b: i32) -> i32 {
    (a * b) >> 15
}
fn mul16_32(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 15) as i32
}
fn mul32(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 31) as i32
}
fn round(x: i32) -> i16 {
    ((x.wrapping_add(2048)) >> 12).clamp(-32767, 32767) as i16
}
fn frac_div(a: i32, b: i32) -> i32 {
    let s = ilog(b) - 29;
    let a = shift(a, s);
    let b = shift(b, s);
    let x = (b + 32768) >> 16;
    let i = ilog(x);
    let n = shift(x, i - 15) - 32768;
    let mut r = 30840 + mul(-15420, n);
    r -= mul(r, mul(r, n) + r - 32768);
    r -= 1 + mul(r, mul(r, n) + r - 32768);
    let rcp = (shift(r, i - 16) + 4) >> 3;
    let mut result = mul16_32(rcp, a);
    let rem = ((a + 2) >> 2) - mul32(result, b);
    result = result.wrapping_add(mul16_32(rcp, rem).wrapping_shl(2));
    if result >= 536_870_912 {
        i32::MAX
    } else if result <= -536_870_912 {
        -i32::MAX
    } else {
        result << 2
    }
}
fn autocorr(x: &[i16], order: usize, overlap: usize) -> Vec<i32> {
    let mut x = x.to_vec();
    let n = x.len();
    for i in 0..overlap {
        x[i] = mul(i32::from(x[i]), i32::from(transform::window(i))) as i16;
        x[n - i - 1] = mul(i32::from(x[n - i - 1]), i32::from(transform::window(i))) as i16;
    }
    let ac0 = 1
        + (n as i32 * 128)
        + x.iter()
            .map(|&v| (i32::from(v) * i32::from(v)) >> 9)
            .sum::<i32>();
    let scale = ((ilog(ac0) - 20) / 2).max(0);
    if scale > 0 {
        for v in &mut x {
            *v = ((i32::from(*v) + (1 << (scale - 1))) >> scale) as i16;
        }
    }
    let mut ac: Vec<i32> = (0..=order)
        .map(|lag| {
            (lag..n)
                .map(|i| i32::from(x[i]) * i32::from(x[i - lag]))
                .sum()
        })
        .collect();
    if scale == 0 {
        ac[0] += 1;
    }
    if ac[0] < 268_435_456 {
        let s = 28 - ilog(ac[0]);
        for v in &mut ac {
            *v <<= s;
        }
    } else if ac[0] >= 536_870_912 {
        let s = if ac[0] >= 1_073_741_824 { 2 } else { 1 };
        for v in &mut ac {
            *v >>= s;
        }
    }
    ac
}
fn lpc(ac: &[i32]) -> Vec<i16> {
    let order = ac.len() - 1;
    let mut values = vec![0_i32; order];
    let mut error = ac[0];
    if error != 0 {
        for i in 0..order {
            let acc: i64 = (0..i)
                .map(|j| i64::from(values[j]) * i64::from(ac[i - j]))
                .sum();
            let rr = (acc >> 31) as i32 + (ac[i + 1] >> 6);
            let r = -frac_div(rr.wrapping_shl(6), error);
            values[i] = r >> 6;
            for j in 0..i.div_ceil(2) {
                let a = values[j];
                let b = values[i - 1 - j];
                values[j] = a + mul32(r, b);
                values[i - 1 - j] = b + mul32(r, a);
            }
            error -= mul32(mul32(r, r), error);
            if error <= ac[0] >> 10 {
                break;
            }
        }
    }
    fit_lpc(values)
}
fn fit_lpc(mut values: Vec<i32>) -> Vec<i16> {
    // Q25 coefficient fitting and rounded bandwidth expansion from celt_lpc.c.
    for _ in 0..10 {
        let mut index = 0;
        let mut maximum = 0;
        for (i, &v) in values.iter().enumerate() {
            if v.abs() > maximum {
                maximum = v.abs();
                index = i;
            }
        }
        let maximum = (maximum + 4096) >> 13;
        if maximum <= 32767 {
            return values.iter().map(|&v| ((v + 4096) >> 13) as i16).collect();
        }
        let maximum = maximum.min(163_838);
        let mut chirp = 65470 - (((maximum - 32767) << 14) / ((maximum * (index as i32 + 1)) >> 2));
        let delta = chirp - 65536;
        let last = values.len() - 1;
        for (i, v) in values.iter_mut().enumerate() {
            *v = ((i64::from(*v) * i64::from(chirp)) >> 16) as i32;
            if i != last {
                chirp += chirp.wrapping_mul(delta).wrapping_add(32768) >> 16;
            }
        }
    }
    let mut fallback = vec![0; values.len()];
    fallback[0] = 4096;
    fallback
}
fn best(corr: &[i32], y: &[i16], len: usize, yshift: u32, maxcorr: i32) -> [usize; 2] {
    let xshift = ilog(maxcorr) - 14;
    let mut syy = 1 + y[..len]
        .iter()
        .map(|&v| (i32::from(v) * i32::from(v)) >> yshift)
        .sum::<i32>();
    let mut num = [-1, -1];
    let mut den = [0, 0];
    let mut best = [0, 1];
    for (i, &value) in corr.iter().enumerate() {
        if value > 0 {
            let v = i32::from(shift(value, xshift) as i16);
            let n = mul(v, v);
            if mul16_32(n, den[1]) > mul16_32(num[1], syy) {
                let slot = if mul16_32(n, den[0]) > mul16_32(num[0], syy) {
                    num[1] = num[0];
                    den[1] = den[0];
                    best[1] = best[0];
                    0
                } else {
                    1
                };
                num[slot] = n;
                den[slot] = syy;
                best[slot] = i;
            }
        }
        syy += ((i32::from(y[i + len]) * i32::from(y[i + len])) >> yshift)
            - ((i32::from(y[i]) * i32::from(y[i])) >> yshift);
        syy = syy.max(1);
    }
    best
}
fn pitch(memory: &[Vec<i32>]) -> usize {
    let max = memory
        .iter()
        .flat_map(|x| &x[..HISTORY])
        .map(|x| x.abs())
        .max()
        .unwrap_or(1)
        .max(1);
    let shift = (ilog(max) - 10).max(0) + i32::from(memory.len() == 2);
    let mut low = vec![0_i16; HISTORY / 2];
    for channel in memory {
        low[0] += ((channel[1] >> (shift + 2)) + (channel[0] >> (shift + 1))) as i16;
        for i in 1..HISTORY / 2 {
            low[i] += ((channel[2 * i - 1] >> (shift + 2))
                + (channel[2 * i + 1] >> (shift + 2))
                + (channel[2 * i] >> (shift + 1))) as i16;
        }
    }
    let mut ac = autocorr(&low, 4, 0);
    ac[0] += ac[0] >> 13;
    for (i, value) in ac.iter_mut().enumerate().take(5).skip(1) {
        *value -= mul16_32((2 * i * i) as i32, *value);
    }
    let mut coeff = lpc(&ac);
    let mut tmp = 32767;
    for v in &mut coeff {
        tmp = mul(29491, tmp);
        *v = mul(i32::from(*v), tmp) as i16;
    }
    let c = [
        i32::from(coeff[0]) + 3277,
        i32::from(coeff[1]) + mul(26214, i32::from(coeff[0])),
        i32::from(coeff[2]) + mul(26214, i32::from(coeff[1])),
        i32::from(coeff[3]) + mul(26214, i32::from(coeff[2])),
        mul(26214, i32::from(coeff[3])),
    ];
    let input = low.clone();
    for i in 0..low.len() {
        let mut sum = i32::from(input[i]) << 12;
        for j in 0..5.min(i) {
            sum += c[j] * i32::from(input[i - j - 1]);
        }
        low[i] = round(sum);
    }
    pitch_search(&low)
}

fn pitch_search(low: &[i16]) -> usize {
    let len = 1328;
    let max_pitch = 620;
    let mut x4: Vec<_> = (0..len / 4).map(|j| low[360 + 2 * j]).collect();
    let mut y4: Vec<_> = (0..(len + max_pitch) / 4).map(|j| low[2 * j]).collect();
    let max = x4
        .iter()
        .chain(&y4)
        .map(|&x| i32::from(x).abs())
        .max()
        .unwrap_or(1)
        .max(1);
    let shift = (ilog(max) - 11).max(0);
    for x in x4.iter_mut().chain(&mut y4) {
        *x >>= shift;
    }
    let corr: Vec<i32> = (0..max_pitch / 4)
        .map(|i| {
            x4.iter()
                .zip(&y4[i..])
                .map(|(&a, &b)| i32::from(a) * i32::from(b))
                .sum()
        })
        .collect();
    let coarse = best(
        &corr,
        &y4,
        len / 4,
        0,
        *corr.iter().max().unwrap_or(&1).max(&1),
    );
    let mut corr = vec![0_i32; max_pitch / 2];
    for (i, c) in corr.iter_mut().enumerate() {
        if i.abs_diff(2 * coarse[0]) > 2 && i.abs_diff(2 * coarse[1]) > 2 {
            continue;
        }
        *c = (0..len / 2)
            .map(|j| (i32::from(low[360 + j]) * i32::from(low[i + j])) >> (2 * shift))
            .sum::<i32>()
            .max(-1);
    }
    let fine = best(
        &corr,
        low,
        len / 2,
        (2 * shift + 1) as u32,
        *corr.iter().max().unwrap_or(&1).max(&1),
    )[0];
    let offset = if fine > 0 && fine < max_pitch / 2 - 1 {
        let a = corr[fine - 1];
        let b = corr[fine];
        let c = corr[fine + 1];
        if c - a > mul16_32(22938, b - a) {
            1
        } else if a - c > mul16_32(22938, b - c) {
            -1
        } else {
            0
        }
    } else {
        0
    };
    (720_i32 - (2 * fine as i32 - offset)) as usize
}

fn fit_prediction(exc: &[i16], coefficients: &mut [i16; ORDER]) {
    let mut ac = autocorr(exc, ORDER, 120);
    ac[0] += ac[0] >> 13;
    for (i, value) in ac.iter_mut().enumerate().skip(1) {
        *value -= mul16_32((2 * i * i) as i32, *value);
    }
    coefficients.copy_from_slice(&lpc(&ac));
    loop {
        let sum = 4096
            + coefficients
                .iter()
                .map(|&x| i32::from(x).abs())
                .sum::<i32>();
        if sum < 65535 {
            break;
        }
        let mut tmp = 32767;
        for x in coefficients.iter_mut() {
            tmp = mul(32440, tmp);
            *x = mul(i32::from(*x), tmp) as i16;
        }
    }
}
