//! Cloneable, bounded `HyperChad` renderer admission and host policy.

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use hyperchad_actions::logic::Value;
use hyperchad_renderer::{
    Color, Handle, RenderRunner, Renderer, ToRenderRunner, View, canvas::CanvasUpdate,
};
use hyperchad_transformer::ResponsiveTrigger;

use crate::{graphics::GraphicsLimits, program::Message, runtime::BmuxRenderRunner};

/// Renderer error shared across terminal and embedding entry points.
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
/// An asynchronous operation explicitly delegated to the embedding application.
pub type HostFuture = Pin<Box<dyn Future<Output = Result<HostResponse, Error>> + Send>>;

/// Operations which must never be implicitly authorized by the renderer.
#[derive(Debug, Clone)]
pub enum HostRequest {
    /// Fetch a bounded resource. The host must authorize redirects as well as the initial URL.
    Resource { source: String, max_bytes: usize },
    /// Choose a file. The host owns consent and bounds; no automatic file access occurs.
    ChooseFile { node: String },
    /// Explicitly authorize and open an external link.
    ExternalLink { url: String },
    /// Execute an application-authorized HTTP request.
    Http(Box<crate::actions::RequestEffect>),
    /// Store response cookies in the host's application-local cookie jar.
    Cookies(Vec<hyperchad_renderer::ResponseCookie>),
}

/// A host operation result, still validated against renderer resource bounds.
pub enum HostResponse {
    Bytes(Vec<u8>),
    Files(Vec<crate::actions::AuthorizedFile>),
    Http {
        status: u16,
        body: Vec<u8>,
    },
    /// A renderer-neutral response from the authorized application transport.
    View(View),
    /// Authorized transport result retaining status, headers and optional renderer-neutral view.
    Route {
        status: u16,
        headers: std::collections::BTreeMap<String, String>,
        view: Option<View>,
        body: Vec<u8>,
    },
    Complete,
}

/// Explicit application policy. Absence denies all host operations.
#[derive(Clone, Default)]
pub struct HostCallbacks {
    pub request: Option<Arc<dyn Fn(HostRequest) -> HostFuture + Send + Sync>>,
}

/// Terminal protocol selection; Auto uses bmux environment capability detection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageProtocol {
    #[default]
    Auto,
    Kitty,
    Sixel,
    Iterm2,
    Disabled,
}

/// Resource, scheduling and startup policy shared by both execution modes.
#[derive(Clone)]
pub struct BmuxConfig {
    pub runtime: bmux_tui_runtime::RuntimeConfig,
    pub graphics: GraphicsLimits,
    pub host: HostCallbacks,
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_text_bytes: usize,
    pub max_event_bytes: usize,
    pub width: u16,
    pub height: u16,
    pub graphics_enabled: bool,
    pub image_protocol: ImageProtocol,
    /// Monotonic deadline for each host/application effect.
    pub effect_timeout: std::time::Duration,
}

impl Default for BmuxConfig {
    fn default() -> Self {
        Self {
            runtime: bmux_tui_runtime::RuntimeConfig::default(),
            graphics: GraphicsLimits::default(),
            host: HostCallbacks::default(),
            max_nodes: 20_000,
            max_depth: 128,
            max_text_bytes: 4 * 1024 * 1024,
            max_event_bytes: 64 * 1024,
            width: 80,
            height: 24,
            graphics_enabled: true,
            image_protocol: ImageProtocol::Auto,
            effect_timeout: std::time::Duration::from_secs(30),
        }
    }
}

pub(crate) struct Shared {
    pub config: BmuxConfig,
    pub stopping: std::sync::atomic::AtomicBool,
    pub receiver: Mutex<Option<flume::Receiver<Message>>>,
    pub navigation_tx: flume::Sender<String>,
    pub request_action: flume::Sender<(String, Option<Value>)>,
    pub on_resize: flume::Sender<(f32, f32)>,
}

/// Cloneable producer. Only the runner/program owns mutable document state.
#[derive(Clone)]
pub struct BmuxRenderer {
    pub(crate) shared: Arc<Shared>,
    sender: flume::Sender<Message>,
    navigation: flume::Receiver<String>,
}

impl BmuxRenderer {
    /// Construct a bounded renderer without entering raw mode or creating threads.
    #[must_use]
    pub fn new(
        mut config: BmuxConfig,
        request_action: flume::Sender<(String, Option<Value>)>,
        on_resize: flume::Sender<(f32, f32)>,
    ) -> Self {
        config.runtime = config.runtime.normalized();
        let (sender, receiver) = flume::bounded(config.runtime.reliable_capacity.max(1));
        let (navigation_tx, navigation) = flume::bounded(config.runtime.reliable_capacity.max(1));
        Self {
            shared: Arc::new(Shared {
                config,
                stopping: std::sync::atomic::AtomicBool::new(false),
                receiver: Mutex::new(Some(receiver)),
                navigation_tx,
                request_action,
                on_resize,
            }),
            sender,
            navigation,
        }
    }

    /// Wait for an internal navigation request for the application's router bridge.
    pub async fn wait_for_navigation(&self) -> Option<String> {
        self.navigation.recv_async().await.ok()
    }

