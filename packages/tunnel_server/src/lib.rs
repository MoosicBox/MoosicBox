#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::LazyLock;

use switchy_async::util::CancellationToken;

/// Global cancellation token for coordinating shutdown of the tunnel server.
///
/// This token is used to signal cancellation to all running services and connections
/// when the server is shutting down. Services should clone this token and check it
/// periodically or use it with cancellable operations.
pub static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
