//! Bounded, synchronous graphics resources and canvas rasterization.
//!
//! This module performs no network requests or terminal output. A runtime authorizes
//! sources, obtains bounded encoded bytes, and registers them here. Scene payloads
//! own their bytes: callers must also bound the number of outstanding frames.

// Integer casts below follow explicit dimension/byte checks or clamped bounds.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::PathBuf,
};

use bmux_tui::{
    geometry::Rect,
    image::{
        ImageBounds, ImageFit, ImageKey, ImageLifecycle, ImagePayload, ImagePixelFormat,
        ImagePlacement,
    },
};
use hyperchad_renderer::canvas::{CanvasAction, CanvasUpdate, Pos};

/// Resource and work budgets. Zero budgets deliberately disable the operation.
#[derive(Debug, Clone, Copy)]
pub struct GraphicsLimits {
    /// Maximum width or height in pixels.
    pub max_dimension: u32,
    /// Maximum bytes of one RGBA resource (also the decoder allocation budget).
    pub max_resource_bytes: usize,
    /// Maximum total resident RGBA bytes.
    pub max_total_bytes: usize,
    /// Maximum resident entries, including canvas state.
    pub max_entries: usize,
    /// Maximum encoded input length.
    pub max_encoded_bytes: usize,
    /// Maximum identity length in UTF-8 bytes.
    pub max_key_bytes: usize,
    /// Maximum actions in one atomic canvas update.
    pub max_actions: usize,
    /// Maximum pixel visits across one update (including clears).
    pub max_pixel_visits: u64,
}

impl Default for GraphicsLimits {
    fn default() -> Self {
        Self {
            max_dimension: 4096,
            max_resource_bytes: 16 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_entries: 128,
            max_encoded_bytes: 8 * 1024 * 1024,
            max_key_bytes: 1024,
            max_actions: 4096,
            max_pixel_visits: 64 * 1024 * 1024,
        }
    }
}

/// Invalid input, policy rejection, or an exhausted explicit budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsError {
    /// Zero/oversized dimensions, malformed RGBA length, or arithmetic overflow.
    InvalidSize,
    /// Empty or oversized identity.
    InvalidKey,
    /// An input, storage, allocation, or work budget was exceeded.
    Limit,
    /// Resource does not exist (possibly evicted).
    Missing,
    /// Resource is not a canvas.
    NotCanvas,
    /// Coordinates or stroke width are nonfinite, or stroke width is negative.
    InvalidAction,
    /// Source is not authorized.
    Unauthorized,
    /// Image decoding failed or the codec is unavailable.
    Decode(String),
}
impl std::fmt::Display for GraphicsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for GraphicsError {}

/// Explicit source capability. Registration of application-owned RGBA needs none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsSource {
    /// Bytes already held by the application; not a data URL.
    Memory,
    /// Local file; authorization resolves symlinks before checking roots.
    File(PathBuf),
    /// An absolute HTTPS URL. Redirect targets must be authorized separately.
    Https(String),
}

/// Deny-by-default policy; never follows redirects or reads image files itself.
#[derive(Debug, Clone, Default)]
pub struct SourcePolicy {
    /// Allow encoded bytes supplied by the application.
    pub allow_memory: bool,
    /// Canonical existing directory roots. All components must be under a root.
    pub file_roots: BTreeSet<PathBuf>,
    /// Exact serialized HTTPS origins, e.g. `https://images.example`.
    pub https_origins: BTreeSet<String>,
}
impl SourcePolicy {
    /// Authorize a source, returning its normalized identity.
    ///
    /// # Errors
    /// Returns `Unauthorized` for invalid URLs, credentials, unlisted origins,
    /// nonexistent files/roots, nonabsolute paths, or paths outside allowed roots.
    /// A runtime must avoid file TOCTOU races and reauthorize every redirect.
    pub fn authorize(&self, source: &GraphicsSource) -> Result<String, GraphicsError> {
        match source {
            GraphicsSource::Memory if self.allow_memory => Ok("memory".to_owned()),
            GraphicsSource::File(path) if path.is_absolute() => {
                let path = path
                    .canonicalize()
                    .map_err(|_| GraphicsError::Unauthorized)?;
                if !path.is_file() {
                    return Err(GraphicsError::Unauthorized);
                }
                for root in &self.file_roots {
                    if let Ok(root) = root.canonicalize()
                        && root.is_dir()
                        && path.starts_with(root)
                    {
                        return path
                            .to_str()
                            .map(str::to_owned)
                            .ok_or(GraphicsError::Unauthorized);
                    }
                }
                Err(GraphicsError::Unauthorized)
            }
            GraphicsSource::Https(raw) => {
                if raw.len() > 8192 {
                    return Err(GraphicsError::Unauthorized);
                }
                let mut url = url::Url::parse(raw).map_err(|_| GraphicsError::Unauthorized)?;
                if url.scheme() != "https"
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || !self
                        .https_origins
                        .contains(&url.origin().ascii_serialization())
                {
                    return Err(GraphicsError::Unauthorized);
                }
                url.set_fragment(None);
                Ok(url.to_string())
            }
            _ => Err(GraphicsError::Unauthorized),
        }
    }
}

