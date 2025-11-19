//! HTML renderer for `HyperChad` UI framework.
//!
//! This crate provides HTML rendering capabilities for `HyperChad` applications,
//! converting `HyperChad` containers into HTML elements with CSS styling. It supports
//! responsive design through media queries and can integrate with various web frameworks.
//!
//! # Features
//!
//! * HTML rendering with CSS styling and responsive design
//! * Support for multiple backend integrations (Actix, Lambda, custom web servers)
//! * Static asset routing
//! * Extensible renderer with custom event handling
//! * Canvas rendering support
//!
//! # Example
//!
//! ```rust
//! use hyperchad_renderer_html::{DefaultHtmlTagRenderer, HtmlRenderer};
//! use hyperchad_renderer_html::stub::StubApp;
//!
//! let tag_renderer = DefaultHtmlTagRenderer::default();
//! let app = StubApp::new(tag_renderer);
//! let renderer = HtmlRenderer::new(app);
//! ```
//!
//! # Feature Flags
//!
//! * `actix` - Enables Actix web framework integration
//! * `lambda` - Enables AWS Lambda integration
//! * `web-server` - Enables custom web server support
//! * `assets` - Enables static asset routing
//! * `extend` - Enables renderer extension capabilities
//! * `sse` - Enables server-sent events support (requires `actix`)

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, io::Write};

use async_trait::async_trait;
use flume::Receiver;
use html::{
    element_classes_to_html, element_style_to_html, number_to_html_string, write_css_attr_important,
};
use hyperchad_renderer::{
    Color, Handle, HtmlTagRenderer, RenderRunner, Renderer, ToRenderRunner, View,
    canvas::CanvasUpdate,
};
use hyperchad_router::Container;
use hyperchad_transformer::{
    OverrideCondition, OverrideItem, ResponsiveTrigger,
    models::{
        AlignItems, LayoutDirection, OverflowWrap, TextAlign, TextOverflow, UserSelect, Visibility,
        WhiteSpace,
    },
};
use maud::{DOCTYPE, PreEscaped, html};

#[cfg(feature = "actix")]
pub use actix::router_to_actix;

#[cfg(feature = "lambda")]
pub use lambda::router_to_lambda;

#[cfg(feature = "web-server")]
pub use web_server::router_to_web_server;

pub mod html;
pub mod stub;

#[cfg(feature = "actix")]
pub mod actix;

#[cfg(feature = "lambda")]
pub mod lambda;

#[cfg(feature = "web-server")]
pub mod web_server;

#[cfg(feature = "extend")]
pub mod extend;

/// Default implementation of HTML tag rendering with responsive trigger support.
///
/// This renderer converts hyperchad containers into HTML elements with CSS styling
/// and supports responsive design through media queries.
#[derive(Debug, Default, Clone)]
pub struct DefaultHtmlTagRenderer {
    /// Map of responsive trigger names to their trigger conditions.
    pub responsive_triggers: BTreeMap<String, ResponsiveTrigger>,
}

impl DefaultHtmlTagRenderer {
    /// Adds a responsive trigger and returns the modified renderer.
    ///
    /// Responsive triggers define media query conditions that can be referenced
    /// by containers to apply responsive overrides.
    #[must_use]
    pub fn with_responsive_trigger(
        mut self,
        name: impl Into<String>,
        trigger: ResponsiveTrigger,
    ) -> Self {
        self.add_responsive_trigger(name.into(), trigger);
        self
    }
}

