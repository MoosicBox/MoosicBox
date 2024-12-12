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
    html::{element_classes_to_html, element_style_to_html, HtmlTagRenderer},
    HeaderMap, HtmlRenderer,
};
use gigachad_router::{Container, Router};
use tokio::runtime::Runtime;

pub struct DatastarTagRenderer;

impl HtmlTagRenderer for DatastarTagRenderer {
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error> {
        element_style_to_html(f, container, is_flex_child)?;
        element_classes_to_html(f, container)?;

        Ok(())
    }

    fn root_html(
        &self,
        _headers: &HeaderMap,
        content: String,
        background: Option<Color>,
    ) -> String {
        if false {
            content
        } else {
            format!(
                r#"
                <html>
                    <head>
                        <script
                            type="module"
                            src="https://cdn.jsdelivr.net/npm/@sudodevnull/datastar@0.19.9/dist/datastar.min.js"
                            defer
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
pub struct DatastarRenderer {
    html_renderer: HtmlRenderer,
}

impl DatastarRenderer {
    #[must_use]
    pub fn new(router: Router, runtime: Arc<Runtime>, request_action: Sender<String>) -> Self {
        Self {
            html_renderer: HtmlRenderer::new_with_tag_renderer(
                router,
                runtime,
                request_action,
                DatastarTagRenderer,
            ),
        }
    }

    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.html_renderer.wait_for_navigation().await
    }
}

#[async_trait]
impl Renderer for DatastarRenderer {
    /// # Errors
    ///
    /// Will error if Datastar app fails to start
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
            .await?;

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if Datastar fails to run the event loop.
    async fn to_runner(&self) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        self.html_renderer.to_runner().await
    }

    /// # Errors
    ///
    /// Will error if Datastar fails to render the elements.
    async fn render(
        &self,
        elements: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer.render(elements).await
    }

    /// # Errors
    ///
    /// Will error if Datastar fails to render the partial view.
    async fn render_partial(
        &self,
        view: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer.render_partial(view).await
    }

    /// # Errors
    ///
    /// Will error if Datastar fails to render the canvas update.
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
