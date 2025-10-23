#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "assets")]
pub mod assets;
#[cfg(feature = "canvas")]
pub mod canvas;
#[cfg(feature = "viewport")]
pub mod viewport;

use async_trait::async_trait;
use bytes::Bytes;
pub use hyperchad_color::Color;
use hyperchad_transformer::{Container, ResponsiveTrigger, html::ParseError};
pub use switchy_async::runtime::Handle;

pub use hyperchad_transformer as transformer;

#[derive(Debug)]
pub enum RendererEvent {
    View(Box<View>),
    #[cfg(feature = "canvas")]
    CanvasUpdate(canvas::CanvasUpdate),
    Event {
        name: String,
        value: Option<String>,
    },
}

pub enum Content {
    View(Box<View>),
    #[cfg(feature = "json")]
    Json(serde_json::Value),
    Raw {
        data: Bytes,
        content_type: String,
    },
}

impl Content {
    /// Create a view with primary content
    #[must_use]
    pub fn view(primary: impl Into<Container>) -> ViewBuilder {
        ViewBuilder {
            primary: Some(primary.into()),
            fragments: vec![],
            delete_selectors: vec![],
        }
    }

    /// Create a fragments-only view (no primary content)
    #[must_use]
    pub const fn fragments_only() -> ViewBuilder {
        ViewBuilder {
            primary: None,
            fragments: vec![],
            delete_selectors: vec![],
        }
    }

    /// # Errors
    ///
    /// * If the `view` fails to convert to a `View`
    pub fn try_view<T: TryInto<View>>(view: T) -> Result<Self, T::Error> {
        Ok(Self::View(Box::new(view.try_into()?)))
    }
}

/// Unified view structure - handles full pages, partials, and composite responses
#[derive(Debug, Clone, Default)]
pub struct View {
    /// Primary content (swaps to triggering element)
    /// None = fragments-only response
    pub primary: Option<Container>,

    /// Additional containers to swap by ID
    /// Each container MUST have an `id` attribute
    /// Client finds DOM elements with matching IDs and swaps them
    pub fragments: Vec<Container>,

    /// Element selectors to delete from the DOM
    /// Client finds DOM elements with matching selectors and removes them
    pub delete_selectors: Vec<hyperchad_transformer::models::Selector>,
}

impl View {
    #[must_use]
    pub fn builder() -> ViewBuilder {
        ViewBuilder::default()
    }
}

#[derive(Debug, Default)]
pub struct ViewBuilder {
    primary: Option<Container>,
    fragments: Vec<Container>,
    delete_selectors: Vec<hyperchad_transformer::models::Selector>,
}

impl ViewBuilder {
    /// Add a fragment container (must have an ID)
    #[must_use]
    pub fn fragment(mut self, container: impl Into<Container>) -> Self {
        self.fragments.push(container.into());
        self
    }

    /// Add multiple fragment containers
    #[must_use]
    pub fn fragments(mut self, containers: impl IntoIterator<Item = impl Into<Container>>) -> Self {
        self.fragments
            .extend(containers.into_iter().map(Into::into));
        self
    }

    /// Add a delete selector
    #[must_use]
    pub fn delete_selector(mut self, selector: hyperchad_transformer::models::Selector) -> Self {
        self.delete_selectors.push(selector);
        self
    }

    /// Add multiple delete selectors
    #[must_use]
    pub fn delete_selectors(
        mut self,
        selectors: impl IntoIterator<Item = hyperchad_transformer::models::Selector>,
    ) -> Self {
        self.delete_selectors.extend(selectors);
        self
    }

    /// Build the View
    #[must_use]
    pub fn build(self) -> Content {
        Content::View(Box::new(View {
            primary: self.primary,
            fragments: self.fragments,
            delete_selectors: self.delete_selectors,
        }))
    }
}

impl From<ViewBuilder> for Content {
    fn from(builder: ViewBuilder) -> Self {
        builder.build()
    }
}

#[cfg(feature = "json")]
impl TryFrom<serde_json::Value> for Content {
    type Error = serde_json::Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        Ok(Self::Json(value))
    }
}

impl<'a> TryFrom<&'a str> for Content {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(Self::Raw {
            data: value.as_bytes().to_vec().into(),
            content_type: "text/html".to_string(),
        })
    }
}

impl TryFrom<String> for Content {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl From<Container> for Content {
    fn from(value: Container) -> Self {
        Self::View(Box::new(View {
            primary: Some(value),
            fragments: vec![],
            delete_selectors: vec![],
        }))
    }
}

impl From<Vec<Container>> for Content {
    fn from(value: Vec<Container>) -> Self {
        Container {
            children: value,
            ..Default::default()
        }
        .into()
    }
}

impl From<View> for Content {
    fn from(value: View) -> Self {
        Self::View(Box::new(value))
    }
}

impl<'a> TryFrom<&'a str> for View {
    type Error = ParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(Self {
            primary: Some(value.try_into()?),
            fragments: vec![],
            delete_selectors: vec![],
        })
    }
}

impl TryFrom<String> for View {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self {
            primary: Some(value.try_into()?),
            fragments: vec![],
            delete_selectors: vec![],
        })
    }
}

impl From<Container> for View {
    fn from(value: Container) -> Self {
        Self {
            primary: Some(value),
            fragments: vec![],
            delete_selectors: vec![],
        }
    }
}

impl From<Vec<Container>> for View {
    fn from(value: Vec<Container>) -> Self {
        Self {
            primary: Some(value.into()),
            fragments: vec![],
            delete_selectors: vec![],
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
    /// Will error if `Renderer` implementation fails to render the canvas update.
    #[cfg(feature = "canvas")]
    async fn render_canvas(
        &self,
        update: canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        unimplemented!("Unable to render canvas update={update:?}")
    }
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
        headers: &std::collections::BTreeMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
    ) -> String;

    #[allow(clippy::too_many_arguments)]
    fn root_html(
        &self,
        headers: &std::collections::BTreeMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
    ) -> String;
}
