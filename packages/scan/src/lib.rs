#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "api")]
pub mod api;
pub mod local;

static CANCELLATION_TOKEN: Lazy<CancellationToken> = Lazy::new(CancellationToken::new);

pub fn cancel() {
    CANCELLATION_TOKEN.cancel();
}
