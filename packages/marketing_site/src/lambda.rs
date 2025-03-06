#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::{Arc, LazyLock};

use hyperchad_renderer_html::lambda::lambda_http::tracing;
use tokio::runtime::Runtime;

static RUNTIME: LazyLock<Arc<Runtime>> = LazyLock::new(|| {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    )
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

    let mut runner = RUNTIME.block_on(async move {
        let builder = moosicbox_marketing_site::init().with_runtime_arc(RUNTIME.clone());
        moosicbox_marketing_site::start(builder)
            .await?
            .into_runner()
    })?;

    runner.run().unwrap();

    Ok(())
}
