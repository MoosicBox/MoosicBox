#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{io::Write, sync::Arc};

use async_trait::async_trait;
use flume::Sender;
use gigachad_renderer::{Color, RenderRunner, Renderer, View};
use gigachad_renderer_html::{
    html::{element_style_to_html, HtmlTagRenderer},
    HeaderMap, HtmlRenderer,
};
use gigachad_router::{ContainerElement, Router};
use tokio::runtime::Runtime;

pub struct DatastarTagRenderer;

impl HtmlTagRenderer for DatastarTagRenderer {
    fn element_attrs_to_html(
        &self,
        f: &mut dyn Write,
        element: &ContainerElement,
    ) -> Result<(), std::io::Error> {
        element_style_to_html(f, element)?;

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
    /// Will error if Datastar fails to run the event loop.
    async fn to_runner(
        &mut self,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        self.html_renderer.to_runner().await
    }

    /// # Errors
    ///
    /// Will error if Datastar fails to render the elements.
    fn render(
        &mut self,
        elements: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.html_renderer.render(elements)?;

        Ok(())
    }
}
