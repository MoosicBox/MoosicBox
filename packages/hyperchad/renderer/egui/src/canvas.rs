//! Ordered, UI-owned canvas state and offscreen compositing.
//!
//! Integration contract: the backend must enqueue `CanvasCommand` in the SAME
//! typed mailbox as document commands and call `WidgetAdapter::apply_canvas` while
//! draining it, in admission order, before painting. This adapter forwards to
//! `apply_command`; no second queue is involved. Enqueue success is not application success;
//! the backend must surface application errors. Do not use a separate canvas queue.
//! `Update` resolves an exact (not CSS) string ID at application time. `Stamped`
//! additionally guards delayed producers against replacement. Missing IDs, duplicate
//! IDs (including non-canvas nodes), wrong element kinds and stale stamps are errors.
//!
//! All six `CanvasAction` variants are handled exhaustively. Each Line is a fresh
//! butt-capped segment (there is no retained path in the protocol). StrokeColor
//! controls both lines and fills, alpha is an optional BYTE, and Clear preserves
//! drawing style. Rectangles use opposite corners, including reversed corners.
//! Clears erase only the canvas's own transparent bitmap, never underlying widgets.
//! Geometry supplies uniform zoom, translation and ancestor clipping; DPI affects
//! raster resolution, not command coordinates. Resize/DPI changes replay the exact
//! ordered history, without compaction. Replacement retires history and textures.
//!
//! Raster edges use deterministic 4x4 coverage sampling, not browser-identical
//! antialiasing. No asynchronous resources or I/O are involved. A native visual
//! fixture still requires the backend ingress/frame integration.

use std::collections::BTreeMap;

use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle, TextureOptions};
use hyperchad_renderer::canvas::{CanvasAction, CanvasUpdate, Pos};
use hyperchad_transformer::{Container, Element};

use crate::{
    backend::NodeOutput,
    document::{Document, ElementKey, ElementStamp, MutationSummary},
    geometry::NodeGeometry,
    widgets::WidgetAdapter,
};

const MAX_PIXELS: usize = 4_194_304;
const MAX_ACTIONS: usize = 1_000_000;

/// Payload for the backend's ordered UI mailbox; not a separate transport.
#[derive(Clone, Debug)]
pub enum CanvasCommand {
    /// Resolve the current target when this command reaches the UI thread.
    Update(CanvasUpdate),
    /// Reject if the target no longer has this complete document stamp.
    Stamped {
        stamp: ElementStamp,
        update: CanvasUpdate,
    },
}

/// Persistent canvas state. Construct on the UI side and install as `WidgetPainter`'s C.
#[derive(Default)]
pub struct CanvasAdapter {
    states: BTreeMap<ElementKey, CanvasState>,
}

struct CanvasState {
    stamp: ElementStamp,
    actions: Vec<CanvasAction>,
    texture: Option<TextureHandle>,
    raster_key: Option<([usize; 2], [u32; 2])>,
    raster: Option<Raster>,
    applied: usize,
    dirty: bool,
}

impl CanvasAdapter {
    /// Construct an empty adapter; no context or GPU resources are needed yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one command on the UI thread, atomically with respect to validation.
    /// Returns the resolved identity for diagnostics/acknowledgement.
    ///
    /// # Errors
    /// * Missing/ambiguous/non-canvas target or stale explicit stamp.
    /// * Nonfinite coordinates, invalid stroke width or retained-history limit.
    pub fn apply_command(
        &mut self,
        document: &Document,
        command: CanvasCommand,
    ) -> Result<ElementStamp, String> {
        let (expected, update) = match command {
            CanvasCommand::Update(update) => (None, update),
            CanvasCommand::Stamped { stamp, update } => (Some(stamp), update),
        };
        let matches: Vec<_> = document
            .keys()
            .into_iter()
            .filter(|key| {
                document
                    .get(*key)
                    .is_some_and(|node| node.str_id.as_deref() == Some(update.target.as_str()))
            })
            .collect();
        let [key] = matches.as_slice() else {
            return Err(format!(
                "canvas target {:?}: expected one exact ID, found {}",
                update.target,
                matches.len()
            ));
        };
        if !document
            .get(*key)
            .is_some_and(|node| matches!(node.element, Element::Canvas))
        {
            return Err(format!("canvas target {:?} is not a canvas", update.target));
        }
        let stamp = document.stamp(*key).ok_or("canvas target lost its stamp")?;
        if expected.is_some_and(|expected| expected != stamp) {
            return Err(format!("stale canvas command for {:?}", update.target));
        }
        validate_actions(&update.canvas_actions)?;
        let retained = self
            .states
            .get(key)
            .filter(|state| state.stamp == stamp)
            .map_or(0, |state| state.actions.len());
        if retained.saturating_add(update.canvas_actions.len()) > MAX_ACTIONS {
            return Err("canvas retained-history limit exceeded (no actions applied)".into());
        }
        let state = self
            .states
            .entry(*key)
            .or_insert_with(|| CanvasState::new(stamp));
        if state.stamp != stamp {
            *state = CanvasState::new(stamp);
        }
        state.dirty |= !update.canvas_actions.is_empty();
        state.actions.extend(update.canvas_actions);
        Ok(stamp)
    }
}

