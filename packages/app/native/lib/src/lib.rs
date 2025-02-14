#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::sync::Arc;

use hyperchad_renderer::{transformer::ResponsiveTrigger, Color, Handle, RenderRunner, Renderer};
use hyperchad_router::Router;
use moosicbox_env_utils::default_env_usize;
use thiserror::Error;
use tokio::runtime::Runtime;

pub use hyperchad_renderer as renderer;
pub use hyperchad_router as router;

#[cfg(any(feature = "egui", feature = "fltk"))]
pub static CLIENT_INFO: std::sync::LazyLock<Arc<hyperchad_router::ClientInfo>> =
    std::sync::LazyLock::new(|| {
        let os_name = os_info::get().os_type().to_string();
        Arc::new(hyperchad_router::ClientInfo {
            os: hyperchad_router::ClientOs { name: os_name },
        })
    });

#[derive(Debug, Error)]
pub enum NativeAppError {
    #[error("Runtime required")]
    RuntimeRequired,
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send>),
}

#[cfg(feature = "logic")]
type ActionHandler = Box<
    dyn Fn(
            (&str, Option<&hyperchad_actions::logic::Value>),
        ) -> Result<bool, Box<dyn std::error::Error>>
        + Send
        + Sync,
>;
type ResizeListener = Box<dyn Fn(f32, f32) -> Result<(), Box<dyn std::error::Error>> + Send + Sync>;

pub struct NativeAppBuilder {
    x: Option<i32>,
    y: Option<i32>,
    background: Option<Color>,
    title: Option<String>,
    description: Option<String>,
    viewport: Option<String>,
    width: Option<f32>,
    height: Option<f32>,
    router: Option<Router>,
    renderer: Option<RendererType>,
    runtime_handle: Option<tokio::runtime::Handle>,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
    #[cfg(feature = "logic")]
    action_handlers: Vec<Arc<ActionHandler>>,
    resize_listeners: Vec<Arc<ResizeListener>>,
    #[cfg(feature = "assets")]
    static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
}