impl HtmlTagRenderer for DefaultHtmlTagRenderer {
    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.responsive_triggers.insert(name, trigger);
    }

    /// Writes HTML element attributes for a container to the output.
    ///
    /// Generates HTML attributes including ID, styling, classes, and data attributes
    /// for the given container element.
    ///
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

        for (key, value) in &container.data {
            f.write_all(b" data-")?;
            f.write_all(key.as_bytes())?;
            f.write_all(b"=\"")?;
            f.write_all(html_escape::encode_quoted_attribute(value).as_bytes())?;
            f.write_all(b"\"")?;
        }

        Ok(())
    }

    /// Writes CSS media queries for responsive conditions to the output.
    ///
    /// Generates CSS `@media` rules based on responsive triggers configured in the
    /// renderer, applying responsive overrides to container styles.
    ///
    /// # Errors
    ///
    /// * If the `HtmlTagRenderer` fails to write the css media-queries
    #[allow(clippy::too_many_lines)]
    fn reactive_conditions_to_css(
        &self,
        f: &mut dyn Write,
        container: &Container,
    ) -> Result<(), std::io::Error> {
        f.write_all(b"<style>")?;

        for (container, config) in container.iter_overrides(true) {
            let Some(id) = &container.str_id else {
                continue;
            };

            let Some(trigger) = (match &config.condition {
                OverrideCondition::ResponsiveTarget { name } => self.responsive_triggers.get(name),
            }) else {
                continue;
            };

            f.write_all(b"@media(")?;

            match trigger {
                ResponsiveTrigger::MaxWidth(number) => {
                    f.write_all(b"max-width:")?;
                    f.write_all(number_to_html_string(number, true).as_bytes())?;
                }
                ResponsiveTrigger::MaxHeight(number) => {
                    f.write_all(b"max-height:")?;
                    f.write_all(number_to_html_string(number, true).as_bytes())?;
                }
            }

            f.write_all(b"){")?;

            f.write_all(b"#")?;
            f.write_all(id.as_bytes())?;
            f.write_all(b"{")?;

            for o in &config.overrides {
                match o {
                    OverrideItem::Direction(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            match x {
                                LayoutDirection::Row => b"row",
                                LayoutDirection::Column => b"column",
                            },
                        )?;
                    }
                    OverrideItem::Visibility(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            match x {
                                Visibility::Visible => b"visible",
                                Visibility::Hidden => b"hidden",
                            },
                        )?;
                    }
                    OverrideItem::UserSelect(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            match x {
                                UserSelect::Auto => b"auto",
                                UserSelect::None => b"none",
                                UserSelect::Text => b"text",
                                UserSelect::All => b"all",
                            },
                        )?;
                    }
                    OverrideItem::OverflowWrap(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            match x {
                                OverflowWrap::Normal => b"normal",
                                OverflowWrap::BreakWord => b"break-word",
                                OverflowWrap::Anywhere => b"anywhere",
                            },
                        )?;
                    }
                    OverrideItem::TextOverflow(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            match x {
                                TextOverflow::Clip => b"clip",
                                TextOverflow::Ellipsis => b"ellipsis",
                            },
                        )?;
                    }
                    OverrideItem::Hidden(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            if *x { b"none" } else { b"initial" },
                        )?;
                    }
                    OverrideItem::AlignItems(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            match x {
                                AlignItems::Start => b"start",
                                AlignItems::Center => b"center",
                                AlignItems::End => b"end",
                            },
                        )?;
                    }
                    OverrideItem::TextAlign(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            match x {
                                TextAlign::Start => b"start",
                                TextAlign::Center => b"center",
                                TextAlign::End => b"end",
                                TextAlign::Justify => b"justify",
                            },
                        )?;
                    }
                    OverrideItem::WhiteSpace(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            match x {
                                WhiteSpace::Normal => b"normal",
                                WhiteSpace::Preserve => b"pre",
                                WhiteSpace::PreserveWrap => b"pre-wrap",
                            },
                        )?;
                    }
                    OverrideItem::MarginLeft(x)
                    | OverrideItem::MarginRight(x)
                    | OverrideItem::MarginTop(x)
                    | OverrideItem::MarginBottom(x)
                    | OverrideItem::Width(x)
                    | OverrideItem::MinWidth(x)
                    | OverrideItem::MaxWidth(x)
                    | OverrideItem::Height(x)
                    | OverrideItem::MinHeight(x)
                    | OverrideItem::MaxHeight(x)
                    | OverrideItem::Left(x)
                    | OverrideItem::Right(x)
                    | OverrideItem::Top(x)
                    | OverrideItem::Bottom(x)
                    | OverrideItem::ColumnGap(x)
                    | OverrideItem::RowGap(x)
                    | OverrideItem::BorderTopLeftRadius(x)
                    | OverrideItem::BorderTopRightRadius(x)
                    | OverrideItem::BorderBottomLeftRadius(x)
                    | OverrideItem::BorderBottomRightRadius(x)
                    | OverrideItem::PaddingLeft(x)
                    | OverrideItem::PaddingRight(x)
                    | OverrideItem::PaddingTop(x)
                    | OverrideItem::PaddingBottom(x)
                    | OverrideItem::Opacity(x)
                    | OverrideItem::TranslateX(x)
                    | OverrideItem::TranslateY(x)
                    | OverrideItem::FontSize(x)
                    | OverrideItem::GridCellSize(x) => {
                        write_css_attr_important(
                            f,
                            override_item_to_css_name(o),
                            number_to_html_string(x, true).as_bytes(),
                        )?;
                    }
                    OverrideItem::StrId(..)
                    | OverrideItem::Classes(..)
                    | OverrideItem::OverflowX(..)
                    | OverrideItem::OverflowY(..)
                    | OverrideItem::JustifyContent(..)
                    | OverrideItem::TextDecoration(..)
                    | OverrideItem::FontFamily(..)
                    | OverrideItem::FontWeight(..)
                    | OverrideItem::Flex(..)
                    | OverrideItem::Cursor(..)
                    | OverrideItem::Position(..)
                    | OverrideItem::Background(..)
                    | OverrideItem::BorderTop(..)
                    | OverrideItem::BorderRight(..)
                    | OverrideItem::BorderBottom(..)
                    | OverrideItem::BorderLeft(..)
                    | OverrideItem::Color(..) => {}
                }
            }

            f.write_all(b"}")?; // container id
            f.write_all(b"}")?; // media query
        }

        f.write_all(b"</style>")?;

        Ok(())
    }

    /// Returns partial HTML content without the document structure.
    ///
    /// Used for rendering fragments or partial updates that will be inserted
    /// into an existing page.
    fn partial_html(
        &self,
        _headers: &BTreeMap<String, String>,
        _container: &Container,
        content: String,
        _viewport: Option<&str>,
        _background: Option<Color>,
    ) -> String {
        content
    }

    /// Returns complete HTML document with doctype, head, and body elements.
    ///
    /// Generates a full HTML page including meta tags, CSS links, inline styles,
    /// and the rendered content wrapped in proper HTML structure.
    ///
    /// # Panics
    ///
    /// * If writing responsive CSS to an in-memory buffer fails (should never happen)
    /// * If the generated CSS contains invalid UTF-8 (should never happen as CSS is ASCII)
    fn root_html(
        &self,
        _headers: &BTreeMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
        title: Option<&str>,
        description: Option<&str>,
        css_urls: &[String],
        css_paths: &[String],
        inline_css: &[String],
    ) -> String {
        let background = background.map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b));
        let background = background.as_deref().unwrap_or("");

        let mut responsive_css = vec![];
        self.reactive_conditions_to_css(&mut responsive_css, container)
            .unwrap();
        let responsive_css = std::str::from_utf8(&responsive_css).unwrap();

        html! {
            (DOCTYPE)
            html style="height:100%" lang="en" {
                head {
                    meta charset="utf-8";
                    @if let Some(title) = title {
                        title { (title) }
                    }
                    @if let Some(description) = description {
                        meta name="description" content=(description);
                    }
                    @for url in css_urls {
                        link rel="stylesheet" href=(url);
                    }
                    @for path in css_paths {
                        link rel="stylesheet" href=(path);
                    }
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
                        table.remove-table-styles {{
                            border-collapse: collapse;
                        }}
                        table.remove-table-styles td {{
                            padding: 0;
                        }}
                    "))}
                    (PreEscaped(responsive_css))
                    @for css in inline_css {
                        style {(PreEscaped(css))}
                    }
                    @if let Some(content) = viewport {
                        meta name="viewport" content=(content);
                    }
                }
                body style="height:100%;overflow:auto;" {
                    (PreEscaped(content))
                }
            }
        }
        .into_string()
    }
}

