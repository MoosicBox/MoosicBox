//! A small, portable, seedable PRNG.
//!
//! [`Rng`] is xoshiro256\*\* seeded through `SplitMix64`. Its output for a given seed is part of
//! this crate's contract: it is identical on every platform and does not change between
//! releases without a major version bump, so a seed reproduces a simulation anywhere.

/// One `SplitMix64` step: advances `state` and returns the next output.
pub const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A deterministic xoshiro256\*\* generator. Not cryptographically secure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// A generator whose sequence is determined by `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        let mut state = seed;
        Self {
            s: [
                splitmix64(&mut state),
                splitmix64(&mut state),
                splitmix64(&mut state),
                splitmix64(&mut state),
            ],
        }
    }

    /// The next 64 random bits.
    pub const fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// The next 32 random bits.
    #[allow(clippy::cast_possible_truncation)]
    pub const fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Fill `dest` with random bytes.
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }

    /// A uniform value in `0..bound`, without modulo bias (Lemire's method).
    ///
    /// # Panics
    ///
    /// * If `bound` is zero
    #[allow(clippy::cast_possible_truncation)]
    pub fn below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "cannot sample below zero");
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let product = u128::from(self.next_u64()) * u128::from(bound);
            if (product as u64) >= threshold {
                return (product >> 64) as u64;
            }
        }
    }

    /// A uniform value in `range` (inclusive of both ends).
    ///
    /// # Panics
    ///
    /// * If the range is empty
    pub fn range_inclusive(&mut self, range: std::ops::RangeInclusive<u64>) -> u64 {
        let (start, end) = range.into_inner();
        assert!(start <= end, "cannot sample an empty range");
        if start == 0 && end == u64::MAX {
            return self.next_u64();
        }
        start + self.below(end - start + 1)
    }

    /// A uniform value in `range`.
    ///
    /// # Panics
    ///
    /// * If the range is empty
    pub fn range(&mut self, range: std::ops::Range<u64>) -> u64 {
        assert!(range.start < range.end, "cannot sample an empty range");
        range.start + self.below(range.end - range.start)
    }

    /// A uniform index in `0..len`.
    ///
    /// # Panics
    ///
    /// * If `len` is zero
    #[allow(clippy::cast_possible_truncation)]
    pub fn index(&mut self, len: usize) -> usize {
        self.below(len as u64) as usize
    }

    /// A uniform `f64` in `[0, 1)`.
    #[allow(clippy::cast_precision_loss)]
    pub const fn unit_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1_u64 << 53) as f64)
    }

    /// `true` with probability `p` (clamped to `[0, 1]`).
    pub const fn chance(&mut self, p: f64) -> bool {
        self.unit_f64() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sequence_is_stable() {
        // Pinned values: changing them breaks reproducibility of recorded seeds.
        let mut rng = Rng::new(0);
        let values: Vec<u64> = (0..3).map(|_| rng.next_u64()).collect();
        let mut again = Rng::new(0);
        assert_eq!(values, (0..3).map(|_| again.next_u64()).collect::<Vec<_>>());
        assert_eq!(values, PINNED_SEED_0);
    }

    const PINNED_SEED_0: [u64; 3] = [
        0x99ec_5f36_cb75_f2b4,
        0xbf6e_1f78_4956_452a,
        0x1a5f_849d_4933_e6e0,
    ];

    #[test]
    fn below_stays_in_bounds() {
        let mut rng = Rng::new(11);
        for bound in [1, 2, 3, 7, 1000, u64::MAX] {
            for _ in 0..100 {
                assert!(rng.below(bound) < bound);
            }
        }
    }

    #[test]
    fn ranges_cover_their_ends() {
        let mut rng = Rng::new(5);
        let mut seen = [false; 3];
        for _ in 0..200 {
            seen[usize::try_from(rng.range_inclusive(10..=12) - 10).unwrap()] = true;
        }
        assert_eq!(seen, [true; 3]);
        assert_eq!(rng.range_inclusive(4..=4), 4);
    }

    #[test]
    fn unit_f64_is_in_unit_interval() {
        let mut rng = Rng::new(99);
        for _ in 0..1000 {
            let x = rng.unit_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn fill_bytes_handles_partial_chunks() {
        let mut rng = Rng::new(1);
        let mut buf = [0_u8; 13];
        rng.fill_bytes(&mut buf);
        assert!(buf.iter().any(|&b| b != 0));
    }
}
