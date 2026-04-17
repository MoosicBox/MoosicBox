use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use hyperchad_renderer::{
    Color, Handle, RenderRunner, Renderer, ToRenderRunner, View, transformer::ResponsiveTrigger,
};

use crate::{dom::DomState, transcript::Transcript};

/// Shared renderer state used by the harness and renderer trait impl.
#[derive(Debug, Clone)]
pub struct RendererSnapshot {
    /// Captured stream transcript.
    pub transcript: Transcript,
    /// Current virtual DOM state.
    pub dom: DomState,
    /// Last known window width.
    pub width: f32,
    /// Last known window height.
    pub height: f32,
    /// Optional x position.
    pub x: Option<i32>,
    /// Optional y position.
    pub y: Option<i32>,
    /// Optional background color.
    pub background: Option<Color>,
    /// Optional title.
    pub title: Option<String>,
    /// Optional description.
    pub description: Option<String>,
    /// Optional viewport metadata.
    pub viewport: Option<String>,
    /// Responsive triggers added at runtime.
    pub responsive_triggers: BTreeMap<String, ResponsiveTrigger>,
}

impl Default for RendererSnapshot {
    fn default() -> Self {
        Self {
            transcript: Transcript::default(),
            dom: DomState::default(),
            width: 0.0,
            height: 0.0,
            x: None,
            y: None,
            background: None,
            title: None,
            description: None,
            viewport: None,
            responsive_triggers: BTreeMap::new(),
        }
    }
}

/// In-process renderer implementation that captures all output.
#[derive(Debug, Clone, Default)]
pub struct TestingRenderer {
    pub(crate) snapshot: Arc<RwLock<RendererSnapshot>>,
}

impl TestingRenderer {
    /// Creates a new empty testing renderer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a clone of the captured state snapshot.
    #[must_use]
    pub fn snapshot(&self) -> RendererSnapshot {
        self.snapshot.read().unwrap().clone()
    }

    /// Applies a view update synchronously.
    pub fn apply_view(&self, view: View) {
        let mut snapshot = self.snapshot.write().unwrap();
        snapshot.transcript.push_view(view.clone());
        snapshot.dom.apply_view(&view);
    }

    /// Records an event frame synchronously.
    pub fn record_event(&self, name: impl Into<String>, value: Option<String>) {
        self.snapshot
            .write()
            .unwrap()
            .transcript
            .push_event(name.into(), value);
    }

    /// Clears transcript and DOM state.
    pub fn reset(&self) {
        let mut snapshot = self.snapshot.write().unwrap();
        snapshot.transcript.clear();
        snapshot.dom = DomState::default();
    }
}

/// No-op run loop for testing renderer.
pub struct TestingRenderRunner;

impl RenderRunner for TestingRenderRunner {
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        Ok(())
    }
}

impl ToRenderRunner for TestingRenderer {
    fn to_runner(
        self,
        _handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        Ok(Box::new(TestingRenderRunner))
    }
}

#[async_trait]
impl Renderer for TestingRenderer {
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
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        let mut snapshot = self.snapshot.write().unwrap();
        snapshot.width = width;
        snapshot.height = height;
        snapshot.x = x;
        snapshot.y = y;
        snapshot.background = background;
        snapshot.title = title.map(ToString::to_string);
        snapshot.description = description.map(ToString::to_string);
        snapshot.viewport = viewport.map(ToString::to_string);
        Ok(())
    }

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        self.snapshot
            .write()
            .unwrap()
            .responsive_triggers
            .insert(name, trigger);
    }

    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.record_event(event_name, event_value);
        Ok(())
    }

    async fn render(&self, view: View) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.apply_view(view);
        Ok(())
    }

    #[cfg(feature = "canvas")]
    async fn render_canvas(
        &self,
        update: hyperchad_renderer::canvas::CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
        self.snapshot
            .write()
            .unwrap()
            .transcript
            .push_canvas_update(update);
        Ok(())
    }
}