const fn override_item_to_css_name(item: &OverrideItem) -> &'static [u8] {
    match item {
        OverrideItem::StrId(..) => b"id",
        OverrideItem::Classes(..) => b"classes",
        OverrideItem::Direction(..) => b"flex-direction",
        OverrideItem::OverflowX(..) => b"overflow-x",
        OverrideItem::OverflowY(..) => b"overflow-y",
        OverrideItem::GridCellSize(..) => b"grid-template-columns",
        OverrideItem::JustifyContent(..) => b"justify-content",
        OverrideItem::AlignItems(..) => b"align-items",
        OverrideItem::TextAlign(..) => b"text-align",
        OverrideItem::WhiteSpace(..) => b"white-space",
        OverrideItem::TextDecoration(..) => b"text-decoration",
        OverrideItem::FontFamily(..) => b"font-family",
        OverrideItem::FontWeight(..) => b"font-weight",
        OverrideItem::Width(..) => b"width",
        OverrideItem::MinWidth(..) => b"min-width",
        OverrideItem::MaxWidth(..) => b"max-width",
        OverrideItem::Height(..) => b"height",
        OverrideItem::MinHeight(..) => b"min-height",
        OverrideItem::MaxHeight(..) => b"max-height",
        OverrideItem::Flex(..) => b"flex",
        OverrideItem::ColumnGap(..) => b"column-gap",
        OverrideItem::RowGap(..) => b"row-gap",
        OverrideItem::Opacity(..) => b"opacity",
        OverrideItem::Left(..) => b"left",
        OverrideItem::Right(..) => b"right",
        OverrideItem::Top(..) => b"top",
        OverrideItem::Bottom(..) => b"bottom",
        OverrideItem::TranslateX(..) | OverrideItem::TranslateY(..) => b"transform",
        OverrideItem::Cursor(..) => b"cursor",
        OverrideItem::UserSelect(..) => b"user-select",
        OverrideItem::OverflowWrap(..) => b"overflow-wrap",
        OverrideItem::TextOverflow(..) => b"text-overflow",
        OverrideItem::Position(..) => b"position",
        OverrideItem::Background(..) => b"background",
        OverrideItem::BorderTop(..) => b"border-top",
        OverrideItem::BorderRight(..) => b"border-right",
        OverrideItem::BorderBottom(..) => b"border-bottom",
        OverrideItem::BorderLeft(..) => b"border-left",
        OverrideItem::BorderTopLeftRadius(..) => b"border-top-left-radius",
        OverrideItem::BorderTopRightRadius(..) => b"border-top-right-radius",
        OverrideItem::BorderBottomLeftRadius(..) => b"border-bottom-left-radius",
        OverrideItem::BorderBottomRightRadius(..) => b"border-bottom-right-radius",
        OverrideItem::MarginLeft(..) => b"margin-left",
        OverrideItem::MarginRight(..) => b"margin-right",
        OverrideItem::MarginTop(..) => b"margin-top",
        OverrideItem::MarginBottom(..) => b"margin-bottom",
        OverrideItem::PaddingLeft(..) => b"padding-left",
        OverrideItem::PaddingRight(..) => b"padding-right",
        OverrideItem::PaddingTop(..) => b"padding-top",
        OverrideItem::PaddingBottom(..) => b"padding-bottom",
        OverrideItem::FontSize(..) => b"font-size",
        OverrideItem::Color(..) => b"color",
        OverrideItem::Hidden(..) => b"display",
        OverrideItem::Visibility(..) => b"visibility",
    }
}

