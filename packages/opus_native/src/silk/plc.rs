//! Persistent SILK concealment parameters (`silk/PLC.c`, pinned reference).
// Copyright (c) 2006-2011, Skype Limited. BSD-3-Clause; see bundled COPYING.

#[derive(Clone, Debug)]
pub(super) struct Concealment {
    pub(super) pitch_q8: i32,
    pub(super) taps_q14: [i16; 5],
    pub(super) lpc_q12: [i16; 16],
    pub(super) gains_q16: [i32; 2],
    pub(super) ltp_scale_q14: i16,
    pub(super) subframe: usize,
    pub(super) subframes: usize,
    rate: usize,
    seed: i32,
    pub(super) random_q14: i16,
    pub(super) losses: usize,
    voiced: bool,
}

impl Default for Concealment {
    fn default() -> Self {
        Self {
            pitch_q8: 0,
            taps_q14: [0; 5],
            lpc_q12: [0; 16],
            gains_q16: [65536; 2],
            ltp_scale_q14: 0,
            subframe: 20,
            subframes: 2,
            rate: 0,
            seed: 0,
            random_q14: 0,
            losses: 0,
            voiced: false,
        }
    }
}

#[allow(clippy::cast_possible_truncation)] // Reference fixed-point recurrence checked for overflow.
fn inverse_prediction_gain(lpc: &[i16]) -> i32 {
    if lpc.iter().map(|&x| i32::from(x)).sum::<i32>() >= 4096 {
        return 0;
    }
    let mut a = lpc.iter().map(|&x| i32::from(x) << 12).collect::<Vec<_>>();
    let mut gain = 1_i32 << 30;
    for k in (0..a.len()).rev() {
        if a[k].abs() > 16_773_022 {
            return 0;
        }
        let reflection = -(a[k] << 7);
        let factor = (1_i32 << 30) - ((i64::from(reflection) * i64::from(reflection)) >> 32) as i32;
        gain = (((i64::from(gain) * i64::from(factor)) >> 32) as i32) << 2;
        if gain < 107_374 {
            return 0;
        }
        if k == 0 {
            break;
        }
        let q = factor.ilog2() + 1;
        let reciprocal = super::voiced::inverse_q(factor, q + 30);
        let round = |value: i64, shift: u32| ((value >> (shift - 1)) + 1) >> 1;
        for n in 0..k.div_ceil(2) {
            let first = a[n];
            let second = a[k - n - 1];
            let x =
                first.saturating_sub(round(i64::from(second) * i64::from(reflection), 31) as i32);
            let y =
                second.saturating_sub(round(i64::from(first) * i64::from(reflection), 31) as i32);
            let x = round(i64::from(x) * i64::from(reciprocal), q);
            let y = round(i64::from(y) * i64::from(reciprocal), q);
            let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) else {
                return 0;
            };
            a[n] = x;
            a[k - n - 1] = y;
        }
    }
    gain
}

