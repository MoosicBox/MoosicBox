use gigachad_renderer::{Color, Handle, HtmlTagRenderer, RenderRunner, ToRenderRunner};

use crate::{HtmlApp, HtmlRenderer};

#[derive(Clone)]
pub struct StubApp;

impl HtmlApp for StubApp {
    #[cfg(feature = "assets")]
    fn with_static_asset_routes(
        self,
        _paths: impl Into<Vec<gigachad_renderer::assets::StaticAssetRoute>>,
    ) -> Self {
        self
    }

    fn with_tag_renderer(
        self,
        _tag_renderer: impl HtmlTagRenderer + Send + Sync + 'static,
    ) -> Self {
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
    HtmlRenderer::new(StubApp)
}