/// Trait for HTML application implementations that handle rendering and configuration.
///
/// Implementations provide access to tag rendering, responsive triggers, assets,
/// viewport settings, and page metadata like title and description.
pub trait HtmlApp {
    /// Returns a reference to the HTML tag renderer.
    fn tag_renderer(&self) -> &dyn HtmlTagRenderer;

    /// Adds a responsive trigger and returns the modified app.
    #[must_use]
    fn with_responsive_trigger(self, _name: String, _trigger: ResponsiveTrigger) -> Self;
    /// Adds a responsive trigger to the app.
    fn add_responsive_trigger(&mut self, _name: String, _trigger: ResponsiveTrigger);

    /// Adds static asset routes and returns the modified app.
    #[cfg(feature = "assets")]
    #[must_use]
    fn with_static_asset_routes(
        self,
        paths: impl Into<Vec<hyperchad_renderer::assets::StaticAssetRoute>>,
    ) -> Self;

    /// Returns an iterator over the static asset routes.
    #[cfg(feature = "assets")]
    fn static_asset_routes(
        &self,
    ) -> impl Iterator<Item = &hyperchad_renderer::assets::StaticAssetRoute>;

    /// Sets the viewport meta tag and returns the modified app.
    #[must_use]
    fn with_viewport(self, viewport: Option<String>) -> Self;
    /// Sets the viewport meta tag.
    fn set_viewport(&mut self, viewport: Option<String>);

