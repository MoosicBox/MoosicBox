//! Runtime abstraction layer for testing and simulation.
//!
//! `switchy` provides runtime-agnostic interfaces for async operations, I/O, networking,
//! and other system interactions. It enables code to be written once and run against
//! different backends (e.g., Tokio, simulator) by switching feature flags.
//!
//! # Feature Flags
//!
//! This crate uses feature flags extensively to control which backends are enabled:
//!
//! * `async` - Core async runtime abstractions via `switchy_async`
//! * `async-tokio` - Use Tokio as the async runtime
//! * `simulator` - Use simulated runtime for deterministic testing
//! * `async-macros` - Enable async macros like `select!`, `join!`, `try_join!`
//! * `database` - Database abstraction layer
//! * `fs` - Filesystem abstraction layer
//! * `http` - HTTP client abstraction layer
//! * `tcp` - TCP networking abstraction layer
//! * `time` - Time and timing abstractions
//! * `all` - Enable all features (default)
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "async")]
//! # async fn example() {
//! use switchy::unsync::time::{sleep, Duration};
//!
//! // This code works with both Tokio and simulator runtimes
//! sleep(Duration::from_secs(1)).await;
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Async runtime abstractions and utilities.
///
/// This module provides runtime-agnostic async primitives that work with both
/// Tokio and the simulator runtime. The actual backend is selected via feature flags.
///
/// # Feature Flags
///
/// * `async-tokio` - Use Tokio as the backend
/// * `simulator` - Use simulator runtime for testing
#[cfg(feature = "async")]
pub mod unsync {
    // Re-export everything from switchy_async
    pub use switchy_async::*;

    // Override the select! macro to use the correct path for switchy::unsync
    /// Waits on multiple concurrent branches, returning when the first completes.
    ///
    /// This macro provides a runtime-agnostic way to wait on multiple async operations.
    /// When using the Tokio runtime, this delegates to `tokio::select!`. When using the
    /// simulator runtime, this uses the simulator's implementation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(all(feature = "async-macros", feature = "async-tokio"))]
    /// # async fn example() {
    /// use switchy::unsync::time::{sleep, Duration};
    ///
    /// switchy::unsync::select! {
    ///     _ = sleep(Duration::from_secs(1)) => println!("Timer elapsed"),
    ///     _ = async { /* other operation */ } => println!("Other completed"),
    /// }
    /// # }
    /// ```
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

    // Override the join! macro to use the correct path for switchy::unsync
    /// Waits for multiple concurrent futures, returning when all complete.
    ///
    /// This macro provides a runtime-agnostic way to execute multiple async operations
    /// concurrently and wait for all of them to complete. When using the Tokio runtime,
    /// this delegates to `tokio::join!`. When using the simulator runtime, this uses
    /// the simulator's implementation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(all(feature = "async-macros", feature = "async-tokio"))]
    /// # async fn example() {
    /// let (a, b) = switchy::unsync::join!(
    ///     async { 1 },
    ///     async { 2 },
    /// );
    /// assert_eq!(a, 1);
    /// assert_eq!(b, 2);
    /// # }
    /// ```
    #[cfg(feature = "async-macros")]
    #[macro_export]
    macro_rules! join {
        ($($tokens:tt)*) => {
            switchy::unsync_macros::join_internal! {
                @path = switchy::unsync;
                $($tokens)*
            }
        };
    }

    #[cfg(feature = "async-macros")]
    pub use join;

    // Override the try_join! macro to use the correct path for switchy::unsync
    /// Waits for multiple fallible concurrent futures, returning when all complete successfully.
    ///
    /// This macro provides a runtime-agnostic way to execute multiple async operations that
    /// return `Result` and wait for all of them to complete. If any operation fails, the error
    /// is returned immediately. When using the Tokio runtime, this delegates to `tokio::try_join!`.
    /// When using the simulator runtime, this uses the simulator's implementation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(all(feature = "async-macros", feature = "async-tokio"))]
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (a, b) = switchy::unsync::try_join!(
    ///     async { Ok::<_, std::io::Error>(1) },
    ///     async { Ok::<_, std::io::Error>(2) },
    /// )?;
    /// assert_eq!(a, 1);
    /// assert_eq!(b, 2);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "async-macros")]
    #[macro_export]
    macro_rules! try_join {
        ($($tokens:tt)*) => {
            switchy::unsync_macros::try_join_internal! {
                @path = switchy::unsync;
                $($tokens)*
            }
        };
    }

    #[cfg(feature = "async-macros")]
    pub use try_join;

    // Re-export test attribute macro
    #[cfg(all(test, feature = "async-macros"))]
    pub use crate::unsync_macros::unsync_test as test;
}

/// Internal macro support for async operations.
///
/// This module contains the internal implementation details for the `select!`, `join!`,
/// and `try_join!` macros. Users should use the macros from the `unsync` module instead
/// of accessing this module directly.
#[cfg(feature = "async-macros")]
pub mod unsync_macros {
    // Re-export everything from switchy_async_macros
    pub use switchy_async_macros::*;

    // For tokio runtime - re-export tokio::select! as select_internal
    /// Internal implementation macro for `select!` using Tokio runtime.
    ///
    /// This macro is an implementation detail and should not be used directly.
    /// Use [`switchy::unsync::select!`](crate::unsync::select) instead.
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