impl Default for NativeAppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub enum RendererType {
    #[cfg(feature = "egui")]
    Egui(hyperchad_renderer_egui::EguiRenderer),
    #[cfg(feature = "fltk")]
    Fltk(hyperchad_renderer_fltk::FltkRenderer),
    #[cfg(feature = "html")]
    #[cfg(feature = "actix")]
    Html(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::actix::ActixApp<
                hyperchad_renderer_html::actix::PreparedRequest,
                hyperchad_renderer_html::actix::HtmlActixResponseProcessor<
                    hyperchad_renderer_html::DefaultHtmlTagRenderer,
                >,
            >,
        >,
    ),
    #[cfg(feature = "html")]
    #[cfg(feature = "lambda")]
    HtmlLambda(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::lambda::LambdaApp<
                hyperchad_renderer_html::lambda::PreparedRequest,
                hyperchad_renderer_html::lambda::HtmlLambdaResponseProcessor<
                    hyperchad_renderer_html::DefaultHtmlTagRenderer,
                >,
            >,
        >,
    ),
    #[cfg(feature = "html")]
    HtmlStub(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::stub::StubApp<hyperchad_renderer_html::DefaultHtmlTagRenderer>,
        >,
    ),
    #[cfg(feature = "datastar")]
    DatastarStub(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::stub::StubApp<
                hyperchad_renderer_datastar::DatastarTagRenderer,
            >,
        >,
    ),
    #[cfg(feature = "htmx")]
    HtmxStub(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::stub::StubApp<hyperchad_renderer_htmx::HtmxTagRenderer>,
        >,
    ),
    #[cfg(feature = "vanilla-js")]
    VanillaJsStub(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::stub::StubApp<
                hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
            >,
        >,
    ),
    #[cfg(feature = "htmx")]
    #[cfg(feature = "actix")]
    Htmx(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::actix::ActixApp<
                hyperchad_renderer_html::actix::PreparedRequest,
                hyperchad_renderer_html::actix::HtmlActixResponseProcessor<
                    hyperchad_renderer_htmx::HtmxTagRenderer,
                >,
            >,
        >,
    ),
    #[cfg(feature = "htmx")]
    #[cfg(feature = "lambda")]
    HtmxLambda(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::lambda::LambdaApp<
                hyperchad_renderer_html::lambda::PreparedRequest,
                hyperchad_renderer_html::lambda::HtmlLambdaResponseProcessor<
                    hyperchad_renderer_htmx::HtmxTagRenderer,
                >,
            >,
        >,
    ),
    #[cfg(feature = "datastar")]
    #[cfg(feature = "actix")]
    Datastar(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::actix::ActixApp<
                hyperchad_renderer_html::actix::PreparedRequest,
                hyperchad_renderer_html::actix::HtmlActixResponseProcessor<
                    hyperchad_renderer_datastar::DatastarTagRenderer,
                >,
            >,
        >,
    ),
    #[cfg(feature = "datastar")]
    #[cfg(feature = "lambda")]
    DatastarLambda(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::lambda::LambdaApp<
                hyperchad_renderer_html::lambda::PreparedRequest,
                hyperchad_renderer_html::lambda::HtmlLambdaResponseProcessor<
                    hyperchad_renderer_datastar::DatastarTagRenderer,
                >,
            >,
        >,
    ),
    #[cfg(feature = "vanilla-js")]
    #[cfg(feature = "actix")]
    VanillaJs(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::actix::ActixApp<
                hyperchad_renderer_html::actix::PreparedRequest,
                hyperchad_renderer_html::actix::HtmlActixResponseProcessor<
                    hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
                >,
            >,
        >,
    ),
    #[cfg(feature = "vanilla-js")]
    #[cfg(feature = "lambda")]
    VanillaJsLambda(
        hyperchad_renderer_html::HtmlRenderer<
            hyperchad_renderer_html::lambda::LambdaApp<
                hyperchad_renderer_html::lambda::PreparedRequest,
                hyperchad_renderer_html::lambda::HtmlLambdaResponseProcessor<
                    hyperchad_renderer_vanilla_js::VanillaJsTagRenderer,
                >,
            >,
        >,
    ),
    None,
}

macro_rules! renderer {
    ($val:expr, $name:ident, $action:expr) => {{
        match $val {
            #[cfg(feature = "egui")]
            RendererType::Egui($name) => $action,
            #[cfg(feature = "fltk")]
            RendererType::Fltk($name) => $action,
            #[cfg(feature = "html")]
            #[cfg(feature = "actix")]
            RendererType::Html($name) => $action,
            #[cfg(feature = "html")]
            #[cfg(feature = "lambda")]
            RendererType::HtmlLambda($name) => $action,
            #[cfg(feature = "html")]
            RendererType::HtmlStub($name) => $action,
            #[cfg(feature = "datastar")]
            RendererType::DatastarStub($name) => $action,
            #[cfg(feature = "htmx")]
            RendererType::HtmxStub($name) => $action,
            #[cfg(feature = "vanilla-js")]
            RendererType::VanillaJsStub($name) => $action,
            #[cfg(feature = "htmx")]
            #[cfg(feature = "actix")]
            RendererType::Htmx($name) => $action,
            #[cfg(feature = "htmx")]
            #[cfg(feature = "lambda")]
            RendererType::HtmxLambda($name) => $action,
            #[cfg(feature = "datastar")]
            #[cfg(feature = "actix")]
            RendererType::Datastar($name) => $action,
            #[cfg(feature = "datastar")]
            #[cfg(feature = "lambda")]
            RendererType::DatastarLambda($name) => $action,
            #[cfg(feature = "vanilla-js")]
            #[cfg(feature = "actix")]
            RendererType::VanillaJs($name) => $action,
            #[cfg(feature = "vanilla-js")]
            #[cfg(feature = "lambda")]
            RendererType::VanillaJsLambda($name) => $action,
            RendererType::None => unimplemented!(),
        }
    }};
}

