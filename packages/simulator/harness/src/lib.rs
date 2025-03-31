#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// # Safety
///
/// This must be called before any multi-threading occurs. Setting environment
/// variables in multi-threaded programs is unsafe on non-windows operating systems
pub unsafe fn init() {
    unsafe {
        std::env::set_var("ENABLE_SIMULATOR", "1");
    }
}

#[must_use]
pub fn sim_buider() -> turmoil::Builder {
    turmoil::Builder::new()
}
