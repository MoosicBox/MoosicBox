use std::sync::Arc;

use gigachad_renderer::{Color, Handle, HtmlTagRenderer, RenderRunner, ToRenderRunner};

use crate::{DefaultHtmlTagRenderer, HtmlApp, HtmlRenderer};

#[derive(Clone)]
pub struct StubApp {
    pub tag_renderer: Arc<Box<dyn HtmlTagRenderer + Send + Sync>>,
}

impl Default for StubApp {
    fn default() -> Self {
        Self {
            tag_renderer: Arc::new(Box::new(DefaultHtmlTagRenderer)),
        }
    }
}

impl HtmlApp for StubApp {
    #[cfg(feature = "assets")]
    fn with_static_asset_routes(
        self,
        _paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self
    }

    fn with_tag_renderer(
        mut self,
        tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static,
    ) -> Self {
        self.tag_renderer = Arc::new(Box::new(tag_renderer));
        self
    }

    fn with_background(self, _background: Option<Color>) -> Self {
        self
    }

    fn set_background(&mut self, _background: Option<Color>) {}
}

#[derive(Clone)]
pub struct StubRunner;

impl RenderRunner for StubRunner {
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        log::debug!("StubRunner::run");
        Ok(())
    }
}

impl ToRenderRunner for StubApp {
    fn to_runner(
        &self,
        _handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(StubRunner))
    }
}

#[must_use]
pub fn stub() -> HtmlRenderer<StubApp> {
    HtmlRenderer::new(StubApp::default())
}