    // For tokio runtime - re-export tokio::join! as join_internal
    /// Internal implementation macro for `join!` using Tokio runtime.
    ///
    /// This macro is an implementation detail and should not be used directly.
    /// Use [`switchy::unsync::join!`](crate::unsync::join) instead.
    #[cfg(all(feature = "async-tokio", not(feature = "simulator")))]
    #[macro_export]
    macro_rules! join_internal {
        // Handle the @path parameter and ignore it for tokio
        (@path = $path:path; $($tokens:tt)*) => {
            switchy::unsync::tokio::join! { $($tokens)* }
        };
        // Fallback for direct calls without @path
        ($($tokens:tt)*) => {
            switchy::unsync::tokio::join! { $($tokens)* }
        };
    }

    #[cfg(all(feature = "async-tokio", not(feature = "simulator")))]
    pub use join_internal;

    // For tokio runtime - re-export tokio::try_join! as try_join_internal
    /// Internal implementation macro for `try_join!` using Tokio runtime.
    ///
    /// This macro is an implementation detail and should not be used directly.
    /// Use [`switchy::unsync::try_join!`](crate::unsync::try_join) instead.
    #[cfg(all(feature = "async-tokio", not(feature = "simulator")))]
    #[macro_export]
    macro_rules! try_join_internal {
        // Handle the @path parameter and ignore it for tokio
        (@path = $path:path; $($tokens:tt)*) => {
            switchy::unsync::tokio::try_join! { $($tokens)* }
        };
        // Fallback for direct calls without @path
        ($($tokens:tt)*) => {
            switchy::unsync::tokio::try_join! { $($tokens)* }
        };
    }

    #[cfg(all(feature = "async-tokio", not(feature = "simulator")))]
    pub use try_join_internal;

    // For simulator runtime - re-export the procedural macro
    #[cfg(feature = "simulator")]
    pub use switchy_async_macros::select_internal;

    // For simulator runtime - re-export join/try_join procedural macros
    #[cfg(feature = "simulator")]
    pub use switchy_async_macros::{join_internal, try_join_internal};

    // Default fallback - use simulator when no specific runtime is chosen
    // but async-macros is enabled (which brings in the dependency)
    #[cfg(all(
        feature = "async-macros",
        not(feature = "async-tokio"),
        not(feature = "simulator")
    ))]
    pub use switchy_async_macros::select_internal;

    // Default fallback - use simulator join/try_join when no specific runtime is chosen
    #[cfg(all(
        feature = "async-macros",
        not(feature = "async-tokio"),
        not(feature = "simulator")
    ))]
    pub use switchy_async_macros::{join_internal, try_join_internal};

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

/// Database abstraction layer.
///
/// Provides runtime-agnostic database operations that work with different backends.
/// Enable the `database` feature to use this module.
#[cfg(feature = "database")]
pub use switchy_database as database;

/// Database connection management.
///
/// Provides connection pooling and management utilities for database operations.
/// Enable the `database-connection` feature to use this module.
#[cfg(feature = "database-connection")]
pub use switchy_database_connection as database_connection;

/// Filesystem abstraction layer.
///
/// Provides runtime-agnostic filesystem operations for reading and writing files.
/// Enable the `fs` feature to use this module.
#[cfg(feature = "fs")]
pub use switchy_fs as fs;

/// `mDNS` service discovery.
///
/// Provides multicast DNS service discovery and announcement capabilities.
/// Enable the `mdns` feature to use this module.
#[cfg(feature = "mdns")]
pub use switchy_mdns as mdns;

/// Random number generation.
///
/// Provides runtime-agnostic random number generation utilities.
/// Enable the `random` feature to use this module.
#[cfg(feature = "random")]
pub use switchy_random as random;

/// TCP networking abstraction.
///
/// Provides runtime-agnostic TCP client and server implementations.
/// Enable the `tcp` feature to use this module.
#[cfg(feature = "tcp")]
pub use switchy_tcp as tcp;

/// Telemetry and observability.
///
/// Provides tracing, metrics, and logging infrastructure for observability.
/// Enable the `telemetry` feature to use this module.
#[cfg(feature = "telemetry")]
pub use switchy_telemetry as telemetry;

/// Time and timing abstractions.
///
/// Provides runtime-agnostic time operations including delays, timeouts, and intervals.
/// Enable the `time` feature to use this module.
#[cfg(feature = "time")]
pub use switchy_time as time;

/// `UPnP` port mapping and discovery.
///
/// Provides Universal Plug and Play functionality for port mapping and device discovery.
/// Enable the `upnp` feature to use this module.
#[cfg(feature = "upnp")]
pub use switchy_upnp as upnp;

/// HTTP client and model abstractions.
///
/// This module provides HTTP functionality through two main components:
///
/// * HTTP client abstractions (when `http` feature is enabled)
/// * HTTP model types and conversions (when `http-models` feature is enabled)
#[cfg(any(feature = "http", feature = "http-models"))]
pub mod http {
    #[cfg(feature = "http")]
    pub use switchy_http::*;
    #[cfg(feature = "http-models")]
    pub use switchy_http_models as models;
}
