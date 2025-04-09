#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub use getrandom;
pub use rand;
pub use turmoil;

/// # Safety
///
/// This must be called before any multi-threading occurs. Setting environment
/// variables in multi-threaded programs is unsafe on non-windows operating systems
pub unsafe fn init() {
    moosicbox_assert::assert_or_panic!(
        std::env::var("ENABLE_ASSERT").as_deref() == Ok("1"),
        "ENABLE_ASSERT=1 is required"
    );

    unsafe {
        std::env::set_var("ENABLE_SIMULATOR", "1");
    }
}