impl RendererType {
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    ///
    /// # Errors
    ///
    /// Will error if FLTK app fails to start
    #[allow(
        unused_variables,
        clippy::unused_async,
        clippy::needless_pass_by_ref_mut,
        clippy::too_many_arguments
    )]
    async fn init(
        &mut self,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
        viewport: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        renderer!(
            self,
            value,
            value
                .init(
                    width,
                    height,
                    x,
                    y,
                    background,
                    title,
                    description,
                    viewport
                )
                .await
        )
    }

    /// # Errors
    ///
    /// * If failed to convert the value to a `RenderRunner`
    #[allow(unused_variables, clippy::needless_pass_by_value)]
    fn into_runner(
        self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        renderer!(self, value, {
            use hyperchad_renderer::ToRenderRunner as _;
            value.to_runner(handle)
        })
    }

    #[allow(unused_variables, clippy::needless_pass_by_value)]
    pub fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        renderer!(self, value, value.add_responsive_trigger(name, trigger));
    }
}

#[cfg(feature = "html")]
impl From<RendererType> for Option<Box<dyn hyperchad_renderer::HtmlTagRenderer + Send + Sync>> {
    fn from(value: RendererType) -> Self {
        Some(match value {
            #[cfg(feature = "egui")]
            RendererType::Egui(..) => return None,
            #[cfg(feature = "fltk")]
            RendererType::Fltk(..) => return None,
            #[cfg(feature = "html")]
            #[cfg(feature = "actix")]
            RendererType::Html(renderer) => Box::new(renderer.app.processor.tag_renderer),
            #[cfg(feature = "html")]
            #[cfg(feature = "lambda")]
            RendererType::HtmlLambda(renderer) => Box::new(renderer.app.processor.tag_renderer),
            #[cfg(feature = "html")]
            RendererType::HtmlStub(renderer) => Box::new(renderer.app.tag_renderer),
            #[cfg(feature = "datastar")]
            RendererType::DatastarStub(renderer) => Box::new(renderer.app.tag_renderer),
            #[cfg(feature = "htmx")]
            RendererType::HtmxStub(renderer) => Box::new(renderer.app.tag_renderer),
            #[cfg(feature = "vanilla-js")]
            RendererType::VanillaJsStub(renderer) => Box::new(renderer.app.tag_renderer),
            #[cfg(feature = "htmx")]
            #[cfg(feature = "actix")]
            RendererType::Htmx(renderer) => Box::new(renderer.app.processor.tag_renderer),
            #[cfg(feature = "htmx")]
            #[cfg(feature = "lambda")]
            RendererType::HtmxLambda(renderer) => Box::new(renderer.app.processor.tag_renderer),
            #[cfg(feature = "datastar")]
            #[cfg(feature = "actix")]
            RendererType::Datastar(renderer) => Box::new(renderer.app.processor.tag_renderer),
            #[cfg(feature = "datastar")]
            #[cfg(feature = "lambda")]
            RendererType::DatastarLambda(renderer) => Box::new(renderer.app.processor.tag_renderer),
            #[cfg(feature = "vanilla-js")]
            #[cfg(feature = "actix")]
            RendererType::VanillaJs(renderer) => Box::new(renderer.app.processor.tag_renderer),
            #[cfg(feature = "vanilla-js")]
            #[cfg(feature = "lambda")]
            RendererType::VanillaJsLambda(renderer) => {
                Box::new(renderer.app.processor.tag_renderer)
            }
            RendererType::None => unimplemented!(),
        })
    }
}

impl From<RendererType> for Box<dyn Renderer> {
    fn from(value: RendererType) -> Self {
        renderer!(value, value, Box::new(value))
    }
}

