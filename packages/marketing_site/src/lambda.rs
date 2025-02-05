#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::{Arc, LazyLock};

use gigachad_renderer_html::lambda::lambda_http::tracing;
use tokio::runtime::Runtime;

static RUNTIME: LazyLock<Arc<Runtime>> = LazyLock::new(|| {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    )
});

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::init_default_subscriber();

    let mut runner = RUNTIME.block_on(async move {
        let app = moosicbox_marketing_site::init().with_runtime_arc(RUNTIME.clone());
        app.start().await?.to_runner().await
    })?;

    runner.run().unwrap();

    Ok(())
}
