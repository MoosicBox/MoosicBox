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

pub static EPSILON: f32 = 0.001;
static SCROLLBAR_SIZE: AtomicU16 = AtomicU16::new(16);

#[macro_export]
macro_rules! float_eq {
    ($a:expr, $b:expr $(,)?) => {{ ($a - $b).abs() < $crate::layout::EPSILON }};
}

#[macro_export]
macro_rules! float_lt {
    ($a:expr, $b:expr $(,)?) => {{ $a - $b <= -$crate::layout::EPSILON }};
}

#[macro_export]
macro_rules! float_lte {
    ($a:expr, $b:expr $(,)?) => {{ $a - $b < $crate::layout::EPSILON }};
}

#[macro_export]
macro_rules! float_gt {
    ($a:expr, $b:expr $(,)?) => {{ $a - $b >= $crate::layout::EPSILON }};
}

#[macro_export]
macro_rules! float_gte {
    ($a:expr, $b:expr $(,)?) => {{ $a - $b > -$crate::layout::EPSILON }};
}

#[macro_export]
macro_rules! min_float {
    ($a:expr, $b:expr $(,)?) => {{ if $a <= $b { $a } else { $b } }};
}

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
