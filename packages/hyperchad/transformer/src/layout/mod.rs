use std::sync::atomic::AtomicU16;

use crate::Container;

pub mod calc;
pub mod calc_v2;
pub mod font;

static EPSILON: f32 = 0.001;
static SCROLLBAR_SIZE: AtomicU16 = AtomicU16::new(16);

pub fn get_scrollbar_size() -> u16 {
    SCROLLBAR_SIZE.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn set_scrollbar_size(size: u16) {
    SCROLLBAR_SIZE.store(size, std::sync::atomic::Ordering::SeqCst);
}

pub trait Calc {
    fn calc(&self, container: &mut Container) -> bool;
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

pub(crate) fn increase_opt(opt: &mut Option<f32>, value: f32) -> f32 {
    if let Some(existing) = *opt {
        opt.replace(existing + value);
        existing + value
    } else {
        opt.replace(value);
        value
    }
}

pub(crate) fn set_value<T: PartialEq + Copy>(opt: &mut Option<T>, value: T) -> Option<T> {
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

pub(crate) fn set_float(opt: &mut Option<f32>, value: f32) -> Option<f32> {
    if let Some(existing) = *opt {
        if (existing - value).abs() >= EPSILON {
            *opt = Some(value);
            return *opt;
        }
    } else {
        *opt = Some(value);
        return *opt;
    }

    None
}
