//! Pinned SILK PLC recovery-energy glue (`silk/PLC.c`).
// Copyright (c) 2006-2011, Skype Limited. BSD-3-Clause; see bundled COPYING.

#[derive(Clone, Debug, Default)]
pub(super) struct Recovery {
    energy: i32,
    shift: u32,
    lost: bool,
}

pub(super) fn energy(samples: &[i16]) -> (i32, u32) {
    let length = u32::try_from(samples.len()).expect("bounded SILK frame");
    let mut shift = length.ilog2();
    let accumulate = |shift: u32| {
        samples.chunks(2).fold(0_u32, |total, pair| {
            let sum = pair.iter().fold(0_u32, |sum, &sample| {
                sum + u32::try_from(i32::from(sample) * i32::from(sample)).expect("square")
            });
            total + (sum >> shift)
        })
    };
    let estimate = length + accumulate(shift);
    shift = (i32::try_from(shift).unwrap() + 3 - estimate.leading_zeros().cast_signed())
        .max(0)
        .cast_unsigned();
    (
        i32::try_from(accumulate(shift)).expect("energy has two headroom bits"),
        shift,
    )
}

pub(super) fn sqrt_approx(value: i32) -> i32 {
    if value <= 0 {
        return 0;
    }
    let zeros = value.leading_zeros();
    let frac = value
        .cast_unsigned()
        .rotate_right((24_u32.wrapping_sub(zeros)) & 31)
        & 127;
    let base = (if zeros & 1 == 1 { 32768 } else { 46214 }) >> (zeros / 2);
    base + i32::try_from((i64::from(base) * i64::from(213 * frac)) >> 16).unwrap()
}

impl Recovery {
    pub(super) fn process(&mut self, samples: &mut [i16], lost: bool) {
        if lost {
            (self.energy, self.shift) = energy(samples);
            self.lost = true;
            return;
        }
        if self.lost {
            let (mut current, shift) = energy(samples);
            if shift > self.shift {
                self.energy >>= shift - self.shift;
            } else {
                current >>= self.shift - shift;
            }
            if current > self.energy {
                let zeros = self.energy.leading_zeros() - 1;
                self.energy <<= zeros;
                current >>= 24_u32.saturating_sub(zeros);
                let fraction = self.energy / current.max(1);
                let mut gain = sqrt_approx(fraction) << 4;
                let slope = ((65536 - gain) / i32::try_from(samples.len()).unwrap()) << 2;
                for sample in samples {
                    *sample = i16::try_from((i64::from(gain) * i64::from(*sample)) >> 16)
                        .expect("recovery gain bounded by unity");
                    gain += slope;
                    if gain > 65536 {
                        break;
                    }
                }
            }
        }
        self.lost = false;
    }
}
