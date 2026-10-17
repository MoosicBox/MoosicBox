//! UI-owned image lifecycle. Paint only selects/enqueues sources and draws textures.
//!
//! Backend integration: drain [`ResourceWorker::try_request`] outside paint, resolve
//! the selected source with an encoded-byte cap, call [`LoadRequest::decode`] on a
//! worker, and submit through [`ResourceWorker::complete`]. Every fetch failure must
//! use [`LoadRequest::fail`]. The UI calls [`ImageResources::install_completions`]
//! before traversal. No runtime, network client, or application asset registry is
//! owned here. Workers must enforce fetch timeouts and observe cancellation.
//!
//! Source policy: `srcset` supports uniform positive `w` or `x` descriptors (not
//! comma-containing/data URLs). Width candidates use authored `sizes`, resolved
//! against calculated content width and the viewport, otherwise content width.
//! Choose the smallest sufficient candidate, falling back to the largest. Source
//! strings are opaque to this module; the backend resolver decides allowed schemes.
//! SVG uses the existing egui extras rasterizer with external/embedded image
//! resolvers disabled. Raster decoders are restricted by dimensions/allocation.

use std::{
    collections::BTreeMap,
    io::Cursor,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use eframe::egui::{self, Color32, ColorImage, Rect, TextureHandle, Vec2};
use hyperchad_transformer::{
    Container, Element,
    models::{ImageFit, ImageLoading},
};

use crate::{
    backend::NodeOutput,
    document::{Document, ElementKey, ElementStamp, MutationSummary},
    geometry::NodeGeometry,
    widgets::WidgetAdapter,
};

/// Hard resource admission budgets, including cancelled jobs not yet released.
#[derive(Clone, Copy, Debug)]
pub struct ResourceLimits {
    /// Maximum current-document image nodes; overflow invalidates readiness.
    pub max_entries: usize,
    /// Maximum admitted jobs, including cancelled but unreleased jobs.
    pub max_inflight: usize,
    /// Maximum fetched body size (backend must enforce while streaming).
    pub max_encoded_bytes: usize,
    /// Maximum RGBA output bytes per job, plus decoder allocation limit.
    pub max_decoded_bytes: usize,
    /// Maximum sum of installed RGBA textures across live entries.
    pub max_texture_bytes: usize,
    /// Maximum raster dimension on either axis.
    pub max_dimension: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_entries: 256,
            max_inflight: 4,
            max_encoded_bytes: 8 * 1024 * 1024,
            max_decoded_bytes: 16 * 1024 * 1024,
            max_texture_bytes: 64 * 1024 * 1024,
            max_dimension: 4096,
        }
    }
}

/// Capture evidence: failed/deferred images are NOT successful readiness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceStatus {
    /// Not requested yet, including offscreen lazy images and admission waiters.
    Deferred,
    /// Fetch/decode or UI installation is outstanding.
    Queued,
    /// A validated texture is installed on the UI thread.
    Ready,
    /// Terminal failure; retry must be requested explicitly.
    Failed(String),
}

/// Exact image membership and status for the current reconciled document.
#[derive(Clone, Debug)]
pub struct ResourceSnapshot {
    /// Full stamps, selected sources, and statuses in stable key order.
    pub entries: Vec<(ElementStamp, Option<String>, ResourceStatus)>,
    /// Total bytes of currently installed RGBA textures.
    pub texture_bytes: usize,
    /// Admission failure invalidates the whole snapshot rather than appearing empty/ready.
    pub diagnostic: Option<String>,
}

impl ResourceSnapshot {
    /// True only if every image, including lazy ones, has an installed texture.
    #[must_use]
    pub fn all_ready(&self) -> bool {
        self.diagnostic.is_none()
            && self
                .entries
                .iter()
                .all(|(_, _, status)| *status == ResourceStatus::Ready)
    }
}

/// Resource evidence for exactly the host-supplied, post-frame image membership.
///
/// Omitted images remain in [`ResourceSnapshot`]. The host must derive required
/// stamps from current participating, clipped viewport-visible image geometry;
/// this type neither guesses visibility nor silently excludes failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneResourceReadiness {
    /// Current stamps and selected sources with installed textures.
    pub ready: Vec<(ElementStamp, String)>,
    /// Current stamps and selected sources awaiting admission or completion.
    pub pending: Vec<(ElementStamp, String)>,
    /// Required failures, including missing sources and inconsistent ready state.
    pub failed: Vec<(ElementStamp, Option<String>, String)>,
    /// Current images absent from reconciliation or not yet source-selected.
    pub unobserved: Vec<ElementStamp>,
    /// Stale document/entry generations or required nodes that are not images.
    pub stale: Vec<ElementStamp>,
    /// Global admission failure blocks even an otherwise empty required scene.
    pub diagnostic: Option<String>,
}

impl SceneResourceReadiness {
    /// True only when every supplied image has current installed resource evidence.
    /// An empty required scene is ready only when no global diagnostic exists.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.diagnostic.is_none()
            && self.pending.is_empty()
            && self.failed.is_empty()
            && self.unobserved.is_empty()
            && self.stale.is_empty()
    }
}

/// One admitted load. Keep this value alive through fetch/decode; it is the permit.
/// A resolver must cap streamed bytes at `max_encoded_bytes` and complete failures.
#[must_use = "an admitted image load must be completed or explicitly cancelled"]
pub struct LoadRequest {
    /// Full document generation to validate before texture installation.
    pub stamp: ElementStamp,
    /// Selected opaque source for the backend resolver.
    pub source: String,
    /// Maximum body bytes; enforce while fetching, not only after buffering.
    pub max_encoded_bytes: usize,
    ticket: u64,
    target: [u32; 2],
    limits: ResourceLimits,
    cancelled: Arc<AtomicBool>,
}

impl LoadRequest {
    /// Whether removal, replacement, retry, or source selection retired this job.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Decode fetched bytes away from the UI thread; never installs textures.
    #[must_use]
    pub fn decode(self, bytes: &[u8]) -> LoadCompletion {
        let result = if self.is_cancelled() {
            Err("image load cancelled".to_owned())
        } else {
            decode(bytes, self.target, self.limits)
        };
        LoadCompletion {
            request: self,
            result,
        }
    }

