//! Persistent comfort-noise estimate from pinned libopus `silk/CNG.c`.
// Copyright (c) 2006-2011, Skype Limited. BSD-3-Clause; see bundled COPYING.

#[derive(Clone, Debug)]
pub(super) struct ComfortNoise {
    pub(super) excitation_q14: [i32; 320],
    pub(super) nlsf_q15: [i16; 16],
    pub(super) gain_q16: i32,
    rate: usize,
    seed: i32,
    synthesis_q14: [i32; 16],
}

fn wb(a: i32, b: i16) -> i32 {
    i32::try_from((i64::from(a) * i64::from(b)) >> 16).expect("Q16 coefficient bounds product")
}

impl Default for ComfortNoise {
    fn default() -> Self {
        Self {
            excitation_q14: [0; 320],
            nlsf_q15: [0; 16],
            gain_q16: 0,
            rate: 0,
            seed: 3_176_576,
            synthesis_q14: [0; 16],
        }
    }
}

impl ComfortNoise {
    pub(super) fn synthesize(
        &mut self,
        samples: &mut [i16],
        bandwidth: crate::Bandwidth,
        plc_random_q14: i16,
        plc_gain_q16: i32,
    ) -> crate::Result<()> {
        let multiply = |a: i32, b: i32| {
            i32::try_from((i64::from(a) * i64::from(b)) >> 16).expect("bounded fixed-point product")
        };
        let mut gain = multiply(i32::from(plc_random_q14), plc_gain_q16);
        if gain >= 1 << 21 || self.gain_q16 > 1 << 23 {
            gain = (gain >> 16) * (gain >> 16);
            gain = (self.gain_q16 >> 16) * (self.gain_q16 >> 16) - (gain << 5);
            gain = super::recovery::sqrt_approx(gain) << 16;
        } else {
            gain = multiply(gain, gain);
            gain = multiply(self.gain_q16, self.gain_q16) - (gain << 5);
            gain = super::recovery::sqrt_approx(gain) << 8;
        }
        let gain_q10 = gain >> 6;
        let order = if bandwidth == crate::Bandwidth::Wideband {
            16
        } else {
            10
        };
        let lpc = super::decoder::SilkDecoder::limit_lpc_coefficients(
            &self.nlsf_q15[..order],
            bandwidth,
        )?;
        let mut mask = 255;
        while mask > samples.len() {
            mask >>= 1;
        }
        let mut signal = self.synthesis_q14.to_vec();
        for sample in samples.iter_mut() {
            self.seed = self
                .seed
                .wrapping_mul(196_314_165)
                .wrapping_add(907_633_515);
            let index = usize::try_from((self.seed >> 24) & i32::try_from(mask).unwrap()).unwrap();
            let mut prediction = i32::try_from(order / 2).unwrap();
            for (j, &coefficient) in lpc.iter().enumerate() {
                prediction = prediction.wrapping_add(wb(signal[signal.len() - j - 1], coefficient));
            }
            let scaled = i32::try_from(
                (i64::from(prediction) << 4).clamp(i64::from(i32::MIN), i64::from(i32::MAX)),
            )
            .unwrap();
            let value = self.excitation_q14[index].saturating_add(scaled);
            signal.push(value);
            let value = multiply(value, gain_q10);
            let rounded = ((value >> 7).wrapping_add(1)) >> 1;
            *sample = sample.saturating_add(
                i16::try_from(rounded.clamp(i32::from(i16::MIN), i32::from(i16::MAX))).unwrap(),
            );
        }
        self.synthesis_q14
            .copy_from_slice(&signal[signal.len() - 16..]);
        Ok(())
    }

    pub(super) fn update(
        &mut self,
        rate: usize,
        inactive: bool,
        nlsf: &[i16],
        gains: &[i32],
        excitation: &[i32],
    ) {
        if self.rate != rate {
            let step = 32767 / i16::try_from(nlsf.len() + 1).expect("SILK LPC order");
            for (index, value) in self.nlsf_q15.iter_mut().take(nlsf.len()).enumerate() {
                *value = step * i16::try_from(index + 1).expect("SILK LPC order");
            }
            self.gain_q16 = 0;
            self.seed = 3_176_576;
            self.rate = rate;
        }
        self.synthesis_q14[..nlsf.len()].fill(0);
        if !inactive {
            return;
        }
        for (smoothed, &current) in self.nlsf_q15.iter_mut().zip(nlsf) {
            *smoothed += i16::try_from(wb(i32::from(current) - i32::from(*smoothed), 16348))
                .expect("convex NLSF update");
        }
        let subframe = rate / 200;
        let mut selected = 0;
        let mut maximum = 0;
        for (index, &gain) in gains.iter().enumerate() {
            if gain > maximum {
                maximum = gain;
                selected = index;
            }
        }
        let length = subframe * gains.len();
        self.excitation_q14
            .copy_within(..length - subframe, subframe);
        self.excitation_q14[..subframe]
            .copy_from_slice(&excitation[selected * subframe..(selected + 1) * subframe]);
        for &gain in gains {
            self.gain_q16 += wb(gain - self.gain_q16, 4634);
            if (i64::from(self.gain_q16) * 46396) >> 16 > i64::from(gain) {
                self.gain_q16 = gain;
            }
        }
    }
}
