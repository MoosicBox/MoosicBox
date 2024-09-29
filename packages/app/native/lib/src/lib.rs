#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use fltk::prelude::FltkError;
use futures::Future;
use moosicbox_app_native_renderer::{Renderer, RoutePath};
use moosicbox_env_utils::default_env_usize;
use moosicbox_giga_chad_transformer::ElementList;
use thiserror::Error;
use tokio::task::JoinHandle;

#[derive(Debug, Error)]
pub enum NativeAppError {
    #[error(transparent)]
    Fltk(#[from] FltkError),
}

#[derive(Clone)]
pub struct NativeApp {
    width: Option<u16>,
    height: Option<u16>,
    renderer: Renderer,
    runtime: Option<tokio::runtime::Handle>,
}

impl Default for NativeApp {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeApp {
    #[must_use]
    pub fn new() -> Self {
        Self {
            width: None,
            height: None,
            renderer: Renderer::new(),
            runtime: None,
        }
    }

    #[must_use]
    pub fn with_width(mut self, width: u16) -> Self {
        self.width.replace(width);
        self
    }

    #[must_use]
    pub fn with_height(mut self, height: u16) -> Self {
        self.height.replace(height);
        self
    }

    #[must_use]
    pub fn with_size(self, width: u16, height: u16) -> Self {
        self.with_width(width).with_height(height)
    }

    /// # Panics
    ///
    /// Will panic if failed to start tokio runtime
    ///
    /// # Errors
    ///
    /// Will error if there was an error starting the FLTK app
    pub fn start(mut self) -> Result<Self, NativeAppError> {
        self.renderer = self
            .renderer
            .start(self.width.unwrap_or(800), self.height.unwrap_or(600))?;

        let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
        log::debug!("Running with {threads} max blocking threads");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(threads)
            .build()
            .unwrap();

        self.runtime.replace(runtime.handle().clone());

        std::thread::spawn({
            let renderer = self.renderer.clone();
            move || {
                runtime.block_on(async move {
                    renderer.listen().await;
                    Ok::<_, String>(())
                })
            }
        });

        Ok(self)
    }

    /// # Errors
    ///
    /// Will error if there was an error starting the FLTK app
    pub fn run(self) -> Result<(), NativeAppError> {
        Ok(self.renderer.run()?)
    }

    #[must_use]
    pub fn with_route<
        F: Future<Output = Result<ElementList, E>> + Send + 'static,
        E: Into<Box<dyn std::error::Error>>,
    >(
        mut self,
        route: impl Into<RoutePath>,
        handler: impl Fn() -> F + Send + Sync + Clone + 'static,
    ) -> Self {
        self.renderer = self.renderer.with_route(route, handler);
        self
    }

    /// # Errors
    ///
    /// Will error if there was an error starting the FLTK app
    pub async fn navigate(&mut self, path: &str) -> Result<(), FltkError> {
        self.renderer.navigate(path).await
    }

    /// # Errors
    ///
    /// Will error if there was an error starting the FLTK app
    pub fn navigate_spawn(&mut self, path: &str) -> JoinHandle<Result<(), FltkError>> {
        let Some(runtime) = &self.runtime else {
            moosicbox_assert::die_or_panic!("NativeApp must be started before navigating");
        };
        let mut renderer = self.renderer.clone();
        let path = path.to_owned();
        moosicbox_task::spawn_on("NativeApp navigate_spawn", runtime, async move {
            renderer.navigate(&path).await
        })
    }
}