    /// Make a fetch/timeout/cancellation failure visible to capture and diagnostics.
    #[must_use]
    pub fn fail(self, message: impl Into<String>) -> LoadCompletion {
        LoadCompletion {
            request: self,
            result: Err(message.into()),
        }
    }
}

/// Decoded CPU data only; the private ticket cannot be forged by the resolver.
pub struct LoadCompletion {
    request: LoadRequest,
    result: Result<ColorImage, String>,
}

/// Backend-owned receiver/sender; clone for an existing async execution facility.
#[derive(Clone)]
pub struct ResourceWorker {
    requests: flume::Receiver<LoadRequest>,
    completions: flume::Sender<LoadCompletion>,
}

impl ResourceWorker {
    /// Nonblocking request admission; perform fetch/decode after returning to backend.
    #[allow(
        clippy::must_use_candidate,
        reason = "repository forbids function-level must_use on Option; LoadRequest itself is must_use"
    )]
    pub fn try_request(&self) -> Option<LoadRequest> {
        self.requests.try_recv().ok()
    }

    /// Submit a completion and wake the UI. Never waits for queue capacity.
    ///
    /// # Errors
    /// * Returns the completion if the UI is gone or its bounded queue is full.
    ///   Retain and retry on full; do not discard a live completion.
    pub fn complete(
        &self,
        completion: LoadCompletion,
        context: &egui::Context,
    ) -> Result<(), Box<LoadCompletion>> {
        self.completions
            .try_send(completion)
            .map_err(|error| Box::new(error.into_inner()))?;
        context.request_repaint();
        Ok(())
    }
}

struct Entry {
    stamp: ElementStamp,
    source: Option<String>,
    status: ResourceStatus,
    ticket: u64,
    cancelled: Option<Arc<AtomicBool>>,
    texture: Option<TextureHandle>,
    bytes: usize,
}

