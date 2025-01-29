#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::Arc;

use gigachad_renderer::{Color, RenderRunner, Renderer};
use gigachad_router::Router;
use moosicbox_env_utils::default_env_usize;
use thiserror::Error;
use tokio::{runtime::Runtime, sync::RwLock};

pub use gigachad_renderer as renderer;
pub use gigachad_router as router;

#[cfg(any(feature = "egui", feature = "fltk"))]
pub static CLIENT_INFO: std::sync::LazyLock<Arc<gigachad_router::ClientInfo>> =
    std::sync::LazyLock::new(|| {
        let os_name = os_info::get().os_type().to_string();
        Arc::new(gigachad_router::ClientInfo {
            os: gigachad_router::ClientOs { name: os_name },
        })
    });

#[derive(Debug, Error)]
pub enum NativeAppError {
    #[error("Runtime required")]
    RuntimeRequired,
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send>),
}

type ActionHandler = Box<
    dyn Fn(
            (&str, Option<&gigachad_actions::logic::Value>),
        ) -> Result<bool, Box<dyn std::error::Error>>
        + Send,
>;
type ResizeListener = Box<dyn Fn(f32, f32) -> Result<(), Box<dyn std::error::Error>> + Send>;

pub struct NativeAppBuilder {
    x: Option<i32>,
    y: Option<i32>,
    background: Option<Color>,
    width: Option<f32>,
    height: Option<f32>,
    router: Option<Router>,
    renderer: Option<Box<dyn Renderer>>,
    runtime_handle: Option<tokio::runtime::Handle>,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
    action_handlers: Vec<ActionHandler>,
    resize_listeners: Vec<ResizeListener>,
    #[cfg(feature = "assets")]
    static_asset_routes: Vec<gigachad_renderer::assets::StaticAssetRoute>,
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
            background: None,
            width: None,
            height: None,
            router: None,
            renderer: None,
            runtime_handle: None,
            runtime: None,
            action_handlers: vec![],
            resize_listeners: vec![],
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
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
    pub fn with_width(mut self, width: f32) -> Self {
        self.width.replace(width);
        self
    }

    #[must_use]
    pub fn with_height(mut self, height: f32) -> Self {
        self.height.replace(height);
        self
    }

    #[must_use]
    pub fn with_size(self, width: f32, height: f32) -> Self {
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
    pub fn with_background(mut self, color: Color) -> Self {
        self.background.replace(color);
        self
    }

    #[must_use]
    pub fn with_action_handler<E: std::error::Error + 'static>(
        mut self,
        func: impl Fn(&str, Option<&gigachad_actions::logic::Value>) -> Result<bool, E> + Send + 'static,
    ) -> Self {
        self.action_handlers.push(Box::new(move |(a, b)| {
            func(a, b).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        }));
        self
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

    #[must_use]
    pub fn with_on_resize<E: std::error::Error + 'static>(
        mut self,
        func: impl Fn(f32, f32) -> Result<(), E> + Send + 'static,
    ) -> Self {
        self.resize_listeners.push(Box::new(move |width, height| {
            func(width, height).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        }));
        self
    }

    #[allow(unused)]
    #[must_use]
    fn listen_actions(
        action_handlers: Vec<ActionHandler>,
    ) -> flume::Sender<(String, Option<gigachad_actions::logic::Value>)> {
        let (action_tx, action_rx) =
            flume::unbounded::<(String, Option<gigachad_actions::logic::Value>)>();

        moosicbox_task::spawn("action listener", {
            async move {
                while let Ok((action, value)) = action_rx.recv_async().await {
                    log::debug!(
                        "Received action: {action} for {} handler(s)",
                        action_handlers.len()
                    );
                    for handler in &action_handlers {
                        if let Err(e) = handler((action.as_str(), value.as_ref())) {
                            moosicbox_assert::die_or_error!(
                                "Action handler error action={action}: {e:?}"
                            );
                        }
                    }
                }
            }
        });

        action_tx
    }

    #[allow(unused)]
    #[must_use]
    fn listen_resize(resize_listeners: Vec<ResizeListener>) -> flume::Sender<(f32, f32)> {
        let (resize_tx, resize_rx) = flume::unbounded::<(f32, f32)>();

        moosicbox_task::spawn("resize listener", {
            async move {
                while let Ok((width, height)) = resize_rx.recv_async().await {
                    log::debug!(
                        "Received resize: {width}, {height} for {} listener(s)",
                        resize_listeners.len()
                    );
                    for listener in &resize_listeners {
                        if let Err(e) = listener(width, height) {
                            moosicbox_assert::die_or_error!(
                                "Action listener error width={width} height={height}: {e:?}"
                            );
                        }
                    }
                }
            }
        });

        resize_tx
    }

    #[cfg(feature = "assets")]
    #[must_use]
    pub fn with_static_asset_route(
        mut self,
        path: impl Into<gigachad_renderer::assets::StaticAssetRoute>,
    ) -> Self {
        self.static_asset_routes.push(path.into());
        self
    }

    /// # Errors
    ///
    /// * If the asset path type is a not found
    /// * If the asset path type is an invalid path type (not a file or directory)
    #[cfg(feature = "assets")]
    pub fn with_static_asset_route_result<
        Path: TryInto<gigachad_renderer::assets::StaticAssetRoute>,
    >(
        mut self,
        path: Path,
    ) -> Result<Self, Path::Error> {
        self.static_asset_routes.push(path.try_into()?);
        Ok(self)
    }

