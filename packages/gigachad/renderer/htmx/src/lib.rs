#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    io::Write,
    sync::{Arc, RwLockReadGuard, RwLockWriteGuard},
};

use async_trait::async_trait;
use flume::Sender;
use gigachad_actions::logic::Value;
use gigachad_renderer::{
    assets::StaticAssetRoute, canvas::CanvasUpdate, Color, PartialView, RenderRunner, Renderer,
    View,
};
use gigachad_renderer_html::{
    html::{element_classes_to_html, element_style_to_html, write_attr, HtmlTagRenderer},
    HeaderMap, HtmlRenderer,
};
use gigachad_router::Router;
use gigachad_transformer::{models::Route, Container};
use tokio::runtime::Runtime;

pub struct HtmxTagRenderer;

impl HtmlTagRenderer for HtmxTagRenderer {
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error> {
        if let Some(route) = &container.route {
            match route {
                Route::Get {
                    route,
                    trigger,
                    swap,
                } => {
                    match swap {
                        gigachad_transformer::models::SwapTarget::This => {
                            write_attr(f, b"hx-swap", b"outerHTML")?;
                        }
                        gigachad_transformer::models::SwapTarget::Children => {
                            write_attr(f, b"hx-swap", b"innerHTML")?;
                        }
                    }
                    write_attr(f, b"hx-get", route.as_bytes())?;
                    if let Some(trigger) = trigger {
                        write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                    }
                }
                Route::Post {
                    route,
                    trigger,
                    swap,
                } => {
                    match swap {
                        gigachad_transformer::models::SwapTarget::This => {
                            write_attr(f, b"hx-swap", b"outerHTML")?;
                        }
                        gigachad_transformer::models::SwapTarget::Children => {
                            write_attr(f, b"hx-swap", b"innerHTML")?;
                        }
                    }
                    write_attr(f, b"hx-swap", b"outerHTML")?;
                    write_attr(f, b"hx-post", route.as_bytes())?;
                    if let Some(trigger) = trigger {
                        write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                    }
                }
            }
        }

        if let Some(id) = &container.str_id {
            f.write_all(b" id=\"")?;
            f.write_all(id.as_bytes())?;
            f.write_all(b"\"")?;
        }

        element_style_to_html(f, container, is_flex_child)?;
        element_classes_to_html(f, container)?;

        Ok(())
    }

    fn root_html(&self, headers: &HeaderMap, content: String, background: Option<Color>) -> String {
        if headers.get("hx-request").is_some() {
            content
        } else {
            format!(
                r#"
                <html>
                    <head>
                        <script
                            src="https://unpkg.com/htmx.org@2.0.3"
                            integrity="sha384-0895/pl2MU10Hqc6jd4RvrthNlDiE9U1tWmX7WRESftEDRosgxNsQG/Ze9YMRzHq"
                            crossorigin="anonymous"
                        ></script>
                        <style>
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
                        </style>
                    </head>
                    <body>{content}</body>
                </html>
                "#,
                background = background
                    .map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b))
                    .as_deref()
                    .unwrap_or("")
            )
        }
    }
}

#[derive(Clone)]
pub struct HtmxRenderer {
    pub html_renderer: HtmlRenderer,
}

impl HtmxRenderer {
    #[must_use]
    pub fn new(
        router: Router,
        runtime: Arc<Runtime>,
        request_action: Sender<(String, Option<Value>)>,
    ) -> Self {
        Self {
            html_renderer: HtmlRenderer::new_with_tag_renderer(
                router,
                runtime,
                request_action,
                HtmxTagRenderer,
            ),
        }
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.html_renderer.wait_for_navigation().await
    }

    #[must_use]
    pub fn with_static_asset_routes(mut self, paths: impl Into<Vec<StaticAssetRoute>>) -> Self {
        self.html_renderer = self.html_renderer.with_static_asset_routes(paths);
        self
    }
}

#[async_trait]
impl Renderer for HtmxRenderer {
    /// # Errors
    ///
    /// Will error if htmx app fails to start
    async fn init(
        &mut self,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<Color>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer
            .init(width, height, x, y, background)
            .await
    }

    /// # Errors
    ///
    /// Will error if htmx fails to run the event loop.
    async fn to_runner(&self) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        self.html_renderer.to_runner().await
    }

    /// # Errors
    ///
    /// Will error if htmx app fails to emit the event.
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
    /// Will error if htmx fails to render the elements.
    async fn render(
        &self,
        elements: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer.render(elements).await
    }

    /// # Errors
    ///
    /// Will error if htmx fails to render the partial view.
    async fn render_partial(
        &self,
        view: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer.render_partial(view).await
    }

    /// # Errors
    ///
    /// Will error if htmx fails to render the canvas update.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    async fn render_canvas(
        &self,
        update: CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer.render_canvas(update).await
    }

    fn container(&self) -> RwLockReadGuard<Container> {
        self.html_renderer.container()
    }

    fn container_mut(&self) -> RwLockWriteGuard<Container> {
        self.html_renderer.container_mut()
    }
}
