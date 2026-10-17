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

/// One physical axis of a sticky box, in a single current coordinate space.
///
/// Normal position already includes ancestor scrolling and ancestor sticky
/// displacement. Intervals must not include this box's own sticky displacement.
/// Call independently for X and Y; never alter the box's normal-flow allocation.
#[derive(Clone, Copy, Debug)]
pub struct StickyAxis {
    /// Normal border-box start.
    pub normal_start: f32,
    /// Border-box size, excluding margins.
    pub size: f32,
    /// Containing block start, adjusted for the box's effective start margin.
    pub containing_start: f32,
    /// Containing block end, adjusted for the box's effective end margin.
    pub containing_end: f32,
    /// Nearest scrollport start (not its intersection with ancestor clips).
    pub scrollport_start: f32,
    /// Nearest scrollport end.
    pub scrollport_end: f32,
    /// Start inset; `None` imposes no start-edge constraint.
    pub start_inset: Option<f32>,
    /// End inset; `None` imposes no end-edge constraint.
    pub end_inset: Option<f32>,
}

/// Invalid sticky input or an arithmetic overflow during constraint resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StickyError;

impl std::fmt::Display for StickyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("nonfinite or inverted sticky geometry")
    }
}

impl std::error::Error for StickyError {}

/// Computes sticky displacement without changing normal flow or scroll extents.
///
/// Insets of `None` count as zero when sizing the sticky view rectangle, but do
/// not constrain their corresponding edge. If that rectangle is smaller than
/// the box, its end inset is reduced (possibly negative) so the box fits; the
/// start edge has priority. Movement remains inside the containing interval.
/// An oversized containing-block child keeps its normal position when no legal
/// movement exists rather than panicking or creating an inverted clamp range.
/// With neither inset, displacement is zero.
///
/// # Errors
/// * Returns `StickyError` for nonfinite inputs, negative size, inverted intervals,
///   or nonfinite intermediate/output arithmetic. Finite negative insets are valid.
pub fn sticky_offset(axis: StickyAxis) -> Result<f32, StickyError> {
    let a = axis;
    if ![
        a.normal_start,
        a.size,
        a.containing_start,
        a.containing_end,
        a.scrollport_start,
        a.scrollport_end,
        a.start_inset.unwrap_or(0.0),
        a.end_inset.unwrap_or(0.0),
    ]
    .iter()
    .all(|x| x.is_finite())
        || a.size < 0.0
        || a.containing_end < a.containing_start
        || a.scrollport_end < a.scrollport_start
    {
        return Err(StickyError);
    }
    if a.start_inset.is_none() && a.end_inset.is_none() {
        return Ok(0.0);
    }
    let start = a.scrollport_start + a.start_inset.unwrap_or(0.0);
    let requested_end = a.scrollport_end - a.end_inset.unwrap_or(0.0);
    let required_end = start + a.size;
    let containing_limit = a.containing_end - a.size;
    if ![start, requested_end, required_end, containing_limit]
        .iter()
        .all(|x| x.is_finite())
    {
        return Err(StickyError);
    }
    let end = requested_end.max(required_end);
    let min_position = a.containing_start.min(a.normal_start);
    let max_position = containing_limit.max(a.normal_start);
    let mut position = a.normal_start;
    if a.end_inset.is_some() {
        position = position.min(end - a.size);
    }
    if a.start_inset.is_some() {
        position = position.max(start);
    }
    let position = position.max(min_position).min(max_position);
    let offset = position - a.normal_start;
    if ![start, end, min_position, max_position, offset]
        .iter()
        .all(|x| x.is_finite())
    {
        return Err(StickyError);
    }
    Ok(offset)
}

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

    fn sticky_axis() -> StickyAxis {
        StickyAxis {
            normal_start: -30.0,
            size: 20.0,
            containing_start: -80.0,
            containing_end: 200.0,
            scrollport_start: 0.0,
            scrollport_end: 100.0,
            start_inset: Some(0.0),
            end_inset: None,
        }
    }

    #[test]
    fn sticky_threshold_and_containing_end_preserve_normal_flow() {
        let axis = sticky_axis();
        assert_eq!(sticky_offset(axis), Ok(30.0));
        assert_eq!(
            sticky_offset(StickyAxis {
                normal_start: 10.0,
                ..axis
            }),
            Ok(0.0)
        );
        assert_eq!(
            sticky_offset(StickyAxis {
                containing_end: 5.0,
                ..axis
            }),
            Ok(15.0)
        );
    }

    #[test]
    fn sticky_end_only_no_inset_and_oversized_box() {
        let axis = sticky_axis();
        assert_eq!(
            sticky_offset(StickyAxis {
                normal_start: 120.0,
                start_inset: None,
                end_inset: Some(10.0),
                ..axis
            }),
            Ok(-50.0)
        );
        assert_eq!(
            sticky_offset(StickyAxis {
                start_inset: None,
                ..axis
            }),
            Ok(0.0)
        );
        // End inset becomes -50: oversized box pins at start rather than
        // switching unpredictably between incompatible edge constraints.
        assert_eq!(
            sticky_offset(StickyAxis {
                size: 150.0,
                end_inset: Some(0.0),
                ..axis
            }),
            Ok(30.0)
        );
        assert_eq!(
            sticky_offset(StickyAxis {
                containing_end: -20.0,
                ..axis
            }),
            Ok(0.0)
        );
    }

    #[test]
    fn sticky_current_coordinate_translation_is_invariant() {
        let axis = sticky_axis();
        let translated = StickyAxis {
            normal_start: axis.normal_start + 37.0,
            containing_start: axis.containing_start + 37.0,
            containing_end: axis.containing_end + 37.0,
            scrollport_start: axis.scrollport_start + 37.0,
            scrollport_end: axis.scrollport_end + 37.0,
            ..axis
        };
        assert_eq!(sticky_offset(axis), sticky_offset(translated));
    }

    #[test]
    fn sticky_rejects_nonfinite_inverted_and_overflowing_input() {
        let axis = sticky_axis();
        for invalid in [
            StickyAxis { size: -1.0, ..axis },
            StickyAxis {
                start_inset: Some(f32::NAN),
                ..axis
            },
            StickyAxis {
                scrollport_end: -1.0,
                ..axis
            },
            StickyAxis {
                containing_end: -90.0,
                ..axis
            },
            StickyAxis {
                scrollport_start: f32::MAX,
                scrollport_end: f32::MAX,
                start_inset: Some(f32::MAX),
                ..axis
            },
        ] {
            assert_eq!(sticky_offset(invalid), Err(StickyError));
        }
    }

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
