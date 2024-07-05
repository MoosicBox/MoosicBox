#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;

pub static CANCELLATION_TOKEN: Lazy<CancellationToken> = Lazy::new(CancellationToken::new);