    /// # Panics
    ///
    /// Will panic if failed to start tokio runtime
    ///
    /// # Errors
    ///
    /// Will error if there was an error starting the app
    #[allow(clippy::too_many_lines)]
    pub async fn start(self) -> Result<NativeApp, NativeAppError> {
        let router = self.router.unwrap();

        let renderer = self.renderer.map_or_else(
            || {
                #[allow(unreachable_code)]
                Ok(if cfg!(feature = "egui") {
                    #[cfg(feature = "egui")]
                    {
                        let action_tx = Self::listen_actions(self.action_handlers);
                        let resize_tx = Self::listen_resize(self.resize_listeners);
                        let renderer = gigachad_renderer_egui::EguiRenderer::new(
                            router.clone(),
                            action_tx,
                            resize_tx,
                            CLIENT_INFO.clone(),
                        );

                        moosicbox_task::spawn("egui navigation listener", {
                            let renderer = renderer.clone();
                            let router = router.clone();
                            async move {
                                while let Some(path) = renderer.wait_for_navigation().await {
                                    if let Err(e) = router
                                        .navigate_send(
                                            &path,
                                            gigachad_router::RequestInfo {
                                                client: CLIENT_INFO.clone(),
                                            },
                                        )
                                        .await
                                    {
                                        log::error!("Failed to navigate: {e:?}");
                                    }
                                }
                            }
                        });
                        Box::new(renderer) as Box<dyn Renderer>
                    }
                    #[cfg(not(feature = "egui"))]
                    unreachable!()
                } else if cfg!(feature = "fltk") {
                    #[cfg(feature = "fltk")]
                    {
                        let action_tx = Self::listen_actions(self.action_handlers);
                        let renderer = gigachad_renderer_fltk::FltkRenderer::new(action_tx);
                        moosicbox_task::spawn("fltk navigation listener", {
                            let renderer = renderer.clone();
                            let router = router.clone();
                            async move {
                                while let Some(path) = renderer.wait_for_navigation().await {
                                    if let Err(e) = router
                                        .navigate_send(
                                            &path,
                                            gigachad_router::RequestInfo {
                                                client: CLIENT_INFO.clone(),
                                            },
                                        )
                                        .await
                                    {
                                        log::error!("Failed to navigate: {e:?}");
                                    }
                                }
                            }
                        });
                        Box::new(renderer) as Box<dyn Renderer>
                    }
                    #[cfg(not(feature = "fltk"))]
                    unreachable!()
                } else if cfg!(feature = "datastar") {
                    #[cfg(feature = "datastar")]
                    {
                        let runtime = self
                            .runtime
                            .clone()
                            .ok_or(NativeAppError::RuntimeRequired)?;
                        let action_tx = Self::listen_actions(self.action_handlers);
                        let renderer = gigachad_renderer_datastar::DatastarRenderer::new(
                            router.clone(),
                            runtime,
                            action_tx,
                        );

                        #[cfg(feature = "assets")]
                        let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                        Box::new(renderer) as Box<dyn Renderer>
                    }
                    #[cfg(not(feature = "datastar"))]
                    unreachable!()
                } else if cfg!(feature = "htmx") {
                    #[cfg(feature = "htmx")]
                    {
                        let runtime = self
                            .runtime
                            .clone()
                            .ok_or(NativeAppError::RuntimeRequired)?;
                        let action_tx = Self::listen_actions(self.action_handlers);
                        let renderer = gigachad_renderer_htmx::HtmxRenderer::new(
                            router.clone(),
                            runtime,
                            action_tx,
                        );

                        #[cfg(feature = "assets")]
                        let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                        Box::new(renderer) as Box<dyn Renderer>
                    }
                    #[cfg(not(feature = "htmx"))]
                    unreachable!()
                } else if cfg!(feature = "vanilla-js") {
                    #[cfg(feature = "vanilla-js")]
                    {
                        let runtime = self
                            .runtime
                            .clone()
                            .ok_or(NativeAppError::RuntimeRequired)?;
                        let action_tx = Self::listen_actions(self.action_handlers);
                        let renderer = gigachad_renderer_vanilla_js::VanillaJsRenderer::new(
                            router.clone(),
                            runtime,
                            action_tx,
                        );

                        #[cfg(feature = "assets")]
                        let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                        Box::new(renderer) as Box<dyn Renderer>
                    }
                    #[cfg(not(feature = "vanilla-js"))]
                    unreachable!()
                } else if cfg!(feature = "html") {
                    #[cfg(feature = "html")]
                    {
                        let runtime = self
                            .runtime
                            .clone()
                            .ok_or(NativeAppError::RuntimeRequired)?;
                        let action_tx = Self::listen_actions(self.action_handlers);
                        let renderer = gigachad_renderer_html::HtmlRenderer::new(
                            router.clone(),
                            runtime,
                            action_tx,
                        );

                        #[cfg(feature = "assets")]
                        let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                        Box::new(renderer) as Box<dyn Renderer>
                    }
                    #[cfg(not(feature = "html"))]
                    unreachable!()
                } else {
                    panic!("Missing renderer")
                })
            },
            Ok::<_, NativeAppError>,
        );

        let mut app = NativeApp {
            x: self.x,
            y: self.y,
            background: self.background,
            width: self.width,
            height: self.height,
            router,
            renderer: Arc::new(RwLock::new(renderer?)),
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
    background: Option<Color>,
    width: Option<f32>,
    height: Option<f32>,
    pub router: Router,
    pub renderer: Arc<RwLock<Box<dyn Renderer>>>,
    runtime_handle: Option<tokio::runtime::Handle>,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
}

impl NativeApp {
    async fn start(&mut self) -> Result<(), NativeAppError> {
        self.renderer
            .write()
            .await
            .init(
                self.width.unwrap_or(800.0),
                self.height.unwrap_or(600.0),
                self.x,
                self.y,
                self.background,
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
                    renderer.read().await.render(element).await?;
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
            .read()
            .await
            .to_runner()
            .await
            .map_err(NativeAppError::Other)
    }
}
