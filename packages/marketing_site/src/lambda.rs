#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{Arc, LazyLock};

use hyperchad::renderer_html::lambda::lambda_http::tracing;
use moosicbox_env_utils::default_env_usize;
use switchy_async::runtime::Runtime;

static RUNTIME: LazyLock<Arc<Runtime>> = LazyLock::new(|| {
    let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
    log::debug!("Running with {threads} max blocking threads");
    let runtime = switchy_async::runtime::Builder::new()
        .max_blocking_threads(u16::try_from(threads).unwrap())
        .build()
        .unwrap();

    Arc::new(runtime)
});

/// # Errors
///
/// * If the lambda fails
///
/// # Panics
///
/// * If the runner fails to run
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing::init_default_subscriber();

    let builder = moosicbox_marketing_site::init().with_runtime_arc(RUNTIME.clone());
    moosicbox_marketing_site::build_app(builder)?.serve_sync()?;

    Ok(())
}
