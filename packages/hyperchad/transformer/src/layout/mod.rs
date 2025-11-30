//! Layout calculation engine for UI containers.
//!
//! This module provides layout calculation functionality including flexbox layout, positioning,
//! and size calculations. It includes utilities for floating-point comparisons, rectangle operations,
//! and the [`Calc`](crate::layout::Calc) trait for implementing custom layout algorithms. Requires the `layout` feature.

use std::sync::atomic::AtomicU16;

use crate::Container;

/// Layout calculation implementation with the `Calculator` type and layout algorithms.
pub mod calc;
/// Font metrics traits and types for text measurement during layout.
pub mod font;

/// Epsilon value for floating-point comparisons in layout calculations.
///
/// Used by float comparison macros to determine equality within a tolerance of 0.001.
pub static EPSILON: f32 = 0.001;
static SCROLLBAR_SIZE: AtomicU16 = AtomicU16::new(16);

/// Compares two floats for approximate equality within epsilon tolerance.
///
/// Returns `true` if the absolute difference between `$a` and `$b` is less than
/// [`EPSILON`](crate::layout::EPSILON) (0.001).
///
/// # Examples
///
/// ```rust
/// # use hyperchad_transformer::float_eq;
/// assert!(float_eq!(1.0_f32, 1.0005_f32));
/// assert!(!float_eq!(1.0_f32, 1.01_f32));
/// ```
#[macro_export]
macro_rules! float_eq {
    ($a:expr, $b:expr $(,)?) => {{ ($a - $b).abs() < $crate::layout::EPSILON }};
}

/// Compares if float `$a` is less than `$b` with epsilon tolerance.
///
/// Returns `true` if `$a` is less than `$b` by at least [`EPSILON`](crate::layout::EPSILON).
///
/// # Examples
///
/// ```rust
/// # use hyperchad_transformer::float_lt;
/// assert!(float_lt!(1.0, 2.0));
/// assert!(!float_lt!(2.0, 1.0));
/// ```
#[macro_export]
macro_rules! float_lt {
    ($a:expr, $b:expr $(,)?) => {{ $a - $b <= -$crate::layout::EPSILON }};
}

/// Compares if float `$a` is less than or approximately equal to `$b` with epsilon tolerance.
///
/// Returns `true` if `$a` is less than `$b` or within [`EPSILON`](crate::layout::EPSILON) of `$b`.
///
/// # Examples
///
/// ```rust
/// # use hyperchad_transformer::float_lte;
/// assert!(float_lte!(1.0, 2.0));
/// assert!(float_lte!(1.0, 1.0001));
/// ```
#[macro_export]
macro_rules! float_lte {
    ($a:expr, $b:expr $(,)?) => {{ $a - $b < $crate::layout::EPSILON }};
}

/// Compares if float `$a` is greater than `$b` with epsilon tolerance.
///
/// Returns `true` if `$a` is greater than `$b` by at least [`EPSILON`](crate::layout::EPSILON).
///
/// # Examples
///
/// ```rust
/// # use hyperchad_transformer::float_gt;
/// assert!(float_gt!(2.0, 1.0));
/// assert!(!float_gt!(1.0, 2.0));
/// ```
#[macro_export]
macro_rules! float_gt {
    ($a:expr, $b:expr $(,)?) => {{ $a - $b >= $crate::layout::EPSILON }};
}

/// Compares if float `$a` is greater than or approximately equal to `$b` with epsilon tolerance.
///
/// Returns `true` if `$a` is greater than `$b` or within [`EPSILON`](crate::layout::EPSILON) of `$b`.
///
/// # Examples
///
/// ```rust
/// # use hyperchad_transformer::float_gte;
/// assert!(float_gte!(2.0, 1.0));
/// assert!(float_gte!(1.0, 1.0001));
/// ```
#[macro_export]
macro_rules! float_gte {
    ($a:expr, $b:expr $(,)?) => {{ $a - $b > -$crate::layout::EPSILON }};
}

