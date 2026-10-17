//! Production-backed, window-independent frame feedback for the v2 renderer.
//!
//! This is an opt-in inspection seam, not a second renderer. Headless callers
//! supply `RawInput` through `Context::run`; native callers can request a real
//! framebuffer screenshot via [`CaptureOptions`].

use std::{collections::BTreeMap, path::PathBuf};

use eframe::egui;
use hyperchad_actions::logic::Value;
use hyperchad_transformer::Container;

use crate::{
    backend::BackendFrame,
    canvas::CanvasAdapter,
    layout::EguiCalc,
    resources::{ImageResources, ResourceLimits, ResourceWorker},
    widgets::{WidgetPainter, forms::FormsAdapter},
};

type ProductionPainter = WidgetPainter<FormsAdapter, ImageResources, CanvasAdapter>;

/// Explicit native capture contract. No capture occurs unless configured.
#[derive(Clone, Debug)]
pub struct CaptureOptions {
    /// PNG output path. Parent directories are created on capture.
    pub output: PathBuf,
    /// Optional readiness marker written only after required geometry stabilizes.
    pub ready: Option<PathBuf>,
    /// Element string IDs that must have finite, positive, visible geometry.
    pub required_ids: Vec<String>,
    /// Maximum rendered frames before capture completion must be observed.
    pub max_frames: u64,
    /// Close the native viewport after capture success or failure.
    pub close: bool,
}

/// Owns the same frame construction used by the native v2 application.
pub struct FeedbackApp<C: EguiCalc + Clone + Send + Sync> {
    handle: Option<crate::runtime::DocumentHandle>,
    app: BackendFrame<C>,
    painter: ProductionPainter,
    completions: std::sync::Arc<std::sync::Mutex<Vec<crate::resources::LoadCompletion>>>,
    worker: ResourceWorker,
    action_outputs: Vec<(String, Option<Value>)>,
    navigation_outputs: Vec<String>,
    file_service: Option<crate::widgets::forms::NativeFileService>,
    resources_ready: bool,
    initialized: bool,
    previous_geometry: BTreeMap<String, egui::Rect>,
    stable: bool,
    capture: Option<NativeCapture>,
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> FeedbackApp<C> {
    /// Queue a real production view and retain observable output channels.
    #[must_use]
    pub fn new(view: Container, calculator: C) -> Self {
        let (handle, app) = BackendFrame::new(calculator);
        handle
            .submit(crate::runtime::DocumentCommand::Apply {
                view: hyperchad_renderer::View {
                    primary: Some(view),
                    ..Default::default()
                },
                origin: crate::document::ViewOrigin::Navigation,
            })
            .expect("new feedback mailbox has capacity");
        let mut app = Self::from_frame(app);
        app.handle = Some(handle);
        app
    }

    /// Install the same UI-owned frame used by the native renderer facade.
    #[must_use]
    pub fn from_frame(app: BackendFrame<C>) -> Self {
        let (resources, worker) = ImageResources::new(ResourceLimits::default());
        let painter = WidgetPainter::new(FormsAdapter::new(), resources, CanvasAdapter::new());
        Self {
            handle: None,
            app,
            completions: std::sync::Arc::default(),
            painter,
            worker,
            action_outputs: Vec::new(),
            navigation_outputs: Vec::new(),
            file_service: None,
            resources_ready: false,
            initialized: false,
            previous_geometry: BTreeMap::new(),
            stable: false,
            capture: None,
        }
    }

    /// Observe surfaced production failures; tests must not treat missing paint as success.
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        self.app.take_diagnostics()
    }

    /// Admit a test/native update on the same ordered document/canvas mailbox.
    pub fn submit(&self, command: crate::runtime::DocumentCommand) -> Result<(), String> {
        self.handle
            .as_ref()
            .ok_or_else(|| "host owns document producer".to_owned())?
            .submit(command)
            .map_err(|error| error.to_string())
    }

    /// Enable native screenshot feedback. Returns `self` for native app setup.
    #[must_use]
    pub fn with_capture(mut self, options: CaptureOptions) -> Self {
        self.capture = Some(NativeCapture::new(options));
        self
    }

