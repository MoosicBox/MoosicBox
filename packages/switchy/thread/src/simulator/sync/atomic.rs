//! Simulated atomics.
//!
//! Every access is a switch point, so races between atomic operations are explored by the
//! scheduler. By default values are sequentially consistent whatever the requested ordering,
//! since only one thread runs at a time; with weak memory (`SIMULATOR_WEAK_MEMORY`) a load may
//! return any store the memory model allows for its ordering.

use std::fmt;

pub use std::sync::atomic::{Ordering, compiler_fence};

use super::super::{Op, Scheduler, new_key, scheduler};

/// A location id for the weak memory model, assigned on first weak access so that moving an
/// atomic keeps its history and a freed atomic's memory never aliases a new one.
fn location(id: &std::sync::atomic::AtomicU64) -> u64 {
    match id.load(Ordering::Relaxed) {
        0 => {
            let fresh = new_key();
            id.store(fresh, Ordering::Relaxed);
            fresh
        }
        existing => existing,
    }
}

/// The scheduler, when this run models weak memory.
fn weak() -> Option<std::sync::Arc<Scheduler>> {
    if std::thread::panicking() {
        return None;
    }
    let scheduler = scheduler();
    scheduler.weak_memory().then_some(scheduler)
}

fn switch(op: Op) {
    if std::thread::panicking() {
        return;
    }
    scheduler().yield_point(op);
}

/// A memory fence; a switch point. With weak memory, a full fence whatever `order` says
/// (conservative).
pub fn fence(order: Ordering) {
    switch(Op::Yield);
    std::sync::atomic::fence(order);
}

macro_rules! atomic_common {
    ($name:ident, $std:ty, $value:ty, $to:expr, $from:expr) => {
        #[doc = concat!("A simulated [`std::sync::atomic::", stringify!($name), "`].")]
        #[derive(Default)]
        pub struct $name {
            inner: $std,
            id: std::sync::atomic::AtomicU64,
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Debug::fmt(&self.inner, f)
            }
        }

        impl From<$value> for $name {
            fn from(value: $value) -> Self {
                Self::new(value)
            }
        }

        #[allow(
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap
        )]
        impl $name {
            /// A new atomic holding `value`.
            #[must_use]
            pub const fn new(value: $value) -> Self {
                Self {
                    inner: <$std>::new(value),
                    id: std::sync::atomic::AtomicU64::new(0),
                }
            }

            const fn bits(value: $value) -> u64 {
                ($to)(value)
            }

            const fn value(bits: u64) -> $value {
                ($from)(bits)
            }

            /// A read-modify-write: `update` gives the new value from the old, if any.
            fn update(
                &self,
                order: Ordering,
                update: impl FnOnce($value) -> Option<$value>,
            ) -> Result<$value, $value> {
                switch(Op::AtomicUpdate);
                if let Some(scheduler) = weak() {
                    let current = Self::bits(self.inner.load(Ordering::SeqCst));
                    let mut stored = None;
                    let old = scheduler.atomic_update(location(&self.id), current, order, |old| {
                        let new = update(Self::value(old))?;
                        stored = Some(new);
                        Some(Self::bits(new))
                    });
                    return match stored {
                        Some(new) => {
                            self.inner.store(new, Ordering::SeqCst);
                            Ok(Self::value(old))
                        }
                        None => Err(Self::value(old)),
                    };
                }
                let mut update = Some(update);
                self.inner.try_update(order, Ordering::SeqCst, |old| {
                    update.take().and_then(|update| update(old))
                })
            }

            /// Load the value; a switch point.
            pub fn load(&self, order: Ordering) -> $value {
                switch(Op::AtomicLoad);
                if let Some(scheduler) = weak() {
                    let current = Self::bits(self.inner.load(Ordering::SeqCst));
                    return Self::value(scheduler.atomic_load(location(&self.id), current, order));
                }
                self.inner.load(order)
            }

            /// Store `value`; a switch point.
            pub fn store(&self, value: $value, order: Ordering) {
                switch(Op::AtomicStore);
                if let Some(scheduler) = weak() {
                    let current = Self::bits(self.inner.load(Ordering::SeqCst));
                    scheduler.atomic_store(location(&self.id), current, Self::bits(value), order);
                    self.inner.store(value, Ordering::SeqCst);
                    return;
                }
                self.inner.store(value, order);
            }

            /// Replace the value, returning the previous one; a switch point.
            pub fn swap(&self, value: $value, order: Ordering) -> $value {
                self.update(order, |_| Some(value))
                    .unwrap_or_else(|old| old)
            }

            /// Replace the value if it is `current`; a switch point.
            ///
            /// # Errors
            ///
            /// * The actual value, if it was not `current`
            pub fn compare_exchange(
                &self,
                current: $value,
                new: $value,
                success: Ordering,
                _failure: Ordering,
            ) -> Result<$value, $value> {
                self.update(success, |old| (old == current).then_some(new))
            }

            /// Like [`Self::compare_exchange`]; never fails spuriously in simulation.
            ///
            /// # Errors
            ///
            /// * The actual value, if it was not `current`
            pub fn compare_exchange_weak(
                &self,
                current: $value,
                new: $value,
                success: Ordering,
                failure: Ordering,
            ) -> Result<$value, $value> {
                self.compare_exchange(current, new, success, failure)
            }

            /// Update the value with `f` until it succeeds or `f` returns `None`; a switch
            /// point.
            ///
            /// # Errors
            ///
            /// * The value, if `f` returned `None`
            pub fn fetch_update<F: FnMut($value) -> Option<$value>>(
                &self,
                set_order: Ordering,
                _fetch_order: Ordering,
                mut f: F,
            ) -> Result<$value, $value> {
                self.update(set_order, |old| f(old))
            }

            /// The value, consuming the atomic.
            pub const fn into_inner(self) -> $value {
                self.inner.into_inner()
            }

            /// A mutable reference to the value; no synchronization is needed.
            pub fn get_mut(&mut self) -> &mut $value {
                self.inner.get_mut()
            }

            /// Bitwise and; a switch point.
            pub fn fetch_and(&self, value: $value, order: Ordering) -> $value {
                self.update(order, |old| Some(old & value))
                    .unwrap_or_else(|old| old)
            }

            /// Bitwise or; a switch point.
            pub fn fetch_or(&self, value: $value, order: Ordering) -> $value {
                self.update(order, |old| Some(old | value))
                    .unwrap_or_else(|old| old)
            }

            /// Bitwise xor; a switch point.
            pub fn fetch_xor(&self, value: $value, order: Ordering) -> $value {
                self.update(order, |old| Some(old ^ value))
                    .unwrap_or_else(|old| old)
            }
        }
    };
}

