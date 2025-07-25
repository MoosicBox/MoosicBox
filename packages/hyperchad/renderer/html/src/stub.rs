use hyperchad_renderer::{Color, Handle, HtmlTagRenderer, RenderRunner, ToRenderRunner};
use hyperchad_transformer::ResponsiveTrigger;

use crate::HtmlApp;

#[derive(Clone)]
pub struct StubApp<T: HtmlTagRenderer> {
    pub tag_renderer: T,
    #[cfg(feature = "assets")]
    pub static_asset_routes: Vec<hyperchad_renderer::assets::StaticAssetRoute>,
}

impl<T: HtmlTagRenderer> StubApp<T> {
    pub const fn new(tag_renderer: T) -> Self {
        Self {
            tag_renderer,
            #[cfg(feature = "assets")]
            static_asset_routes: vec![],
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

    #[cfg(feature = "sse")]
    fn with_html_renderer_event_rx(
        self,
        _rx: flume::Receiver<hyperchad_renderer::RendererEvent>,
    ) -> Self {
        self
    }

    #[cfg(feature = "sse")]
    fn set_html_renderer_event_rx(
        &mut self,
        _rx: flume::Receiver<hyperchad_renderer::RendererEvent>,
    ) {
    }
}

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
