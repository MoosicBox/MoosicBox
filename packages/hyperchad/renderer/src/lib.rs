#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "assets")]
pub mod assets;
#[cfg(feature = "canvas")]
pub mod canvas;
#[cfg(feature = "viewport")]
pub mod viewport;

use std::{
    future::Future,
    pin::Pin,
    sync::{RwLockReadGuard, RwLockWriteGuard},
};

use async_trait::async_trait;
pub use hyperchad_color::Color;
use hyperchad_transformer::{html::ParseError, Container, ResponsiveTrigger};
pub use tokio::runtime::Handle;

pub use hyperchad_transformer as transformer;

#[derive(Debug)]
pub enum RendererEvent {
    View(View),
    Partial(PartialView),
    #[cfg(feature = "canvas")]
    CanvasUpdate(canvas::CanvasUpdate),
    Event {
        name: String,
        value: Option<String>,
    },
}

pub enum Content {
    View(View),
    PartialView(PartialView),
    #[cfg(feature = "json")]
    Json(serde_json::Value),
}

#[derive(Default, Debug, Clone)]
pub struct PartialView {
    pub target: String,
    pub container: Container,
}

#[derive(Default)]
pub struct View {
    pub future: Option<Pin<Box<dyn Future<Output = Container> + Send>>>,
    pub immediate: Container,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View")
            .field("future", &self.future.is_some())
            .field("immediate", &self.immediate)
            .finish()
    }
}

#[cfg(feature = "json")]
impl TryFrom<serde_json::Value> for Content {
    type Error = serde_json::Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        Ok(Self::Json(value))
    }
}

#[cfg(feature = "maud")]
impl TryFrom<maud::Markup> for Content {
    type Error = ParseError;

    fn try_from(value: maud::Markup) -> Result<Self, Self::Error> {
        Ok(Self::View(value.into_string().try_into()?))
    }
}

impl<'a> TryFrom<&'a str> for Content {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(Self::View(View {
            future: None,
            immediate: value.try_into()?,
        }))
    }
}

impl TryFrom<String> for Content {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self::View(View {
            future: None,
            immediate: value.try_into()?,
        }))
    }
}

impl From<Container> for Content {
    fn from(value: Container) -> Self {
        Self::View(View {
            future: None,
            immediate: value,
        })
    }
}

impl From<View> for Content {
    fn from(value: View) -> Self {
        Self::View(value)
    }
}

impl From<PartialView> for Content {
    fn from(value: PartialView) -> Self {
        Self::PartialView(value)
    }
}

#[cfg(feature = "maud")]
impl TryFrom<maud::Markup> for View {
    type Error = ParseError;

    fn try_from(value: maud::Markup) -> Result<Self, Self::Error> {
        value.into_string().try_into()
    }
}

impl<'a> TryFrom<&'a str> for View {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(Self {
            future: None,
            immediate: value.try_into()?,
        })
    }
}

impl TryFrom<String> for View {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self {
            future: None,
            immediate: value.try_into()?,
        })
    }
}

impl From<Container> for View {
    fn from(value: Container) -> Self {
        Self {
            future: None,
            immediate: value,
        }
    }
}

pub trait RenderRunner: Send + Sync {
    /// # Errors
    ///
    /// Will error if fails to run
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;
}

pub trait ToRenderRunner {
    /// # Errors
    ///
    /// * If failed to convert the value to a `RenderRunner`
    fn to_runner(
        self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>>;
}

#[async_trait]
pub trait Renderer: ToRenderRunner + Send + Sync {
    /// # Errors
    ///
    /// Will error if `Renderer` implementation app fails to start
    #[allow(clippy::too_many_arguments)]
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
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger);

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to emit the event.
    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the view.
    async fn render(&self, view: View) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the partial elements.
    async fn render_partial(
        &self,
        partial: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the canvas update.
    #[cfg(feature = "canvas")]
    async fn render_canvas(
        &self,
        update: canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    fn container(&self) -> RwLockReadGuard<Container>;
    fn container_mut(&self) -> RwLockWriteGuard<Container>;
}

#[cfg(feature = "html")]
pub trait HtmlTagRenderer {
    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger);

    /// # Errors
    ///
    /// * If the `HtmlTagRenderer` fails to write the element attributes
    fn element_attrs_to_html(
        &self,
        f: &mut dyn std::io::Write,
        container: &Container,
        is_flex_child: bool,
    ) -> Result<(), std::io::Error>;

    /// # Errors
    ///
    /// * If the `HtmlTagRenderer` fails to write the css media-queries
    fn reactive_conditions_to_css(
        &self,
        _f: &mut dyn std::io::Write,
        _container: &Container,
    ) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn partial_html(
        &self,
        headers: &std::collections::HashMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
    ) -> String;

    #[allow(clippy::too_many_arguments)]
    fn root_html(
        &self,
        headers: &std::collections::HashMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
    ) -> String;
}
