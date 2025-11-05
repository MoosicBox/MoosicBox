//! Stub implementations for testing and minimal rendering scenarios.
//!
//! This module provides stub implementations of HTML application and render runner
//! that perform no actual rendering. These are useful for testing, development,
//! or scenarios where rendering is not required.

use hyperchad_renderer::{Color, Handle, HtmlTagRenderer, RenderRunner, ToRenderRunner};
use hyperchad_transformer::ResponsiveTrigger;

use crate::HtmlApp;

/// Stub HTML application for testing or minimal rendering scenarios.
///
/// This implementation provides basic HTML app functionality without
/// actual rendering or server integration.
#[derive(Clone)]
pub struct StubApp<T: HtmlTagRenderer> {
    /// The HTML tag renderer.
    pub tag_renderer: T,
    #[cfg(feature = "assets")]
    /// Static asset routes.
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
    /// CSS URLs from CDN.
    pub css_urls: Vec<String>,
    /// CSS paths for static assets.
    pub css_paths: Vec<String>,
    /// Inline CSS content.
    pub inline_css: Vec<String>,
}

impl<T: HtmlTagRenderer> StubApp<T> {
    /// Creates a new stub app with the given tag renderer.
    #[must_use]
    pub const fn new(tag_renderer: T) -> Self {
        Self {
            tag_renderer,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
            css_urls: vec![],
            css_paths: vec![],
            inline_css: vec![],
        }
    }
}

impl<T: HtmlTagRenderer> HtmlApp for StubApp<T> {
    fn tag_renderer(&self) -> &dyn HtmlTagRenderer {
        &self.tag_renderer
    }

    fn with_responsive_trigger(mut self, name: String, trigger: ResponsiveTrigger) -> Self {
        self.tag_renderer.add_responsive_trigger(name, trigger);
        self
    }

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.tag_renderer.add_responsive_trigger(name, trigger);
    }

    #[cfg(feature = "assets")]
    fn with_static_asset_routes(
        mut self,
        paths: impl Into<Vec<hyperchad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self.static_asset_routes = paths.into();
        self
    }

    #[cfg(feature = "assets")]
    fn static_asset_routes(
        &self,
    ) -> impl Iterator<Item = &hyperchad_renderer::assets::StaticAssetRoute> {
        self.static_asset_routes.iter()
    }

    fn with_viewport(self, _viewport: Option<String>) -> Self {
        self
    }

    fn set_viewport(&mut self, _viewport: Option<String>) {}

    fn with_title(self, _title: Option<String>) -> Self {
        self
    }

    fn set_title(&mut self, _title: Option<String>) {}

    fn with_description(self, _description: Option<String>) -> Self {
        self
    }

    fn set_description(&mut self, _description: Option<String>) {}

    fn with_background(self, _background: Option<Color>) -> Self {
        self
    }

    fn set_background(&mut self, _background: Option<Color>) {}

    #[cfg(feature = "extend")]
    fn with_html_renderer_event_rx(
        self,
        _rx: flume::Receiver<hyperchad_renderer::RendererEvent>,
    ) -> Self {
        self
    }

    #[cfg(feature = "extend")]
    fn set_html_renderer_event_rx(
        &mut self,
        _rx: flume::Receiver<hyperchad_renderer::RendererEvent>,
    ) {
    }

    fn with_css_url(mut self, url: impl Into<String>) -> Self {
        self.css_urls.push(url.into());
        self
    }

    fn add_css_url(&mut self, url: impl Into<String>) {
        self.css_urls.push(url.into());
    }

    fn with_css_path(mut self, path: impl Into<String>) -> Self {
        self.css_paths.push(path.into());
        self
    }

    fn add_css_path(&mut self, path: impl Into<String>) {
        self.css_paths.push(path.into());
    }

    fn with_inline_css(mut self, css: impl Into<String>) -> Self {
        self.inline_css.push(css.into());
        self
    }

    fn add_inline_css(&mut self, css: impl Into<String>) {
        self.inline_css.push(css.into());
    }

    fn css_urls(&self) -> &[String] {
        &self.css_urls
    }

    fn css_paths(&self) -> &[String] {
        &self.css_paths
    }

    fn inline_css_blocks(&self) -> &[String] {
        &self.inline_css
    }
}

/// Stub render runner that performs no actual rendering.
///
/// Useful for testing or scenarios where render execution is not required.
#[derive(Clone)]
pub struct StubRunner;

impl RenderRunner for StubRunner {
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::debug!("StubRunner::run");
        Ok(())
    }
}

impl<T: HtmlTagRenderer> ToRenderRunner for StubApp<T> {
    fn to_runner(
        self,
        _handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(StubRunner))
    }
}
