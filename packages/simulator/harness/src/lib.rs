#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub use getrandom;
pub use rand;
pub use turmoil;

#[cfg(feature = "database")]
pub use moosicbox_database_connection as database_connection;
#[cfg(feature = "http")]
pub use moosicbox_http as http;
#[cfg(feature = "mdns")]
pub use moosicbox_mdns as mdns;
#[cfg(feature = "random")]
pub use moosicbox_random as random;
#[cfg(feature = "tcp")]
pub use moosicbox_tcp as tcp;
#[cfg(feature = "telemetry")]
pub use moosicbox_telemetry as telemetry;
#[cfg(feature = "time")]
pub use moosicbox_time as time;
#[cfg(feature = "upnp")]
pub use moosicbox_upnp as upnp;

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
