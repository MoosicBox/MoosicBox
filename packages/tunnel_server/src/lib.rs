#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::LazyLock;

use switchy_async::util::CancellationToken;

pub static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
