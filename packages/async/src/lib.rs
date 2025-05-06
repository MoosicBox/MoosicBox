#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    cell::RefCell,
    sync::{LazyLock, atomic::AtomicU64},
};

#[cfg(feature = "_any_backend")]
pub use futures;
#[cfg(feature = "macros")]
pub use moosicbox_async_macros::{inject_yields, inject_yields_mod};

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "simulator")]
pub mod simulator;

pub mod runtime;

static THREAD_ID_COUNTER: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(1));

thread_local! {
    static THREAD_ID: RefCell<u64> = RefCell::new(THREAD_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
}

#[must_use]
pub fn thread_id() -> u64 {
    THREAD_ID.with_borrow(|x| *x)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Join")]
    Join,
}

#[allow(unused)]
macro_rules! impl_async {
    ($module:ident $(,)?) => {
        pub use $module::task;

        pub use $module::runtime::Runtime;

        #[cfg(feature = "io")]
        pub use $module::io;
        #[cfg(feature = "sync")]
        pub use $module::sync;
        #[cfg(feature = "time")]
        pub use $module::time;
        #[cfg(feature = "util")]
        pub use $module::util;

        #[cfg(feature = "macros")]
        pub use $module::select;

        impl $module::runtime::Runtime {
            pub fn block_on<F: Future + Send + 'static>(&self, f: F) -> F::Output
            where
                F::Output: Send,
            {
                <Self as crate::runtime::GenericRuntime>::block_on(self, f)
            }

            /// # Errors
            ///
            /// * If the `Runtime` fails to join
            pub fn wait(self) -> Result<(), Error> {
                <Self as crate::runtime::GenericRuntime>::wait(self)
            }
        }

        impl crate::runtime::Builder {
            /// # Errors
            ///
            /// * If the underlying `Runtime` fails to build
            pub fn build(&self) -> Result<$module::runtime::Runtime, Error> {
                $module::runtime::build_runtime(self)
            }
        }
    };
}

#[cfg(feature = "simulator")]
impl_async!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "tokio"))]
impl_async!(tokio);