    /// Sets the page title and returns the modified app.
    #[must_use]
    fn with_title(self, title: Option<String>) -> Self;
    /// Sets the page title.
    fn set_title(&mut self, title: Option<String>);

    /// Sets the page description and returns the modified app.
    #[must_use]
    fn with_description(self, description: Option<String>) -> Self;
    /// Sets the page description.
    fn set_description(&mut self, description: Option<String>);

    /// Sets the background color and returns the modified app.
    #[must_use]
    fn with_background(self, background: Option<Color>) -> Self;
    /// Sets the background color.
    fn set_background(&mut self, background: Option<Color>);

    /// Sets the renderer event receiver and returns the modified app.
    #[cfg(feature = "extend")]
    #[must_use]
    fn with_html_renderer_event_rx(self, rx: Receiver<hyperchad_renderer::RendererEvent>) -> Self;
    /// Sets the renderer event receiver.
    #[cfg(feature = "extend")]
    fn set_html_renderer_event_rx(&mut self, rx: Receiver<hyperchad_renderer::RendererEvent>);

    /// Adds a CSS URL and returns the modified app.
    #[must_use]
    fn with_css_url(self, url: impl Into<String>) -> Self;
    /// Adds a CSS URL to the app.
    fn add_css_url(&mut self, url: impl Into<String>);

    /// Adds multiple CSS URLs and returns the modified app.
    #[must_use]
    fn with_css_urls(mut self, urls: impl IntoIterator<Item = impl Into<String>>) -> Self
    where
        Self: Sized,
    {
        for url in urls {
            self.add_css_url(url);
        }
        self
    }

    /// Adds a CSS path and returns the modified app.
    #[must_use]
    fn with_css_path(self, path: impl Into<String>) -> Self;
    /// Adds a CSS path to the app.
    fn add_css_path(&mut self, path: impl Into<String>);