impl CanvasState {
    const fn new(stamp: ElementStamp) -> Self {
        Self {
            stamp,
            actions: Vec::new(),
            texture: None,
            raster_key: None,
            raster: None,
            applied: 0,
            dirty: true,
        }
    }
}

impl WidgetAdapter for CanvasAdapter {
    fn apply_canvas(
        &mut self,
        document: &Document,
        command: CanvasCommand,
    ) -> Result<ElementStamp, String> {
        self.apply_command(document, command)
    }

    fn reconcile(
        &mut self,
        document: &Document,
        _mutations: &[MutationSummary],
    ) -> Result<(), String> {
        self.states.retain(|key, state| {
            document.is_current(state.stamp)
                && document
                    .get(*key)
                    .is_some_and(|node| matches!(node.element, Element::Canvas))
        });
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
        if !matches!(authored.element, Element::Canvas) || geometry.key != stamp.key {
            return Err("canvas adapter received a non-canvas node or mismatched geometry".into());
        }
        let rect = geometry.content_box;
        let scale = geometry.transform.scale;
        let dpi = ui.ctx().pixels_per_point();
        if !rect.is_finite() || !scale.is_finite() || scale <= 0.0 || !dpi.is_finite() || dpi <= 0.0
        {
            return Err("invalid canvas geometry/DPI".into());
        }
        let clip = rect
            .intersect(geometry.paint_clip)
            .intersect(ui.clip_rect());
        let response = ui.interact(
            if geometry.visible {
                clip
            } else {
                Rect::NOTHING
            },
            ui.id().with(("canvas", stamp.key)),
            egui::Sense::hover(),
        );
        if !geometry.visible || !rect.is_positive() || !clip.is_positive() {
            return Ok(NodeOutput {
                response,
                value: None,
            });
        }
        let size = [
            raster_side(f64::from(rect.width() * dpi))?,
            raster_side(f64::from(rect.height() * dpi))?,
        ];
        if size[0]
            .checked_mul(size[1])
            .is_none_or(|pixels| pixels > MAX_PIXELS)
        {
            return Err("canvas raster exceeds 4194304 pixels".into());
        }
        let max_side = ui.ctx().input(|input| input.max_texture_side);
        if size.into_iter().any(|side| side == 0 || side > max_side) {
            return Err(format!(
                "canvas raster exceeds native texture limit {max_side}"
            ));
        }
        let logical = [rect.width() / scale, rect.height() / scale];
        if logical
            .into_iter()
            .any(|side| !side.is_finite() || side <= 0.0)
        {
            return Err("canvas logical dimensions must be finite and positive".into());
        }
        let raster_key = (size, [logical[0].to_bits(), logical[1].to_bits()]);
        let state = self
            .states
            .entry(stamp.key)
            .or_insert_with(|| CanvasState::new(stamp));
        if state.stamp != stamp {
            *state = CanvasState::new(stamp);
        }
        if state.dirty || state.raster_key != Some(raster_key) {
            if state.raster_key != Some(raster_key) {
                state.raster = Some(Raster::render(size, logical, &state.actions));
            } else if let Some(raster) = &mut state.raster {
                raster.apply(&state.actions[state.applied..]);
            }
            let raster = state.raster.as_ref().ok_or("canvas raster missing")?;
            state.applied = state.actions.len();
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &raster.rgba);
            if let Some(texture) = &mut state.texture {
                texture.set(image, TextureOptions::LINEAR);
            } else {
                state.texture = Some(ui.ctx().load_texture(
                    format!("hyperchad-canvas-{:?}", stamp.key),
                    image,
                    TextureOptions::LINEAR,
                ));
            }
            state.raster_key = Some(raster_key);
            state.dirty = false;
        }
        if let Some(texture) = &state.texture {
            ui.painter().with_clip_rect(clip).image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        Ok(NodeOutput {
            response,
            value: None,
        })
    }

