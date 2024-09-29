#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use fltk::prelude::FltkError;
use futures::Future;
use moosicbox_env_utils::{default_env_u16, default_env_usize};
use moosicbox_renderer_fltk::Renderer;

static WIDTH: u16 = default_env_u16!("WINDOW_WIDTH", 1000);
static HEIGHT: u16 = default_env_u16!("WINDOW_HEIGHT", 600);

/// # Panics
///
/// Will panic if failed to start tokio runtime
///
/// # Errors
///
/// Will error if there was an error starting the FLTK app
pub fn app<F: Future<Output = Result<(), E>>, E: Into<Box<dyn std::error::Error>>>(
    on_start: impl Fn(Renderer) -> F + Send + 'static,
) -> Result<(), FltkError> {
    let renderer = Renderer::new(WIDTH, HEIGHT)?;

    std::thread::spawn({
        let renderer = renderer.clone();
        move || {
            let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
            log::debug!("Running with {threads} max blocking threads");
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .max_blocking_threads(threads)
                .build()
                .unwrap();

            // let on_start = on_start.clone();
            runtime.block_on(async move {
                on_start(renderer.clone())
                    .await
                    .map_err(|e| e.into().to_string())?;
                renderer.listen().await;
                Ok::<_, String>(())
            })
        }
    });

    renderer.run()
}
