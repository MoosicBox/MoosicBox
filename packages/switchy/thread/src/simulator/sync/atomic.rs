//! Simulated atomics.
//!
//! Every access is a switch point, so races between atomic operations are explored by the
//! scheduler. Values are sequentially consistent in simulation whatever the requested
//! ordering, since only one thread runs at a time.

use std::fmt;

pub use std::sync::atomic::{Ordering, compiler_fence};

use super::super::{Op, scheduler};

fn switch(op: Op) {
    if std::thread::panicking() {
        return;
    }
    scheduler().yield_point(op);
}

/// A memory fence; a switch point.
pub fn fence(order: Ordering) {
    switch(Op::AtomicUpdate);
    std::sync::atomic::fence(order);
}

macro_rules! atomic_common {
    ($name:ident, $std:ty, $value:ty) => {
        #[doc = concat!("A simulated [`std::sync::atomic::", stringify!($name), "`].")]
        #[derive(Default)]
        pub struct $name {
            inner: $std,
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

        impl $name {
            /// A new atomic holding `value`.
            #[must_use]
            pub const fn new(value: $value) -> Self {
                Self {
                    inner: <$std>::new(value),
                }
            }

            /// Load the value; a switch point.
            pub fn load(&self, order: Ordering) -> $value {
                switch(Op::AtomicLoad);
                self.inner.load(order)
            }

            /// Store `value`; a switch point.
            pub fn store(&self, value: $value, order: Ordering) {
                switch(Op::AtomicStore);
                self.inner.store(value, order);
            }

            /// Replace the value, returning the previous one; a switch point.
            pub fn swap(&self, value: $value, order: Ordering) -> $value {
                switch(Op::AtomicUpdate);
                self.inner.swap(value, order)
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
                failure: Ordering,
            ) -> Result<$value, $value> {
                switch(Op::AtomicUpdate);
                self.inner.compare_exchange(current, new, success, failure)
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
                fetch_order: Ordering,
                f: F,
            ) -> Result<$value, $value> {
                switch(Op::AtomicUpdate);
                self.inner.try_update(set_order, fetch_order, f)
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
                switch(Op::AtomicUpdate);
                self.inner.fetch_and(value, order)
            }

            /// Bitwise or; a switch point.
            pub fn fetch_or(&self, value: $value, order: Ordering) -> $value {
                switch(Op::AtomicUpdate);
                self.inner.fetch_or(value, order)
            }

            /// Bitwise xor; a switch point.
            pub fn fetch_xor(&self, value: $value, order: Ordering) -> $value {
                switch(Op::AtomicUpdate);
                self.inner.fetch_xor(value, order)
            }
        }
    };
}

macro_rules! atomic_int {
    ($name:ident, $std:ty, $value:ty) => {
        atomic_common!($name, $std, $value);

        impl $name {
            /// Wrapping addition; a switch point.
            pub fn fetch_add(&self, value: $value, order: Ordering) -> $value {
                switch(Op::AtomicUpdate);
                self.inner.fetch_add(value, order)
            }

            /// Wrapping subtraction; a switch point.
            pub fn fetch_sub(&self, value: $value, order: Ordering) -> $value {
                switch(Op::AtomicUpdate);
                self.inner.fetch_sub(value, order)
            }

            /// Maximum; a switch point.
            pub fn fetch_max(&self, value: $value, order: Ordering) -> $value {
                switch(Op::AtomicUpdate);
                self.inner.fetch_max(value, order)
            }

            /// Minimum; a switch point.
            pub fn fetch_min(&self, value: $value, order: Ordering) -> $value {
                switch(Op::AtomicUpdate);
                self.inner.fetch_min(value, order)
            }
        }
    };
}

atomic_common!(AtomicBool, std::sync::atomic::AtomicBool, bool);

impl AtomicBool {
    /// Logical nand; a switch point.
    pub fn fetch_nand(&self, value: bool, order: Ordering) -> bool {
        switch(Op::AtomicUpdate);
        self.inner.fetch_nand(value, order)
    }

    /// Logical not; a switch point.
    pub fn fetch_not(&self, order: Ordering) -> bool {
        switch(Op::AtomicUpdate);
        self.inner.fetch_not(order)
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
