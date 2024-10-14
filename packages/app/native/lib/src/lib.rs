#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::Arc;

use gigachad_renderer::{RenderRunner, Renderer};
use moosicbox_env_utils::default_env_usize;
use router::Router;
use thiserror::Error;
use tokio::{runtime::Runtime, sync::RwLock};

pub mod router;

#[derive(Debug, Error)]
pub enum NativeAppError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send>),
}

pub struct NativeAppBuilder {
    x: Option<i32>,
    y: Option<i32>,
    width: Option<u16>,
    height: Option<u16>,
    router: Option<Router>,
    renderer: Option<Box<dyn Renderer>>,
    runtime_handle: Option<tokio::runtime::Handle>,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
}

impl Default for NativeAppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeAppBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            x: None,
            y: None,
            width: None,
            height: None,
            router: None,
            renderer: None,
            runtime_handle: None,
            runtime: None,
        }
    }

    #[must_use]
    pub fn with_renderer(mut self, renderer: impl Renderer + 'static) -> Self {
        self.renderer.replace(Box::new(renderer));
        self
    }

    #[must_use]
    pub fn with_router(mut self, router: Router) -> Self {
        self.router.replace(router);
        self
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

    #[must_use]
    pub fn with_x(mut self, x: i32) -> Self {
        self.x.replace(x);
        self
    }

    #[must_use]
    pub fn with_y(mut self, y: i32) -> Self {
        self.y.replace(y);
        self
    }

    #[must_use]
    pub fn with_position(self, x: i32, y: i32) -> Self {
        self.with_x(x).with_y(y)
    }

    #[must_use]
    pub fn with_runtime(self, runtime: Runtime) -> Self {
        self.with_runtime_arc(Arc::new(runtime))
    }

    #[must_use]
    pub fn with_runtime_arc(mut self, runtime: Arc<Runtime>) -> Self {
        self.runtime.replace(runtime);
        self
    }

    /// # Panics
    ///
    /// Will panic if failed to start tokio runtime
    ///
    /// # Errors
    ///
    /// Will error if there was an error starting the FLTK app
    pub async fn start(self) -> Result<NativeApp, NativeAppError> {
        let router = self.router.unwrap();

        let renderer = self.renderer.unwrap_or_else(|| {
            if cfg!(feature = "egui") {
                #[cfg(feature = "egui")]
                {
                    Box::new(gigachad_renderer_egui::EguiRenderer::new()) as Box<dyn Renderer>
                }
                #[cfg(not(feature = "egui"))]
                unreachable!()
            } else if cfg!(feature = "fltk") {
                #[cfg(feature = "fltk")]
                {
                    let renderer = gigachad_renderer_fltk::FltkRenderer::new();
                    moosicbox_task::spawn("fltk navigation listener", {
                        let renderer = renderer.clone();
                        let mut router = router.clone();
                        async move {
                            while let Some(path) = renderer.wait_for_navigation().await {
                                if let Err(e) = router.navigate(&path).await {
                                    log::error!("Failed to navigate: {e:?}");
                                }
                            }
                        }
                    });
                    Box::new(renderer) as Box<dyn Renderer>
                }
                #[cfg(not(feature = "fltk"))]
                unreachable!()
            } else {
                panic!("Missing renderer")
            }
        });

        let mut app = NativeApp {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            router,
            renderer: Arc::new(RwLock::new(renderer)),
            runtime_handle: self.runtime_handle,
            runtime: self.runtime,
        };
        app.start().await?;
        Ok(app)
    }
}

pub struct NativeApp {
    x: Option<i32>,
    y: Option<i32>,
    width: Option<u16>,
    height: Option<u16>,
    pub router: Router,
    renderer: Arc<RwLock<Box<dyn Renderer>>>,
    runtime_handle: Option<tokio::runtime::Handle>,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
}

impl NativeApp {
    async fn start(&mut self) -> Result<(), NativeAppError> {
        self.renderer
            .write()
            .await
            .init(
                self.width.unwrap_or(800),
                self.height.unwrap_or(600),
                self.x,
                self.y,
            )
            .await?;

        let runtime = self.runtime.take().unwrap_or_else(|| {
            let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
            log::debug!("Running with {threads} max blocking threads");
            Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .max_blocking_threads(threads)
                    .build()
                    .unwrap(),
            )
        });

        self.runtime_handle.replace(runtime.handle().clone());

        moosicbox_task::spawn_on("app_native_lib::start: router", runtime.handle(), {
            let router = self.router.clone();
            let renderer = self.renderer.clone();
            async move {
                log::debug!("app_native_lib::start: router listening");
                while let Some(element) = router.wait_for_navigation().await {
                    log::debug!("app_native_lib::start: router received element");
                    renderer.write().await.render(element)?;
                }
                Ok::<_, NativeAppError>(())
            }
        });

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if there was an error starting the app
    pub async fn to_runner(self) -> Result<Box<dyn RenderRunner>, NativeAppError> {
        log::debug!("run: getting runner");
        self.renderer
            .write()
            .await
            .to_runner()
            .await
            .map_err(NativeAppError::Other)
    }
}