    /// Adds multiple CSS paths and returns the modified app.
    #[must_use]
    fn with_css_paths(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self
    where
        Self: Sized,
    {
        for path in paths {
            self.add_css_path(path);
        }
        self
    }

    /// Adds inline CSS and returns the modified app.
    #[must_use]
    fn with_inline_css(self, css: impl Into<String>) -> Self;
    /// Adds inline CSS to the app.
    fn add_inline_css(&mut self, css: impl Into<String>);

    /// Adds multiple inline CSS blocks and returns the modified app.
    #[must_use]
    fn with_inline_css_blocks(
        mut self,
        css_blocks: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self
    where
        Self: Sized,
    {
        for css in css_blocks {
            self.add_inline_css(css);
        }
        self
    }

    /// Returns a slice of CSS URLs.
    #[must_use]
    fn css_urls(&self) -> &[String];
    /// Returns a slice of CSS paths.
    #[must_use]
    fn css_paths(&self) -> &[String];
    /// Returns a slice of inline CSS blocks.
    #[must_use]
    fn inline_css_blocks(&self) -> &[String];
}

/// HTML renderer that wraps an HTML application and manages rendering state.
///
/// This renderer handles dimensions, navigation events, and optional extensions
/// for custom rendering behavior.
#[derive(Clone)]
pub struct HtmlRenderer<T: HtmlApp + ToRenderRunner + Send + Sync> {
    width: Option<f32>,
    height: Option<f32>,
    x: Option<i32>,
    y: Option<i32>,
    /// The HTML application being rendered.
    pub app: T,
    receiver: Receiver<String>,
    #[cfg(feature = "extend")]
    extend: Option<std::sync::Arc<Box<dyn extend::ExtendHtmlRenderer + Send + Sync>>>,
    #[cfg(feature = "extend")]
    publisher: Option<extend::HtmlRendererEventPub>,
}

impl<T: HtmlApp + ToRenderRunner + Send + Sync> HtmlRenderer<T> {
    /// Creates a new HTML renderer with the given application.
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
            #[cfg(feature = "extend")]
            extend: None,
            #[cfg(feature = "extend")]
            publisher: None,
        }
    }

    /// Sets the background color and returns the modified renderer.
    #[must_use]
    pub fn with_background(mut self, background: Option<Color>) -> Self {
        self.app = self.app.with_background(background);
        self
    }

    /// Sets the page title and returns the modified renderer.
    #[must_use]
    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.app = self.app.with_title(title);
        self
    }

    /// Sets the page description and returns the modified renderer.
    #[must_use]
    pub fn with_description(mut self, description: Option<String>) -> Self {
        self.app = self.app.with_description(description);
        self
    }

    /// Waits for navigation events from the renderer.
    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.receiver.recv_async().await.ok()
    }

    /// Adds a CSS URL and returns the modified renderer.
    #[must_use]
    pub fn with_css_url(mut self, url: impl Into<String>) -> Self {
        self.app = self.app.with_css_url(url);
        self
    }

    /// Adds multiple CSS URLs and returns the modified renderer.
    #[must_use]
    pub fn with_css_urls(mut self, urls: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.app = self.app.with_css_urls(urls);
        self
    }

    /// Adds a CSS path and returns the modified renderer.
    #[must_use]
    pub fn with_css_path(mut self, path: impl Into<String>) -> Self {
        self.app = self.app.with_css_path(path);
        self
    }

    /// Adds multiple CSS paths and returns the modified renderer.
    #[must_use]
    pub fn with_css_paths(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.app = self.app.with_css_paths(paths);
        self
    }

    /// Adds inline CSS and returns the modified renderer.
    #[must_use]
    pub fn with_inline_css(mut self, css: impl Into<String>) -> Self {
        self.app = self.app.with_inline_css(css);
        self
    }

    /// Adds multiple inline CSS blocks and returns the modified renderer.
    #[must_use]
    pub fn with_inline_css_blocks(
        mut self,
        css_blocks: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.app = self.app.with_inline_css_blocks(css_blocks);
        self
    }

    /// Adds static asset routes and returns the modified renderer.
    #[cfg(feature = "assets")]
    #[must_use]
    pub fn with_static_asset_routes(
        mut self,
        paths: impl Into<Vec<hyperchad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self.app = self.app.with_static_asset_routes(paths);
        self
    }

    /// Returns an iterator over the static asset routes.
    #[cfg(feature = "assets")]
    pub fn static_asset_routes(
        &self,
    ) -> impl Iterator<Item = &hyperchad_renderer::assets::StaticAssetRoute> {
        self.app.static_asset_routes()
    }

    /// Sets a custom HTML renderer extension and returns the modified renderer.
    #[cfg(feature = "extend")]
    #[must_use]
    pub fn with_extend_html_renderer(
        mut self,
        renderer: impl extend::ExtendHtmlRenderer + Send + Sync + 'static,
    ) -> Self {
        self.extend = Some(std::sync::Arc::new(Box::new(renderer)));
        self
    }

    /// Sets the renderer event publisher and returns the modified renderer.
    #[cfg(feature = "extend")]
    #[must_use]
    pub fn with_html_renderer_event_pub(mut self, publisher: extend::HtmlRendererEventPub) -> Self {
        self.publisher = Some(publisher);
        self
    }
}