    fn pending_resources(&self) -> usize {
        0
    }
}

fn validate_actions(actions: &[CanvasAction]) -> Result<(), String> {
    for action in actions {
        let valid = match action {
            CanvasAction::StrokeSize(width) => width.is_finite() && *width > 0.0,
            CanvasAction::StrokeColor(_) | CanvasAction::Clear => true,
            CanvasAction::Line(a, b)
            | CanvasAction::FillRect(a, b)
            | CanvasAction::ClearRect(a, b) => [a.0, a.1, b.0, b.1].into_iter().all(f32::is_finite),
        };
        if !valid {
            return Err(format!("invalid canvas action: {action:?}"));
        }
    }
    Ok(())
}

// Every production raster side is checked before allocation. Use a checked u32
// bridge: all u32 integers are represented exactly by f64, unlike arbitrary usize.
fn raster_coordinate(index: usize) -> f64 {
    f64::from(u32::try_from(index).expect("raster coordinate must fit u32"))
}

fn raster_side(pixels: f64) -> Result<usize, String> {
    let rounded = pixels.ceil();
    if !rounded.is_finite() || !(1.0..=raster_coordinate(MAX_PIXELS)).contains(&rounded) {
        return Err("canvas raster side must be within 1..=4194304 pixels".into());
    }
    Ok(raster_index(rounded, MAX_PIXELS))
}

fn raster_index(value: f64, limit: usize) -> usize {
    // limit is checked to fit u32 by raster_coordinate. Bounds are floor/ceil
    // results (or raster_side's ceil), so conversion discards no fractional bits.
    // Finite canvas f32 inputs promoted to f64 cannot overflow our intermediates;
    // clipping also permits +/-infinity here, but never NaN.
    assert!(!value.is_nan(), "raster bound must not be NaN");
    let clipped = value.clamp(0.0, raster_coordinate(limit));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bounded = clipped as u32; // integral, nonnegative and <= checked u32 limit
    usize::try_from(bounded).expect("clipped raster index cannot exceed usize limit")
}

fn quantize_byte(value: f64) -> u8 {
    // Source-over is a convex combination of bytes and alpha stays in [0, 1].
    // Clamp possible floating-point roundoff, then explicitly round to a byte.
    assert!(value.is_finite(), "composited channel must be finite");
    let rounded = value.clamp(0.0, 255.0).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        rounded as u8 // integral and proven within 0..=255 above
    }
}

struct Raster {
    size: [usize; 2],
    logical: [f32; 2],
    // Straight (unpremultiplied) RGBA. Texture upload performs egui conversion.
    rgba: Vec<u8>,
    color: [u8; 4],
    width: f32,
}

impl Raster {
    fn render(size: [usize; 2], logical: [f32; 2], actions: &[CanvasAction]) -> Self {
        let mut raster = Self {
            size,
            logical,
            rgba: vec![0; size[0] * size[1] * 4],
            color: [0, 0, 0, 255],
            width: 1.0,
        };
        raster.apply(actions);
        raster
    }

    fn apply(&mut self, actions: &[CanvasAction]) {
        for action in actions {
            match *action {
                CanvasAction::StrokeSize(value) => self.width = value,
                CanvasAction::StrokeColor(value) => {
                    self.color = [value.r, value.g, value.b, value.a.unwrap_or(255)];
                }
                CanvasAction::Clear => self.rgba.fill(0),
                CanvasAction::FillRect(a, b) => self.rectangle(a, b, Some(self.color)),
                CanvasAction::ClearRect(a, b) => self.rectangle(a, b, None),
                CanvasAction::Line(a, b) => self.line(a, b, self.width, self.color),
            }
        }
    }