/// Returns the minimum of two float values.
///
/// # Examples
///
/// ```rust
/// # use hyperchad_transformer::min_float;
/// assert_eq!(min_float!(1.0, 2.0), 1.0);
/// assert_eq!(min_float!(3.0, 1.5), 1.5);
/// ```
#[macro_export]
macro_rules! min_float {
    ($a:expr, $b:expr $(,)?) => {{ if $a <= $b { $a } else { $b } }};
}

/// Returns the maximum of two float values.
///
/// # Examples
///
/// ```rust
/// # use hyperchad_transformer::max_float;
/// assert_eq!(max_float!(1.0, 2.0), 2.0);
/// assert_eq!(max_float!(3.0, 1.5), 3.0);
/// ```
#[macro_export]
macro_rules! max_float {
    ($a:expr, $b:expr $(,)?) => {{ if $a > $b { $a } else { $b } }};
}

/// Gets the current scrollbar size in pixels.
#[must_use]
pub fn get_scrollbar_size() -> u16 {
    SCROLLBAR_SIZE.load(std::sync::atomic::Ordering::SeqCst)
}

/// Sets the scrollbar size in pixels for layout calculations.
pub fn set_scrollbar_size(size: u16) {
    SCROLLBAR_SIZE.store(size, std::sync::atomic::Ordering::SeqCst);
}

/// Trait for types that can perform layout calculations on containers.
pub trait Calc {
    /// Performs layout calculation on the given container.
    ///
    /// Returns `true` if the layout changed, `false` otherwise.
    fn calc(&self, container: &mut Container) -> bool;
}