impl<T: HtmlApp + ToRenderRunner + Send + Sync> ToRenderRunner for HtmlRenderer<T> {
    /// Converts the renderer into a render runner that can execute the event loop.
    ///
    /// # Errors
    ///
    /// * If the HTML app fails to initialize the render runner
    fn to_runner(
        self,
        handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        self.app.to_runner(handle)
    }
}

#[async_trait]
impl<T: HtmlApp + ToRenderRunner + Send + Sync> Renderer for HtmlRenderer<T> {
    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.app.add_responsive_trigger(name, trigger);
    }

    /// Initializes the renderer with dimensions and page metadata.
    ///
    /// Sets up the renderer's initial state including viewport dimensions,
    /// positioning, background color, and page metadata like title and description.
    ///
    /// # Errors
    ///
    /// * If the HTML app fails to initialize
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
        self.width = Some(width);
        self.height = Some(height);
        self.x = x;
        self.y = y;
        self.app.set_background(background);
        self.app.set_title(title.map(ToString::to_string));
        self.app
            .set_description(description.map(ToString::to_string));
        self.app.set_viewport(viewport.map(ToString::to_string));

        Ok(())
    }

    /// Emits a custom event to the renderer extension.
    ///
    /// Publishes events that can be handled by custom renderer extensions
    /// for server-sent events, WebSocket updates, or other custom behavior.
    ///
    /// # Errors
    ///
    /// * If the renderer extension fails to handle the event
    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::trace!("emit_event: event_name={event_name} event_value={event_value:?}");

        #[cfg(feature = "extend")]
        if let (Some(extend), Some(publisher)) = (self.extend.as_ref(), self.publisher.as_ref()) {
            extend
                .emit_event(publisher.clone(), event_name, event_value)
                .await?;
        }

        Ok(())
    }

    /// Renders a view containing the primary container and optional fragments.
    ///
    /// Processes the view through any configured renderer extensions to generate
    /// HTML output or trigger updates via server-sent events or `WebSockets`.
    ///
    /// # Errors
    ///
    /// * If the renderer extension fails to process the view
    async fn render(
        &self,
        elements: View,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        moosicbox_logging::debug_or_trace!(
            ("render: start"),
            ("render: start {:?}", elements.primary)
        );

        #[cfg(feature = "extend")]
        if let (Some(extend), Some(publisher)) = (self.extend.as_ref(), self.publisher.as_ref()) {
            extend.render(publisher.clone(), elements).await?;
        }

        log::debug!("render: finished");

        Ok(())
    }

    /// Renders canvas drawing updates to the HTML renderer.
    ///
    /// Processes canvas drawing operations through any configured renderer extensions
    /// to update canvas elements in the rendered output.
    ///
    /// # Errors
    ///
    /// * If the renderer extension fails to process the canvas update
    ///
    /// # Panics
    ///
    /// * If the elements `Mutex` is poisoned
    #[allow(unused_variables)]
    async fn render_canvas(
        &self,
        update: CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::trace!("render_canvas");

        #[cfg(feature = "extend")]
        if let (Some(extend), Some(publisher)) = (self.extend.as_ref(), self.publisher.as_ref()) {
            extend.render_canvas(publisher.clone(), update).await?;
        }

        log::debug!("render_canvas: finished");

        Ok(())
    }
}