    fn rectangle(&mut self, a: Pos, b: Pos, color: Option<[u8; 4]>) {
        let bounds = [
            f64::from(a.0.min(b.0)),
            f64::from(a.1.min(b.1)),
            f64::from(a.0.max(b.0)),
            f64::from(a.1.max(b.1)),
        ];
        self.cover(bounds, color, |x, y| {
            x >= bounds[0] && x < bounds[2] && y >= bounds[1] && y < bounds[3]
        });
    }

    fn line(&mut self, a: Pos, b: Pos, width: f32, color: [u8; 4]) {
        // Use f64 so finite f32 endpoints/width cannot overflow intermediates.
        let (ax, ay, bx, by) = (
            f64::from(a.0),
            f64::from(a.1),
            f64::from(b.0),
            f64::from(b.1),
        );
        let (dx, dy) = (bx - ax, by - ay);
        let length = dx.hypot(dy);
        if length == 0.0 {
            return;
        }
        let radius = f64::from(width) / 2.0;
        let bounds = [
            ax.min(bx) - radius,
            ay.min(by) - radius,
            ax.max(bx) + radius,
            ay.max(by) + radius,
        ];
        self.cover(bounds, Some(color), |x, y| {
            let along = (y - ay).mul_add(dy, (x - ax) * dx) / length;
            let across = (y - ay).mul_add(-dx, (x - ax) * dy).abs() / length;
            along >= 0.0 && along < length && across < radius
        });
    }

    fn cover(
        &mut self,
        bounds: [f64; 4],
        color: Option<[u8; 4]>,
        inside: impl Fn(f64, f64) -> bool,
    ) {
        let sx = raster_coordinate(self.size[0]) / f64::from(self.logical[0]);
        let sy = raster_coordinate(self.size[1]) / f64::from(self.logical[1]);
        let x0 = raster_index((bounds[0] * sx).floor(), self.size[0]);
        let y0 = raster_index((bounds[1] * sy).floor(), self.size[1]);
        let x1 = raster_index((bounds[2] * sx).ceil(), self.size[0]);
        let y1 = raster_index((bounds[3] * sy).ceil(), self.size[1]);
        for y in y0..y1 {
            for x in x0..x1 {
                let mut samples = 0_u8;
                for j in 0..4 {
                    for i in 0..4 {
                        if inside(
                            (raster_coordinate(x) + (f64::from(i) + 0.5) / 4.0) / sx,
                            (raster_coordinate(y) + (f64::from(j) + 0.5) / 4.0) / sy,
                        ) {
                            samples += 1;
                        }
                    }
                }
                if samples == 0 {
                    continue;
                }
                let offset = (y * self.size[0] + x) * 4;
                composite(
                    &mut self.rgba[offset..offset + 4],
                    color,
                    f64::from(samples) / 16.0,
                );
            }
        }
    }
}

