#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "async")]
pub mod unsync {
    // Re-export everything from switchy_async
    pub use switchy_async::*;

    // Override the select! macro to use the correct path for switchy::unsync
    #[cfg(feature = "async-macros")]
    #[macro_export]
    macro_rules! select {
        ($($tokens:tt)*) => {
            switchy::unsync_macros::select_internal! {
                @path = switchy::unsync;
                $($tokens)*
            }
        };
    }

    #[cfg(feature = "async-macros")]
    pub use select;

    // Re-export test attribute macro
    #[cfg(all(test, feature = "async-macros"))]
    pub use crate::unsync_macros::unsync_test as test;
}
#[cfg(feature = "async-macros")]
pub mod unsync_macros {
    // Re-export everything from switchy_async_macros
    pub use switchy_async_macros::*;

    // For tokio runtime - re-export tokio::select! as select_internal
    #[cfg(all(feature = "async-tokio", not(feature = "simulator")))]
    #[macro_export]
    macro_rules! select_internal {
        // Handle the @path parameter and ignore it for tokio
        (@path = $path:path; $($tokens:tt)*) => {
            switchy::unsync::tokio::select! { $($tokens)* }
        };
        // Fallback for direct calls without @path
        ($($tokens:tt)*) => {
            switchy::unsync::tokio::select! { $($tokens)* }
        };
    }

    #[cfg(all(feature = "async-tokio", not(feature = "simulator")))]
    pub use select_internal;

    // For simulator runtime - re-export the procedural macro
    #[cfg(feature = "simulator")]
    pub use switchy_async_macros::select_internal;

    // Default fallback - use simulator when no specific runtime is chosen
    // but async-macros is enabled (which brings in the dependency)
    #[cfg(all(
        feature = "async-macros",
        not(feature = "async-tokio"),
        not(feature = "simulator")
    ))]
    pub use switchy_async_macros::select_internal;

    // For tokio runtime - re-export tokio::test as test_internal
    #[cfg(all(test, feature = "async-tokio", not(feature = "simulator")))]
    pub use tokio::test as test_internal;

    // For simulator runtime - re-export the procedural macro
    #[cfg(feature = "simulator")]
    pub use switchy_async_macros::test_internal;

    // Default fallback - use simulator when no specific runtime is chosen
    // but async-macros is enabled (which brings in the dependency)
    #[cfg(all(
        feature = "async-macros",
        not(feature = "async-tokio"),
        not(feature = "simulator")
    ))]
    pub use switchy_async_macros::test_internal;
}
#[cfg(feature = "database")]
pub use switchy_database as database;
#[cfg(feature = "database-connection")]
pub use switchy_database_connection as database_connection;
#[cfg(feature = "fs")]
pub use switchy_fs as fs;
#[cfg(feature = "mdns")]
pub use switchy_mdns as mdns;
#[cfg(feature = "random")]
pub use switchy_random as random;
#[cfg(feature = "tcp")]
pub use switchy_tcp as tcp;
#[cfg(feature = "telemetry")]
pub use switchy_telemetry as telemetry;
#[cfg(feature = "time")]
pub use switchy_time as time;
#[cfg(feature = "upnp")]
pub use switchy_upnp as upnp;

#[cfg(any(feature = "http", feature = "http-models"))]
pub mod http {
    #[cfg(feature = "http")]
    pub use switchy_http::*;
    #[cfg(feature = "http-models")]
    pub use switchy_http_models as models;
}
