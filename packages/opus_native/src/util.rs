#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

/// Integer base-2 logarithm of x
///
/// Returns `floor(log2(x)) + 1` for x > 0, or 0 for x == 0
///
/// The minimum number of bits required to store a positive integer n in
/// binary, or 0 for a non-positive integer n.
///
/// # Examples
/// * `ilog(0)` = 0
/// * `ilog(1)` = 1 (floor(log2(1)) + 1 = 0 + 1)
/// * `ilog(2)` = 2 (floor(log2(2)) + 1 = 1 + 1)
/// * `ilog(3)` = 2
/// * `ilog(4)` = 3 (floor(log2(4)) + 1 = 2 + 1)
///
/// # Use Cases
/// * Range decoder: Bit counting for entropy coding (Section 4.1.6)
/// * SILK decoder: Division precision computation in Levinson recursion (Section 4.2.7.5.8)
///
/// RFC 6716 lines 368-375
#[must_use]
pub const fn ilog(x: u32) -> u32 {
    if x == 0 { 0 } else { 32 - x.leading_zeros() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ilog_zero() {
        assert_eq!(ilog(0), 0);
    }

    #[test]
    fn test_ilog_powers_of_two() {
        assert_eq!(ilog(1), 1); // floor(log2(1)) + 1 = 0 + 1
        assert_eq!(ilog(2), 2); // floor(log2(2)) + 1 = 1 + 1
        assert_eq!(ilog(4), 3); // floor(log2(4)) + 1 = 2 + 1
        assert_eq!(ilog(8), 4);
        assert_eq!(ilog(16), 5);
        assert_eq!(ilog(256), 9);
        assert_eq!(ilog(1024), 11);
    }

    #[test]
    fn test_ilog_non_powers() {
        assert_eq!(ilog(3), 2); // floor(log2(3)) + 1 = 1 + 1
        assert_eq!(ilog(5), 3); // floor(log2(5)) + 1 = 2 + 1
        assert_eq!(ilog(255), 8);
        assert_eq!(ilog(257), 9);
    }
}
