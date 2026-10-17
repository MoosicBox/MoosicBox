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
    pending_route_admission: Option<crate::runtime::DocumentCommand>,
    routing: Option<crate::routing::Routing>,
    resource_router: Option<hyperchad_router::Router>,
    router: Option<hyperchad_router::Router>,
    current_url: String,
    app: BackendFrame<C>,
    painter: ProductionPainter,
    completions: std::sync::Arc<std::sync::Mutex<Vec<crate::resources::LoadCompletion>>>,
    worker: ResourceWorker,
    action_outputs: Vec<(String, Option<Value>)>,
    navigation_outputs: Vec<String>,
    file_service: Option<crate::widgets::forms::NativeFileService>,
    previous_scene: Option<crate::resources::SceneResourceReadiness>,
    resources_ready: bool,
    initialized: bool,
    previous_geometry: BTreeMap<String, egui::Rect>,
    stable: bool,
    capture: Option<NativeCapture>,
}

impl<C: EguiCalc + Clone + Send + Sync + 'static> FeedbackApp<C> {
    /// Queue a real production view and retain observable output channels.
    #[must_use]
    /// # Panics
    /// * The fixed positive queue capacity is rejected by the resource adapter.
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
            pending_route_admission: None,
            routing: None,
            resource_router: None,
            router: None,
            current_url: "/".to_owned(),
            app,
            completions: std::sync::Arc::default(),
            painter,
            worker,
            action_outputs: Vec::new(),
            navigation_outputs: Vec::new(),
            file_service: None,
            previous_scene: None,
            resources_ready: false,
            initialized: false,
            previous_geometry: BTreeMap::new(),
            stable: false,
            capture: None,
        }
    }

    /// Enable environment opt-in only when constructing the native production host.
    pub(crate) fn with_native_capture_from_env(mut self) -> Self {
        self.capture = NativeCapture::from_env();
        self
    }

    /// Attach the real application router; handlers execute on a background runtime.
    #[must_use]
    pub fn with_router(mut self, router: hyperchad_router::Router, current_url: String) -> Self {
        self.resource_router = Some(router.clone());
        self.router = Some(router);
        self.current_url = current_url;
        self
    }

    /// Deliver metadata to the single application consumer without blocking UI paint.
    pub fn flush_responses(&mut self, ctx: &egui::Context) {
        self.app.flush_responses(ctx);
    }

    /// Observe surfaced production failures; tests must not treat missing paint as success.
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        self.app.take_diagnostics()
    }

    /// Admit a test/native update on the same ordered document/canvas mailbox.
    /// Submit one bounded UI command without waiting.
    /// # Errors
    /// * Queue saturation or disconnected renderer.
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
    /// # Panics
    /// * The fixed file-service capacity is rejected during initialization.
    pub fn frame(&mut self, ctx: &egui::Context) {
        if self.routing.is_none()
            && let Some(router) = self.router.take()
        {
            let wake_context = ctx.clone();
            match crate::routing::Routing::new(
                64,
                8 * 1024 * 1024,
                std::sync::Arc::new(move || wake_context.request_repaint()),
            ) {
                Ok((routing, worker)) => {
                    self.routing = Some(routing);
                    std::thread::spawn(move || {
                        match switchy_async::runtime::Builder::new().build() {
                            Ok(runtime) => runtime.block_on(worker.run(router)),
                            Err(error) => log::error!("route worker startup: {error}"),
                        }
                    });
                }
                Err(error) => self.app.diagnostics.push(error.to_string()),
            }
        }
        self.enqueue_routes(ctx);
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
        self.poll_routes(ctx);
        if let Some(service) = &mut self.file_service {
            for error in service.dispatch(self.painter.forms_mut()) {
                self.app
                    .diagnostics
                    .push(format!("file service: {error:?}"));
            }
        }
        self.route_widget_outputs(ctx);
        for effects in std::mem::take(&mut self.app.action_effects) {
            self.action_outputs.extend(effects.custom);
            if !effects.navigation.is_empty() {
                self.app
                    .diagnostics
                    .push("deferred navigation requires source-stamped H effects".to_owned());
            }
        }
        for (source, url) in std::mem::take(&mut self.app.navigation_effects) {
            self.navigate(ctx, source, &url);
        }
        let links: Vec<_> = self
            .app
            .interactions
            .iter()
            .filter_map(|interaction| {
                if !interaction.output.response.clicked() {
                    return None;
                }
                match &self.app.document().get(interaction.stamp.key)?.element {
                    hyperchad_transformer::Element::Anchor {
                        href: Some(href), ..
                    } => Some((interaction.stamp, href.clone())),
                    _ => None,
                }
            })
            .collect();
        for (source, href) in links {
            self.navigate(ctx, source, &href);
        }
        let scene_stable = self.update_scene_readiness();
        service_resource_requests(
            &self.worker,
            &self.completions,
            ctx,
            self.resource_router.as_ref(),
        );
        let geometry = self.geometry();
        self.stable = scene_stable && !geometry.is_empty() && geometry == self.previous_geometry;
        self.previous_geometry = geometry;
        self.update_capture(ctx);
    }

    fn update_scene_readiness(&mut self) -> bool {
        let required_images = self.app.scene_images(&self.painter);
        let scene = self
            .painter
            .resources_mut()
            .scene_readiness(self.app.document(), &required_images);
        let stable = self.previous_scene.as_ref() == Some(&scene);
        self.resources_ready = scene.is_ready();
        self.previous_scene = Some(scene);
        stable
    }

    fn update_capture(&mut self, ctx: &egui::Context) {
        if let Some(mut capture) = self.capture.take() {
            let ready = self.ready(ctx, &capture.options.required_ids);
            capture.blocked_details = Some(format!(
                "stable={} frame_ready={} pending_images={} resources_ready={} route_admission_pending={} required_ids={:?} diagnostics={:?} scene={:?} all_document_resources={:?}",
                self.stable,
                self.app.projection_ready(),
                ctx.has_pending_images(),
                self.resources_ready,
                self.pending_route_admission.is_some(),
                capture.options.required_ids,
                self.app.diagnostics,
                self.previous_scene,
                self.painter.resources_mut().snapshot()
            ));
            capture.frame(ctx, ready, &self.previous_geometry);
            if let Some(Err(error)) = &capture.result {
                self.app.fail_native_host(ctx, error.clone());
            }
            self.capture = Some(capture);
        }
    }

    fn navigate(&mut self, ctx: &egui::Context, source: crate::document::ElementStamp, url: &str) {
        let Some(routing) = &self.routing else {
            self.app
                .diagnostics
                .push("navigation requires installed router".to_owned());
            return;
        };
        match routing.link(url) {
            Ok(crate::routing::Link::Local(url)) => match crate::routing::RequestContext::get(
                source,
                &url,
                hyperchad_transformer::models::Selector::SelfTarget,
                hyperchad_transformer::models::SwapStrategy::This,
            ) {
                Ok(mut request) => {
                    request.trigger = crate::routing::Trigger::Navigation;
                    "navigation".clone_into(&mut request.scope);
                    self.admit_route(ctx, request);
                }
                Err(error) => self.app.diagnostics.push(error.to_string()),
            },
            Ok(crate::routing::Link::External(url)) => {
                self.navigation_outputs.push(url);
            }
            Err(error) => self.app.diagnostics.push(error.to_string()),
        }
    }

    fn admit_route(&mut self, ctx: &egui::Context, request: crate::routing::RequestContext) {
        let Some(mut routing) = self.routing.take() else {
            self.app
                .diagnostics
                .push("route request requires installed application router".to_owned());
            return;
        };
        match routing.prepare(self.app.document_mut(), request) {
            Ok(id) => {
                let before = routing.before_event(id);
                if let Some(event) = before {
                    if let Err(error) = self.app.dispatch_host_event(ctx, event) {
                        self.app.diagnostics.push(error);
                        routing.cancel(self.app.document_mut(), id);
                    } else {
                        let _ = routing.start(id);
                    }
                } else {
                    let _ = routing.start(id);
                }
            }
            Err(error) => self.app.diagnostics.push(error.to_string()),
        }
        self.routing = Some(routing);
    }

    fn enqueue_routes(&mut self, ctx: &egui::Context) {
        let Some(routing) = &mut self.routing else {
            return;
        };
        for _ in 0..64 {
            let command = self.pending_route_admission.take().or_else(|| {
                routing
                    .try_take()
                    .map(crate::runtime::DocumentCommand::RouteCompletion)
            });
            let Some(command) = command else {
                break;
            };
            match self.app.handle().submit(command) {
                Ok(()) => {}
                Err(flume::TrySendError::Full(command)) => {
                    self.pending_route_admission = Some(command);
                    ctx.request_repaint();
                    break;
                }
                Err(flume::TrySendError::Disconnected(_)) => {
                    self.app
                        .diagnostics
                        .push("route completion mailbox disconnected".to_owned());
                    break;
                }
            }
        }
    }

    fn poll_routes(&mut self, ctx: &egui::Context) {
        if self.app.geometry().is_none() {
            return;
        }
        let Some(prepared) = self.app.route_boundary.take() else {
            return;
        };
        let Some(mut routing) = self.routing.take() else {
            self.app
                .diagnostics
                .push("route boundary has no installed routing owner".to_owned());
            return;
        };
        routing.reconcile(self.app.document_mut());
        {
            for event in routing.terminal_events(self.app.document(), &prepared) {
                if let Err(error) = self.app.dispatch_host_event(ctx, event) {
                    self.app.diagnostics.push(error);
                }
            }
            if let Some(completion) = routing.complete(self.app.document_mut(), prepared) {
                match completion.outcome {
                    Ok(crate::routing::NativeOutcome::Applied(mutation)) => {
                        if mutation.response != hyperchad_renderer::ResponseMetadata::default() {
                            self.app.response_effects.push(mutation.response.clone());
                        }
                        self.app.retain_route_mutation(mutation);
                        ctx.request_repaint();
                    }
                    Ok(
                        crate::routing::NativeOutcome::Redirected(metadata)
                        | crate::routing::NativeOutcome::Metadata(metadata),
                    ) => {
                        self.app.response_effects.push(metadata);
                    }
                    Ok(crate::routing::NativeOutcome::NoContent) => {}
                    Err(error) => self.app.diagnostics.push(error.to_string()),
                }
            }
        }
        self.routing = Some(routing);
    }

    fn route_widget_outputs(&mut self, ctx: &egui::Context) {
        let clicked: Vec<_> = self
            .app
            .interactions
            .iter()
            .filter(|interaction| interaction.output.response.clicked())
            .map(|interaction| interaction.stamp)
            .collect();
        let mut requests = Vec::new();
        for source in clicked {
            if let Some(node) = self.app.document().get(source.key).cloned() {
                if matches!(node.element, hyperchad_transformer::Element::Button { .. })
                    && let Err(error) = self
                        .painter
                        .forms_mut()
                        .activate_button(self.app.document(), source)
                {
                    self.app.diagnostics.push(error);
                }
                if let Some(route) = &node.route {
                    match crate::routing::RequestContext::route(source, route, "click") {
                        Ok(Some(request)) => requests.push(request),
                        Ok(None) => {}
                        Err(error) => self.app.diagnostics.push(error.to_string()),
                    }
                }
            }
        }
        for action in self.painter.forms_mut().take_actions() {
            match action {
                crate::widgets::forms::FormAction::Submit(submission) => {
                    match crate::routing::RequestContext::form(
                        submission,
                        &self.current_url,
                        hyperchad_transformer::models::Selector::SelfTarget,
                        hyperchad_transformer::models::SwapStrategy::This,
                        8 * 1024 * 1024,
                    ) {
                        Ok(request) => requests.push(request),
                        Err(error) => self.app.diagnostics.push(error.to_string()),
                    }
                }
                crate::widgets::forms::FormAction::Reset { form, .. } => {
                    if let Err(error) = self.painter.forms_mut().reset_form(form.key) {
                        self.app.diagnostics.push(error);
                    }
                }
            }
        }
        for request in requests {
            self.admit_route(ctx, request);
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
                ) && let Some(bounds) = geometry.visible_bounds()
                {
                    result.insert(id.clone(), bounds);
                }
            }
        }
        result
    }

    /// True after two matching geometry frames, required widgets and assets ready.
    #[must_use]
    pub fn ready(&self, ctx: &egui::Context, required_ids: &[String]) -> bool {
        self.app.diagnostics.is_empty()
            && self.stable
            && !ctx.has_pending_images()
            && self.app.projection_ready()
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
    blocked_details: Option<String>,
    options: CaptureOptions,
    frames: u64,
    requested: bool,
    result: Option<Result<PathBuf, String>>,
}