macro_rules! atomic_int {
    ($name:ident, $std:ty, $value:ty) => {
        atomic_common!(
            $name,
            $std,
            $value,
            {
                // Exact: the value's bits, widened.
                #[allow(clippy::cast_lossless, clippy::cast_sign_loss)]
                const fn to_bits(value: $value) -> u64 {
                    value as u64
                }
                to_bits
            },
            {
                // Exact: the bits a value of this type was stored as.
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                const fn from_bits(bits: u64) -> $value {
                    bits as $value
                }
                from_bits
            }
        );

        impl $name {
            /// Wrapping addition; a switch point.
            pub fn fetch_add(&self, value: $value, order: Ordering) -> $value {
                self.update(order, |old| Some(old.wrapping_add(value)))
                    .unwrap_or_else(|old| old)
            }

            /// Wrapping subtraction; a switch point.
            pub fn fetch_sub(&self, value: $value, order: Ordering) -> $value {
                self.update(order, |old| Some(old.wrapping_sub(value)))
                    .unwrap_or_else(|old| old)
            }

            /// Maximum; a switch point.
            pub fn fetch_max(&self, value: $value, order: Ordering) -> $value {
                self.update(order, |old| Some(old.max(value)))
                    .unwrap_or_else(|old| old)
            }

            /// Minimum; a switch point.
            pub fn fetch_min(&self, value: $value, order: Ordering) -> $value {
                self.update(order, |old| Some(old.min(value)))
                    .unwrap_or_else(|old| old)
            }
        }
    };
}

atomic_common!(
    AtomicBool,
    std::sync::atomic::AtomicBool,
    bool,
    {
        const fn to_bits(value: bool) -> u64 {
            value as u64
        }
        to_bits
    },
    {
        const fn from_bits(bits: u64) -> bool {
            bits != 0
        }
        from_bits
    }
);

impl AtomicBool {
    /// Logical nand; a switch point.
    pub fn fetch_nand(&self, value: bool, order: Ordering) -> bool {
        self.update(order, |old| Some(!(old && value)))
            .unwrap_or_else(|old| old)
    }

    /// Logical not; a switch point.
    pub fn fetch_not(&self, order: Ordering) -> bool {
        self.update(order, |old| Some(!old))
            .unwrap_or_else(|old| old)
    }
}

atomic_int!(AtomicU8, std::sync::atomic::AtomicU8, u8);
atomic_int!(AtomicU16, std::sync::atomic::AtomicU16, u16);
atomic_int!(AtomicU32, std::sync::atomic::AtomicU32, u32);
atomic_int!(AtomicU64, std::sync::atomic::AtomicU64, u64);
atomic_int!(AtomicUsize, std::sync::atomic::AtomicUsize, usize);
atomic_int!(AtomicI8, std::sync::atomic::AtomicI8, i8);
atomic_int!(AtomicI16, std::sync::atomic::AtomicI16, i16);
atomic_int!(AtomicI32, std::sync::atomic::AtomicI32, i32);
atomic_int!(AtomicI64, std::sync::atomic::AtomicI64, i64);
atomic_int!(AtomicIsize, std::sync::atomic::AtomicIsize, isize);
