#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    io::Write,
    sync::{Arc, RwLockReadGuard, RwLockWriteGuard},
};

use async_trait::async_trait;
use flume::Sender;
use gigachad_renderer::{canvas::CanvasUpdate, Color, PartialView, RenderRunner, Renderer, View};
use gigachad_renderer_html::{
    html::{element_style_to_html, write_attr, HtmlTagRenderer},
    HeaderMap, HtmlRenderer,
};
use gigachad_router::Router;
use gigachad_transformer::{models::Route, ContainerElement};
use tokio::runtime::Runtime;

pub struct HtmxTagRenderer;

impl HtmlTagRenderer for HtmxTagRenderer {
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        element: &ContainerElement,
    ) -> Result<(), std::io::Error> {
        if let Some(route) = &element.route {
            match route {
                Route::Get { route, trigger } => {
                    write_attr(f, b"hx-swap", b"outerHTML")?;
                    write_attr(f, b"hx-get", route.as_bytes())?;
                    if let Some(trigger) = trigger {
                        write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                    }
                }
                Route::Post { route, trigger } => {
                    write_attr(f, b"hx-swap", b"outerHTML")?;
                    write_attr(f, b"hx-post", route.as_bytes())?;
                    if let Some(trigger) = trigger {
                        write_attr(f, b"hx-trigger", trigger.as_bytes())?;
                    }
                }
            }
        }

        element_style_to_html(f, element)?;

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
                                margin: 0;{background}
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
    html_renderer: HtmlRenderer,
}

impl HtmxRenderer {
    #[must_use]
    pub fn new(router: Router, runtime: Arc<Runtime>, request_action: Sender<String>) -> Self {
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
}

#[async_trait]
impl Renderer for HtmxRenderer {
    /// # Errors
    ///
    /// Will error if htmx app fails to start
    async fn init(
        &mut self,
        width: u16,
        height: u16,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<Color>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer
            .init(width, height, x, y, background)
            .await?;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if htmx fails to run the event loop.
    async fn to_runner(
        &mut self,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        self.html_renderer.to_runner().await
    }

    /// # Errors
    ///
    /// Will error if htmx fails to render the elements.
    fn render(
        &mut self,
        elements: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer.render(elements)?;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if htmx fails to render the partial view.
    fn render_partial(
        &mut self,
        view: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer.render_partial(view)?;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if htmx fails to render the canvas update.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
    fn render_canvas(
        &mut self,
        _update: CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::trace!("render_canvas");

        Ok(())
    }

    fn container(&self) -> RwLockReadGuard<ContainerElement> {
        unimplemented!();
    }

    fn container_mut(&self) -> RwLockWriteGuard<ContainerElement> {
        unimplemented!();
    }
}
