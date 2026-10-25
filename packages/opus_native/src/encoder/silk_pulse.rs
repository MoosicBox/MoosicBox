//! Exact minimization over the encoder's original finite pulse candidates.

#[cfg(test)]
std::thread_local! {
    pub(super) static REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone, Copy)]
pub(super) struct PulseCost {
    pub lambda: i128,
    pub shrinkage: i64,
    pub optimize: bool,
}

impl PulseCost {
    fn centers(self, residual: i64) -> (i16, i16) {
        let nearest = ((residual + 8192) >> 14).clamp(-126, 126) as i16;
        let adjusted = residual.signum() * (residual.abs() - self.shrinkage).max(0);
        let penalized = ((adjusted + 8192) >> 14).clamp(-126, 126) as i16;
        (nearest, penalized)
    }

    fn evaluate(self, residual: i64, pulse: i16) -> i128 {
        let level = i64::from(pulse) * 16384 - i64::from(pulse.signum()) * 1280;
        let difference = i128::from(residual - level);
        difference * difference + self.lambda * 16384 * i128::from(pulse.unsigned_abs())
    }

    pub(super) fn select(self, residual: i64) -> i16 {
        let (nearest, penalized) = self.centers(residual);
        // On the matching-sign branch, t=abs(pulse), the cost is a strictly
        // convex quadratic centered at (2*(abs(residual)+1280)-lambda)/32768.
        // Opposite-sign nonzero pulses strictly lose to zero. Find the allowed
        // predecessor/successor of this vertex, NOT its unconstrained rounding.
        let vertex = 2 * (i128::from(residual.abs()) + 1280) - self.lambda;
        let floor = (vertex >> 15).clamp(-1, 128) as i16;
        let sign = residual.signum() as i16;
        let mut below = 0_i16;
        let mut above = 128_i16;
        for center in [nearest, penalized]
            .into_iter()
            .take(if self.optimize { 2 } else { 1 })
        {
            let center = center * sign;
            let low = (center - 1).max(1);
            let high = center + 1;
            if low > high {
                continue;
            }
            if low <= floor {
                below = below.max(high.min(floor));
            }
            if high > floor {
                above = above.min(low.max(floor + 1));
            }
        }
        let ordinal = |pulse: i16| {
            if (nearest - 1..=nearest + 1).contains(&pulse) {
                (pulse - nearest + 1) as usize
            } else if pulse == 0 {
                3
            } else {
                (pulse - penalized + 5) as usize
            }
        };
        let mut best = (self.evaluate(residual, 0), ordinal(0), 0);
        for magnitude in [below, above] {
            if magnitude == 0 || magnitude == 128 {
                continue;
            }
            let pulse = magnitude * sign;
            let cost = self.evaluate(residual, pulse);
            let ordinal = ordinal(pulse);
            if (cost, ordinal) < (best.0, best.1) {
                best = (cost, ordinal, pulse);
            }
        }
        best.2
    }

