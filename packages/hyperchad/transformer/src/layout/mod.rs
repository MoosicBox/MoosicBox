use std::sync::atomic::AtomicU16;

use crate::Container;

pub mod calc;
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

pub fn get_scrollbar_size() -> u16 {
    SCROLLBAR_SIZE.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn set_scrollbar_size(size: u16) {
    SCROLLBAR_SIZE.store(size, std::sync::atomic::Ordering::SeqCst);
}

pub trait Calc {
    fn calc(&self, container: &mut Container) -> bool;
}

#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
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

pub fn increase_opt(opt: &mut Option<f32>, value: f32) -> f32 {
    if let Some(existing) = *opt {
        opt.replace(existing + value);
        existing + value
    } else {
        opt.replace(value);
        value
    }
}

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