    /// Execute the production frame inside an active egui pass.
    ///
    /// Uses the same default bundled fonts, image loaders and calculator context
    /// initialization as native v2. Does not start a window, worker, or runtime.
    pub fn frame(&mut self, ctx: &egui::Context) {
        if self.file_service.is_none() {
            let service = crate::widgets::forms::NativeFileService::new(ctx.clone());
            service.attach(self.painter.forms_mut());
            self.file_service = Some(service);
        }
        if let Some(service) = &mut self.file_service {
            for error in service.poll(self.painter.forms_mut()) {
                self.app
                    .diagnostics
                    .push(format!("file service: {error:?}"));
            }
        }
        self.initialized = true;
        let completed = std::mem::take(&mut *self.completions.lock().unwrap());
        for completion in completed {
            if let Err(completion) = self.worker.complete(completion, ctx) {
                self.completions.lock().unwrap().push(*completion);
            }
        }
        self.painter
            .resources_mut()
            .install_completions(self.app.document(), ctx);
        self.app.frame(ctx, &mut self.painter);
        if let Some(service) = &mut self.file_service {
            for error in service.dispatch(self.painter.forms_mut()) {
                self.app
                    .diagnostics
                    .push(format!("file service: {error:?}"));
            }
        }
        for effects in std::mem::take(&mut self.app.action_effects) {
            self.action_outputs.extend(effects.custom);
            self.navigation_outputs.extend(effects.navigation);
        }
        for interaction in &self.app.interactions {
            if interaction.output.response.clicked()
                && let Some(hyperchad_transformer::Element::Anchor {
                    href: Some(href), ..
                }) = self
                    .app
                    .document()
                    .get(interaction.stamp.key)
                    .map(|node| &node.element)
            {
                self.navigation_outputs.push(href.clone());
            }
        }
        self.resources_ready = self.painter.resources_mut().snapshot().all_ready();
        service_resource_requests(&self.worker, &self.completions, ctx);
        let geometry = self.geometry();
        self.stable = !geometry.is_empty() && geometry == self.previous_geometry;
        self.previous_geometry = geometry;
        if let Some(mut capture) = self.capture.take() {
            let ready = self.ready(ctx, &capture.options.required_ids);
            capture.frame(ctx, ready, &self.previous_geometry);
            self.capture = Some(capture);
        }
    }

    /// Snapshot of production widget response rectangles keyed by HTML IDs.
    ///
    /// # Panics
    /// * Panics if the production geometry lock is poisoned.
    #[must_use]
    pub fn geometry(&self) -> BTreeMap<String, egui::Rect> {
        let mut result = BTreeMap::new();
        if let Some(snapshot) = self.app.geometry() {
            for key in self.app.document().keys() {
                if let (Some(id), Some(geometry)) = (
                    self.app
                        .document()
                        .get(key)
                        .and_then(|node| node.str_id.as_ref()),
                    snapshot.node(key),
                ) {
                    if let Some(bounds) = geometry.visible_bounds() {
                        result.insert(id.clone(), bounds);
                    }
                }
            }
        }
        result
    }

    /// True after two matching geometry frames, required widgets and assets ready.
    #[must_use]
    pub fn ready(&self, ctx: &egui::Context, required_ids: &[String]) -> bool {
        self.stable
            && !ctx.has_pending_images()
            && self.app.ready(&self.painter)
            && self.resources_ready
            && !required_ids.is_empty()
            && required_ids.iter().all(|id| {
                self.previous_geometry.get(id).is_some_and(|rect| {
                    rect.is_finite()
                        && rect.is_positive()
                        && ctx.content_rect().contains_rect(*rect)
                })
            })
    }

    /// Drain production custom actions, including their evaluated values.
    #[must_use]
    pub fn drain_actions(&mut self) -> Vec<(String, Option<Value>)> {
        std::mem::take(&mut self.action_outputs)
    }

    /// Drain production navigation requests without following them in tests.
    #[must_use]
    pub fn drain_navigation(&mut self) -> Vec<String> {
        std::mem::take(&mut self.navigation_outputs)
    }

    /// Native capture result, including surfaced filesystem/timeout errors.
    #[allow(clippy::must_use_candidate)] // Option is already must_use.
    pub fn capture_result(&self) -> Option<&Result<PathBuf, String>> {
        self.capture
            .as_ref()
            .and_then(|capture| capture.result.as_ref())
    }
}