/// Represents a rectangular region with position and dimensions.
#[derive(Clone, Copy, Default)]
pub struct Rect {
    /// X coordinate.
    pub x: f32,
    /// Y coordinate.
    pub y: f32,
    /// Width of the rectangle.
    pub width: f32,
    /// Height of the rectangle.
    pub height: f32,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
#[inline]
#[must_use]
pub(crate) fn order_float(a: &f32, b: &f32) -> std::cmp::Ordering {
    if a > b {
        std::cmp::Ordering::Greater
    } else if a < b {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Equal
    }
}

/// Increases an optional float value by the given amount.
///
/// If the option is `None`, sets it to `value`. Returns the new value.
pub fn increase_opt(opt: &mut Option<f32>, value: f32) -> f32 {
    if let Some(existing) = *opt {
        opt.replace(existing + value);
        existing + value
    } else {
        opt.replace(value);
        value
    }
}

/// Sets an optional value if it differs from the current value.
///
/// Returns `Some(value)` if changed, `None` if unchanged.
pub fn set_value<T: PartialEq + Copy>(opt: &mut Option<T>, value: T) -> Option<T> {
    if let Some(existing) = *opt {
        if existing != value {
            *opt = Some(value);
            return *opt;
        }
    } else {
        *opt = Some(value);
        return *opt;
    }

    None
}

/// Sets an optional float value if it differs significantly from the current value.
///
/// Uses epsilon comparison to avoid float precision issues.
/// Returns `Some(value)` if changed, `None` if unchanged.
pub fn set_float(opt: &mut Option<f32>, value: f32) -> Option<f32> {
    if let Some(existing) = *opt {
        if !float_eq!(existing, value) {
            *opt = Some(value);
            return *opt;
        }
    } else {
        *opt = Some(value);
        return *opt;
    }

    None
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    #[test_log::test]
    #[serial(scrollbar_size)]
    fn get_and_set_scrollbar_size_updates_global_value() {
        let original = get_scrollbar_size();

        set_scrollbar_size(42);
        assert_eq!(get_scrollbar_size(), 42);

        set_scrollbar_size(100);
        assert_eq!(get_scrollbar_size(), 100);

        // Restore original value
        set_scrollbar_size(original);
    }

    #[test_log::test]
    fn order_float_returns_greater_when_a_greater_than_b() {
        assert_eq!(order_float(&2.0, &1.0), std::cmp::Ordering::Greater);
        assert_eq!(order_float(&100.5, &100.4), std::cmp::Ordering::Greater);
    }

    #[test_log::test]
    fn order_float_returns_less_when_a_less_than_b() {
        assert_eq!(order_float(&1.0, &2.0), std::cmp::Ordering::Less);
        assert_eq!(order_float(&-5.0, &0.0), std::cmp::Ordering::Less);
    }

    #[test_log::test]
    fn order_float_returns_equal_when_values_equal() {
        assert_eq!(order_float(&1.0, &1.0), std::cmp::Ordering::Equal);
        assert_eq!(order_float(&0.0, &0.0), std::cmp::Ordering::Equal);
    }

    #[test_log::test]
    fn increase_opt_sets_value_when_none() {
        let mut opt: Option<f32> = None;
        let result = increase_opt(&mut opt, 10.0);

        assert!((result - 10.0).abs() < f32::EPSILON);
        assert!((opt.unwrap() - 10.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn increase_opt_adds_to_existing_value() {
        let mut opt: Option<f32> = Some(5.0);
        let result = increase_opt(&mut opt, 10.0);

        assert!((result - 15.0).abs() < f32::EPSILON);
        assert!((opt.unwrap() - 15.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn increase_opt_handles_negative_values() {
        let mut opt: Option<f32> = Some(10.0);
        let result = increase_opt(&mut opt, -3.0);

        assert!((result - 7.0).abs() < f32::EPSILON);
        assert!((opt.unwrap() - 7.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn increase_opt_accumulates_multiple_calls() {
        let mut opt: Option<f32> = None;

        increase_opt(&mut opt, 5.0);
        increase_opt(&mut opt, 3.0);
        let result = increase_opt(&mut opt, 2.0);

        assert!((result - 10.0).abs() < f32::EPSILON);
        assert!((opt.unwrap() - 10.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn set_value_sets_when_none_and_returns_new_value() {
        let mut opt: Option<i32> = None;
        let result = set_value(&mut opt, 42);

        assert_eq!(result, Some(42));
        assert_eq!(opt, Some(42));
    }

    #[test_log::test]
    fn set_value_updates_when_different_value() {
        let mut opt: Option<i32> = Some(10);
        let result = set_value(&mut opt, 42);

        assert_eq!(result, Some(42));
        assert_eq!(opt, Some(42));
    }

    #[test_log::test]
    fn set_value_returns_none_when_same_value() {
        let mut opt: Option<i32> = Some(42);
        let result = set_value(&mut opt, 42);

        assert_eq!(result, None);
        assert_eq!(opt, Some(42));
    }

    #[test_log::test]
    fn set_float_sets_when_none_and_returns_new_value() {
        let mut opt: Option<f32> = None;
        let result = set_float(&mut opt, 7.25);

        assert!(result.is_some());
        assert!((result.unwrap() - 7.25).abs() < f32::EPSILON);
        assert!((opt.unwrap() - 7.25).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn set_float_updates_when_significantly_different() {
        let mut opt: Option<f32> = Some(1.0);
        let result = set_float(&mut opt, 2.0);

        assert!(result.is_some());
        assert!((result.unwrap() - 2.0).abs() < f32::EPSILON);
        assert!((opt.unwrap() - 2.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn set_float_returns_none_when_within_epsilon() {
        let mut opt: Option<f32> = Some(1.0);
        // Value within epsilon (0.001) should not trigger an update
        let result = set_float(&mut opt, 1.0005);

        assert!(result.is_none());
        // Original value should be preserved
        assert!((opt.unwrap() - 1.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn set_float_updates_when_just_outside_epsilon() {
        let mut opt: Option<f32> = Some(1.0);
        // Value just outside epsilon should trigger an update
        let result = set_float(&mut opt, 1.002);

        assert!(result.is_some());
        assert!((opt.unwrap() - 1.002).abs() < f32::EPSILON);
    }
}