impl NativeCapture {
    const fn new(options: CaptureOptions) -> Self {
        Self {
            blocked_details: None,
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

    #[cfg(feature = "feedback")]
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

    #[cfg(feature = "feedback")]
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
                "capture timed out after {} frames (requested={}, geometry={:?}, readiness={:?})",
                self.frames, self.requested, geometry, self.blocked_details
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

async fn load_router_resource(
    router: &hyperchad_router::Router,
    source: &str,
    maximum: usize,
) -> Result<Vec<u8>, String> {
    if !source.starts_with('/') || source.starts_with("//") {
        return Err("asset URL must be an application-local path".to_owned());
    }
    match router
        .navigate(source)
        .await
        .map_err(|error| error.to_string())?
    {
        Some(hyperchad_renderer::Content::Raw { data, .. }) if data.len() <= maximum => {
            Ok(data.to_vec())
        }
        Some(hyperchad_renderer::Content::Raw { .. }) => {
            Err("encoded application image exceeds budget".to_owned())
        }
        _ => Err("application image route did not return raw bytes".to_owned()),
    }
}

fn load_application_resource(
    request: &crate::resources::LoadRequest,
    router: Option<&hyperchad_router::Router>,
) -> Result<Vec<u8>, String> {
    switchy_async::runtime::Builder::new()
        .build()
        .map_err(|error| error.to_string())
        .and_then(|runtime| {
            runtime.block_on(async {
                switchy_async::time::timeout(std::time::Duration::from_secs(30), async {
                    if request.is_cancelled() {
                        return Err("image request cancelled".to_owned());
                    }
                    let router = router.ok_or_else(|| {
                        "application image URL requires installed Router".to_owned()
                    })?;
                    let bytes =
                        load_router_resource(router, &request.source, request.max_encoded_bytes)
                            .await?;
                    if request.is_cancelled() {
                        return Err("image request cancelled".to_owned());
                    }
                    Ok(bytes)
                })
                .await
                .map_err(|_| "application image route timed out".to_owned())?
            })
        })
}

fn service_resource_requests(
    worker: &ResourceWorker,
    completions: &std::sync::Arc<std::sync::Mutex<Vec<crate::resources::LoadCompletion>>>,
    ctx: &egui::Context,
    router: Option<&hyperchad_router::Router>,
) {
    while let Some(request) = worker.try_request() {
        let router = router.cloned();
        let completions = completions.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let path = request
                .source
                .strip_prefix("file://")
                .unwrap_or(&request.source);
            let result = if request.source.starts_with('/') {
                load_application_resource(&request, router.as_ref())
            } else if path.starts_with("http://") || path.starts_with("https://") {
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
    fn scene_image_membership_excludes_hidden_clipped_and_transparent_but_not_missing_source() {
        use hyperchad_transformer::models::Visibility;
        fn image(id: usize, x: f32) -> Container {
            Container {
                id,
                calculated_x: Some(x),
                calculated_y: Some(0.0),
                calculated_width: Some(20.0),
                calculated_height: Some(20.0),
                element: hyperchad_transformer::Element::Image {
                    source: None,
                    alt: None,
                    fit: None,
                    source_set: None,
                    sizes: None,
                    loading: None,
                },
                ..Container::default()
            }
        }
        let mut hidden = image(3, 0.0);
        hidden.visibility = Some(Visibility::Hidden);
        let mut transparent = Container {
            id: 4,
            calculated_x: Some(0.0),
            calculated_y: Some(0.0),
            calculated_width: Some(30.0),
            calculated_height: Some(30.0),
            calculated_opacity: Some(0.0),
            children: vec![image(5, 0.0)],
            ..Container::default()
        };
        transparent.opacity = Some(0.0.into());
        let root = Container {
            id: 1,
            str_id: Some("root".to_owned()),
            calculated_width: Some(100.0),
            calculated_height: Some(100.0),
            children: vec![image(2, 0.0), hidden, transparent, image(6, 200.0)],
            ..Container::default()
        };
        let ctx = egui::Context::default();
        let mut host = FeedbackApp::new(root, NoLayout);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(100.0, 100.0),
                )),
                ..Default::default()
            },
            |ctx| {
                host.frame(ctx);
            },
        );
        let members = host.app.scene_images(&host.painter);
        assert_eq!(members.len(), 1);
        assert_eq!(host.app.document().get(members[0].key).unwrap().id, 2);
        let scene = host.previous_scene.as_ref().unwrap();
        assert!(!scene.is_ready());
        assert_eq!(scene.failed.len(), 1, "visible missing source is a failure");
        assert_eq!(
            host.painter.resources_mut().snapshot().entries.len(),
            4,
            "excluded entries remain observable"
        );
    }

    #[test]
    fn registered_raw_asset_routes_resolve_without_filesystem_fallback() {
        let router = hyperchad_router::Router::new()
            .with_route("/public/img/test.svg", |_| async {
                hyperchad_renderer::Content::Raw {
                    data: b"<svg/>".as_slice().into(),
                    content_type: "image/svg+xml".to_owned(),
                }
            })
            .with_route("/not-image", |_| async {
                hyperchad_renderer::View::default()
            });
        let runtime = switchy_async::runtime::Builder::new().build().unwrap();
        runtime.block_on(async {
            assert_eq!(
                load_router_resource(&router, "/public/img/test.svg", 64)
                    .await
                    .unwrap(),
                b"<svg/>"
            );
            assert!(
                load_router_resource(&router, "/public/img/test.svg", 1)
                    .await
                    .is_err()
            );
            assert!(
                load_router_resource(&router, "/not-image", 64)
                    .await
                    .is_err()
            );
            assert!(
                load_router_resource(&router, "/unregistered", 64)
                    .await
                    .is_err()
            );
            assert!(
                load_router_resource(&router, "//external/image", 64)
                    .await
                    .is_err()
            );
        });
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "explicit end-to-end FIFO setup and assertions kept together for auditability"
    )]
    fn canvas_commands_surround_completion_at_real_mailbox_boundary() {
        use crate::{
            canvas::CanvasCommand,
            routing::{RequestContext, Routing},
            runtime::DocumentCommand,
        };
        use hyperchad_renderer::{
            View,
            canvas::{CanvasAction, CanvasUpdate},
        };
        use hyperchad_transformer::models::{Selector, SwapStrategy};
        use std::future::Future;
        let ctx = egui::Context::default();
        let canvas = Container {
            id: 2,
            str_id: Some("drawing".to_owned()),
            calculated_x: Some(0.0),
            calculated_y: Some(0.0),
            element: hyperchad_transformer::Element::Canvas,
            calculated_width: Some(32.0),
            calculated_height: Some(32.0),
            ..Container::default()
        };
        let mut host = FeedbackApp::new(
            Container {
                id: 1,
                children: vec![canvas.clone()],
                calculated_width: Some(100.0),
                calculated_height: Some(100.0),
                ..Container::default()
            },
            NoLayout,
        );
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        let source = host
            .app
            .document()
            .stamp(host.app.document().root_keys()[0])
            .unwrap();
        let canvas_key = host
            .app
            .document()
            .select(&Selector::Id("drawing".into()), None)[0];
        let canvas_stamp = host.app.document().stamp(canvas_key).unwrap();
        let (mut routing, worker) = Routing::new(8, 65536, std::sync::Arc::new(|| {})).unwrap();
        routing
            .submit(
                host.app.document_mut(),
                RequestContext::get(
                    source,
                    "/replace",
                    Selector::Id("drawing".into()),
                    SwapStrategy::This,
                )
                .unwrap(),
            )
            .unwrap();
        let router = hyperchad_router::Router::new().with_route("/replace", move |_| {
            let canvas = canvas.clone();
            async move {
                View::from(Container {
                    id: 3,
                    children: vec![canvas],
                    ..Container::default()
                })
            }
        });
        let mut work = Box::pin(worker.run(router));
        assert!(
            work.as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
                .is_pending()
        );
        let prepared = routing.try_take().unwrap();
        host.routing = Some(routing);
        let update = || {
            DocumentCommand::Canvas(CanvasCommand::Stamped {
                stamp: canvas_stamp,
                update: CanvasUpdate {
                    target: "drawing".to_owned(),
                    canvas_actions: vec![CanvasAction::Clear],
                },
            })
        };
        let handle = host.app.handle();
        handle.submit(update()).unwrap();
        handle
            .submit(DocumentCommand::RouteCompletion(prepared))
            .unwrap();
        handle.submit(update()).unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        assert!(
            host.app.diagnostics.is_empty(),
            "preceding canvas accepted: {:?}",
            host.app.diagnostics
        );
        assert!(!host.app.document().is_current(canvas_stamp));
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        assert_eq!(
            host.app.diagnostics.len(),
            1,
            "only following stale canvas rejected: {:?}",
            host.app.diagnostics
        );
        assert!(host.app.diagnostics[0].contains("stale"));
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "explicit replacement and ticket lifecycle regression kept in admission order"
    )]
    fn partial_replacement_before_completion_emits_abort_not_success() {
        use crate::{
            document::ViewOrigin,
            routing::{RequestContext, Routing},
            runtime::DocumentCommand,
        };
        use hyperchad_actions::{Action, ActionTrigger, ActionType};
        use hyperchad_renderer::View;
        use hyperchad_transformer::models::{Selector, SwapStrategy};
        use std::future::Future;
        let ctx = egui::Context::default();
        let target = Container {
            id: 2,
            str_id: Some("target".to_owned()),
            calculated_x: Some(0.0),
            calculated_y: Some(0.0),
            calculated_width: Some(20.0),
            calculated_height: Some(20.0),
            ..Container::default()
        };
        let mut host = FeedbackApp::new(
            Container {
                id: 1,
                children: vec![
                    target,
                    Container {
                        id: 5,
                        str_id: Some("request-source".to_owned()),
                        calculated_x: Some(30.0),
                        calculated_y: Some(0.0),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        actions: [
                            (ActionTrigger::HttpRequestAbort, "abort"),
                            (ActionTrigger::HttpRequestSuccess, "success"),
                            (ActionTrigger::HttpAfterRequest, "after"),
                        ]
                        .into_iter()
                        .map(|(trigger, name)| Action {
                            trigger,
                            effect: ActionType::Custom {
                                action: name.to_owned(),
                            }
                            .into(),
                        })
                        .collect(),
                        ..Container::default()
                    },
                ],
                actions: [
                    (ActionTrigger::HttpRequestAbort, "abort"),
                    (ActionTrigger::HttpRequestSuccess, "success"),
                    (ActionTrigger::HttpAfterRequest, "after"),
                ]
                .into_iter()
                .map(|(trigger, name)| Action {
                    trigger,
                    effect: ActionType::Custom {
                        action: name.to_owned(),
                    }
                    .into(),
                })
                .collect(),
                calculated_width: Some(100.0),
                calculated_height: Some(100.0),
                ..Container::default()
            },
            NoLayout,
        );
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        let source = host
            .app
            .document()
            .stamp(
                host.app
                    .document()
                    .select(&Selector::Id("request-source".into()), None)[0],
            )
            .unwrap();
        let (mut routing, worker) = Routing::new(8, 65536, std::sync::Arc::new(|| {})).unwrap();
        routing
            .submit(
                host.app.document_mut(),
                RequestContext::get(
                    source,
                    "/late",
                    Selector::Id("target".into()),
                    SwapStrategy::This,
                )
                .unwrap(),
            )
            .unwrap();
        let router = hyperchad_router::Router::new().with_route("/late", |_| async {
            View::from(Container {
                id: 4,
                str_id: Some("late".into()),
                ..Container::default()
            })
        });
        let mut work = Box::pin(worker.run(router));
        assert!(
            work.as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
                .is_pending()
        );
        let prepared = routing.try_take().unwrap();
        host.routing = Some(routing);
        let handle = host.app.handle();
        handle
            .submit(DocumentCommand::Apply {
                view: View::from(Container {
                    id: 6,
                    children: vec![Container {
                        id: 3,
                        str_id: Some("replacement".into()),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        ..Container::default()
                    }],
                    ..Container::default()
                }),
                origin: ViewOrigin::Element {
                    source: source.key,
                    target: Selector::Id("target".into()),
                    strategy: SwapStrategy::This,
                },
            })
            .unwrap();
        handle
            .submit(DocumentCommand::RouteCompletion(prepared))
            .unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        assert_eq!(
            host.drain_actions(),
            vec![("after".to_owned(), None), ("abort".to_owned(), None)],
            "diagnostics={:?}, source_current={}",
            host.app.diagnostics,
            host.app.document().is_current(source)
        );
        assert_eq!(
            host.app
                .document()
                .select(&Selector::Id("replacement".into()), None)
                .len(),
            1
        );
        assert_eq!(
            host.app
                .document()
                .select(&Selector::Id("late".into()), None),
            []
        );
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        assert_eq!(host.drain_actions(), []);
    }

    #[test]
    fn explicit_capture_configuration_does_not_leak_into_headless_hosts() {
        let plain = FeedbackApp::new(Container::default(), NoLayout);
        assert!(plain.capture.is_none());
        let configured = plain.with_capture(CaptureOptions {
            output: PathBuf::from("explicit.png"),
            ready: Some(PathBuf::from("explicit.ready")),
            required_ids: vec!["root".to_owned()],
            max_frames: 7,
            close: false,
        });
        let capture = configured.capture.as_ref().unwrap();
        assert_eq!(capture.options.output, PathBuf::from("explicit.png"));
        assert_eq!(capture.options.max_frames, 7);
        assert!(!capture.options.close);
        assert!(
            FeedbackApp::new(Container::default(), NoLayout)
                .capture
                .is_none()
        );
    }

    #[test]
    fn fifo_completion_cannot_overtake_admitted_navigation() {
        use crate::{
            document::ViewOrigin,
            routing::{RequestContext, Routing},
            runtime::DocumentCommand,
        };
        use hyperchad_transformer::models::{Selector, SwapStrategy};
        use std::future::Future;
        let ctx = egui::Context::default();
        let root = Container {
            id: 1,
            str_id: Some("old".to_owned()),
            calculated_width: Some(100.0),
            calculated_height: Some(100.0),
            ..Container::default()
        };
        let mut host = FeedbackApp::new(root, NoLayout);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        let source = host
            .app
            .document()
            .stamp(host.app.document().root_keys()[0])
            .unwrap();
        let (mut routing, worker) = Routing::new(8, 65536, std::sync::Arc::new(|| {})).unwrap();
        routing
            .submit(
                host.app.document_mut(),
                RequestContext::get(source, "/result", Selector::SelfTarget, SwapStrategy::This)
                    .unwrap(),
            )
            .unwrap();
        let router = hyperchad_router::Router::new().with_route("/result", |_| async {
            hyperchad_renderer::View::from(Container {
                id: 3,
                str_id: Some("must-not-apply".to_owned()),
                ..Container::default()
            })
        });
        let mut work = Box::pin(worker.run(router));
        assert!(
            work.as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
                .is_pending()
        );
        host.routing = Some(routing);
        host.app
            .handle()
            .submit(DocumentCommand::Apply {
                view: hyperchad_renderer::View::from(Container {
                    id: 2,
                    str_id: Some("new-navigation".to_owned()),
                    calculated_width: Some(100.0),
                    calculated_height: Some(100.0),
                    ..Container::default()
                }),
                origin: ViewOrigin::Navigation,
            })
            .unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        assert_eq!(
            host.app.document().roots()[0].str_id.as_deref(),
            Some("new-navigation")
        );
        assert!(host.app.route_boundary.is_none());
        assert!(
            host.app
                .diagnostics
                .iter()
                .all(|message| !message.contains("geometry")),
            "{:?}",
            host.app.diagnostics
        );
    }

    #[test]
    fn two_ready_completions_yield_separate_projected_frames() {
        use crate::routing::{RequestContext, Routing};
        use hyperchad_actions::{Action, ActionTrigger, ActionType};
        use hyperchad_transformer::models::{Selector, SwapStrategy};
        use std::future::Future;
        let ctx = egui::Context::default();
        let mut host = FeedbackApp::new(
            Container {
                id: 1,
                actions: [
                    ActionTrigger::HttpAfterRequest,
                    ActionTrigger::HttpRequestSuccess,
                ]
                .into_iter()
                .map(|trigger| Action {
                    trigger,
                    effect: ActionType::Custom {
                        action: "terminal".to_owned(),
                    }
                    .into(),
                })
                .collect(),
                calculated_width: Some(100.0),
                calculated_height: Some(100.0),
                ..Container::default()
            },
            NoLayout,
        );
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        let source = host
            .app
            .document()
            .stamp(host.app.document().root_keys()[0])
            .unwrap();
        let (mut routing, worker) = Routing::new(8, 65536, std::sync::Arc::new(|| {})).unwrap();
        for scope in ["first", "second"] {
            let mut request =
                RequestContext::get(source, "/result", Selector::SelfTarget, SwapStrategy::None)
                    .unwrap();
            request.scope = scope.to_owned();
            routing.submit(host.app.document_mut(), request).unwrap();
        }
        let router = hyperchad_router::Router::new().with_route("/result", |_| async {
            hyperchad_renderer::View {
                primary: None,
                ..Default::default()
            }
        });
        let mut work = Box::pin(worker.run(router));
        assert!(
            work.as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
                .is_pending()
        );
        host.routing = Some(routing);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        assert!(host.app.route_boundary.is_none());
        assert!(
            host.app.geometry().is_none(),
            "first mutation invalidates projection"
        );
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        assert!(host.app.route_boundary.is_none());
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            host.frame(ctx);
        });
        assert_eq!(
            host.drain_actions(),
            vec![("terminal".to_owned(), None); 4],
            "two terminal phases exactly once for each completion"
        );
        assert!(host.app.geometry().is_some());
        assert!(
            host.app.diagnostics.is_empty(),
            "{:?}",
            host.app.diagnostics
        );
    }

    #[test]
    fn integrated_native_focus_selection_and_blur_use_forms_identity() {
        use crate::backend::FramePainter;
        let ctx = egui::Context::default();
        let node = Container {
            str_id: Some("editor".to_owned()),
            element: hyperchad_transformer::Element::Input {
                input: hyperchad_transformer::Input::Text {
                    value: Some("Aλ界".to_owned()),
                    placeholder: None,
                },
                name: None,
                autofocus: None,
            },
            calculated_width: Some(200.0),
            calculated_height: Some(32.0),
            ..Container::default()
        };
        let mut app = FeedbackApp::new(node, NoLayout);
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 100.0),
            )),
            focused: true,
            ..Default::default()
        };
        let _ = ctx.run(input(), |ctx| {
            app.frame(ctx);
        });
        let key = app.app.document().root_keys()[0];
        let stamp = app.app.document().stamp(key).unwrap();
        let id = app.painter.forms_mut().focus_id(stamp).unwrap();
        FramePainter::focus(&mut app.painter, &ctx, stamp).unwrap();
        FramePainter::select_all(&mut app.painter, &ctx, stamp).unwrap();
        assert_eq!(ctx.memory(egui::Memory::focused), Some(id));
        let state = egui::text_edit::TextEditState::load(&ctx, id).unwrap();
        let range = state.cursor.char_range().unwrap();
        assert_eq!(range.sorted_cursors()[0].index, 0);
        assert_eq!(range.sorted_cursors()[1].index, 3);
        let _ = ctx.run(input(), |ctx| {
            app.frame(ctx);
        });
        assert_eq!(ctx.memory(egui::Memory::focused), Some(id));
        FramePainter::blur(&mut app.painter, &ctx, stamp).unwrap();
        assert_ne!(ctx.memory(egui::Memory::focused), Some(id));
        let _ = ctx.run(input(), |ctx| {
            app.frame(ctx);
        });
        assert_ne!(ctx.memory(egui::Memory::focused), Some(id));
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
