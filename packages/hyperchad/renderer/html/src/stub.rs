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

    #[cfg(feature = "assets")]
    fn with_asset_not_found_behavior(
        self,
        _behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) -> Self {
        // Stub doesn't use this behavior
        self
    }

    #[cfg(feature = "assets")]
    fn set_asset_not_found_behavior(
        &mut self,
        _behavior: hyperchad_renderer::assets::AssetNotFoundBehavior,
    ) {
        // Stub doesn't use this behavior
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DefaultHtmlTagRenderer;

    #[test_log::test]
    fn test_stub_app_new() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let app = StubApp::new(tag_renderer);
        assert!(app.css_urls.is_empty());
        assert!(app.css_paths.is_empty());
        assert!(app.inline_css.is_empty());
    }

    #[test_log::test]
    fn test_stub_app_with_css_url() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let app = StubApp::new(tag_renderer).with_css_url("https://example.com/style.css");
        assert_eq!(app.css_urls().len(), 1);
        assert_eq!(app.css_urls()[0], "https://example.com/style.css");
    }

    #[test_log::test]
    fn test_stub_app_add_css_url() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let mut app = StubApp::new(tag_renderer);
        app.add_css_url("https://example.com/style.css");
        app.add_css_url("https://example.com/theme.css");
        assert_eq!(app.css_urls().len(), 2);
    }

    #[test_log::test]
    fn test_stub_app_with_css_path() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let app = StubApp::new(tag_renderer).with_css_path("/static/style.css");
        assert_eq!(app.css_paths().len(), 1);
        assert_eq!(app.css_paths()[0], "/static/style.css");
    }

    #[test_log::test]
    fn test_stub_app_add_css_path() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let mut app = StubApp::new(tag_renderer);
        app.add_css_path("/static/main.css");
        app.add_css_path("/static/reset.css");
        assert_eq!(app.css_paths().len(), 2);
    }

    #[test_log::test]
    fn test_stub_app_with_inline_css() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let css = "body { margin: 0; }";
        let app = StubApp::new(tag_renderer).with_inline_css(css);
        assert_eq!(app.inline_css_blocks().len(), 1);
        assert_eq!(app.inline_css_blocks()[0], css);
    }

    #[test_log::test]
    fn test_stub_app_add_inline_css() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let mut app = StubApp::new(tag_renderer);
        app.add_inline_css("body { margin: 0; }");
        app.add_inline_css("* { box-sizing: border-box; }");
        assert_eq!(app.inline_css_blocks().len(), 2);
    }

    #[test_log::test]
    fn test_stub_app_with_responsive_trigger() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let app = StubApp::new(tag_renderer).with_responsive_trigger(
            "mobile".to_string(),
            ResponsiveTrigger::MaxWidth(hyperchad_transformer::Number::Integer(768)),
        );
        let _renderer = app.tag_renderer();
        // Test that the responsive trigger was added (implicitly through tag_renderer)
    }

    #[test_log::test]
    fn test_stub_app_add_responsive_trigger() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let mut app = StubApp::new(tag_renderer);
        app.add_responsive_trigger(
            "tablet".to_string(),
            ResponsiveTrigger::MaxWidth(hyperchad_transformer::Number::Integer(1024)),
        );
        let _renderer = app.tag_renderer();
    }

    #[test_log::test]
    fn test_stub_runner_run() {
        let mut runner = StubRunner;
        let result = runner.run();
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_stub_app_to_runner() {
        let tag_renderer = DefaultHtmlTagRenderer::default();
        let app = StubApp::new(tag_renderer);
        let handle = Handle::current();
        let result = app.to_runner(handle);
        assert!(result.is_ok());
    }

    #[cfg(feature = "assets")]
    #[test_log::test]
    fn test_stub_app_with_static_asset_routes() {
        use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};
        use std::path::PathBuf;

        let tag_renderer = DefaultHtmlTagRenderer::default();
        let routes = vec![StaticAssetRoute {
            route: "/static".to_string(),
            target: AssetPathTarget::Directory(PathBuf::from("./assets")),
            not_found_behavior: None,
        }];
        let app = StubApp::new(tag_renderer).with_static_asset_routes(routes);
        let mut routes_iter = app.static_asset_routes();
        assert!(routes_iter.next().is_some());
    }
}
