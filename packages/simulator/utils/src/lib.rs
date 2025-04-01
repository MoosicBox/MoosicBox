#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[must_use]
pub fn simulator_enabled() -> bool {
    std::env::var("ENABLE_SIMULATOR").as_deref() == Ok("1")
}
