#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

pub mod html;

use std::{
    collections::HashMap,
    io::Write,
    sync::{RwLockReadGuard, RwLockWriteGuard},
};

use async_trait::async_trait;
use flume::Receiver;
use gigachad_renderer::{
    canvas::CanvasUpdate, Color, HtmlTagRenderer, PartialView, RenderRunner, Renderer,
    ToRenderRunner, View,
};
use gigachad_router::Container;
use html::{element_classes_to_html, element_style_to_html};
use maud::{html, PreEscaped};
use tokio::runtime::Handle;

pub struct DefaultHtmlTagRenderer;

impl HtmlTagRenderer for DefaultHtmlTagRenderer {
    /// # Errors
    ///
    /// * If the `HtmlTagRenderer` fails to write the element attributes
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error> {
        if let Some(id) = &container.str_id {
            f.write_all(b" id=\"")?;
            f.write_all(id.as_bytes())?;
            f.write_all(b"\"")?;
        }

        element_style_to_html(f, container, is_flex_child)?;
        element_classes_to_html(f, container)?;

        Ok(())
    }

    fn root_html(
        &self,
        _headers: &HashMap<String, String>,
        content: String,
        background: Option<Color>,
    ) -> String {
        let background = background.map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b));
        let background = background.as_deref().unwrap_or("");

        html! {
            html {
                head {
                    style {(format!(r"
                        body {{
                            margin: 0;{background};
                            overflow: hidden;
                        }}
                        .remove-button-styles {{
                            background: none;
                            color: inherit;
                            border: none;
                            padding: 0;
                            font: inherit;
                            cursor: pointer;
                            outline: inherit;
                        }}
                    "))}
                }
                body {
                    (PreEscaped(content))
                }
            }
        }
        .into_string()
    }
}

pub trait HtmlApp {
    #[cfg(feature = "assets")]
    #[must_use]
    fn with_static_asset_routes(
        self,
        paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
    ) -> Self;

    #[must_use]
    fn with_tag_renderer(self, tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static) -> Self;

    #[must_use]
    fn with_background(self, background: Option<Color>) -> Self;

    fn set_background(&mut self, background: Option<Color>);
}

#[cfg(feature = "actix")]
pub mod actix {
    use std::{
        collections::HashMap,
        sync::{Arc, LazyLock},
    };

    use async_trait::async_trait;
    use gigachad_renderer::{Color, HtmlTagRenderer};
    use gigachad_renderer_html_actix::actix_web::{
        error::ErrorInternalServerError, http::header::USER_AGENT,
    };
    use gigachad_router::{ClientInfo, ClientOs, RequestInfo, Router};
    use uaparser::{Parser as _, UserAgentParser};

    use crate::{
        html::{container_element_to_html, container_element_to_html_response},
        DefaultHtmlTagRenderer, HtmlApp,
    };

    pub use gigachad_renderer_html_actix::*;

    #[derive(Clone)]
    pub struct HtmlActixResponseProcessor {
        pub router: Router,
        pub tag_renderer: Arc<Box<dyn HtmlTagRenderer + Send + Sync>>,
        pub background: Option<Color>,
    }

    impl HtmlActixResponseProcessor {
        #[must_use]
        pub fn new(router: Router) -> Self {
            Self {
                router,
                tag_renderer: Arc::new(Box::new(DefaultHtmlTagRenderer)),
                background: None,
            }
        }
    }

    impl HtmlApp for ActixApp<PreparedRequest, HtmlActixResponseProcessor> {
        #[must_use]
        fn with_tag_renderer(
            mut self,
            tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static,
        ) -> Self {
            self.processor.tag_renderer = Arc::new(Box::new(tag_renderer));
            self
        }

        #[must_use]
        fn with_background(mut self, background: Option<Color>) -> Self {
            self.processor.background = background;
            self
        }

        fn set_background(&mut self, background: Option<Color>) {
            self.processor.background = background;
        }

        #[cfg(feature = "assets")]
        #[must_use]
        fn with_static_asset_routes(
            mut self,
            paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
        ) -> Self {
            self.static_asset_routes = paths.into();
            self
        }
    }

    #[derive(Clone)]
    pub struct PreparedRequest {
        full: bool,
        path: String,
        info: RequestInfo,
    }

    #[async_trait]
    impl gigachad_renderer_html_actix::ActixResponseProcessor<PreparedRequest>
        for HtmlActixResponseProcessor
    {
        fn prepare_request(
            &self,
            req: actix_web::HttpRequest,
        ) -> Result<PreparedRequest, actix_web::Error> {
            static UA_PARSER: LazyLock<UserAgentParser> = LazyLock::new(|| {
                UserAgentParser::from_bytes(include_bytes!("../ua-regexes.yaml"))
                    .expect("Parser creation failed")
            });

            let query_string = req.query_string();
            let query_string = if query_string.is_empty() {
                String::new()
            } else {
                format!("?{query_string}")
            };

            let path = format!("{}{}", req.path(), query_string);

            let os_name =
                if let Some(Ok(user_agent)) = req.headers().get(USER_AGENT).map(|x| x.to_str()) {
                    let os = UA_PARSER.parse_os(user_agent);

                    os.family.to_string()
                } else {
                    "unknown".to_string()
                };

            Ok(PreparedRequest {
                full: req.headers().get("hx-request").is_none(),
                path,
                info: RequestInfo {
                    client: Arc::new(ClientInfo {
                        os: ClientOs { name: os_name },
                    }),
                },
            })
        }

        async fn to_html(&self, req: PreparedRequest) -> Result<String, actix_web::Error> {
            static HEADERS: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

            let view = self
                .router
                .navigate(&req.path, req.info)
                .await
                .map_err(ErrorInternalServerError)?;

            if req.full {
                container_element_to_html_response(
                    &HEADERS,
                    &view.immediate,
                    self.background,
                    &**self.tag_renderer,
                )
                .map_err(ErrorInternalServerError)
            } else {
                container_element_to_html(&view.immediate, &**self.tag_renderer)
                    .map_err(ErrorInternalServerError)
            }
        }
    }
}

#[derive(Clone)]
pub struct StubApp;

impl HtmlApp for StubApp {
    #[cfg(feature = "assets")]
    fn with_static_asset_routes(
        self,
        _paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self
    }

    fn with_tag_renderer(
        self,
        _tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static,
    ) -> Self {
        self
    }

    fn with_background(self, _background: Option<Color>) -> Self {
        self
    }

    fn set_background(&mut self, _background: Option<Color>) {}
}

#[derive(Clone)]
pub struct StubRunner;

impl RenderRunner for StubRunner {
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }
}