impl NativeAppBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            x: None,
            y: None,
            background: None,
            title: None,
            description: None,
            viewport: None,
            width: None,
            height: None,
            router: None,
            renderer: None,
            runtime_handle: None,
            runtime: None,
            #[cfg(feature = "logic")]
            action_handlers: vec![],
            resize_listeners: vec![],
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
        }
    }

    #[must_use]
    pub fn with_renderer(mut self, renderer: RendererType) -> Self {
        self.renderer.replace(renderer);
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
    pub fn with_viewport(mut self, content: String) -> Self {
        self.viewport.replace(content);
        self
    }

    #[must_use]
    pub fn with_background(mut self, color: Color) -> Self {
        self.background.replace(color);
        self
    }

    #[must_use]
    pub fn with_title(mut self, title: String) -> Self {
        self.title.replace(title);
        self
    }

    #[must_use]
    pub fn with_description(mut self, description: String) -> Self {
        self.description.replace(description);
        self
    }

    #[cfg(feature = "logic")]
    #[must_use]
    pub fn with_action_handler<E: std::error::Error + 'static>(
        mut self,
        func: impl Fn(&str, Option<&hyperchad_actions::logic::Value>) -> Result<bool, E>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.action_handlers.push(Arc::new(Box::new(move |(a, b)| {
            func(a, b).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        })));
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
        func: impl Fn(f32, f32) -> Result<(), E> + Send + Sync + 'static,
    ) -> Self {
        self.resize_listeners
            .push(Arc::new(Box::new(move |width, height| {
                func(width, height).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            })));
        self
    }

    #[cfg(feature = "logic")]
    #[allow(unused)]
    #[must_use]
    fn listen_actions(
        action_handlers: Vec<Arc<ActionHandler>>,
    ) -> flume::Sender<(String, Option<hyperchad_actions::logic::Value>)> {
        let (action_tx, action_rx) =
            flume::unbounded::<(String, Option<hyperchad_actions::logic::Value>)>();

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
    fn listen_resize(resize_listeners: Vec<Arc<ResizeListener>>) -> flume::Sender<(f32, f32)> {
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
        path: impl Into<hyperchad_renderer::assets::StaticAssetRoute>,
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
        Path: TryInto<hyperchad_renderer::assets::StaticAssetRoute>,
    >(
        mut self,
        path: Path,
    ) -> Result<Self, Path::Error> {
        self.static_asset_routes.push(path.try_into()?);
        Ok(self)
    }

    /// # Panics
    ///
    /// * If missing router
    ///
    /// # Errors
    ///
    /// * If there was an error starting the app
    #[allow(clippy::too_many_lines)]
    pub async fn start(self) -> Result<NativeApp, NativeAppError> {
        let mut app = NativeApp {
            x: self.x,
            y: self.y,
            background: self.background,
            title: self.title.clone(),
            description: self.description.clone(),
            viewport: self.viewport.clone(),
            width: self.width,
            height: self.height,
            router: self.router.clone().unwrap(),
            runtime_handle: self.runtime_handle.clone(),
            runtime: self.runtime.clone(),
            renderer: if let Some(renderer) = self.renderer {
                renderer
            } else {
                self.get_renderer()?
            },
        };
        app.start().await?;
        Ok(app)
    }

    /// # Panics
    ///
    /// * If missing router
    /// * If failed to start tokio runtime
    ///
    /// # Errors
    ///
    /// * If there was an error getting the renderer
    #[allow(clippy::too_many_lines)]
    pub fn get_renderer(self) -> Result<RendererType, NativeAppError> {
        #[allow(unreachable_code)]
        Ok(if cfg!(feature = "egui") {
            #[cfg(feature = "egui")]
            {
                let router = self.router.unwrap();
                let action_tx = Self::listen_actions(self.action_handlers);
                let resize_tx = Self::listen_resize(self.resize_listeners);
                let renderer = hyperchad_renderer_egui::EguiRenderer::new(
                    router.clone(),
                    #[cfg(feature = "logic")]
                    action_tx,
                    resize_tx,
                    CLIENT_INFO.clone(),
                );

                moosicbox_task::spawn("egui navigation listener", {
                    let renderer = renderer.clone();
                    async move {
                        while let Some(path) = renderer.wait_for_navigation().await {
                            if let Err(e) = router
                                .navigate_send(
                                    &path,
                                    hyperchad_router::RequestInfo {
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
                RendererType::Egui(renderer)
            }
            #[cfg(not(feature = "egui"))]
            unreachable!()
        } else if cfg!(feature = "fltk") {
            #[cfg(feature = "fltk")]
            {
                let router = self.router.unwrap();
                let action_tx = Self::listen_actions(self.action_handlers);
                let renderer = hyperchad_renderer_fltk::FltkRenderer::new(action_tx);
                moosicbox_task::spawn("fltk navigation listener", {
                    let renderer = renderer.clone();
                    async move {
                        while let Some(path) = renderer.wait_for_navigation().await {
                            if let Err(e) = router
                                .navigate_send(
                                    &path,
                                    hyperchad_router::RequestInfo {
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
                RendererType::Fltk(renderer)
            }
            #[cfg(not(feature = "fltk"))]
            unreachable!()
        } else if cfg!(all(feature = "actix", feature = "datastar")) {
            #[cfg(all(feature = "actix", feature = "datastar"))]
            {
                let router = self.router.unwrap();
                let renderer = hyperchad_renderer_html::router_to_actix(
                    hyperchad_renderer_datastar::DatastarTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                RendererType::Datastar(renderer)
            }
            #[cfg(not(all(feature = "actix", feature = "datastar")))]
            unreachable!()
        } else if cfg!(all(feature = "lambda", feature = "datastar")) {
            #[cfg(all(feature = "lambda", feature = "datastar"))]
            {
                let router = self.router.unwrap();
                let renderer = hyperchad_renderer_html::router_to_lambda(
                    hyperchad_renderer_datastar::DatastarTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                RendererType::DatastarLambda(renderer)
            }
            #[cfg(not(all(feature = "lambda", feature = "datastar")))]
            unreachable!()
        } else if cfg!(all(feature = "actix", feature = "htmx")) {
            #[cfg(all(feature = "actix", feature = "htmx"))]
            {
                let router = self.router.unwrap();
                let renderer = hyperchad_renderer_html::router_to_actix(
                    hyperchad_renderer_htmx::HtmxTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                RendererType::Htmx(renderer)
            }
            #[cfg(not(all(feature = "actix", feature = "htmx")))]
            unreachable!()
        } else if cfg!(all(feature = "lambda", feature = "htmx")) {
            #[cfg(all(feature = "lambda", feature = "htmx"))]
            {
                let router = self.router.unwrap();
                let renderer = hyperchad_renderer_html::router_to_lambda(
                    hyperchad_renderer_htmx::HtmxTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                RendererType::HtmxLambda(renderer)
            }
            #[cfg(not(all(feature = "lambda", feature = "htmx")))]
            unreachable!()
        } else if cfg!(all(feature = "actix", feature = "vanilla-js")) {
            #[cfg(all(feature = "actix", feature = "vanilla-js"))]
            {
                let router = self.router.unwrap();
                let renderer = hyperchad_renderer_html::router_to_actix(
                    hyperchad_renderer_vanilla_js::VanillaJsTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                RendererType::VanillaJs(renderer)
            }
            #[cfg(not(all(feature = "actix", feature = "vanilla-js")))]
            unreachable!()
        } else if cfg!(all(feature = "lambda", feature = "vanilla-js")) {
            #[cfg(all(feature = "lambda", feature = "vanilla-js"))]
            {
                let router = self.router.unwrap();
                let renderer = hyperchad_renderer_html::router_to_lambda(
                    hyperchad_renderer_vanilla_js::VanillaJsTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                RendererType::VanillaJsLambda(renderer)
            }
            #[cfg(not(all(feature = "lambda", feature = "vanilla-js")))]
            unreachable!()
        } else if cfg!(all(feature = "actix", feature = "html")) {
            #[cfg(all(feature = "actix", feature = "html"))]
            {
                let router = self.router.unwrap();
                let renderer = hyperchad_renderer_html::router_to_actix(
                    hyperchad_renderer_html::DefaultHtmlTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                RendererType::Html(renderer)
            }
            #[cfg(not(all(feature = "actix", feature = "html")))]
            unreachable!()
        } else if cfg!(all(feature = "lambda", feature = "html")) {
            #[cfg(all(feature = "lambda", feature = "html"))]
            {
                let router = self.router.unwrap();
                let renderer = hyperchad_renderer_html::router_to_lambda(
                    hyperchad_renderer_html::DefaultHtmlTagRenderer::default(),
                    router,
                );

                #[cfg(feature = "assets")]
                let renderer = renderer.with_static_asset_routes(self.static_asset_routes);

                RendererType::HtmlLambda(renderer)
            }
            #[cfg(not(all(feature = "lambda", feature = "html")))]
            unreachable!()
        } else if cfg!(feature = "html") {
            #[cfg(feature = "html")]
            {
                if cfg!(feature = "datastar") {
                    #[cfg(feature = "datastar")]
                    {
                        RendererType::DatastarStub(hyperchad_renderer_html::HtmlRenderer::new(
                            hyperchad_renderer_html::stub::StubApp::new(
                                hyperchad_renderer_datastar::DatastarTagRenderer::default(),
                            ),
                        ))
                    }
                    #[cfg(not(feature = "datastar"))]
                    unreachable!()
                } else if cfg!(feature = "htmx") {
                    #[cfg(feature = "htmx")]
                    {
                        RendererType::HtmxStub(hyperchad_renderer_html::HtmlRenderer::new(
                            hyperchad_renderer_html::stub::StubApp::new(
                                hyperchad_renderer_htmx::HtmxTagRenderer::default(),
                            ),
                        ))
                    }
                    #[cfg(not(feature = "htmx"))]
                    unreachable!()
                } else if cfg!(feature = "vanilla-js") {
                    #[cfg(feature = "vanilla-js")]
                    {
                        RendererType::VanillaJsStub(hyperchad_renderer_html::HtmlRenderer::new(
                            hyperchad_renderer_html::stub::StubApp::new(
                                hyperchad_renderer_vanilla_js::VanillaJsTagRenderer::default(),
                            ),
                        ))
                    }
                    #[cfg(not(feature = "vanilla-js"))]
                    unreachable!()
                } else {
                    RendererType::HtmlStub(hyperchad_renderer_html::HtmlRenderer::new(
                        hyperchad_renderer_html::stub::StubApp::new(
                            hyperchad_renderer_html::DefaultHtmlTagRenderer::default(),
                        ),
                    ))
                }
            }
            #[cfg(not(feature = "html"))]
            unreachable!()
        } else {
            panic!("Missing renderer")
        })
    }
}

pub struct NativeApp {
    x: Option<i32>,
    y: Option<i32>,
    background: Option<Color>,
    title: Option<String>,
    description: Option<String>,
    viewport: Option<String>,
    width: Option<f32>,
    height: Option<f32>,
    pub router: Router,
    pub renderer: RendererType,
    runtime_handle: Option<tokio::runtime::Handle>,
    runtime: Option<Arc<tokio::runtime::Runtime>>,
}

impl NativeApp {
    async fn start(&mut self) -> Result<(), NativeAppError> {
        self.renderer
            .init(
                self.width.unwrap_or(800.0),
                self.height.unwrap_or(600.0),
                self.x,
                self.y,
                self.background,
                self.title.as_deref(),
                self.description.as_deref(),
                self.viewport.as_deref(),
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
            let renderer = self.renderer.clone();
            let router = self.router.clone();
            async move {
                log::debug!("app_native_lib::start: router listening");
                #[allow(unused_variables, clippy::never_loop)]
                while let Some(element) = router.wait_for_navigation().await {
                    log::debug!("app_native_lib::start: router received element");
                    renderer!(&renderer, value, value.render(element).await?);
                }
                Ok::<_, NativeAppError>(())
            }
        });

        Ok(())
    }

    /// # Errors
    ///
    /// * If there was an error starting the app
    ///
    /// # Panics
    ///
    /// * If the runtime handle doesn't exist
    pub fn to_runner(self) -> Result<Box<dyn RenderRunner>, NativeAppError> {
        log::debug!("run: getting runner");
        self.renderer
            .into_runner(self.runtime_handle.unwrap())
            .map_err(NativeAppError::Other)
    }
}