/// Shared native screenshot state machine, also used by the production app.
pub(crate) struct NativeCapture {
    options: CaptureOptions,
    frames: u64,
    requested: bool,
    result: Option<Result<PathBuf, String>>,
}

impl NativeCapture {
    const fn new(options: CaptureOptions) -> Self {
        Self {
            options,
            frames: 0,
            requested: false,
            result: None,
        }
    }

    pub(crate) fn from_env() -> Option<Self> {
        let output = std::env::var_os("HYPERCHAD_EGUI_SCREENSHOT")?;
        Some(Self::new(CaptureOptions {
            output: output.into(),
            ready: std::env::var_os("HYPERCHAD_EGUI_READY").map(Into::into),
            required_ids: std::env::var("HYPERCHAD_EGUI_REQUIRED_IDS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect(),
            max_frames: 120,
            close: std::env::var("HYPERCHAD_EGUI_SCREENSHOT_EXIT").as_deref() == Ok("1"),
        }))
    }

    pub(crate) fn required_geometry_ready(
        &self,
        ctx: &egui::Context,
        geometry: &BTreeMap<String, egui::Rect>,
    ) -> bool {
        !self.options.required_ids.is_empty()
            && self.options.required_ids.iter().all(|id| {
                geometry.get(id).is_some_and(|rect| {
                    rect.is_finite()
                        && rect.is_positive()
                        && ctx.content_rect().contains_rect(*rect)
                })
            })
    }

    pub(crate) fn failed(&self) -> Option<&str> {
        self.result
            .as_ref()
            .and_then(|result| result.as_ref().err().map(String::as_str))
            .or_else(|| {
                self.result
                    .is_none()
                    .then_some("window closed before screenshot completed")
            })
    }

    pub(crate) fn frame(
        &mut self,
        ctx: &egui::Context,
        ready: bool,
        geometry: &BTreeMap<String, egui::Rect>,
    ) {
        self.frames += 1;
        let options = &self.options;
        if self.frames == 1 {
            let paths = std::iter::once(&options.output).chain(options.ready.iter());
            if paths
                .clone()
                .any(|path| std::fs::symlink_metadata(path).is_ok())
            {
                self.result = Some(Err(
                    "capture output or readiness marker already exists".to_owned()
                ));
            }
            if options.required_ids.is_empty() {
                self.result = Some(Err(
                    "capture requires HYPERCHAD_EGUI_REQUIRED_IDS semantic IDs".to_owned(),
                ));
            }
        }
        if self.result.is_some() && self.frames > 1 {
            if options.close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        if self.result.is_some() {
            // Validation failed before requesting any framebuffer.
        } else if self.requested {
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = screenshot {
                self.result = Some(
                    save_capture(&image, &options.output)
                        .map(|()| options.output.clone())
                        .map_err(|error| error.to_string()),
                );
            }
        } else if ready {
            let ready_result = options.ready.as_ref().map_or(Ok(()), |path| {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, b"ready\n")
            });
            match ready_result {
                Ok(()) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                    self.requested = true;
                }
                Err(error) => self.result = Some(Err(format!("readiness marker: {error}"))),
            }
        }
        if self.result.is_none() && self.frames >= options.max_frames {
            self.result = Some(Err(format!(
                "capture timed out after {} frames (requested={}, geometry={:?})",
                self.frames, self.requested, geometry
            )));
        }
        if let Some(result) = &self.result {
            if let Err(error) = result {
                if self.requested
                    && let Some(path) = &options.ready
                {
                    let _ = std::fs::remove_file(path);
                }
                let error_path = options.output.with_extension("error.txt");
                if let Some(parent) = error_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(write_error) = std::fs::write(&error_path, error) {
                    eprintln!("capture failure: {error}; error artifact: {write_error}");
                }
            }
            if options.close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else {
            ctx.request_repaint();
        }
    }
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> eframe::App for FeedbackApp<C> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct NoLayout;
    impl hyperchad_transformer::layout::Calc for NoLayout {
        fn calc(&self, _: &mut Container) -> bool {
            false
        }
    }
    impl EguiCalc for NoLayout {
        fn with_context(self, _: egui::Context) -> Self {
            self
        }
    }

    #[test]
    fn production_image_jobs_and_required_geometry_gate_readiness() {
        let ctx = egui::Context::default();
        let mut app = FeedbackApp::new(Container::default(), NoLayout);
        app.stable = true;
        app.previous_geometry.insert(
            "required".to_owned(),
            egui::Rect::from_min_size(egui::pos2(1.0, 1.0), egui::vec2(10.0, 10.0)),
        );
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            assert!(!app.ready(ctx, &["absent".to_owned()]));
            assert!(!app.ready(ctx, &[]));
            // Cache readiness is covered by K's generation-checked resource tests.
            app.resources_ready = false;
            assert!(!app.ready(ctx, &["required".to_owned()]));
            app.resources_ready = true;
            assert!(!app.ready(ctx, &["required".to_owned()])); // no calculated snapshot installed
        });
    }
}

