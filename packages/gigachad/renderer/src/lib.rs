#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "canvas")]
pub mod canvas;
#[cfg(feature = "viewport")]
pub mod viewport;

use std::{future::Future, pin::Pin};

use async_trait::async_trait;
pub use gigachad_color::Color;
use gigachad_transformer::{html::ParseError, ContainerElement};

#[derive(Default, Debug, Clone)]
pub struct PartialView {
    pub target: String,
    pub container: ContainerElement,
}

#[derive(Default)]
pub struct View {
    pub future: Option<Pin<Box<dyn Future<Output = ContainerElement> + Send>>>,
    pub immediate: ContainerElement,
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

impl From<ContainerElement> for View {
    fn from(value: ContainerElement) -> Self {
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

#[async_trait]
pub trait Renderer: Send + Sync {
    /// # Errors
    ///
    /// Will error if `Renderer` implementation app fails to start
    async fn init(
        &mut self,
        width: u16,
        height: u16,
        x: Option<i32>,
        y: Option<i32>,
        background: Option<Color>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to run.
    async fn to_runner(
        &mut self,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the view.
    fn render(&mut self, view: View) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the partial elements.
    fn render_partial(
        &mut self,
        partial: PartialView,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;

    /// # Errors
    ///
    /// Will error if `Renderer` implementation fails to render the canvas update.
    #[cfg(feature = "canvas")]
    fn render_canvas(
        &mut self,
        update: canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>>;
}