impl Concealment {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::too_many_lines
    )] // Sequential reference PLC algorithm; fixed-point narrowing is bounded.
    pub(super) fn synthesize(
        &mut self,
        excitation: &[i32],
        pcm_history: &[i16],
        lpc_history: &mut [i32],
        geometry: (usize, usize),
    ) -> Vec<i16> {
        let (subframe, subframes) = geometry;
        let order = if self.rate == 16000 { 16 } else { 10 };
        let n = subframe * subframes;
        let memory = self.rate / 50;
        let wb = |a: i32, b: i16| ((i64::from(a) * i64::from(b)) >> 16) as i32;
        let ww = |a: i32, b: i32| ((i64::from(a) * i64::from(b)) >> 16) as i32;
        let rounded = |a: i32, shift: u32| ((a >> (shift - 1)).wrapping_add(1)) >> 1;
        let sat = |a: i32| a.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let mut saved_excitation = [0_i32; 320];
        saved_excitation[..excitation.len()].copy_from_slice(excitation);
        let excitation = &saved_excitation;
        let gains = self.gains_q16.map(|g| g >> 6);
        let mut random_pcm = Vec::new();
        for (k, gain) in gains.into_iter().enumerate() {
            random_pcm.extend(
                excitation[(k + subframes - 2) * subframe..(k + subframes - 1) * subframe]
                    .iter()
                    .map(|&x| sat(ww(x, gain) >> 8)),
            );
        }
        let (e1, s1) = super::recovery::energy(&random_pcm[..self.subframe]);
        let (e2, s2) = super::recovery::energy(&random_pcm[self.subframe..]);
        let random_start = if (e1 >> s2) < (e2 >> s1) {
            ((self.subframes - 1) * self.subframe).saturating_sub(128)
        } else {
            (self.subframes * self.subframe).saturating_sub(128)
        };
        let harmonic = [32440, 31130][self.losses.min(1)];
        let mut random_gain = if self.voiced {
            [31130, 26214]
        } else {
            [32440, 29491]
        }[self.losses.min(1)];
        let mut chirp = 64881_i32;
        for index in 0..order {
            self.lpc_q12[index] = rounded(chirp * i32::from(self.lpc_q12[index]), 16) as i16;
            if index + 1 < order {
                chirp += rounded(chirp * (64881 - 65536), 16);
            }
        }
        if self.losses == 0 {
            self.random_q14 = 16384;
            if self.voiced {
                self.random_q14 = (16384 - self.taps_q14.iter().map(|&x| i32::from(x)).sum::<i32>())
                    .max(3277) as i16;
                self.random_q14 =
                    ((i32::from(self.random_q14) * i32::from(self.ltp_scale_q14)) >> 14) as i16;
            } else {
                let inverse = inverse_prediction_gain(&self.lpc_q12[..order]);
                let scale = inverse.clamp(1 << 22, 1 << 27) << 3;
                random_gain = wb(scale, random_gain as i16) >> 14;
            }
        }
        let mut lag = rounded(self.pitch_q8, 8) as usize;
        let start = memory - lag - order - 2;
        let mut pcm = vec![0_i16; memory];
        let available = pcm_history.len().min(memory);
        pcm[memory - available..].copy_from_slice(&pcm_history[pcm_history.len() - available..]);
        let mut ltp = vec![0_i32; memory + n];
        let inv = super::voiced::inverse_q(self.gains_q16[1], 46).min(i32::MAX >> 1);
        for index in start + order..memory {
            let mut prediction = 0_i32;
            for j in 0..order {
                prediction = prediction
                    .wrapping_add(i32::from(pcm[index - j - 1]) * i32::from(self.lpc_q12[j]));
            }
            let value = (i32::from(pcm[index]) << 12).wrapping_sub(prediction);
            ltp[index] = wb(inv, sat(rounded(value, 12)));
        }
        let mut cursor = memory;
        for _ in 0..subframes {
            for _ in 0..subframe {
                let mut prediction = 2_i32;
                for j in 0..5 {
                    prediction =
                        prediction.wrapping_add(wb(ltp[cursor - lag + 2 - j], self.taps_q14[j]));
                }
                self.seed = self
                    .seed
                    .wrapping_mul(196_314_165)
                    .wrapping_add(907_633_515);
                let index = ((self.seed >> 25) & 127) as usize;
                ltp[cursor] = prediction
                    .wrapping_add(wb(excitation[random_start + index], self.random_q14))
                    .wrapping_shl(2);
                cursor += 1;
            }
            for tap in &mut self.taps_q14 {
                *tap = ((i32::from(*tap) * harmonic) >> 15) as i16;
            }
            self.random_q14 = ((i32::from(self.random_q14) * random_gain) >> 15) as i16;
            self.pitch_q8 = self
                .pitch_q8
                .wrapping_add(wb(self.pitch_q8, 655))
                .min(i32::try_from(self.rate / 1000 * 18 * 256).unwrap());
            lag = rounded(self.pitch_q8, 8) as usize;
        }
        let mut synthesis = lpc_history.to_vec();
        synthesis.resize(16, 0);
        let mut out = Vec::with_capacity(n);
        for index in 0..n {
            let mut prediction = i32::try_from(order / 2).unwrap();
            for j in 0..order {
                prediction =
                    prediction.wrapping_add(wb(synthesis[16 + index - j - 1], self.lpc_q12[j]));
            }
            let value = ltp[memory + index].wrapping_add(prediction.wrapping_shl(4));
            synthesis.push(value);
            out.push(sat(rounded(ww(value, gains[1]), 8)));
        }
        lpc_history.copy_from_slice(&synthesis[n..n + 16]);
        self.losses += 1;
        out
    }

    pub(super) fn initialize(&mut self, rate: usize, samples: usize) {
        self.rate = rate;
        self.pitch_q8 = i32::try_from(samples * 128).unwrap();
    }

    pub(super) fn update(
        &mut self,
        geometry: (usize, bool),
        lags: &[i16],
        taps_q7: &[[i8; 5]],
        lpc: &[i32],
        gains: &[i32],
        scale_q14: i16,
    ) {
        let (rate, voiced) = geometry;
        self.rate = rate;
        self.voiced = voiced;
        self.losses = 0;
        self.subframe = rate / 200;
        self.subframes = gains.len();
        if voiced {
            let mut maximum = 0_i32;
            for back in 0..self.subframes {
                if back * self.subframe >= usize::try_from(lags[self.subframes - 1]).unwrap() {
                    break;
                }
                let index = self.subframes - 1 - back;
                let gain = taps_q7[index].iter().map(|&tap| i32::from(tap) << 7).sum();
                if gain > maximum {
                    maximum = gain;
                    self.pitch_q8 = i32::from(lags[index]) << 8;
                }
            }
            self.taps_q14 = [0; 5];
            let center = if maximum < 11469 {
                let scale = (11469 << 10) / maximum.max(1);
                (maximum * scale) >> 10
            } else if maximum > 15565 {
                let scale = (15565 << 14) / maximum;
                (maximum * scale) >> 14
            } else {
                maximum
            };
            self.taps_q14[2] = i16::try_from(center).expect("bounded PLC pitch gain");
        } else {
            self.pitch_q8 = i32::try_from(rate / 1000 * 18).unwrap() << 8;
            self.taps_q14 = [0; 5];
        }
        for (target, &value) in self.lpc_q12.iter_mut().zip(lpc) {
            *target = i16::try_from(value).expect("SILK LPC Q12 coefficients");
        }
        self.gains_q16.copy_from_slice(&gains[gains.len() - 2..]);
        self.ltp_scale_q14 = scale_q14;
    }
}