#[derive(Debug, Clone)]
struct CanvasState {
    color: [u8; 4],
    stroke: f32,
}
impl Default for CanvasState {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 255],
            stroke: 1.0,
        }
    }
}
#[derive(Debug, Clone)]
struct Resource {
    width: u32,
    height: u32,
    rgba: Box<[u8]>,
    canvas: Option<CanvasState>,
    touched: u64,
}

/// Image fitting. Cover crops a centered source rectangle before placement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GraphicsFit {
    /// Preserve aspect ratio, centered by the presenter using cell pixel metrics.
    #[default]
    Contain,
    /// Fill without preserving aspect ratio.
    Stretch,
    /// Preserve aspect ratio and crop centrally using explicit cell pixel metrics.
    Cover,
}

/// Placement result, including a deliberate non-graphics rendering path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsPlacement {
    /// Submit to the bmux image scene; runtime selects Kitty/Sixel/iTerm2.
    Image(ImagePlacement),
    /// Missing resource or unavailable graphics. Paint text only inside this rect.
    Fallback {
        /// Visible destination.
        rect: Rect,
        /// Application-provided alt text.
        text: String,
    },
    /// Fully clipped or empty allocation; paint nothing.
    Hidden,
}

/// Bounded LRU cache keyed by caller-owned stable resource identity.
///
/// Canvases participate in eviction. Keep live resources refreshed or handle
/// `Missing` by recreating/reloading; scene placement keys are independently stable.
///
/// # Examples
///
/// ```
/// use hyperchad_renderer_bmux::graphics::{GraphicsLimits, GraphicsStore};
/// use hyperchad_renderer::canvas::{CanvasAction, CanvasUpdate, Pos};
///
/// let mut graphics = GraphicsStore::new(GraphicsLimits::default());
/// graphics.create_canvas("plot", 64, 32)?;
/// graphics.update_canvas(&CanvasUpdate {
///     target: "plot".into(),
///     canvas_actions: vec![CanvasAction::Line(Pos(0.5, 0.5), Pos(63.5, 31.5))],
/// })?;
/// # Ok::<(), hyperchad_renderer_bmux::graphics::GraphicsError>(())
/// ```
#[derive(Debug)]
pub struct GraphicsStore {
    limits: GraphicsLimits,
    resources: BTreeMap<String, Resource>,
    bytes: usize,
    clock: u64,
}
impl GraphicsStore {
    /// Create an empty cache with explicit budgets.
    #[must_use]
    pub const fn new(limits: GraphicsLimits) -> Self {
        Self {
            limits,
            resources: BTreeMap::new(),
            bytes: 0,
            clock: 0,
        }
    }
    /// Resident RGBA bytes (excludes caller-owned scene payloads).
    #[must_use]
    pub const fn resident_bytes(&self) -> usize {
        self.bytes
    }
    /// Number of resident resources.
    #[must_use]
    pub fn len(&self) -> usize {
        self.resources.len()
    }
    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }
    /// Borrow RGBA bytes and dimensions without allocating or refreshing LRU.
    // Option already carries must_use; follow the repository API convention.
    #[allow(clippy::must_use_candidate)]
    pub fn rgba(&self, key: &str) -> Option<(u32, u32, &[u8])> {
        self.resources
            .get(key)
            .map(|r| (r.width, r.height, r.rgba.as_ref()))
    }
    /// Remove a resource. Existing caller-owned placements remain valid.
    pub fn remove(&mut self, key: &str) -> bool {
        self.resources.remove(key).is_some_and(|r| {
            self.bytes -= r.rgba.len();
            true
        })
    }
    const fn key(&self, key: &str) -> Result<(), GraphicsError> {
        if key.is_empty() || key.len() > self.limits.max_key_bytes {
            Err(GraphicsError::InvalidKey)
        } else {
            Ok(())
        }
    }
    fn size(&self, width: u32, height: u32) -> Result<usize, GraphicsError> {
        if width == 0
            || height == 0
            || width > self.limits.max_dimension
            || height > self.limits.max_dimension
        {
            return Err(GraphicsError::InvalidSize);
        }
        let size = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(GraphicsError::InvalidSize)?;
        if size > self.limits.max_resource_bytes
            || size > self.limits.max_total_bytes
            || self.limits.max_entries == 0
        {
            return Err(GraphicsError::Limit);
        }
        Ok(size)
    }
    fn tick(&mut self) -> u64 {
        if self.clock == u64::MAX {
            // Preserve relative ordering even after counter rollover.
            let mut order: Vec<_> = self
                .resources
                .iter()
                .map(|(k, r)| (r.touched, k.clone()))
                .collect();
            order.sort();
            for (i, (_, key)) in order.into_iter().enumerate() {
                self.resources.get_mut(&key).expect("existing key").touched = i as u64;
            }
            self.clock = self.resources.len() as u64;
        }
        self.clock += 1;
        self.clock
    }
    fn insert(&mut self, key: &str, mut resource: Resource) {
        self.remove(key);
        while self.resources.len() >= self.limits.max_entries
            || self.bytes > self.limits.max_total_bytes - resource.rgba.len()
        {
            let oldest = self
                .resources
                .iter()
                .min_by_key(|(_, r)| r.touched)
                .map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                self.remove(&oldest);
            } else {
                break;
            }
        }
        resource.touched = self.tick();
        self.bytes += resource.rgba.len();
        self.resources.insert(key.to_owned(), resource);
    }
    /// Register tightly packed straight-alpha RGBA8, replacing an existing key.
    ///
    /// # Errors
    /// Returns `InvalidKey`, `InvalidSize`, or `Limit`; failure leaves cache unchanged.
    pub fn register_rgba(
        &mut self,
        key: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<(), GraphicsError> {
        self.key(key)?;
        if self.size(width, height)? != rgba.len() {
            return Err(GraphicsError::InvalidSize);
        }
        self.insert(
            key,
            Resource {
                width,
                height,
                rgba: rgba.into_boxed_slice(),
                canvas: None,
                touched: 0,
            },
        );
        Ok(())
    }
    /// Authorize and decode bounded encoded bytes. Only the first frame is used.
    ///
    /// # Errors
    /// Returns policy, identity, input/dimension/allocation budget, or codec errors.
    /// Failure never replaces an existing resource. Dimensions and resulting RGBA
    /// bytes are strictly bounded; the image crate's scratch-allocation limit is
    /// best-effort for some codecs, not an isolation boundary. Untrusted decoding
    /// requiring a hard process-memory bound belongs in a sandboxed worker.
    pub fn decode(
        &mut self,
        key: &str,
        source: &GraphicsSource,
        policy: &SourcePolicy,
        encoded: &[u8],
    ) -> Result<(), GraphicsError> {
        self.key(key)?;
        policy.authorize(source)?;
        if encoded.len() > self.limits.max_encoded_bytes {
            return Err(GraphicsError::Limit);
        }
        let reader = || {
            let mut reader = image::ImageReader::new(Cursor::new(encoded))
                .with_guessed_format()
                .map_err(|e| GraphicsError::Decode(e.to_string()))?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(self.limits.max_dimension);
            limits.max_image_height = Some(self.limits.max_dimension);
            limits.max_alloc = Some(self.limits.max_resource_bytes as u64);
            reader.limits(limits);
            Ok::<_, GraphicsError>(reader)
        };
        let classify = |error: image::ImageError| match error {
            image::ImageError::Limits(_) => GraphicsError::Limit,
            other => GraphicsError::Decode(other.to_string()),
        };
        let (width, height) = reader()?.into_dimensions().map_err(classify)?;
        self.size(width, height)?;
        let decoded = reader()?.decode().map_err(classify)?;
        self.register_rgba(key, width, height, decoded.into_rgba8().into_raw())
    }
    /// Create/reset a transparent canvas with default black, one-pixel stroke.
    ///
    /// # Errors
    /// Returns identity, size, allocation, or resource budget errors without mutation.
    pub fn create_canvas(
        &mut self,
        key: &str,
        width: u32,
        height: u32,
    ) -> Result<(), GraphicsError> {
        self.key(key)?;
        let size = self.size(width, height)?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(size)
            .map_err(|_| GraphicsError::Limit)?;
        rgba.resize(size, 0);
        self.insert(
            key,
            Resource {
                width,
                height,
                rgba: rgba.into_boxed_slice(),
                canvas: Some(CanvasState::default()),
                touched: 0,
            },
        );
        Ok(())
    }
    /// Apply every existing `HyperChad` canvas action atomically, preserving style.
    /// Rectangles normalize reversed corners, use half-open pixel-center coverage;
    /// lines use round caps. Drawing uses straight-alpha source-over compositing.
    ///
    /// # Errors
    /// Returns `Missing`, `NotCanvas`, `InvalidAction`, or `Limit`. Invalid actions
    /// and work exhaustion leave both pixels and drawing state unchanged.
    pub fn update_canvas(&mut self, update: &CanvasUpdate) -> Result<(), GraphicsError> {
        if update.canvas_actions.len() > self.limits.max_actions {
            return Err(GraphicsError::Limit);
        }
        let original = self
            .resources
            .get(&update.target)
            .ok_or(GraphicsError::Missing)?;
        if original.canvas.is_none() {
            return Err(GraphicsError::NotCanvas);
        }
        // Preflight bounds/work before allocating or modifying the canvas.
        let mut state = original.canvas.clone().ok_or(GraphicsError::NotCanvas)?;
        let mut work = 0u64;
        for action in &update.canvas_actions {
            let bounds = action_bounds(action, &mut state, original.width, original.height)?;
            let visits = u64::from(bounds.2 - bounds.0) * u64::from(bounds.3 - bounds.1);
            work = work.checked_add(visits).ok_or(GraphicsError::Limit)?;
            if work > self.limits.max_pixel_visits {
                return Err(GraphicsError::Limit);
            }
        }
        let touched = self.tick();
        let resource = self
            .resources
            .get_mut(&update.target)
            .ok_or(GraphicsError::Missing)?;
        for action in &update.canvas_actions {
            raster(resource, action);
        }
        resource.touched = touched;
        Ok(())
    }
    /// Generate a scene placement from resolved geometry, never computing layout.
    /// `cell_pixels` is required for cover cropping (actual terminal cell aspect).
    /// Missing resources or disabled graphics return caller-supplied alt text.
    ///
    /// # Errors
    /// Returns `InvalidKey` for an invalid placement identity, `InvalidAction` for
    /// invalid cell metrics, or `Limit` on allocation or oversized alt text (bounded
    /// by `max_key_bytes`).
    /// Output payload bytes are bounded by `max_resource_bytes` per placement.
    #[allow(clippy::too_many_arguments)]
    pub fn placement(
        &mut self,
        resource: &str,
        placement_key: &str,
        rect: Rect,
        clip: Rect,
        fit: GraphicsFit,
        cell_pixels: (f64, f64),
        graphics: bool,
        alt: &str,
    ) -> Result<GraphicsPlacement, GraphicsError> {
        self.key(placement_key)?;
        let visible = intersect(rect, clip);
        if visible.width == 0 || visible.height == 0 {
            return Ok(GraphicsPlacement::Hidden);
        }
        if !graphics || !self.resources.contains_key(resource) {
            if alt.len() > self.limits.max_key_bytes {
                return Err(GraphicsError::Limit);
            }
            return Ok(GraphicsPlacement::Fallback {
                rect: visible,
                text: alt.to_owned(),
            });
        }
        if !cell_pixels.0.is_finite()
            || !cell_pixels.1.is_finite()
            || cell_pixels.0 <= 0.0
            || cell_pixels.1 <= 0.0
        {
            return Err(GraphicsError::InvalidAction);
        }
        let touched = self.tick();
        let source = self
            .resources
            .get_mut(resource)
            .ok_or(GraphicsError::Missing)?;
        source.touched = touched;
        let (width, height, bytes) = if fit == GraphicsFit::Cover {
            let aspect =
                (f64::from(rect.width) / f64::from(rect.height)) * (cell_pixels.0 / cell_pixels.1);
            if !aspect.is_finite() || aspect <= 0.0 {
                return Err(GraphicsError::InvalidAction);
            }
            let mut width = source.width;
            let mut height = source.height;
            if f64::from(width) / f64::from(height) > aspect {
                width = (f64::from(height) * aspect)
                    .round()
                    .clamp(1.0, f64::from(width)) as u32;
            } else {
                height = (f64::from(width) / aspect)
                    .round()
                    .clamp(1.0, f64::from(height)) as u32;
            }
            let x = (source.width - width) / 2;
            let y = (source.height - height) / 2;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(width as usize * height as usize * 4)
                .map_err(|_| GraphicsError::Limit)?;
            for row in y..y + height {
                let start = (row as usize * source.width as usize + x as usize) * 4;
                bytes.extend_from_slice(&source.rgba[start..start + width as usize * 4]);
            }
            (width, height, bytes)
        } else {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(source.rgba.len())
                .map_err(|_| GraphicsError::Limit)?;
            bytes.extend_from_slice(&source.rgba);
            (source.width, source.height, bytes)
        };
        Ok(GraphicsPlacement::Image(ImagePlacement {
            key: ImageKey::new(placement_key),
            payload: ImagePayload::Pixels {
                bytes,
                width,
                height,
                format: ImagePixelFormat::Rgba8,
            },
            destination: visible,
            allocation: Some(ImageBounds::from(rect)),
            clip: visible,
            fit: if fit == GraphicsFit::Contain {
                ImageFit::Contain
            } else {
                ImageFit::Stretch
            },
            lifecycle: ImageLifecycle::Frame,
        }))
    }
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let x = u32::from(a.x).max(u32::from(b.x));
    let y = u32::from(a.y).max(u32::from(b.y));
    let right = (u32::from(a.x) + u32::from(a.width))
        .min(u32::from(b.x) + u32::from(b.width))
        .min(u32::from(u16::MAX));
    let bottom = (u32::from(a.y) + u32::from(a.height))
        .min(u32::from(b.y) + u32::from(b.height))
        .min(u32::from(u16::MAX));
    Rect {
        x: x as u16,
        y: y as u16,
        width: right.saturating_sub(x) as u16,
        height: bottom.saturating_sub(y) as u16,
    }
}

