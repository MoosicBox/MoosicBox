#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

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

pub use stub::stub;

#[cfg(feature = "actix")]
pub use actix::router_to_actix;

#[cfg(feature = "lambda")]
pub use lambda::router_to_lambda;

pub mod html;
pub mod stub;

#[cfg(feature = "actix")]
pub mod actix;

#[cfg(feature = "lambda")]
pub mod lambda;

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

#[derive(Clone)]
pub struct HtmlRenderer<T: HtmlApp + ToRenderRunner + Send + Sync + Clone> {
    width: Option<f32>,
    height: Option<f32>,
    x: Option<i32>,
    y: Option<i32>,
    pub app: T,
    receiver: Receiver<String>,
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
