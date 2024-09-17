#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::sync::LazyLock;

use tokio_util::sync::CancellationToken;

pub static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);