type Bounds = (u32, u32, u32, u32);
fn action_bounds(
    action: &CanvasAction,
    state: &mut CanvasState,
    width: u32,
    height: u32,
) -> Result<Bounds, GraphicsError> {
    let empty = (0, 0, 0, 0);
    let (a, b, pad) = match action {
        CanvasAction::StrokeSize(size) => {
            if !size.is_finite() || *size < 0.0 {
                return Err(GraphicsError::InvalidAction);
            }
            state.stroke = *size;
            return Ok(empty);
        }
        CanvasAction::StrokeColor(c) => {
            state.color = [c.r, c.g, c.b, c.a.unwrap_or(255)];
            return Ok(empty);
        }
        CanvasAction::Clear => return Ok((0, 0, width, height)),
        CanvasAction::Line(a, b) => {
            if ![a.0, a.1, b.0, b.1].iter().all(|n| n.is_finite()) {
                return Err(GraphicsError::InvalidAction);
            }
            if state.stroke == 0.0 {
                return Ok(empty);
            }
            (*a, *b, f64::from(state.stroke) / 2.0)
        }
        CanvasAction::FillRect(a, b) | CanvasAction::ClearRect(a, b) => (*a, *b, 0.0),
    };
    if ![a.0, a.1, b.0, b.1].iter().all(|n| n.is_finite()) {
        return Err(GraphicsError::InvalidAction);
    }
    let x0 = (f64::from(a.0.min(b.0)) - pad)
        .floor()
        .clamp(0.0, f64::from(width)) as u32;
    let y0 = (f64::from(a.1.min(b.1)) - pad)
        .floor()
        .clamp(0.0, f64::from(height)) as u32;
    let x1 = (f64::from(a.0.max(b.0)) + pad)
        .ceil()
        .clamp(0.0, f64::from(width)) as u32;
    let y1 = (f64::from(a.1.max(b.1)) + pad)
        .ceil()
        .clamp(0.0, f64::from(height)) as u32;
    Ok((x0, y0, x1, y1))
}
fn raster(resource: &mut Resource, action: &CanvasAction) {
    let state = resource.canvas.as_mut().expect("canvas checked");
    let (x0, y0, x1, y1) =
        action_bounds(action, state, resource.width, resource.height).expect("preflight checked");
    for y in y0..y1 {
        for x in x0..x1 {
            let px = f64::from(x) + 0.5;
            let py = f64::from(y) + 0.5;
            let covered = match action {
                CanvasAction::Line(a, b) => {
                    if state.stroke == 0.0 {
                        false
                    } else {
                        let dx = f64::from(b.0) - f64::from(a.0);
                        let dy = f64::from(b.1) - f64::from(a.1);
                        let length = dx.hypot(dy);
                        let distance = if length == 0.0 {
                            (px - f64::from(a.0)).hypot(py - f64::from(a.1))
                        } else {
                            // Midpoint coordinates avoid losing small pixel offsets
                            // when a huge segment crosses the visible canvas.
                            let qx = px - f64::midpoint(f64::from(a.0), f64::from(b.0));
                            let qy = py - f64::midpoint(f64::from(a.1), f64::from(b.1));
                            let ux = dx / length;
                            let uy = dy / length;
                            let perpendicular = qy.mul_add(-ux, qx * uy);
                            let beyond_cap =
                                ((qy.mul_add(uy, qx * ux)).abs() - length / 2.0).max(0.0);
                            perpendicular.hypot(beyond_cap)
                        };
                        distance <= f64::from(state.stroke) / 2.0
                    }
                }
                CanvasAction::FillRect(Pos(ax, ay), Pos(bx, by))
                | CanvasAction::ClearRect(Pos(ax, ay), Pos(bx, by)) => {
                    px >= f64::from(ax.min(*bx))
                        && px < f64::from(ax.max(*bx))
                        && py >= f64::from(ay.min(*by))
                        && py < f64::from(ay.max(*by))
                }
                CanvasAction::Clear => true,
                _ => false,
            };
            if covered {
                let i = (y as usize * resource.width as usize + x as usize) * 4;
                let pixel = &mut resource.rgba[i..i + 4];
                if matches!(action, CanvasAction::Clear | CanvasAction::ClearRect(..)) {
                    pixel.fill(0);
                } else {
                    blend(pixel, state.color);
                }
            }
        }
    }
}
fn blend(dst: &mut [u8], src: [u8; 4]) {
    let sa = u32::from(src[3]);
    let da = u32::from(dst[3]);
    let alpha = sa * 255 + da * (255 - sa);
    if alpha == 0 {
        dst.fill(0);
        return;
    }
    for c in 0..3 {
        let n = u32::from(src[c]) * sa * 255 + u32::from(dst[c]) * da * (255 - sa);
        dst[c] = ((n + alpha / 2) / alpha) as u8;
    }
    dst[3] = ((alpha + 127) / 255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperchad_renderer::Color;

    fn store() -> GraphicsStore {
        GraphicsStore::new(GraphicsLimits::default())
    }
    fn update(store: &mut GraphicsStore, actions: Vec<CanvasAction>) -> Result<(), GraphicsError> {
        store.update_canvas(&CanvasUpdate {
            target: "canvas".into(),
            canvas_actions: actions,
        })
    }
    fn pixel(store: &GraphicsStore, x: usize, y: usize) -> &[u8] {
        let (width, _, bytes) = store.rgba("canvas").unwrap();
        &bytes[(y * width as usize + x) * 4..(y * width as usize + x) * 4 + 4]
    }
    fn red(alpha: u8) -> CanvasAction {
        CanvasAction::StrokeColor(Color {
            r: 255,
            g: 0,
            b: 0,
            a: Some(alpha),
        })
    }
    fn rect() -> Rect {
        Rect {
            x: 2,
            y: 3,
            width: 4,
            height: 2,
        }
    }

    #[test]
    fn rejects_huge_zero_overflow_and_wrong_lengths_without_replacing() {
        let mut s = store();
        s.register_rgba("a", 1, 1, vec![1; 4]).unwrap();
        for (w, h) in [(0, 1), (1, 0), (u32::MAX, u32::MAX), (4097, 1)] {
            assert!(s.create_canvas("a", w, h).is_err());
        }
        assert_eq!(
            s.register_rgba("a", 1, 1, vec![0; 3]),
            Err(GraphicsError::InvalidSize)
        );
        assert_eq!(
            s.register_rgba("", 1, 1, vec![0; 4]),
            Err(GraphicsError::InvalidKey)
        );
        assert_eq!(s.rgba("a").unwrap().2, &[1; 4]);
        let mut s = GraphicsStore::new(GraphicsLimits {
            max_dimension: u32::MAX,
            max_resource_bytes: usize::MAX,
            max_total_bytes: usize::MAX,
            ..GraphicsLimits::default()
        });
        assert_eq!(
            s.create_canvas("huge", u32::MAX, u32::MAX),
            Err(GraphicsError::InvalidSize)
        );
    }

    #[test]
    fn cache_bounds_replacement_removal_and_lru() {
        let mut s = GraphicsStore::new(GraphicsLimits {
            max_total_bytes: 8,
            max_entries: 2,
            ..GraphicsLimits::default()
        });
        for key in ["a", "b"] {
            s.register_rgba(key, 1, 1, vec![0; 4]).unwrap();
        }
        s.placement(
            "a",
            "scene",
            rect(),
            rect(),
            GraphicsFit::Contain,
            (8.0, 16.0),
            true,
            "",
        )
        .unwrap();
        s.register_rgba("c", 1, 1, vec![0; 4]).unwrap();
        assert!(s.rgba("a").is_some());
        assert!(s.rgba("b").is_none());
        assert_eq!(s.resident_bytes(), 8);
        assert_eq!(s.len(), 2);
        s.register_rgba("a", 2, 1, vec![1; 8]).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s.resident_bytes(), 8);
        assert!(s.remove("a"));
        assert!(!s.remove("a"));
        assert!(s.is_empty());
        assert_eq!(s.resident_bytes(), 0);
    }

    #[test]
    fn disabled_cache_never_evicts_on_failure() {
        for limits in [
            GraphicsLimits {
                max_entries: 0,
                ..GraphicsLimits::default()
            },
            GraphicsLimits {
                max_total_bytes: 3,
                ..GraphicsLimits::default()
            },
        ] {
            let mut s = GraphicsStore::new(limits);
            assert_eq!(s.create_canvas("canvas", 1, 1), Err(GraphicsError::Limit));
            assert!(s.is_empty());
        }
    }

    #[test]
    fn all_canvas_actions_and_style_persistence() {
        let mut s = store();
        s.create_canvas("canvas", 4, 4).unwrap();
        update(
            &mut s,
            vec![
                red(255),
                CanvasAction::FillRect(Pos(3.0, 3.0), Pos(0.0, 0.0)),
            ],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[255, 0, 0, 255]);
        assert_eq!(pixel(&s, 3, 3), &[0; 4]);
        update(
            &mut s,
            vec![CanvasAction::ClearRect(Pos(1.0, 1.0), Pos(2.0, 2.0))],
        )
        .unwrap();
        assert_eq!(pixel(&s, 1, 1), &[0; 4]);
        update(
            &mut s,
            vec![
                CanvasAction::Clear,
                CanvasAction::StrokeSize(1.0),
                CanvasAction::Line(Pos(0.5, 0.5), Pos(3.5, 3.5)),
            ],
        )
        .unwrap();
        for i in 0..4 {
            assert_eq!(pixel(&s, i, i), &[255, 0, 0, 255]);
        }
        assert_eq!(pixel(&s, 3, 0), &[0; 4]);
        update(&mut s, vec![CanvasAction::Clear]).unwrap();
        assert!(s.rgba("canvas").unwrap().2.iter().all(|b| *b == 0));
        update(
            &mut s,
            vec![CanvasAction::Line(Pos(0.5, 0.5), Pos(0.5, 0.5))],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[255, 0, 0, 255]);
        s.create_canvas("canvas", 2, 2).unwrap();
        update(
            &mut s,
            vec![CanvasAction::FillRect(Pos(0.0, 0.0), Pos(1.0, 1.0))],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[0, 0, 0, 255]);
    }

    #[test]
    fn alpha_and_fractional_clipped_rectangles() {
        let mut s = store();
        s.create_canvas("canvas", 2, 2).unwrap();
        update(
            &mut s,
            vec![
                red(128),
                CanvasAction::FillRect(Pos(-1e30, -1e30), Pos(1.5, 1.5)),
            ],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[255, 0, 0, 128]);
        assert_eq!(pixel(&s, 1, 0), &[0; 4]);
        update(
            &mut s,
            vec![
                CanvasAction::StrokeColor(Color {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: Some(128),
                }),
                CanvasAction::FillRect(Pos(0.0, 0.0), Pos(1.0, 1.0)),
            ],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[85, 0, 170, 192]);
        update(
            &mut s,
            vec![red(0), CanvasAction::FillRect(Pos(0.0, 0.0), Pos(2.0, 2.0))],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[85, 0, 170, 192]);
    }

    #[test]
    fn invalid_actions_are_atomic_including_style() {
        let mut s = store();
        s.create_canvas("canvas", 2, 2).unwrap();
        for invalid in [
            CanvasAction::StrokeSize(f32::NAN),
            CanvasAction::StrokeSize(-1.0),
            CanvasAction::StrokeSize(f32::INFINITY),
            CanvasAction::Line(Pos(f32::NAN, 0.0), Pos(1.0, 1.0)),
            CanvasAction::FillRect(Pos(0.0, 0.0), Pos(f32::INFINITY, 1.0)),
            CanvasAction::ClearRect(Pos(0.0, f32::NEG_INFINITY), Pos(1.0, 1.0)),
        ] {
            assert_eq!(
                update(
                    &mut s,
                    vec![
                        red(255),
                        CanvasAction::FillRect(Pos(0.0, 0.0), Pos(2.0, 2.0)),
                        invalid
                    ]
                ),
                Err(GraphicsError::InvalidAction)
            );
            assert!(s.rgba("canvas").unwrap().2.iter().all(|b| *b == 0));
        }
        update(
            &mut s,
            vec![CanvasAction::FillRect(Pos(0.0, 0.0), Pos(2.0, 2.0))],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[0, 0, 0, 255]);
    }

    #[test]
    fn bounded_work_and_action_count_are_atomic() {
        let mut s = GraphicsStore::new(GraphicsLimits {
            max_pixel_visits: 3,
            max_actions: 2,
            ..GraphicsLimits::default()
        });
        s.create_canvas("canvas", 2, 2).unwrap();
        assert_eq!(
            update(&mut s, vec![CanvasAction::Clear]),
            Err(GraphicsError::Limit)
        );
        assert_eq!(
            update(&mut s, vec![red(0), red(1), red(2)]),
            Err(GraphicsError::Limit)
        );
        assert!(s.rgba("canvas").unwrap().2.iter().all(|b| *b == 0));
        assert_eq!(
            s.update_canvas(&CanvasUpdate {
                target: "missing".into(),
                canvas_actions: vec![]
            }),
            Err(GraphicsError::Missing)
        );
        s.register_rgba("canvas", 1, 1, vec![0; 4]).unwrap();
        assert_eq!(update(&mut s, vec![]), Err(GraphicsError::NotCanvas));
    }

    #[test]
    fn huge_finite_lines_are_bounded_and_zero_stroke_is_noop() {
        let mut s = store();
        s.create_canvas("canvas", 2, 2).unwrap();
        update(
            &mut s,
            vec![
                CanvasAction::StrokeSize(f32::MAX),
                CanvasAction::Line(Pos(-f32::MAX, -f32::MAX), Pos(f32::MAX, f32::MAX)),
            ],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[0, 0, 0, 255]);
        update(
            &mut s,
            vec![
                CanvasAction::Clear,
                CanvasAction::StrokeSize(1.0),
                CanvasAction::Line(Pos(-f32::MAX, -f32::MAX), Pos(f32::MAX, f32::MAX)),
            ],
        )
        .unwrap();
        assert_eq!(pixel(&s, 0, 0), &[0, 0, 0, 255]);
        assert_eq!(pixel(&s, 1, 0), &[0; 4]);
        update(
            &mut s,
            vec![
                CanvasAction::Clear,
                CanvasAction::StrokeSize(0.0),
                CanvasAction::Line(Pos(0.0, 0.0), Pos(2.0, 2.0)),
            ],
        )
        .unwrap();
        assert!(s.rgba("canvas").unwrap().2.iter().all(|b| *b == 0));
    }

    #[test]
    fn policy_is_exact_origin_default_deny_and_normalizes_fragments() {
        let mut policy = SourcePolicy::default();
        assert_eq!(
            policy.authorize(&GraphicsSource::Memory),
            Err(GraphicsError::Unauthorized)
        );
        policy.https_origins.insert("https://example.com".into());
        for url in [
            "http://example.com/a",
            "https://example.com.evil/a",
            "https://example.com:8443/a",
            "https://user@example.com/a",
            "https://example.com@evil/a",
            "file:///etc/passwd",
            "data:image/png;base64,a",
        ] {
            assert_eq!(
                policy.authorize(&GraphicsSource::Https(url.into())),
                Err(GraphicsError::Unauthorized)
            );
        }
        assert_eq!(
            policy
                .authorize(&GraphicsSource::Https("https://EXAMPLE.com:443/a#x".into()))
                .unwrap(),
            "https://example.com/a"
        );
        assert_eq!(
            policy.authorize(&GraphicsSource::File(PathBuf::from("relative"))),
            Err(GraphicsError::Unauthorized)
        );
        policy.allow_memory = true;
        assert!(policy.authorize(&GraphicsSource::Memory).is_ok());
    }

    #[test]
    fn file_policy_uses_canonical_component_roots() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let file = root.join("src/graphics.rs");
        let mut policy = SourcePolicy::default();
        assert_eq!(
            policy.authorize(&GraphicsSource::File(file.clone())),
            Err(GraphicsError::Unauthorized)
        );
        policy.file_roots.insert(root.join("src"));
        assert!(policy.authorize(&GraphicsSource::File(file)).is_ok());
        assert_eq!(
            policy.authorize(&GraphicsSource::File(root.join("Cargo.toml"))),
            Err(GraphicsError::Unauthorized)
        );
        assert_eq!(
            policy.authorize(&GraphicsSource::File(root.join("src"))),
            Err(GraphicsError::Unauthorized)
        );
    }

    #[test]
    fn decode_png_and_reject_broken_and_oversized_encoded_data() {
        let mut s = store();
        let policy = SourcePolicy {
            allow_memory: true,
            ..SourcePolicy::default()
        };
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(2, 1, vec![255; 8]).unwrap())
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        s.decode("png", &GraphicsSource::Memory, &policy, encoded.get_ref())
            .unwrap();
        assert_eq!(s.rgba("png").unwrap(), (2, 1, &[255; 8][..]));
        assert!(
            s.decode("png", &GraphicsSource::Memory, &policy, b"not an image")
                .is_err()
        );
        assert_eq!(s.rgba("png").unwrap().2, &[255; 8]);
        assert_eq!(
            s.decode(
                "png",
                &GraphicsSource::Memory,
                &SourcePolicy::default(),
                encoded.get_ref()
            ),
            Err(GraphicsError::Unauthorized)
        );
        let mut small = GraphicsStore::new(GraphicsLimits {
            max_encoded_bytes: 1,
            ..GraphicsLimits::default()
        });
        assert_eq!(
            small.decode("png", &GraphicsSource::Memory, &policy, encoded.get_ref()),
            Err(GraphicsError::Limit)
        );
        let mut small = GraphicsStore::new(GraphicsLimits {
            max_resource_bytes: 4,
            ..GraphicsLimits::default()
        });
        assert_eq!(
            small.decode("png", &GraphicsSource::Memory, &policy, encoded.get_ref()),
            Err(GraphicsError::Limit)
        );
    }

    #[test]
    fn scene_clipping_retains_allocation_and_stable_key() {
        let mut s = store();
        s.register_rgba("a", 1, 1, vec![255; 4]).unwrap();
        let clip = Rect {
            x: 3,
            y: 3,
            width: 1,
            height: 1,
        };
        let GraphicsPlacement::Image(p) = s
            .placement(
                "a",
                "stable",
                rect(),
                clip,
                GraphicsFit::Contain,
                (8.0, 16.0),
                true,
                "alt",
            )
            .unwrap()
        else {
            panic!("image expected");
        };
        assert_eq!(p.destination, clip);
        assert_eq!(p.clip, clip);
        assert_eq!(p.allocation, Some(ImageBounds::from(rect())));
        assert_eq!(p.key.as_str(), "stable");
        assert_eq!(p.fit, ImageFit::Contain);
        assert_eq!(p.lifecycle, ImageLifecycle::Frame);
        assert_eq!(
            s.placement(
                "a",
                "stable",
                rect(),
                Rect::default(),
                GraphicsFit::Contain,
                (8.0, 16.0),
                true,
                "alt"
            )
            .unwrap(),
            GraphicsPlacement::Hidden
        );
        assert_eq!(
            s.placement(
                "missing",
                "stable",
                rect(),
                clip,
                GraphicsFit::Stretch,
                (8.0, 16.0),
                true,
                "alt"
            )
            .unwrap(),
            GraphicsPlacement::Fallback {
                rect: clip,
                text: "alt".into()
            }
        );
        assert!(matches!(
            s.placement(
                "a",
                "stable",
                rect(),
                clip,
                GraphicsFit::Stretch,
                (8.0, 16.0),
                false,
                "alt"
            )
            .unwrap(),
            GraphicsPlacement::Fallback { .. }
        ));
    }

    #[test]
    fn cover_crop_uses_cell_aspect_and_clip_does_not_recrop() {
        let mut s = store();
        // Four distinguishable horizontal pixels; square destination in pixels
        // keeps the two center source columns.
        let bytes: Vec<_> = (0u8..4).flat_map(|i| [i, i, i, 255]).collect();
        s.register_rgba("a", 4, 1, bytes).unwrap();
        let clip = Rect {
            x: 3,
            y: 3,
            width: 1,
            height: 1,
        };
        let GraphicsPlacement::Image(p) = s
            .placement(
                "a",
                "stable",
                rect(),
                clip,
                GraphicsFit::Cover,
                (1.0, 1.0),
                true,
                "",
            )
            .unwrap()
        else {
            panic!("image expected");
        };
        assert_eq!(p.fit, ImageFit::Stretch);
        assert_eq!(p.allocation, Some(ImageBounds::from(rect())));
        let ImagePayload::Pixels {
            width,
            height,
            bytes,
            ..
        } = p.payload
        else {
            panic!("pixels expected");
        };
        assert_eq!((width, height), (2, 1));
        assert_eq!(bytes, [1, 1, 1, 255, 2, 2, 2, 255]);
        for metrics in [
            (f64::NAN, 1.0),
            (1.0, f64::INFINITY),
            (0.0, 1.0),
            (-1.0, 1.0),
            (f64::MAX, f64::MIN_POSITIVE),
        ] {
            assert_eq!(
                s.placement(
                    "a",
                    "stable",
                    rect(),
                    rect(),
                    GraphicsFit::Cover,
                    metrics,
                    true,
                    ""
                ),
                Err(GraphicsError::InvalidAction)
            );
        }
    }

    #[test]
    fn clipping_near_cell_coordinate_limit_does_not_overflow() {
        let a = Rect {
            x: 65530,
            y: 65530,
            width: 20,
            height: 20,
        };
        let b = Rect {
            x: 65532,
            y: 65532,
            width: 20,
            height: 20,
        };
        assert_eq!(
            intersect(a, b),
            Rect {
                x: 65532,
                y: 65532,
                width: 3,
                height: 3
            }
        );
        assert_eq!(intersect(rect(), Rect::default()).width, 0);
    }

    #[test]
    fn lru_rollover_preserves_order() {
        let mut s = GraphicsStore::new(GraphicsLimits {
            max_entries: 2,
            ..GraphicsLimits::default()
        });
        for key in ["a", "b"] {
            s.register_rgba(key, 1, 1, vec![0; 4]).unwrap();
        }
        s.clock = u64::MAX;
        s.placement(
            "a",
            "a",
            rect(),
            rect(),
            GraphicsFit::Contain,
            (1.0, 1.0),
            true,
            "",
        )
        .unwrap();
        s.register_rgba("c", 1, 1, vec![0; 4]).unwrap();
        assert!(s.rgba("a").is_some());
        assert!(s.rgba("b").is_none());
    }
}