impl Entry {
    fn cancel(&mut self) {
        if let Some(cancelled) = self.cancelled.take() {
            cancelled.store(true, Ordering::Release);
        }
        self.texture = None;
        self.bytes = 0;
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Persistent image adapter. It must be reconciled before painting a document.
/// No orphan texture cache: removal/replacement drops its texture handle at once.
pub struct ImageResources {
    limits: ResourceLimits,
    entries: BTreeMap<ElementKey, Entry>,
    requests: flume::Sender<LoadRequest>,
    completions: flume::Receiver<LoadCompletion>,
    permits: Vec<Weak<AtomicBool>>,
    next_ticket: u64,
    diagnostic: Option<String>,
}

impl ImageResources {
    /// Create bounded queues. Zero budgets reject admission rather than unbounding.
    #[must_use]
    pub fn new(limits: ResourceLimits) -> (Self, ResourceWorker) {
        let (requests, receiver) = flume::bounded(limits.max_inflight.max(1));
        let (sender, completions) = flume::bounded(limits.max_inflight.max(1));
        (
            Self {
                limits,
                entries: BTreeMap::new(),
                requests,
                completions,
                permits: Vec::new(),
                next_ticket: 0,
                diagnostic: None,
            },
            ResourceWorker {
                requests: receiver,
                completions: sender,
            },
        )
    }

    /// Capture exact membership, failures, and installed texture memory.
    #[must_use]
    pub fn snapshot(&self) -> ResourceSnapshot {
        ResourceSnapshot {
            entries: self
                .entries
                .values()
                .map(|entry| (entry.stamp, entry.source.clone(), entry.status.clone()))
                .collect(),
            texture_bytes: self.entries.values().map(|entry| entry.bytes).sum(),
            diagnostic: self.diagnostic.clone(),
        }
    }

    /// Assess exactly the host's current post-frame required image stamps.
    ///
    /// This does not mutate loads or weaken global [`ResourceSnapshot::all_ready`].
    /// Unselected valid authored sources are unobserved, not ready. Missing sources
    /// are explicit failures, even if the node was never painted. Duplicate stamps
    /// are reported once, in their first supplied order. The caller must also keep
    /// document/geometry and required membership stable across capture frames.
    #[must_use]
    pub fn scene_readiness(
        &self,
        document: &Document,
        required: &[ElementStamp],
    ) -> SceneResourceReadiness {
        let mut result = SceneResourceReadiness {
            ready: Vec::new(),
            pending: Vec::new(),
            failed: Vec::new(),
            unobserved: Vec::new(),
            stale: Vec::new(),
            diagnostic: self.diagnostic.clone(),
        };
        for (index, &stamp) in required.iter().enumerate() {
            if required[..index].contains(&stamp) {
                continue;
            }
            if !document.is_current(stamp) {
                result.stale.push(stamp);
                continue;
            }
            let Some(Container {
                element:
                    Element::Image {
                        source, source_set, ..
                    },
                ..
            }) = document.get(stamp.key)
            else {
                result.stale.push(stamp);
                continue;
            };
            let entry = self.entries.get(&stamp.key);
            if entry.is_some_and(|entry| entry.stamp != stamp) {
                result.stale.push(stamp);
                continue;
            }
            if let Some(Entry {
                source,
                status: ResourceStatus::Failed(error),
                ..
            }) = entry
            {
                result.failed.push((stamp, source.clone(), error.clone()));
                continue;
            }
            let has_authored_source = source
                .as_deref()
                .is_some_and(|source| !source.trim().is_empty())
                || source_set
                    .as_deref()
                    .is_some_and(|set| !set.trim().is_empty());
            if !has_authored_source {
                result
                    .failed
                    .push((stamp, None, "image source missing".into()));
                continue;
            }
            let Some(entry) = entry else {
                result.unobserved.push(stamp);
                continue;
            };
            let Some(source) = entry
                .source
                .as_ref()
                .filter(|source| !source.trim().is_empty())
            else {
                result.unobserved.push(stamp);
                continue;
            };
            match &entry.status {
                ResourceStatus::Ready if entry.texture.is_some() => {
                    result.ready.push((stamp, source.clone()));
                }
                ResourceStatus::Ready => result.failed.push((
                    stamp,
                    Some(source.clone()),
                    "ready image texture missing".into(),
                )),
                ResourceStatus::Deferred | ResourceStatus::Queued => {
                    result.pending.push((stamp, source.clone()));
                }
                ResourceStatus::Failed(error) => {
                    result
                        .failed
                        .push((stamp, Some(source.clone()), error.clone()));
                }
            }
        }
        result
    }

    /// Explicit retry; no automatic infinite retry loop for broken artwork.
    /// Returns false if the key is not a current reconciled image.
    pub fn retry(&mut self, key: ElementKey) -> bool {
        let Some(entry) = self.entries.get_mut(&key) else {
            return false;
        };
        entry.cancel();
        entry.status = ResourceStatus::Deferred;
        true
    }

    /// UI-thread-only installation, before traversal. Validates both document
    /// generation and request ticket, including same-generation responsive changes.
    /// Returns number of accepted terminal completions (success OR failure).
    pub fn install_completions(&mut self, document: &Document, context: &egui::Context) -> usize {
        let mut accepted = 0;
        while let Ok(completion) = self.completions.try_recv() {
            let request = &completion.request;
            let used: usize = self.entries.values().map(|entry| entry.bytes).sum();
            let Some(entry) = self.entries.get_mut(&request.stamp.key) else {
                continue;
            };
            if !document.is_current(request.stamp)
                || entry.stamp != request.stamp
                || entry.ticket != request.ticket
                || request.is_cancelled()
                || entry.status != ResourceStatus::Queued
            {
                continue;
            }
            entry.cancelled = None;
            match completion.result {
                Ok(image) => {
                    let bytes = image.pixels.len().saturating_mul(4);
                    let max_side = context.input(|input| input.max_texture_side);
                    if image.size.iter().any(|side| *side > max_side) {
                        entry.status =
                            ResourceStatus::Failed("image exceeds GPU texture dimensions".into());
                    } else if bytes > self.limits.max_texture_bytes.saturating_sub(used) {
                        entry.status =
                            ResourceStatus::Failed("image texture budget exhausted".into());
                    } else {
                        entry.texture = Some(context.load_texture(
                            format!("hyperchad-image-{}", request.ticket),
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                        entry.bytes = bytes;
                        entry.status = ResourceStatus::Ready;
                    }
                }
                Err(error) => entry.status = ResourceStatus::Failed(error),
            }
            accepted += 1;
        }
        self.permits.retain(|permit| permit.strong_count() > 0);
        accepted
    }

    fn queue(&mut self, key: ElementKey, target: [u32; 2]) -> Result<(), String> {
        self.permits.retain(|permit| permit.strong_count() > 0);
        if self.permits.len() >= self.limits.max_inflight {
            return Ok(());
        }
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or("image was not reconciled")?;
        if entry.status != ResourceStatus::Deferred {
            return Ok(());
        }
        let Some(source) = entry.source.clone() else {
            return Ok(());
        };
        self.next_ticket = self
            .next_ticket
            .checked_add(1)
            .ok_or("image ticket exhausted")?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let request = LoadRequest {
            stamp: entry.stamp,
            source,
            max_encoded_bytes: self.limits.max_encoded_bytes,
            ticket: self.next_ticket,
            target,
            limits: self.limits,
            cancelled: cancelled.clone(),
        };
        match self.requests.try_send(request) {
            Ok(()) => {
                self.permits.push(Arc::downgrade(&cancelled));
                entry.cancelled = Some(cancelled);
                entry.ticket = self.next_ticket;
                entry.status = ResourceStatus::Queued;
            }
            Err(flume::TrySendError::Full(_)) => {}
            Err(flume::TrySendError::Disconnected(_)) => {
                entry.status = ResourceStatus::Failed("image worker disconnected".into());
            }
        }
        Ok(())
    }
}

impl WidgetAdapter for ImageResources {
    fn reconcile(
        &mut self,
        document: &Document,
        _mutations: &[MutationSummary],
    ) -> Result<(), String> {
        self.diagnostic = None;
        self.entries
            .retain(|_, entry| document.is_current(entry.stamp));
        let keys = document.keys();
        let count = keys
            .iter()
            .filter(|key| {
                document
                    .get(**key)
                    .is_some_and(|node| matches!(node.element, Element::Image { .. }))
            })
            .count();
        if count > self.limits.max_entries {
            // Refuse the entire snapshot: never claim partial image membership is ready.
            self.entries.clear();
            let diagnostic = format!(
                "image entry budget exceeded: {count} > {}",
                self.limits.max_entries
            );
            self.diagnostic = Some(diagnostic.clone());
            return Err(diagnostic);
        }
        for key in keys {
            let Some(node) = document.get(key) else {
                continue;
            };
            if !matches!(node.element, Element::Image { .. }) {
                continue;
            }
            let stamp = document.stamp(key).ok_or("image stamp missing")?;
            self.entries.entry(key).or_insert_with(|| Entry {
                stamp,
                source: None,
                status: ResourceStatus::Deferred,
                ticket: 0,
                cancelled: None,
                texture: None,
                bytes: 0,
            });
        }
        Ok(())
    }

    fn paint_node(
        &mut self,
        ui: &mut egui::Ui,
        stamp: ElementStamp,
        authored: &Container,
        _calculated: &Container,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<NodeOutput, String> {
        let Element::Image {
            source,
            source_set,
            sizes,
            loading,
            fit,
            alt,
        } = &authored.element
        else {
            return Err("image adapter received a non-image node".into());
        };
        let rect = geometry.content_box;
        let density = ui.ctx().pixels_per_point();
        let viewport = ui.ctx().content_rect().size();
        let slot = sizes.as_ref().map_or_else(
            || rect.width(),
            |size| size.calc(rect.width(), viewport.x, viewport.y),
        );
        let selected = select_source(source.as_deref(), source_set.as_deref(), slot, density);
        let entry = self
            .entries
            .get_mut(&stamp.key)
            .ok_or("image adapter must reconcile before painting")?;
        if entry.stamp != stamp || geometry.key != stamp.key {
            return Err("image stamp/geometry mismatch".into());
        }
        match selected {
            Ok(source) => {
                if entry.source.as_deref() != Some(&source) {
                    entry.cancel();
                    entry.source = Some(source);
                    entry.status = ResourceStatus::Deferred;
                }
            }
            Err(error) => {
                entry.cancel();
                entry.source = None;
                entry.status = ResourceStatus::Failed(error);
            }
        }
        let visible = geometry.visible_bounds().is_some() && rect.intersects(ui.clip_rect());
        let lazy = *loading == Some(ImageLoading::Lazy);
        if !lazy || visible {
            let target = raster_target(rect.size(), density, self.limits.max_dimension);
            match target {
                Ok(target) => self.queue(stamp.key, target)?,
                Err(error) => {
                    let entry = self
                        .entries
                        .get_mut(&stamp.key)
                        .ok_or("image state missing")?;
                    entry.cancel();
                    entry.status = ResourceStatus::Failed(error);
                }
            }
        }
        let entry = self.entries.get(&stamp.key).ok_or("image state missing")?;
        let response = ui.interact(
            rect,
            ui.id().with(("image", stamp.key)),
            egui::Sense::hover(),
        );
        if visible {
            let painter = ui.painter().with_clip_rect(
                geometry
                    .paint_clip
                    .intersect(ui.clip_rect())
                    .intersect(rect),
            );
            if let Some(texture) = &entry.texture {
                let (destination, uv) =
                    fitted(rect, texture.size_vec2() / density, fit.unwrap_or_default());
                painter.image(texture.id(), destination, uv, Color32::WHITE);
            } else {
                painter.rect_filled(rect, 0.0, Color32::from_gray(40));
                let label = match &entry.status {
                    ResourceStatus::Failed(error) => {
                        format!("{}: {error}", alt.as_deref().unwrap_or("Image failed"))
                    }
                    _ => alt.clone().unwrap_or_else(|| "Loading image…".into()),
                };
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(12.0),
                    Color32::LIGHT_GRAY,
                );
            }
        }
        if let ResourceStatus::Failed(error) = &entry.status {
            // Surface failure through FramePainter as well as the exact capture snapshot.
            return Err(format!("image {:?}: {error}", entry.source));
        }
        Ok(NodeOutput {
            response,
            value: None,
        })
    }

    fn pending_resources(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| {
                matches!(
                    entry.status,
                    ResourceStatus::Queued | ResourceStatus::Deferred
                )
            })
            .count()
    }
}

/// Responsive selection without performing I/O or depending on a native loader.
///
/// # Errors
/// * Missing source, invalid dimensions, malformed/mixed/duplicate descriptors.
pub fn select_source(
    source: Option<&str>,
    source_set: Option<&str>,
    slot: f32,
    density: f32,
) -> Result<String, String> {
    if !slot.is_finite() || slot < 0.0 || !density.is_finite() || density <= 0.0 {
        return Err("invalid responsive image dimensions".into());
    }
    let Some(set) = source_set.filter(|set| !set.trim().is_empty()) else {
        return source
            .filter(|source| !source.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "image source missing".into());
    };
    let mut candidates = Vec::new();
    let mut kind = None;
    for candidate in set.split(',') {
        let mut parts = candidate.split_whitespace();
        let url = parts.next().ok_or("empty srcset candidate")?;
        let descriptor = parts.next().unwrap_or("1x");
        if parts.next().is_some() {
            return Err("invalid srcset candidate".into());
        }
        let unit = descriptor.chars().last().ok_or("empty srcset descriptor")?;
        if !matches!(unit, 'w' | 'x') || kind.is_some_and(|kind| kind != unit) {
            return Err("mixed or invalid srcset descriptors".into());
        }
        kind = Some(unit);
        let value: f32 = descriptor[..descriptor.len() - 1]
            .parse()
            .map_err(|_| "invalid srcset size")?;
        if !value.is_finite() || value <= 0.0 || (unit == 'w' && value.fract() != 0.0) {
            return Err("invalid srcset size".into());
        }
        if candidates
            .iter()
            .any(|(existing, _): &(f32, &str)| existing.total_cmp(&value).is_eq())
        {
            return Err("duplicate srcset size".into());
        }
        candidates.push((value, url));
    }
    if kind == Some('x')
        && !candidates
            .iter()
            .any(|(value, _)| value.total_cmp(&1.0).is_eq())
        && let Some(source) = source.filter(|source| !source.trim().is_empty())
    {
        candidates.push((1.0, source));
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let target = if kind == Some('w') {
        slot * density
    } else {
        density
    };
    candidates
        .iter()
        .find(|(size, _)| *size >= target)
        .or_else(|| candidates.last())
        .map(|(_, url)| (*url).to_owned())
        .ok_or_else(|| "empty srcset".into())
}

// Convert physical raster dimensions only after rejecting invalid layout input.
// Work in f64 so a finite f32 size/density product cannot overflow; clamp BEFORE
// integer conversion. Clipping the target limits work, not authored layout size.
fn raster_target(size: Vec2, density: f32, maximum: u32) -> Result<[u32; 2], String> {
    if maximum == 0 || !density.is_finite() || density <= 0.0 {
        return Err("invalid image raster density or dimension budget".into());
    }
    let dimension = |logical: f32| {
        if !logical.is_finite() || logical < 0.0 {
            return Err("invalid image raster dimensions".to_owned());
        }
        let bounded = (f64::from(logical) * f64::from(density))
            .ceil()
            .clamp(1.0, f64::from(maximum));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "finite ceiled value clamped to inclusive 1..=u32 maximum before conversion"
        )]
        let pixels = bounded as u32;
        Ok(pixels)
    };
    Ok([dimension(size.x)?, dimension(size.y)?])
}

fn fitted(rect: Rect, intrinsic: Vec2, fit: ImageFit) -> (Rect, Rect) {
    let uv = Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    if !intrinsic.x.is_finite() || !intrinsic.y.is_finite() || intrinsic.min_elem() <= 0.0 {
        return (rect, uv);
    }
    match fit {
        ImageFit::Fill => (rect, uv),
        ImageFit::Cover => {
            let scale = (rect.size() / intrinsic).max_elem();
            let fraction = rect.size() / (intrinsic * scale);
            (rect, Rect::from_center_size(egui::pos2(0.5, 0.5), fraction))
        }
        ImageFit::None => (Rect::from_center_size(rect.center(), intrinsic), uv),
        ImageFit::Default | ImageFit::Contain => {
            let scale = (rect.size() / intrinsic).min_elem();
            (Rect::from_center_size(rect.center(), intrinsic * scale), uv)
        }
    }
}

fn decode(bytes: &[u8], target: [u32; 2], limits: ResourceLimits) -> Result<ColorImage, String> {
    if bytes.len() > limits.max_encoded_bytes {
        return Err("encoded image budget exceeded".into());
    }
    let svg = std::str::from_utf8(bytes).is_ok_and(|text| {
        text.trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with('<')
    });
    let image = if svg {
        // Resolve neither local files nor nested raster/data images from SVG.
        // The rasterizer caps the output regardless of authored SVG dimensions.
        let side = u32::try_from((limits.max_decoded_bytes / 4).isqrt())
            .unwrap_or(u32::MAX)
            .min(limits.max_dimension);
        if side == 0 {
            return Err("decoded image budget is zero".into());
        }
        let mut options = default_loader_options(egui_extras::image::load_svg_bytes_with_size);
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
        let target = target.map(|dimension| dimension.clamp(1, side));
        egui_extras::image::load_svg_bytes_with_size(
            bytes,
            egui::SizeHint::Size {
                width: target[0],
                height: target[1],
                maintain_aspect_ratio: true,
            },
            &options,
        )?
    } else {
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| error.to_string())?;
        let mut decoder_limits = image::Limits::default();
        decoder_limits.max_image_width = Some(limits.max_dimension);
        decoder_limits.max_image_height = Some(limits.max_dimension);
        decoder_limits.max_alloc = Some(limits.max_decoded_bytes as u64);
        reader.limits(decoder_limits);
        let image = reader
            .decode()
            .map_err(|error| error.to_string())?
            .into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        if size[0].saturating_mul(size[1]).saturating_mul(4) > limits.max_decoded_bytes {
            return Err("decoded image budget exceeded".into());
        }
        ColorImage::from_rgba_unmultiplied(size, image.as_raw())
    };
    if image.pixels.len().saturating_mul(4) > limits.max_decoded_bytes {
        return Err("decoded image budget exceeded".into());
    }
    Ok(image)
}

// Infer the transitive SVG options type through its public loader signature.
fn default_loader_options<T: Default>(
    _loader: impl Fn(&[u8], egui::SizeHint, &T) -> Result<ColorImage, String>,
) -> T {
    T::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ViewOrigin;
    use hyperchad_renderer::View;

    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="4"><rect width="8" height="4" fill="#ff0000"/></svg>"##;

    fn document() -> Document {
        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        element: Element::Image {
                            source: Some("fixture.svg".into()),
                            alt: None,
                            fit: None,
                            source_set: None,
                            sizes: None,
                            loading: None,
                        },
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        document
    }

    fn fixture(limits: ResourceLimits) -> (Document, ElementKey, ImageResources, ResourceWorker) {
        let document = document();
        let key = document.keys()[0];
        let (mut resources, worker) = ImageResources::new(limits);
        resources.reconcile(&document, &[]).unwrap();
        resources.entries.get_mut(&key).unwrap().source = Some("fixture.svg".into());
        (document, key, resources, worker)
    }

    fn submit(worker: &ResourceWorker, completion: LoadCompletion, context: &egui::Context) {
        assert!(worker.complete(completion, context).is_ok());
    }

    #[test]
    fn successful_svg_raster_and_texture_reclamation() {
        let (document, key, mut resources, worker) = fixture(ResourceLimits::default());
        let context = egui::Context::default();
        let baseline_textures = context.tex_manager().read().num_allocated();
        resources.queue(key, [8, 4]).unwrap();
        assert_eq!(resources.pending_resources(), 1);
        let completion = worker.try_request().unwrap().decode(SVG);
        assert!(completion.result.is_ok());
        submit(&worker, completion, &context);
        assert_eq!(resources.install_completions(&document, &context), 1);
        assert!(resources.snapshot().all_ready());
        assert_eq!(resources.snapshot().texture_bytes, 8 * 4 * 4);
        assert_eq!(
            context.tex_manager().read().num_allocated(),
            baseline_textures + 1
        );
        assert!(resources.retry(key));
        assert_eq!(
            context.tex_manager().read().num_allocated(),
            baseline_textures
        );
        assert_eq!(resources.snapshot().texture_bytes, 0);
        assert!(!resources.snapshot().all_ready());
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        resources.queue(key, [2, 1]).unwrap();
        submit(
            &worker,
            worker.try_request().unwrap().decode(png.get_ref()),
            &context,
        );
        assert_eq!(resources.install_completions(&document, &context), 1);
        assert_eq!(resources.snapshot().texture_bytes, 8);
        resources.reconcile(&Document::new(), &[]).unwrap();
        assert_eq!(resources.snapshot().texture_bytes, 0);
        assert!(resources.entries.is_empty());
    }

    #[test]
    fn failed_decode_fetch_and_retry_are_not_successful_readiness() {
        let (document, key, mut resources, worker) = fixture(ResourceLimits::default());
        let context = egui::Context::default();
        for bytes in [b"not an image".as_slice(), b"<svg invalid".as_slice()] {
            resources.retry(key);
            resources.queue(key, [8, 4]).unwrap();
            submit(
                &worker,
                worker.try_request().unwrap().decode(bytes),
                &context,
            );
            assert_eq!(resources.install_completions(&document, &context), 1);
            assert!(matches!(
                resources.entries[&key].status,
                ResourceStatus::Failed(_)
            ));
            assert_eq!(resources.pending_resources(), 0);
            assert!(!resources.snapshot().all_ready());
        }
        resources.retry(key);
        resources.queue(key, [8, 4]).unwrap();
        submit(
            &worker,
            worker.try_request().unwrap().fail("HTTP 404"),
            &context,
        );
        resources.install_completions(&document, &context);
        assert_eq!(
            resources.entries[&key].status,
            ResourceStatus::Failed("HTTP 404".into())
        );
    }

    #[test]
    fn stale_generation_and_cancelled_same_generation_cannot_install() {
        let (document, key, mut resources, worker) = fixture(ResourceLimits::default());
        let context = egui::Context::default();
        resources.queue(key, [8, 4]).unwrap();
        let old = worker.try_request().unwrap();
        resources.retry(key);
        assert!(old.is_cancelled());
        resources.queue(key, [8, 4]).unwrap();
        let current = worker.try_request().unwrap();
        submit(&worker, old.decode(SVG), &context);
        assert_eq!(resources.install_completions(&document, &context), 0);
        assert_eq!(resources.pending_resources(), 1);
        submit(&worker, current.decode(SVG), &context);
        // Even an otherwise current load is rejected against another document epoch.
        assert_eq!(resources.install_completions(&Document::new(), &context), 0);
        resources.reconcile(&Document::new(), &[]).unwrap();
        assert_eq!(resources.pending_resources(), 0);
        assert_eq!(resources.snapshot().texture_bytes, 0);
    }

    #[test]
    fn cancelled_jobs_retain_bounded_admission_until_worker_releases_them() {
        let limits = ResourceLimits {
            max_inflight: 1,
            ..ResourceLimits::default()
        };
        let (document, key, mut resources, worker) = fixture(limits);
        resources.queue(key, [8, 4]).unwrap();
        let old = worker.try_request().unwrap();
        resources.retry(key);
        resources.queue(key, [8, 4]).unwrap();
        assert!(worker.try_request().is_none());
        assert!(old.is_cancelled());
        drop(old);
        resources.queue(key, [8, 4]).unwrap();
        let current = worker.try_request().unwrap();
        let context = egui::Context::default();
        submit(&worker, current.decode(SVG), &context);
        resources.install_completions(&document, &context);
        assert!(resources.snapshot().all_ready());
    }

    #[test]
    fn memory_budgets_and_disconnected_worker_fail_explicitly() {
        let limits = ResourceLimits {
            max_texture_bytes: 1,
            ..ResourceLimits::default()
        };
        let (document, key, mut resources, worker) = fixture(limits);
        let context = egui::Context::default();
        resources.queue(key, [8, 4]).unwrap();
        submit(&worker, worker.try_request().unwrap().decode(SVG), &context);
        resources.install_completions(&document, &context);
        assert!(matches!(
            resources.entries[&key].status,
            ResourceStatus::Failed(_)
        ));
        assert_eq!(resources.snapshot().texture_bytes, 0);
        drop(worker);
        resources.retry(key);
        resources.queue(key, [8, 4]).unwrap();
        assert_eq!(
            resources.entries[&key].status,
            ResourceStatus::Failed("image worker disconnected".into())
        );
        assert!(
            decode(
                SVG,
                [8, 4],
                ResourceLimits {
                    max_encoded_bytes: 1,
                    ..limits
                }
            )
            .is_err()
        );
        assert!(
            decode(
                SVG,
                [8, 4],
                ResourceLimits {
                    max_decoded_bytes: 0,
                    ..limits
                }
            )
            .is_err()
        );
    }

    #[test]
    fn replaced_document_and_entry_budget_never_claim_readiness() {
        let (mut document, key, mut resources, worker) = fixture(ResourceLimits::default());
        resources.queue(key, [8, 4]).unwrap();
        let request = worker.try_request().unwrap();
        let stale_stamp = request.stamp;
        document
            .apply_view(
                View {
                    primary: Some(Container::default()),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        assert!(!document.is_current(stale_stamp));
        resources.reconcile(&document, &[]).unwrap();
        assert!(request.is_cancelled());
        let context = egui::Context::default();
        submit(&worker, request.decode(SVG), &context);
        assert_eq!(resources.install_completions(&document, &context), 0);
        assert!(resources.entries.is_empty());
        let (mut resources, _worker) = ImageResources::new(ResourceLimits {
            max_entries: 0,
            ..ResourceLimits::default()
        });
        assert!(resources.reconcile(&super::tests::document(), &[]).is_err());
        assert!(!resources.snapshot().all_ready());
        assert!(resources.snapshot().diagnostic.is_some());
    }

    #[test]
    fn lazy_paint_only_enqueues_when_visible_and_never_installs_texture() {
        let (document, key, mut resources, worker) = fixture(ResourceLimits::default());
        let mut authored = document.get(key).unwrap().clone();
        if let Element::Image { loading, .. } = &mut authored.element {
            *loading = Some(ImageLoading::Lazy);
        }
        let stamp = document.stamp(key).unwrap();
        let rect = Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(80.0, 40.0));
        let mut geometry = NodeGeometry {
            key,
            container_id: authored.id,
            parent: None,
            containing_block: None,
            margin_box: rect,
            padding_box: rect,
            content_box: rect,
            border_box: rect,
            paint_clip: rect,
            descendant_clip: rect,
            transform: crate::geometry::GeometryTransform {
                origin: rect.min,
                scale: 1.0,
            },
            scroll_extent: rect,
            scroll_max: Vec2::ZERO,
            scroll_offset: Vec2::ZERO,
            visible: false,
            fixed: false,
        };
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                assert!(
                    resources
                        .paint_node(ui, stamp, &authored, &authored, &geometry)
                        .is_ok()
                );
            });
        });
        assert!(worker.try_request().is_none());
        assert_eq!(resources.snapshot().entries[0].2, ResourceStatus::Deferred);
        geometry.visible = true;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                assert!(
                    resources
                        .paint_node(ui, stamp, &authored, &authored, &geometry)
                        .is_ok()
                );
            });
        });
        assert!(worker.try_request().is_some());
        assert_eq!(resources.snapshot().texture_bytes, 0);
        assert_eq!(resources.snapshot().entries[0].2, ResourceStatus::Queued);
    }

    fn scene_fixture(
        sources: &[(Option<&str>, Option<&str>)],
        limits: ResourceLimits,
    ) -> (Document, Vec<ElementStamp>, ImageResources, ResourceWorker) {
        let children = sources
            .iter()
            .map(|(source, source_set)| Container {
                element: Element::Image {
                    source: source.map(str::to_owned),
                    source_set: source_set.map(str::to_owned),
                    alt: None,
                    fit: None,
                    sizes: None,
                    loading: Some(ImageLoading::Lazy),
                },
                ..Container::default()
            })
            .collect();
        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        children,
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let stamps = document
            .keys()
            .into_iter()
            .filter_map(|key| {
                document
                    .get(key)
                    .filter(|node| matches!(node.element, Element::Image { .. }))?;
                document.stamp(key)
            })
            .collect();
        let (mut resources, worker) = ImageResources::new(limits);
        resources.reconcile(&document, &[]).unwrap();
        (document, stamps, resources, worker)
    }

    fn select_and_queue(resources: &mut ImageResources, stamp: ElementStamp, source: &str) {
        resources.entries.get_mut(&stamp.key).unwrap().source = Some(source.into());
        resources.queue(stamp.key, [8, 4]).unwrap();
    }

    #[test]
    fn scene_membership_tracks_hidden_reveal_scroll_without_weakening_snapshot() {
        let (document, stamps, mut resources, worker) = scene_fixture(
            &[(Some("first.svg"), None), (Some("hidden.svg"), None)],
            ResourceLimits::default(),
        );
        let context = egui::Context::default();
        // Host omits both hidden/offscreen images; global accounting retains them.
        assert!(resources.scene_readiness(&document, &[]).is_ready());
        assert!(!resources.snapshot().all_ready());
        assert_eq!(resources.snapshot().entries.len(), 2);
        assert_eq!(
            resources
                .scene_readiness(&document, &stamps[..1])
                .unobserved,
            stamps[..1]
        );
        select_and_queue(&mut resources, stamps[0], "first.svg");
        let pending = resources.scene_readiness(&document, &stamps[..1]);
        assert!(!pending.is_ready());
        assert_eq!(pending.pending, [(stamps[0], "first.svg".into())]);
        submit(&worker, worker.try_request().unwrap().decode(SVG), &context);
        resources.install_completions(&document, &context);
        let first = resources.scene_readiness(&document, &stamps[..1]);
        assert!(first.is_ready());
        assert_eq!(first.ready, [(stamps[0], "first.svg".into())]);
        assert!(!resources.snapshot().all_ready());
        // Revealing/scrolling the second image into view blocks until its load finishes.
        assert!(!resources.scene_readiness(&document, &stamps).is_ready());
        assert_eq!(
            resources
                .scene_readiness(&document, &stamps[1..])
                .unobserved,
            stamps[1..]
        );
        select_and_queue(&mut resources, stamps[1], "hidden.svg");
        submit(&worker, worker.try_request().unwrap().decode(SVG), &context);
        resources.install_completions(&document, &context);
        assert!(
            resources
                .scene_readiness(&document, &stamps[1..])
                .is_ready()
        );
        assert!(resources.snapshot().all_ready());
        // Duplicate host membership must not duplicate evidence.
        assert_eq!(
            resources
                .scene_readiness(&document, &[stamps[0]; 2])
                .ready
                .len(),
            1
        );
    }

    #[test]
    fn scene_required_broken_and_missing_sources_are_explicit_failures() {
        let (document, stamps, mut resources, worker) = scene_fixture(
            &[
                (Some("broken.svg"), None),
                (None, None),
                (Some("  "), Some("  ")),
            ],
            ResourceLimits::default(),
        );
        let before = resources.scene_readiness(&document, &stamps);
        assert_eq!(before.unobserved, stamps[..1]);
        assert_eq!(before.failed.len(), 2);
        assert!(!before.is_ready());
        select_and_queue(&mut resources, stamps[0], "broken.svg");
        let context = egui::Context::default();
        submit(
            &worker,
            worker.try_request().unwrap().fail("HTTP 404"),
            &context,
        );
        resources.install_completions(&document, &context);
        let broken = resources.scene_readiness(&document, &stamps);
        assert_eq!(broken.failed.len(), 3);
        assert_eq!(
            broken.failed[0],
            (stamps[0], Some("broken.svg".into()), "HTTP 404".into())
        );
        assert!(!broken.is_ready());
        // Exclusion is supplied by actual host visibility, never inferred from failure.
        assert!(resources.scene_readiness(&document, &[]).is_ready());
        assert_eq!(
            resources.snapshot().entries[0].2,
            ResourceStatus::Failed("HTTP 404".into())
        );
    }

    #[test]
    fn scene_srcset_only_needs_selected_source_and_installed_texture() {
        let (document, stamps, mut resources, worker) = scene_fixture(
            &[(None, Some("small.svg 1x, retina.svg 2x"))],
            ResourceLimits::default(),
        );
        let stamp = stamps[0];
        assert_eq!(
            resources.scene_readiness(&document, &stamps).unobserved,
            stamps
        );
        let selected = select_source(None, Some("small.svg 1x, retina.svg 2x"), 8.0, 2.0).unwrap();
        select_and_queue(&mut resources, stamp, &selected);
        let context = egui::Context::default();
        submit(&worker, worker.try_request().unwrap().decode(SVG), &context);
        resources.install_completions(&document, &context);
        assert_eq!(
            resources.scene_readiness(&document, &stamps).ready,
            [(stamp, "retina.svg".into())]
        );
        assert!(resources.scene_readiness(&document, &stamps).is_ready());
        resources.entries.get_mut(&stamp.key).unwrap().texture = None;
        let inconsistent = resources.scene_readiness(&document, &stamps);
        assert!(!inconsistent.is_ready());
        assert_eq!(inconsistent.failed[0].2, "ready image texture missing");
    }

    #[test]
    fn scene_stale_missing_reconciliation_and_global_budgets_block() {
        let (mut document, stamps, mut resources, worker) =
            scene_fixture(&[(Some("old.svg"), None)], ResourceLimits::default());
        select_and_queue(&mut resources, stamps[0], "old.svg");
        let request = worker.try_request().unwrap();
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        element: Element::Image {
                            source: Some("new.svg".into()),
                            alt: None,
                            fit: None,
                            source_set: None,
                            sizes: None,
                            loading: None,
                        },
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let current = document.stamp(document.keys()[0]).unwrap();
        assert_eq!(resources.scene_readiness(&document, &stamps).stale, stamps);
        assert_eq!(
            resources.scene_readiness(&document, &[current]).unobserved,
            [current]
        );
        resources.reconcile(&document, &[]).unwrap();
        assert!(request.is_cancelled());
        let context = egui::Context::default();
        submit(&worker, request.decode(SVG), &context);
        assert_eq!(resources.install_completions(&document, &context), 0);
        assert!(!resources.scene_readiness(&document, &[current]).is_ready());
        let (mut limited, _worker) = ImageResources::new(ResourceLimits {
            max_entries: 0,
            ..ResourceLimits::default()
        });
        assert!(limited.reconcile(&document, &[]).is_err());
        assert!(limited.scene_readiness(&document, &[]).diagnostic.is_some());
        assert!(!limited.scene_readiness(&document, &[]).is_ready());
    }

    #[test]
    fn scene_texture_budget_failure_is_not_pending_or_ready() {
        let (document, stamps, mut resources, worker) = scene_fixture(
            &[(Some("large.svg"), None)],
            ResourceLimits {
                max_texture_bytes: 1,
                ..ResourceLimits::default()
            },
        );
        select_and_queue(&mut resources, stamps[0], "large.svg");
        let context = egui::Context::default();
        submit(&worker, worker.try_request().unwrap().decode(SVG), &context);
        resources.install_completions(&document, &context);
        let scene = resources.scene_readiness(&document, &stamps);
        assert!(!scene.is_ready());
        assert_eq!(scene.ready, [] as [(ElementStamp, String); 0]);
        assert_eq!(scene.pending, [] as [(ElementStamp, String); 0]);
        assert_eq!(scene.failed[0].2, "image texture budget exhausted");
    }

    #[test]
    fn raster_target_checks_invalid_inputs_rounding_and_hard_bounds() {
        assert_eq!(
            raster_target(egui::vec2(0.0, 0.25), 2.0, 4096).unwrap(),
            [1, 1]
        );
        assert_eq!(
            raster_target(egui::vec2(10.1, 20.5), 2.0, 4096).unwrap(),
            [21, 41]
        );
        assert_eq!(
            raster_target(Vec2::splat(f32::MAX), f32::MAX, 4096).unwrap(),
            [4096, 4096]
        );
        assert_eq!(
            raster_target(Vec2::splat(f32::MAX), f32::MAX, u32::MAX).unwrap(),
            [u32::MAX; 2]
        );
        assert_eq!(raster_target(Vec2::splat(10.0), 1.0, 1).unwrap(), [1; 2]);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            assert!(raster_target(egui::vec2(bad, 1.0), 1.0, 4096).is_err());
            assert!(raster_target(egui::vec2(1.0, bad), 1.0, 4096).is_err());
            assert!(raster_target(Vec2::splat(1.0), bad, 4096).is_err());
        }
        assert!(raster_target(Vec2::splat(1.0), 0.0, 4096).is_err());
        assert!(raster_target(Vec2::splat(1.0), 1.0, 0).is_err());
    }

    #[test]
    fn responsive_duplicates_use_exact_descriptor_values_not_epsilon() {
        assert!(select_source(None, Some("a 1x, b 1.0000000x"), 1.0, 1.0).is_err());
        assert_eq!(
            select_source(Some("base"), Some("a 1.000001x"), 1.0, 1.0).unwrap(),
            "base"
        );
        assert_eq!(
            select_source(None, Some("a 1x, b 1.000001x"), 1.0, 1.000_001).unwrap(),
            "b"
        );
        assert!(select_source(None, Some("a -0x"), 1.0, 1.0).is_err());
    }

    #[test]
    fn svg_side_budget_accepts_large_usize_without_truncation() {
        let limits = ResourceLimits {
            max_decoded_bytes: usize::MAX,
            max_dimension: 8,
            ..ResourceLimits::default()
        };
        assert_eq!(decode(SVG, [u32::MAX; 2], limits).unwrap().size, [8, 4]);
        assert!(
            decode(
                SVG,
                [8, 4],
                ResourceLimits {
                    max_dimension: 0,
                    ..limits
                }
            )
            .is_err()
        );
    }

    #[test]
    fn responsive_selection_and_fit() {
        assert_eq!(
            select_source(Some("base"), Some("small 100w, large 400w"), 150.0, 2.0).unwrap(),
            "large"
        );
        assert_eq!(
            select_source(Some("base"), Some("retina 2x"), 100.0, 1.0).unwrap(),
            "base"
        );
        assert_eq!(
            select_source(None, Some("a 1x, b 2x"), 100.0, 4.0).unwrap(),
            "b"
        );
        for invalid in [
            "a 0w",
            "a 1x,b 2w",
            "a NaNx",
            "a 1x,b 1x",
            "a 1.5w",
            "a 1x extra",
        ] {
            assert!(
                select_source(None, Some(invalid), 100.0, 1.0).is_err(),
                "{invalid}"
            );
        }
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let intrinsic = egui::vec2(200.0, 100.0);
        let (contained, _) = fitted(rect, intrinsic, ImageFit::Contain);
        assert_eq!(contained.size(), egui::vec2(100.0, 50.0));
        let (covered, uv) = fitted(rect, intrinsic, ImageFit::Cover);
        assert_eq!(covered, rect);
        assert_eq!(uv.size(), egui::vec2(0.5, 1.0));
    }
}
