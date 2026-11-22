//! Persistent fixed-point CELT pitch postfilter from `celt.c`.
// ADD32/SUB32 operate at the reference's 32-bit signal width before SIG_SAT.
// In particular, hostile entropy can produce transform outputs outside SIG_SAT;
// delayed-sample sums must wrap rather than panic in checked Rust builds.
use super::{PostFilterParams, transform};
#[derive(Clone, Copy, Default)]
struct Params {
    period: usize,
    gain: i32,
    tap: usize,
}
#[derive(Default)]
pub(super) struct Postfilter {
    history: Vec<Vec<i32>>,
    old: Params,
    current: Params,
}
fn mul(a: i32, b: i32) -> i32 {
    i32::try_from((i64::from(a) * i64::from(b)) >> 15).expect("bounded CELT filter coefficient")
}
fn taps(p: Params) -> [i32; 3] {
    let gains = [[10048, 7112, 4248], [15200, 8784, 0], [26208, 3280, 0]];
    gains[p.tap].map(|v| (p.gain * v + 16384) >> 15)
}
#[allow(clippy::many_single_char_names)] // Mirrors reference filter equations.
fn comb(x: &mut [i32], offset: usize, n: usize, old: Params, new: Params) {
    if old.gain == 0 && new.gain == 0 {
        return;
    }
    let p0 = old.period.max(15);
    let p1 = new.period.max(15);
    let a = taps(old);
    let b = taps(new);
    let overlap = if old.gain == new.gain && p0 == p1 && old.tap == new.tap {
        0
    } else {
        120
    };
    // The optimized upstream constant filter caches the next two input samples
    // before writing output. Preserve that distinction from recursive reads.
    let mut delayed = [
        x[offset - p1 - 2],
        x[offset - p1 - 1],
        x[offset - p1],
        x[offset - p1 + 1],
    ];
    for i in 0..n {
        let pos = offset + i;
        let next = x[pos - p1 + 2];
        let contribution = |taps: [i32; 3], center: i32, near: i32, far: i32| {
            mul(taps[0], center)
                .wrapping_add(mul(taps[1], near))
                .wrapping_add(mul(taps[2], far))
        };
        if i < overlap {
            let f = mul(
                i32::from(transform::window(i)),
                i32::from(transform::window(i)),
            );
            let old_taps = a.map(|v| mul(32767 - f, v));
            let new_taps = b.map(|v| mul(f, v));
            let value = x[pos]
                .wrapping_add(contribution(
                    old_taps,
                    x[pos - p0],
                    x[pos - p0 + 1].wrapping_add(x[pos - p0 - 1]),
                    x[pos - p0 + 2].wrapping_add(x[pos - p0 - 2]),
                ))
                .wrapping_add(contribution(
                    new_taps,
                    delayed[2],
                    delayed[3].wrapping_add(delayed[1]),
                    next.wrapping_add(delayed[0]),
                ))
                .wrapping_sub(3);
            x[pos] = value.clamp(-536_870_911, 536_870_911);
        } else if new.gain != 0 {
            let value = x[pos]
                .wrapping_add(contribution(
                    b,
                    delayed[2],
                    delayed[3].wrapping_add(delayed[1]),
                    next.wrapping_add(delayed[0]),
                ))
                .wrapping_sub(1);
            x[pos] = value.clamp(-536_870_911, 536_870_911);
        }
        delayed = [delayed[1], delayed[2], delayed[3], next];
    }
}
impl Postfilter {
    pub(super) fn conceal_tail(&self, memory: &[i32]) -> Vec<i32> {
        let source = memory;
        let p = self.current;
        let t = taps(Params { gain: -p.gain, ..p });
        let period = p.period.max(15);
        let work: Vec<_> = (0..120)
            .map(|i| {
                let pos = 2048 + i;
                let value = source[pos]
                    .wrapping_add(mul(t[0], source[pos - period]))
                    .wrapping_add(mul(
                        t[1],
                        source[pos - period - 1].wrapping_add(source[pos - period + 1]),
                    ))
                    .wrapping_add(mul(
                        t[2],
                        source[pos - period - 2].wrapping_add(source[pos - period + 2]),
                    ));
                if p.gain == 0 {
                    value
                } else {
                    value.wrapping_sub(1).clamp(-536_870_911, 536_870_911)
                }
            })
            .collect();
        (0..60)
            .map(|i| {
                mul(i32::from(transform::window(i)), work[119 - i])
                    .wrapping_add(mul(i32::from(transform::window(119 - i)), work[i]))
            })
            .collect()
    }
    pub(super) fn remember_concealed(&mut self, channels: &[Vec<i32>]) {
        self.history = channels
            .iter()
            .map(|x| x[2048 - 1026..2048].to_vec())
            .collect();
    }
    pub(super) fn apply(
        &mut self,
        channels: &mut [Vec<i32>],
        params: Option<PostFilterParams>,
        lm: u8,
    ) {
        let next = params.map_or_else(Params::default, |p| Params {
            period: usize::from(p.period),
            gain: i32::from(p.gain_q8) * 128,
            tap: usize::from(p.tapset),
        });
        while self.history.len() < channels.len() {
            self.history.push(vec![0; 1026]);
        }
        for (channel, samples) in channels.iter_mut().enumerate() {
            let mut memory = self.history[channel].clone();
            let offset = memory.len();
            memory.extend_from_slice(samples);
            comb(&mut memory, offset, 120, self.old, self.current);
            if lm != 0 {
                comb(
                    &mut memory,
                    offset + 120,
                    samples.len() - 120,
                    self.current,
                    next,
                );
            }
            samples.copy_from_slice(&memory[offset..]);
            self.history[channel] = memory[memory.len() - 1026..].to_vec();
        }
        self.old = self.current;
        self.current = next;
        if lm != 0 {
            self.old = self.current;
        }
    }
}