    #[cfg(test)]
    pub(super) fn reference(self, residual: i64) -> i16 {
        // Intentionally retain the original enumeration independently of the
        // convex selector (including duplicate candidates and strict ties).
        let nearest = ((residual + 8192) >> 14).clamp(-126, 126) as i16;
        let adjusted = residual.signum() * (residual.abs() - self.shrinkage).max(0);
        let penalized = ((adjusted + 8192) >> 14).clamp(-126, 126) as i16;
        let mut best = (i128::MAX, 0);
        for pulse in (nearest - 1..=nearest + 1)
            .chain(std::iter::once(0))
            .chain((penalized - 1..=penalized + 1).filter(|_| self.optimize))
        {
            let level = i32::from(pulse) * 16384 - i32::from(pulse.signum()) * 1280;
            let difference = residual - i64::from(level);
            let cost = i128::from(difference) * i128::from(difference)
                + self.lambda * 16384 * i128::from(pulse.unsigned_abs());
            if cost < best.0 {
                best = (cost, pulse);
            }
        }
        best.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_pulse_crossings_extremes() {
        let mut checked = 0_u64;
        for base in [300_i128, 1800] {
            for pressure in 0..=12 {
                for optimize in [false, true] {
                    let lambda = base << pressure;
                    let cost = PulseCost {
                        lambda,
                        shrinkage: (lambda / 2) as i64,
                        optimize,
                    };
                    let mut check = |residual| {
                        assert_eq!(
                            cost.select(residual),
                            cost.reference(residual),
                            "residual={residual} lambda={lambda} optimize={optimize}"
                        );
                        checked += 1;
                    };
                    // Every adjacent signed level crossing, both sides of the
                    // zero discontinuity, and both nearest-center transitions.
                    for p in -127_i64..=127 {
                        let level = p * 16384 - p.signum() * 1280;
                        for center in [
                            p * 16384 - 8192,
                            p * 16384 + 8192,
                            level + 8192 + (lambda / 2) as i64,
                            level - 8192 - (lambda / 2) as i64,
                            level.midpoint(
                                (lambda * 16384 * i128::from(p.abs())
                                    / i128::from(level.abs().max(1)))
                                    as i64,
                            ),
                        ] {
                            for delta in -2..=2 {
                                check(center + delta);
                                check(-center + delta);
                            }
                        }
                    }
                    for residual in -32768..=32768 {
                        check(residual);
                    }
                    for residual in [i64::from(i32::MIN) * 8, i64::from(i32::MAX) * 8, -1, 0, 1] {
                        check(residual);
                    }
                }
            }
        }
        eprintln!("SILK_PULSE_DIFFERENTIAL checked={checked}");
    }
    #[test]
    fn finite_pulse_all_pair_crossings_offsets_signs() {
        let mut checked = 0_u64;
        for base in [300_i128, 1800] {
            for pressure in 0..=12 {
                let lambda = base << pressure;
                for p in -127_i64..=127 {
                    let lp = p * 16384 - p.signum() * 1280;
                    for q in p + 1..=127 {
                        let lq = q * 16384 - q.signum() * 1280;
                        let crossing = (i128::from(lp).pow(2) - i128::from(lq).pow(2)
                            + lambda * 16384 * i128::from(p.abs() - q.abs()))
                        .div_euclid(2 * i128::from(lp - lq));
                        for delta in -1..=1 {
                            let residual = crossing as i64 + delta;
                            for optimize in [false, true] {
                                let cost = PulseCost {
                                    lambda,
                                    shrinkage: (lambda / 2) as i64,
                                    optimize,
                                };
                                let pulse = cost.select(residual);
                                for offset in [512_i64, 1600] {
                                    for sign in [-1_i64, 1] {
                                        let shaped = (residual + offset) * sign;
                                        let (nearest, penalized) = cost.centers(residual);
                                        let mut best = (i128::MAX, 0);
                                        for candidate in (nearest - 1..=nearest + 1)
                                            .chain(std::iter::once(0))
                                            .chain(
                                                (penalized - 1..=penalized + 1)
                                                    .filter(|_| optimize),
                                            )
                                        {
                                            let level = i32::from(candidate) * 16384
                                                - i32::from(candidate.signum()) * 1280
                                                + offset as i32;
                                            let excitation = (i64::from(level) * sign) as i32;
                                            let difference =
                                                i128::from(shaped - i64::from(excitation));
                                            let original = difference * difference
                                                + lambda
                                                    * 16384
                                                    * i128::from(candidate.unsigned_abs());
                                            if original < best.0 {
                                                best = (original, candidate);
                                            }
                                        }
                                        assert_eq!(
                                            pulse, best.1,
                                            "r={residual} lambda={lambda} optimize={optimize} offset={offset} sign={sign}"
                                        );
                                        checked += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        eprintln!("SILK_PAIR_CROSSINGS checked={checked}");
    }
}

#[cfg(test)]
mod quantizer_tests {
    use super::super::*;

    #[test]
    fn pulse_quantizer_full_state_parity() {
        let mut rng = 1_u32;
        let input: Vec<_> = (0..80)
            .map(|_| {
                rng = rng.wrapping_mul(196_314_165).wrapping_add(907_633_515);
                (rng >> 16) as i32 - 32768
            })
            .collect();
        let previous: Vec<_> = input.iter().cycle().take(320).map(|&v| v as i16).collect();
        let pitch = Pitch {
            lag: 40,
            periodicity: 0,
            filters: vec![0; 4],
        };
        let lpc = [
            12000, -2000, 1000, -500, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let history = [100_i32; 16];
        let mut checked = 0;
        for gain_index in 0..64 {
            for rate_pressure in 0..=12 {
                for optimize_pulse_cost in [false, true] {
                    for pitch in [None, Some(&pitch)] {
                        for bitrate in [16000, 32000] {
                            let context = QuantizerInput {
                                input: &input,
                                lpc: &lpc,
                                history: &history,
                                old_gain: SilkDecoder::dequantize_gain(32),
                                gain: SilkDecoder::dequantize_gain(gain_index),
                                pitch,
                                previous_pcm: &previous,
                                rate: 16000,
                                bitrate,
                                rate_pressure,
                                optimize_pulse_cost,
                                unshaped: gain_index % 2 == 0,
                            };
                            pulse::REFERENCE.set(true);
                            let expected = quantize(context);
                            pulse::REFERENCE.set(false);
                            assert_eq!(
                                quantize(context),
                                expected,
                                "gain={gain_index} pressure={rate_pressure} optimize={optimize_pulse_cost} voiced={} bitrate={bitrate}",
                                pitch.is_some()
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
        eprintln!("SILK_QUANTIZER_DIFFERENTIAL checked={checked}");
    }
}