fn composite(pixel: &mut [u8], color: Option<[u8; 4]>, coverage: f64) {
    let dest_alpha = f64::from(pixel[3]) / 255.0;
    let Some(color) = color else {
        pixel[3] = quantize_byte(dest_alpha * (1.0 - coverage) * 255.0);
        if pixel[3] == 0 {
            pixel.fill(0);
        }
        return;
    };
    let alpha = f64::from(color[3]) / 255.0 * coverage;
    let output_alpha = dest_alpha.mul_add(1.0 - alpha, alpha);
    if output_alpha == 0.0 {
        pixel.fill(0);
        return;
    }
    for channel in 0..3 {
        pixel[channel] = quantize_byte(
            (f64::from(pixel[channel]) * dest_alpha)
                .mul_add(1.0 - alpha, f64::from(color[channel]) * alpha)
                / output_alpha,
        );
    }
    pixel[3] = quantize_byte(output_alpha * 255.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ViewOrigin;
    use hyperchad_color::Color;
    use hyperchad_renderer::View;

    fn color(r: u8, g: u8, b: u8, a: Option<u8>) -> CanvasAction {
        CanvasAction::StrokeColor(Color { r, g, b, a })
    }

    fn pixel(raster: &Raster, x: usize, y: usize) -> &[u8] {
        let start = (y * raster.size[0] + x) * 4;
        &raster.rgba[start..start + 4]
    }

    #[test]
    fn raster_conversions_enforce_bounds_before_casting() {
        assert_eq!(raster_side(0.25).unwrap(), 1);
        assert_eq!(raster_side(4_194_304.0).unwrap(), MAX_PIXELS);
        for invalid in [0.0, -1.0, 4_194_304.01, f64::INFINITY, f64::NAN] {
            assert!(raster_side(invalid).is_err());
        }
        assert_eq!(raster_index(f64::NEG_INFINITY, MAX_PIXELS), 0);
        assert_eq!(raster_index(f64::INFINITY, MAX_PIXELS), MAX_PIXELS);
        assert_eq!(raster_index(4_194_304.0, MAX_PIXELS), MAX_PIXELS);
        assert_eq!(
            raster_coordinate(MAX_PIXELS).to_bits(),
            4_194_304.0_f64.to_bits()
        );
        assert_eq!(quantize_byte(-f64::EPSILON), 0);
        assert_eq!(quantize_byte(255.0 + f64::EPSILON), 255);
        assert_eq!(quantize_byte(127.5), 128);
    }

    #[test]
    fn partial_clear_preserves_overlapping_shapes_outside_clear() {
        let raster = Raster::render(
            [8, 4],
            [8.0, 4.0],
            &[
                color(255, 0, 0, None),
                CanvasAction::FillRect(Pos(0.0, 0.0), Pos(8.0, 4.0)),
                color(0, 0, 255, None),
                CanvasAction::FillRect(Pos(2.0, 0.0), Pos(6.0, 4.0)),
                CanvasAction::ClearRect(Pos(3.0, 1.0), Pos(5.0, 3.0)),
            ],
        );
        assert_eq!(pixel(&raster, 0, 2), [255, 0, 0, 255]);
        assert_eq!(pixel(&raster, 2, 2), [0, 0, 255, 255]);
        assert_eq!(pixel(&raster, 3, 2), [0, 0, 0, 0]);
        assert_eq!(pixel(&raster, 5, 2), [0, 0, 255, 255]);
        assert_eq!(pixel(&raster, 4, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn incremental_batches_equal_full_replay_at_every_split() {
        // Native visualization pattern: dark bars, cursor clear, redraw, cursor.
        let actions = vec![
            color(34, 34, 34, None),
            CanvasAction::FillRect(Pos(0.0, 1.0), Pos(8.0, 3.0)),
            color(255, 255, 255, None),
            CanvasAction::FillRect(Pos(2.0, 0.0), Pos(3.0, 4.0)),
            CanvasAction::ClearRect(Pos(2.0, 0.0), Pos(3.0, 4.0)),
            color(34, 34, 34, None),
            CanvasAction::FillRect(Pos(2.0, 1.0), Pos(3.0, 3.0)),
            CanvasAction::StrokeSize(2.0),
            color(255, 255, 255, Some(128)),
            CanvasAction::Line(Pos(0.0, 2.0), Pos(8.0, 2.0)),
            CanvasAction::Clear,
            CanvasAction::Line(Pos(0.0, 2.0), Pos(8.0, 2.0)),
        ];
        let expected = Raster::render([16, 8], [8.0, 4.0], &actions);
        for split in 0..=actions.len() {
            let mut actual = Raster::render([16, 8], [8.0, 4.0], &actions[..split]);
            actual.apply(&actions[split..]);
            assert_eq!(actual.rgba, expected.rgba, "split {split}");
        }
        assert_eq!(pixel(&expected, 4, 3), [255, 255, 255, 128]);
        assert_eq!(pixel(&expected, 4, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn alpha_source_over_then_clear_then_redraw() {
        let mut raster = Raster::render(
            [2, 2],
            [2.0, 2.0],
            &[
                color(255, 0, 0, None),
                CanvasAction::FillRect(Pos(0.0, 0.0), Pos(2.0, 2.0)),
                color(0, 0, 255, Some(128)),
                CanvasAction::FillRect(Pos(0.0, 0.0), Pos(2.0, 2.0)),
            ],
        );
        assert_eq!(pixel(&raster, 0, 0), [127, 0, 128, 255]);
        raster.apply(&[
            CanvasAction::ClearRect(Pos(1.0, 1.0), Pos(-1.0, -1.0)),
            CanvasAction::FillRect(Pos(0.0, 0.0), Pos(1.0, 1.0)),
        ]);
        assert_eq!(pixel(&raster, 0, 0), [0, 0, 255, 128]);
        assert_eq!(pixel(&raster, 1, 1), [127, 0, 128, 255]);
    }

    #[test]
    fn clipped_reversed_rectangles_and_butt_cap_lines() {
        let raster = Raster::render(
            [4, 4],
            [4.0, 4.0],
            &[
                color(0, 255, 0, None),
                CanvasAction::FillRect(Pos(2.0, 2.0), Pos(-10.0, -10.0)),
                CanvasAction::StrokeSize(2.0),
                color(255, 0, 0, None),
                CanvasAction::Line(Pos(1.0, 3.0), Pos(3.0, 3.0)),
            ],
        );
        assert_eq!(pixel(&raster, 0, 0), [0, 255, 0, 255]);
        assert_eq!(pixel(&raster, 0, 3), [0, 0, 0, 0]);
        assert_eq!(pixel(&raster, 1, 3), [255, 0, 0, 255]);
        assert_eq!(pixel(&raster, 3, 3), [0, 0, 0, 0]);
    }

    fn canvas_document() -> Document {
        let mut document = Document::new();
        replace_document(&mut document);
        document
    }

    fn replace_document(document: &mut Document) {
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        id: 1,
                        str_id: Some("visualization".into()),
                        element: Element::Canvas,
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
    }

    fn update(actions: Vec<CanvasAction>) -> CanvasUpdate {
        CanvasUpdate {
            target: "visualization".into(),
            canvas_actions: actions,
        }
    }

    #[test]
    fn trait_bound_canvas_ingress_forwards_order_and_stale_errors() {
        fn deliver<C: WidgetAdapter>(
            adapter: &mut C,
            document: &Document,
            command: CanvasCommand,
        ) -> Result<ElementStamp, String> {
            adapter.apply_canvas(document, command)
        }

        let mut document = canvas_document();
        let mut adapter = CanvasAdapter::new();
        let stamp = deliver(
            &mut adapter,
            &document,
            CanvasCommand::Update(update(vec![
                color(255, 0, 0, None),
                CanvasAction::FillRect(Pos(0.0, 0.0), Pos(4.0, 4.0)),
            ])),
        )
        .unwrap();
        assert_eq!(
            deliver(
                &mut adapter,
                &document,
                CanvasCommand::Stamped {
                    stamp,
                    update: update(vec![CanvasAction::ClearRect(Pos(1.0, 1.0), Pos(3.0, 3.0))]),
                },
            )
            .unwrap(),
            stamp
        );
        let raster = Raster::render([4, 4], [4.0, 4.0], &adapter.states[&stamp.key].actions);
        assert_eq!(pixel(&raster, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&raster, 1, 1), [0, 0, 0, 0]);
        assert!(
            deliver(
                &mut adapter,
                &document,
                CanvasCommand::Update(update(vec![CanvasAction::StrokeSize(f32::NAN)])),
            )
            .is_err()
        );
        assert_eq!(adapter.states[&stamp.key].actions.len(), 3);
        replace_document(&mut document);
        assert!(
            deliver(
                &mut adapter,
                &document,
                CanvasCommand::Stamped {
                    stamp,
                    update: update(vec![CanvasAction::Clear])
                },
            )
            .is_err()
        );
    }

    #[test]
    fn replacement_retires_history_and_rejects_old_stamp() {
        let mut document = canvas_document();
        let mut adapter = CanvasAdapter::new();
        let stamp = adapter
            .apply_command(
                &document,
                CanvasCommand::Update(update(vec![
                    color(255, 0, 0, None),
                    CanvasAction::FillRect(Pos(0.0, 0.0), Pos(2.0, 2.0)),
                ])),
            )
            .unwrap();
        adapter
            .apply_command(
                &document,
                CanvasCommand::Update(update(vec![CanvasAction::Clear])),
            )
            .unwrap();
        assert_eq!(adapter.states[&stamp.key].actions.len(), 3);
        replace_document(&mut document);
        adapter.reconcile(&document, &[]).unwrap();
        assert!(adapter.states.is_empty());
        assert!(
            adapter
                .apply_command(
                    &document,
                    CanvasCommand::Stamped {
                        stamp,
                        update: update(vec![CanvasAction::Clear]),
                    }
                )
                .is_err()
        );
        let next = adapter
            .apply_command(
                &document,
                CanvasCommand::Update(update(vec![CanvasAction::Clear])),
            )
            .unwrap();
        assert_ne!(stamp, next);
        assert_eq!(adapter.states[&next.key].actions.len(), 1);
    }

    #[test]
    fn duplicate_ids_and_non_canvas_targets_are_errors() {
        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        children: vec![
                            Container {
                                id: 1,
                                str_id: Some("visualization".into()),
                                element: Element::Canvas,
                                ..Container::default()
                            },
                            Container {
                                id: 2,
                                str_id: Some("visualization".into()),
                                ..Container::default()
                            },
                        ],
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let mut adapter = CanvasAdapter::new();
        assert!(
            adapter
                .apply_command(&document, CanvasCommand::Update(update(vec![])))
                .unwrap_err()
                .contains("found 2")
        );
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        str_id: Some("visualization".into()),
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        assert!(
            adapter
                .apply_command(&document, CanvasCommand::Update(update(vec![])))
                .unwrap_err()
                .contains("not a canvas")
        );
    }

    #[test]
    fn translated_zoomed_texture_fixture_preserves_clear_and_clip() {
        use crate::geometry::GeometryTransform;
        let document = canvas_document();
        let mut adapter = CanvasAdapter::new();
        let stamp = adapter
            .apply_command(
                &document,
                CanvasCommand::Update(update(vec![
                    color(255, 0, 0, None),
                    CanvasAction::FillRect(Pos(0.0, 0.0), Pos(4.0, 4.0)),
                    CanvasAction::ClearRect(Pos(1.0, 1.0), Pos(3.0, 3.0)),
                ])),
            )
            .unwrap();
        let rect = Rect::from_min_size(egui::pos2(10.0, 12.0), egui::vec2(8.0, 8.0));
        let clip = Rect::from_min_max(egui::pos2(12.0, 12.0), egui::pos2(18.0, 20.0));
        let geometry = NodeGeometry {
            key: stamp.key,
            container_id: 1,
            parent: None,
            containing_block: None,
            margin_box: rect,
            padding_box: rect,
            content_box: rect,
            border_box: rect,
            paint_clip: clip,
            descendant_clip: clip,
            transform: GeometryTransform {
                origin: rect.min,
                scale: 2.0,
            },
            scroll_extent: Rect::from_min_size(Pos2::ZERO, egui::vec2(4.0, 4.0)),
            scroll_max: egui::Vec2::ZERO,
            scroll_offset: egui::Vec2::ZERO,
            visible: true,
            fixed: false,
        };
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(64.0, 64.0))),
            ..egui::RawInput::default()
        };
        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let authored = document.get(stamp.key).unwrap();
                    let result = adapter
                        .paint_node(ui, stamp, authored, authored, &geometry)
                        .unwrap();
                    assert_eq!(result.response.rect, clip);
                });
        });
        let state = &adapter.states[&stamp.key];
        let raster = state.raster.as_ref().unwrap();
        assert_eq!(raster.size, [8, 8]);
        assert_eq!(pixel(raster, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(raster, 3, 3), [0, 0, 0, 0]);
        assert!(output.shapes.iter().any(|shape| shape.clip_rect == clip
            && matches!(shape.shape, egui::Shape::Mesh(_))));
        assert!(
            output
                .textures_delta
                .set
                .iter()
                .any(|(id, _)| *id == state.texture.as_ref().unwrap().id())
        );
        assert_eq!(adapter.pending_resources(), 0);
    }

    #[test]
    fn invalid_batch_is_atomic_and_missing_target_is_diagnosed() {
        let document = canvas_document();
        let mut adapter = CanvasAdapter::new();
        assert!(
            adapter
                .apply_command(
                    &document,
                    CanvasCommand::Update(update(vec![
                        CanvasAction::Clear,
                        CanvasAction::StrokeSize(f32::NAN),
                    ]))
                )
                .is_err()
        );
        assert!(adapter.states.is_empty());
        assert!(
            adapter
                .apply_command(
                    &document,
                    CanvasCommand::Update(CanvasUpdate {
                        target: "missing".into(),
                        canvas_actions: vec![],
                    })
                )
                .is_err()
        );
        assert!(
            validate_actions(&[CanvasAction::Line(Pos(f32::INFINITY, 0.0), Pos(0.0, 0.0))])
                .is_err()
        );
    }
}