fn service_resource_requests(
    worker: &ResourceWorker,
    completions: &std::sync::Arc<std::sync::Mutex<Vec<crate::resources::LoadCompletion>>>,
    ctx: &egui::Context,
) {
    while let Some(request) = worker.try_request() {
        let completions = completions.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let path = request
                .source
                .strip_prefix("file://")
                .unwrap_or(&request.source);
            let result = if path.starts_with("http://") || path.starts_with("https://") {
                let runtime = switchy_async::runtime::Builder::new().build();
                match runtime {
                    Ok(runtime) => runtime.block_on(async {
                        switchy_async::time::timeout(std::time::Duration::from_secs(30), async {
                            use futures_util::StreamExt;
                            let response = switchy_http::Client::new()
                                .get(path)
                                .send()
                                .await
                                .map_err(|error| error.to_string())?;
                            if !response.status().is_success() {
                                return Err(format!("image HTTP status {}", response.status()));
                            }
                            let mut stream = std::pin::pin!(response.bytes_stream());
                            let mut bytes = Vec::new();
                            while let Some(chunk) = stream.next().await {
                                if request.is_cancelled() {
                                    return Err("image request cancelled".to_owned());
                                }
                                let chunk = chunk.map_err(|error| error.to_string())?;
                                if bytes.len().saturating_add(chunk.len())
                                    > request.max_encoded_bytes
                                {
                                    return Err("encoded image exceeds budget".to_owned());
                                }
                                bytes.extend_from_slice(&chunk);
                            }
                            Ok(bytes)
                        })
                        .await
                        .map_err(|_| "image fetch timed out".to_owned())?
                    }),
                    Err(error) => Err(error.to_string()),
                }
            } else if path.contains("://") {
                Err("unsupported image URL scheme".to_owned())
            } else {
                std::fs::File::open(path)
                    .map_err(|error| error.to_string())
                    .and_then(|file| {
                        let limit = u64::try_from(request.max_encoded_bytes)
                            .map_err(|error| error.to_string())?;
                        let mut bytes = Vec::new();
                        file.take(limit.saturating_add(1))
                            .read_to_end(&mut bytes)
                            .map_err(|error| error.to_string())?;
                        if bytes.len() > request.max_encoded_bytes {
                            return Err("encoded image exceeds budget".to_owned());
                        }
                        Ok(bytes)
                    })
            };
            if request.is_cancelled() {
                return;
            }
            let completion = match result {
                Ok(bytes) => request.decode(&bytes),
                Err(error) => request.fail(error),
            };
            completions.lock().unwrap().push(completion);
            ctx.request_repaint();
        });
    }
}

fn save_capture(image: &egui::ColorImage, path: &std::path::Path) -> Result<(), image::ImageError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_array)
        .collect();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let width = u32::try_from(image.size[0]).map_err(std::io::Error::other)?;
    let height = u32::try_from(image.size[1]).map_err(std::io::Error::other)?;
    let buffer = image::RgbaImage::from_raw(width, height, bytes)
        .ok_or_else(|| std::io::Error::other("invalid screenshot dimensions"))?;
    let result =
        image::DynamicImage::ImageRgba8(buffer).write_to(&mut file, image::ImageFormat::Png);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}
