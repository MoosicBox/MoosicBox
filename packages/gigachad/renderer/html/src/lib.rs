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
use gigachad_transformer::{
    models::{AlignItems, LayoutDirection, Visibility},
    OverrideCondition, OverrideItem, ResponsiveTrigger,
};
use html::{
    element_classes_to_html, element_style_to_html, number_to_html_string, write_css_attr_important,
};
use maud::{html, PreEscaped};
use tokio::runtime::Handle;

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

#[derive(Default, Clone)]
pub struct DefaultHtmlTagRenderer {
    pub responsive_triggers: HashMap<String, ResponsiveTrigger>,
}

impl DefaultHtmlTagRenderer {
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
                            b"flex-direction",
                            match x {
                                LayoutDirection::Row => b"row",
                                LayoutDirection::Column => b"column",
                            },
                        )?;
                    }
                    OverrideItem::Visibility(x) => {
                        write_css_attr_important(
                            f,
                            b"visibility",
                            match x {
                                Visibility::Visible => b"visible",
                                Visibility::Hidden => b"hidden",
                            },
                        )?;
                    }
                    OverrideItem::Hidden(x) => {
                        write_css_attr_important(
                            f,
                            b"display",
                            if *x { b"none" } else { b"initial" },
                        )?;
                    }
                    OverrideItem::AlignItems(x) => {
                        write_css_attr_important(
                            f,
                            b"align-items",
                            match x {
                                AlignItems::Start => b"start",
                                AlignItems::Center => b"center",
                                AlignItems::End => b"end",
                            },
                        )?;
                    }
                    OverrideItem::MarginLeft(x) => {
                        write_css_attr_important(
                            f,
                            b"margin-left",
                            number_to_html_string(x, true).as_bytes(),
                        )?;
                    }
                    OverrideItem::MarginRight(x) => {
                        write_css_attr_important(
                            f,
                            b"margin-right",
                            number_to_html_string(x, true).as_bytes(),
                        )?;
                    }
                    OverrideItem::MarginTop(x) => {
                        write_css_attr_important(
                            f,
                            b"margin-top",
                            number_to_html_string(x, true).as_bytes(),
                        )?;
                    }
                    OverrideItem::MarginBottom(x) => {
                        write_css_attr_important(
                            f,
                            b"margin-bottom",
                            number_to_html_string(x, true).as_bytes(),
                        )?;
                    }
                    OverrideItem::StrId(..)
                    | OverrideItem::Classes(..)
                    | OverrideItem::Data(..)
                    | OverrideItem::OverflowX(..)
                    | OverrideItem::OverflowY(..)
                    | OverrideItem::JustifyContent(..)
                    | OverrideItem::TextAlign(..)
                    | OverrideItem::TextDecoration(..)
                    | OverrideItem::FontFamily(..)
                    | OverrideItem::Width(..)
                    | OverrideItem::MinWidth(..)
                    | OverrideItem::MaxWidth(..)
                    | OverrideItem::Height(..)
                    | OverrideItem::MinHeight(..)
                    | OverrideItem::MaxHeight(..)
                    | OverrideItem::Flex(..)
                    | OverrideItem::Gap(..)
                    | OverrideItem::Opacity(..)
                    | OverrideItem::Left(..)
                    | OverrideItem::Right(..)
                    | OverrideItem::Top(..)
                    | OverrideItem::Bottom(..)
                    | OverrideItem::TranslateX(..)
                    | OverrideItem::TranslateY(..)
                    | OverrideItem::Cursor(..)
                    | OverrideItem::Position(..)
                    | OverrideItem::Background(..)
                    | OverrideItem::BorderTop(..)
                    | OverrideItem::BorderRight(..)
                    | OverrideItem::BorderBottom(..)
                    | OverrideItem::BorderLeft(..)
                    | OverrideItem::BorderTopLeftRadius(..)
                    | OverrideItem::BorderTopRightRadius(..)
                    | OverrideItem::BorderBottomLeftRadius(..)
                    | OverrideItem::BorderBottomRightRadius(..)
                    | OverrideItem::PaddingLeft(..)
                    | OverrideItem::PaddingRight(..)
                    | OverrideItem::PaddingTop(..)
                    | OverrideItem::PaddingBottom(..)
                    | OverrideItem::FontSize(..)
                    | OverrideItem::Color(..)
                    | OverrideItem::Debug(..)
                    | OverrideItem::Route(..) => {}
                }
            }

            f.write_all(b"}")?; // container id
            f.write_all(b"}")?; // media query
        }

        f.write_all(b"</style>")?;

        Ok(())
    }

    fn partial_html(
        &self,
        _headers: &HashMap<String, String>,
        _container: &Container,
        content: String,
        _viewport: Option<&str>,
        _background: Option<Color>,
    ) -> String {
        content
    }

    fn root_html(
        &self,
        _headers: &HashMap<String, String>,
        container: &Container,
        content: String,
        viewport: Option<&str>,
        background: Option<Color>,
    ) -> String {
        let background = background.map(|x| format!("background:rgb({},{},{})", x.r, x.g, x.b));
        let background = background.as_deref().unwrap_or("");

        let mut responsive_css = vec![];
        self.reactive_conditions_to_css(&mut responsive_css, container)
            .unwrap();
        let responsive_css = std::str::from_utf8(&responsive_css).unwrap();

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
                    (PreEscaped(responsive_css))
                    @if let Some(content) = viewport {
                        meta name="viewport" content=(content);
                    }
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
    #[must_use]
    fn with_responsive_trigger(self, _name: String, _trigger: ResponsiveTrigger) -> Self;
    fn add_responsive_trigger(&mut self, _name: String, _trigger: ResponsiveTrigger);

    #[cfg(feature = "assets")]
    #[must_use]
    fn with_static_asset_routes(
        self,
        paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
    ) -> Self;

    #[must_use]
    fn with_viewport(self, viewport: Option<String>) -> Self;
    fn set_viewport(&mut self, viewport: Option<String>);

    #[must_use]
    fn with_background(self, background: Option<Color>) -> Self;
    fn set_background(&mut self, background: Option<Color>);
}

#[derive(Clone)]
pub struct HtmlRenderer<T: HtmlApp + ToRenderRunner + Send + Sync> {
    width: Option<f32>,
    height: Option<f32>,
    x: Option<i32>,
    y: Option<i32>,
    pub app: T,
    receiver: Receiver<String>,
}

impl<T: HtmlApp + ToRenderRunner + Send + Sync> HtmlRenderer<T> {
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

impl<T: HtmlApp + ToRenderRunner + Send + Sync> ToRenderRunner for HtmlRenderer<T> {
    /// # Errors
    ///
    /// Will error if html fails to run the event loop.
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
        viewport: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.width = Some(width);
        self.height = Some(height);
        self.x = x;
        self.y = y;
        self.app.set_background(background);
        self.app.set_viewport(viewport.map(ToString::to_string));

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
