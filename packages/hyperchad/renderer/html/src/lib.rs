#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    collections::HashMap,
    io::Write,
    sync::{RwLockReadGuard, RwLockWriteGuard},
};

use async_trait::async_trait;
use flume::Receiver;
use html::{
    element_classes_to_html, element_style_to_html, number_to_html_string, write_css_attr_important,
};
use hyperchad_renderer::{
    Color, HtmlTagRenderer, PartialView, RenderRunner, Renderer, ToRenderRunner, View,
    canvas::CanvasUpdate,
};
use hyperchad_router::Container;
use hyperchad_transformer::{
    OverrideCondition, OverrideItem, ResponsiveTrigger,
    models::{AlignItems, LayoutDirection, TextAlign, Visibility},
};
use maud::{DOCTYPE, PreEscaped, html};
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

#[cfg(feature = "extend")]
pub mod extend;

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

        for (key, value) in &container.data {
            f.write_all(b" data-")?;
            f.write_all(key.as_bytes())?;
            f.write_all(b"=\"")?;
            f.write_all(html_escape::encode_quoted_attribute(value).as_bytes())?;
            f.write_all(b"\"")?;
        }

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
                    | OverrideItem::FontSize(x) => {
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
        title: Option<&str>,
        description: Option<&str>,
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
                    @if let Some(title) = title {
                        title { (title) }
                    }
                    @if let Some(description) = description {
                        meta name="description" content=(description);
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
                    "))}
                    (PreEscaped(responsive_css))
                    @if let Some(content) = viewport {
                        meta name="viewport" content=(content);
                    }
                }
                body style="height:100%" {
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
        OverrideItem::JustifyContent(..) => b"justify-content",
        OverrideItem::AlignItems(..) => b"align-items",
        OverrideItem::TextAlign(..) => b"text-align",
        OverrideItem::TextDecoration(..) => b"text-decoration",
        OverrideItem::FontFamily(..) => b"font-family",
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

pub trait HtmlApp {
    #[must_use]
    fn with_responsive_trigger(self, _name: String, _trigger: ResponsiveTrigger) -> Self;
    fn add_responsive_trigger(&mut self, _name: String, _trigger: ResponsiveTrigger);

    #[cfg(feature = "assets")]
    #[must_use]
    fn with_static_asset_routes(
        self,
        paths: impl Into<Vec<hyperchad_renderer::assets::StaticAssetRoute>>,
    ) -> Self;

    #[must_use]
    fn with_viewport(self, viewport: Option<String>) -> Self;
    fn set_viewport(&mut self, viewport: Option<String>);

    #[must_use]
    fn with_title(self, title: Option<String>) -> Self;
    fn set_title(&mut self, title: Option<String>);

    #[must_use]
    fn with_description(self, description: Option<String>) -> Self;
    fn set_description(&mut self, description: Option<String>);

    #[must_use]
    fn with_background(self, background: Option<Color>) -> Self;
    fn set_background(&mut self, background: Option<Color>);

    #[cfg(feature = "extend")]
    #[must_use]
    fn with_html_renderer_event_rx(self, rx: Receiver<hyperchad_renderer::RendererEvent>) -> Self;
    #[cfg(feature = "extend")]
    fn set_html_renderer_event_rx(&mut self, rx: Receiver<hyperchad_renderer::RendererEvent>);
}

#[derive(Clone)]
pub struct HtmlRenderer<T: HtmlApp + ToRenderRunner + Send + Sync> {
    width: Option<f32>,
    height: Option<f32>,
    x: Option<i32>,
    y: Option<i32>,
    pub app: T,
    receiver: Receiver<String>,
    #[cfg(feature = "extend")]
    extend: Option<std::sync::Arc<Box<dyn extend::ExtendHtmlRenderer + Send + Sync>>>,
    #[cfg(feature = "extend")]
    publisher: Option<extend::HtmlRendererEventPub>,
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
            #[cfg(feature = "extend")]
            extend: None,
            #[cfg(feature = "extend")]
            publisher: None,
        }
    }

    #[must_use]
    pub fn with_background(mut self, background: Option<Color>) -> Self {
        self.app = self.app.with_background(background);
        self
    }

    #[must_use]
    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.app = self.app.with_title(title);
        self
    }

    #[must_use]
    pub fn with_description(mut self, description: Option<String>) -> Self {
        self.app = self.app.with_description(description);
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
        paths: impl Into<Vec<hyperchad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self.app = self.app.with_static_asset_routes(paths);
        self
    }

    #[cfg(feature = "extend")]
    #[must_use]
    pub fn with_extend_html_renderer(
        mut self,
        renderer: impl extend::ExtendHtmlRenderer + Send + Sync + 'static,
    ) -> Self {
        self.extend = Some(std::sync::Arc::new(Box::new(renderer)));
        self
    }

    #[cfg(feature = "extend")]
    #[must_use]
    pub fn with_html_renderer_event_pub(mut self, publisher: extend::HtmlRendererEventPub) -> Self {
        self.publisher = Some(publisher);
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

    /// # Errors
    ///
    /// Will error if html app fails to emit the event.
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

        #[cfg(feature = "extend")]
        if let (Some(extend), Some(publisher)) = (self.extend.as_ref(), self.publisher.as_ref()) {
            extend.render(publisher.clone(), elements).await?;
        }

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

        #[cfg(feature = "extend")]
        if let (Some(extend), Some(publisher)) = (self.extend.as_ref(), self.publisher.as_ref()) {
            extend.render_partial(publisher.clone(), view).await?;
        }

        log::debug!("render_partial: finished");

        Ok(())
    }

    /// # Errors
    ///
    /// Will error if html fails to render the canvas update.
    ///
    /// # Panics
    ///
    /// Will panic if elements `Mutex` is poisoned.
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

    fn container(&self) -> RwLockReadGuard<Container> {
        unimplemented!();
    }

    fn container_mut(&self) -> RwLockWriteGuard<Container> {
        unimplemented!();
    }
}