impl ToRenderRunner for StubApp {
    fn to_runner(
        &self,
        _handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(StubRunner))
    }
}

#[must_use]
pub fn stub() -> HtmlRenderer<StubApp> {
    HtmlRenderer::new(StubApp)
}

#[derive(Clone)]
pub struct HtmlRenderer<T: HtmlApp + ToRenderRunner + Send + Sync + Clone> {
    width: Option<f32>,
    height: Option<f32>,
    x: Option<i32>,
    y: Option<i32>,
    pub app: T,
    receiver: Receiver<String>,
}

#[cfg(feature = "actix")]
#[must_use]
pub fn router_to_actix(
    value: gigachad_router::Router,
) -> HtmlRenderer<
    gigachad_renderer_html_actix::ActixApp<
        actix::PreparedRequest,
        actix::HtmlActixResponseProcessor,
    >,
> {
    HtmlRenderer::new(gigachad_renderer_html_actix::ActixApp::new(
        actix::HtmlActixResponseProcessor::new(value),
    ))
}

impl<T: HtmlApp + ToRenderRunner + Send + Sync + Clone> HtmlRenderer<T> {
    #[must_use]
    pub fn new(app: T) -> Self {
        let (_tx, rx) = flume::unbounded();

        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            app,
            receiver: rx,
        }
    }

    #[must_use]
    pub fn with_tag_renderer(
        mut self,
        tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static,
    ) -> Self {
        self.app = self.app.with_tag_renderer(tag_renderer);
        self
    }

    #[must_use]
    pub fn with_background(mut self, background: Option<Color>) -> Self {
        self.app = self.app.with_background(background);
        self
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.receiver.recv_async().await.ok()
    }

    #[cfg(feature = "assets")]
    #[must_use]
    pub fn with_static_asset_routes(
        mut self,
        paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self.app = self.app.with_static_asset_routes(paths);
        self
    }
}

impl<T: HtmlApp + ToRenderRunner + Send + Sync + Clone> ToRenderRunner for HtmlRenderer<T> {
    /// # Errors
    ///
    /// Will error if html fails to run the event loop.
    fn to_runner(
        &self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        self.app.to_runner(handle)
    }
}

#[async_trait]
impl<T: HtmlApp + ToRenderRunner + Send + Sync + Clone> Renderer for HtmlRenderer<T> {
    /// # Errors
    ///
    /// Will error if html app fails to start
    async fn init(
        &mut self,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<Color>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.width = Some(width);
        self.height = Some(height);
        self.x = x;
        self.y = y;
        self.app.set_background(background);

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html app fails to emit the event.
    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::trace!("emit_event: event_name={event_name} event_value={event_value:?}");

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html fails to render the elements.
    async fn render(
        &self,
        elements: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        moosicbox_logging::debug_or_trace!(
            ("render: start"),
            ("render: start {:?}", elements.immediate)
        );

        log::debug!("render: finished");

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html fails to render the partial view.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render_partial(
        &self,
        view: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        moosicbox_logging::debug_or_trace!(
            ("render_partial: start"),
            ("render_partial: start {:?}", view)
        );

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html fails to render the canvas update.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render_canvas(
        &self,
        _update: CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::trace!("render_canvas");

        Ok(())
    }

    fn container(&self) -> RwLockReadGuard<Container> {
        unimplemented!();
    }

    fn container_mut(&self) -> RwLockWriteGuard<Container> {
        unimplemented!();
    }
}