    /// Request an orderly final presentation and shutdown.
    ///
    /// # Errors
    /// * Returns an error when runner admission has closed.
    pub async fn shutdown(&self) -> Result<(), Error> {
        if self
            .shared
            .stopping
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return Ok(());
        }
        self.sender
            .send_async(Message::Shutdown)
            .await
            .map_err(|_| -> Error {
                std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "bmux shutdown admission closed",
                )
                .into()
            })
    }

    /// Cancel an explicitly correlated HTTP request, preserving Abort/After lifecycle dispatch.
    ///
    /// # Errors
    /// * Returns an error when admission is closed.
    pub async fn abort_request(&self, request_id: String) -> Result<(), Error> {
        self.admit(Message::AbortRequest(request_id)).await
    }

    /// Retry one image through the same bounded and authorized resource path.
    ///
    /// # Errors
    /// * Returns an error when admission is closed.
    pub async fn retry_resource(&self, key: String) -> Result<(), Error> {
        self.admit(Message::RetryResource(key)).await
    }

    pub(crate) async fn admit(&self, message: Message) -> Result<(), Error> {
        if self
            .shared
            .stopping
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "bmux renderer stopping",
            )
            .into());
        }
        self.sender.send_async(message).await.map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "bmux renderer admission closed",
            )
            .into()
        })
    }

    pub(crate) fn take_receiver(&self) -> Result<flume::Receiver<Message>, Error> {
        self.shared
            .receiver
            .lock()
            .map_err(|_| std::io::Error::other("bmux runner ownership poisoned"))?
            .take()
            .ok_or_else(|| {
                std::io::Error::other("bmux renderer already has a runner or embedded owner").into()
            })
    }
}

impl ToRenderRunner for BmuxRenderer {
    fn to_runner(
        self,
        _handle: Handle,
    ) -> Result<Box<dyn RenderRunner>, Box<dyn std::error::Error + Send>> {
        let receiver = self
            .take_receiver()
            .map_err(|error| -> Box<dyn std::error::Error + Send> { error })?;
        Ok(Box::new(BmuxRenderRunner::new(self.shared, receiver)))
    }
}

#[async_trait]
impl Renderer for BmuxRenderer {
    async fn init(
        &mut self,
        _width: f32,
        _height: f32,
        _x: Option<i32>,
        _y: Option<i32>,
        background: Option<Color>,
        _title: Option<&str>,
        _description: Option<&str>,
        _viewport: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.admit(Message::Background(background))
            .await
            .map_err(|error| -> Box<dyn std::error::Error + Send> { error })
    }

    fn add_responsive_trigger(&mut self, name: String, trigger: ResponsiveTrigger) {
        // Configuration is synchronous in Renderer: reject visibly rather than silently drop it.
        if let Err(error) = self.sender.try_send(Message::Responsive(name, trigger)) {
            log::error!("unable to admit responsive trigger: {error}");
        }
    }

    async fn emit_event(
        &self,
        event_name: String,
        event_value: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        if event_name
            .len()
            .saturating_add(event_value.as_ref().map_or(0, String::len))
            > self.shared.config.max_event_bytes
        {
            return Err(Box::new(std::io::Error::other(
                "event exceeds bmux admission limit",
            )));
        }
        self.admit(Message::Emit(event_name, event_value))
            .await
            .map_err(|error| -> Box<dyn std::error::Error + Send> { error })
    }

    async fn render(&self, view: View) -> Result<(), Box<dyn std::error::Error + Send>> {
        crate::program::validate_view(&view, &self.shared.config)
            .map_err(|error| -> Box<dyn std::error::Error + Send> { error })?;
        self.admit(Message::View(view, None))
            .await
            .map_err(|error| -> Box<dyn std::error::Error + Send> { error })
    }

    async fn render_scoped(
        &self,
        scope: String,
        view: View,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        // A terminal has one document surface; other scopes must not become broadcasts.
        if scope != "default" {
            return Err(Box::new(std::io::Error::other(
                "unknown bmux surface scope",
            )));
        }
        self.render(view).await
    }

    async fn render_canvas(
        &self,
        update: CanvasUpdate,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        if update.canvas_actions.len() > self.shared.config.graphics.max_actions
            || update.target.len() > self.shared.config.graphics.max_key_bytes
        {
            return Err(Box::new(std::io::Error::other(
                "canvas update exceeds bmux admission limit",
            )));
        }
        self.admit(Message::Canvas(update))
            .await
            .map_err(|error| -> Box<dyn std::error::Error + Send> { error })
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;

    #[tokio::test]
    async fn shutdown_closes_facade_admission_and_is_idempotent() {
        let (actions, _) = flume::bounded(1);
        let (resize, _) = flume::bounded(1);
        let renderer = BmuxRenderer::new(BmuxConfig::default(), actions, resize);
        let receiver = renderer.take_receiver().unwrap();
        renderer.shutdown().await.unwrap();
        renderer.shutdown().await.unwrap();
        assert!(matches!(receiver.try_recv(), Ok(Message::Shutdown)));
        assert!(receiver.try_recv().is_err());
        assert!(renderer.emit_event("late".into(), None).await.is_err());
    }
}
