//! Native painting of projected `HyperChad` nodes, without child layout or traversal.
//!
//! Native Raw policy: Raw and Text are literal text runs. The HTML parser emits
//! Raw for ordinary text and provides no provenance distinguishing injected HTML.
//! We never reparse it, guess markup from angle brackets, or execute browser HTML;
//! HTML-only unescaped source is displayed verbatim with the same font/clip/alpha
//! path and accessibility naming as Text. No application rewrite is required.
//!
//! The frame core owns traversal, stable identity, clipping and calculated clones.
//! Adapters own editing/resource state; buttons and links never paint descendant text.

use eframe::egui;
use hyperchad_transformer::{Container, Element};

mod disclosure;
mod dispatch;
pub mod forms;
mod paint;
#[cfg(test)]
mod tests;
mod text;

use crate::{
    backend::{FramePainter, NodeOutput},
    document::{Document, ElementKey, ElementStamp, MutationSummary},
    geometry::NodeGeometry,
};

/// Adapter shared by the forms, image-resource and canvas lanes.
///
/// Implementations paint only this node, in the supplied screen-space geometry.
/// They must return the actual native response (including changed values), and must
/// not arrange or recursively paint `HyperChad` children. Identity is `stamp.key`;
/// asynchronous completion must additionally validate the complete stamp.
pub trait WidgetAdapter {
    /// Install the current calculated root-to-editor ancestry before painting.
    ///
    /// # Errors
    /// * Rejects text editors when their adapter does not support calculated typography.
    fn set_calculated_text_path(
        &mut self,
        _source: ElementStamp,
        _path: &[&Container],
    ) -> Result<(), String> {
        Err("calculated editor typography adapter unavailable".into())
    }

    /// Retire removed generations and reconcile retained native state.
    ///
    /// # Errors
    /// * Returns a diagnostic if reconciliation cannot safely complete.
    fn reconcile(
        &mut self,
        document: &Document,
        mutations: &[MutationSummary],
    ) -> Result<(), String>;

    /// Paint one node; never perform `HyperChad` child layout.
    ///
    /// # Errors
    /// * Returns a diagnostic for unsupported data or a failed native operation.
    fn paint_node(
        &mut self,
        ui: &mut egui::Ui,
        stamp: ElementStamp,
        authored: &Container,
        calculated: &Container,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<NodeOutput, String>;

    /// Apply a canvas update at its mailbox position.
    ///
    /// # Errors
    /// * Returns a diagnostic when the canvas adapter is absent or rejects a stale/invalid update.
    fn apply_canvas(
        &mut self,
        _document: &Document,
        _command: crate::canvas::CanvasCommand,
    ) -> Result<ElementStamp, String> {
        Err("canvas adapter not installed".to_owned())
    }

    /// Focus a current eligible native control using its authoritative identity.
    ///
    /// # Errors
    /// * Returns an error when unsupported, stale, unmounted or ineligible.
    fn focus(&mut self, _ctx: &egui::Context, _stamp: ElementStamp) -> Result<(), String> {
        Err("native focus adapter unavailable".to_owned())
    }

    /// Blur a current eligible native control.
    ///
    /// # Errors
    /// * Returns an error when unsupported, stale, unmounted or ineligible.
    fn blur(&mut self, _ctx: &egui::Context, _stamp: ElementStamp) -> Result<(), String> {
        Err("native blur adapter unavailable".to_owned())
    }

    /// Select all text in a current eligible native editor.
    ///
    /// # Errors
    /// * Returns an error when unsupported, stale, unmounted or not editable.
    fn select_all(&mut self, _ctx: &egui::Context, _stamp: ElementStamp) -> Result<(), String> {
        Err("native selection adapter unavailable".to_owned())
    }

    /// Drain lossless native changes (arrays/files cannot use `NodeOutput::value`).
    fn take_changes(&mut self) -> Vec<(ElementStamp, crate::events::EventValue)> {
        Vec::new()
    }

    /// Number of current-generation asynchronous operations still outstanding.
    fn pending_resources(&self) -> usize {
        0
    }
}

/// Fully installed production widget stack. The resource worker must be serviced
/// outside paint; construction never starts network or file I/O.
pub type NativeWidgetPainter = WidgetPainter<
    forms::FormsAdapter,
    crate::resources::ImageResources,
    crate::canvas::CanvasAdapter,
>;

impl NativeWidgetPainter {
    /// Install completed image work on the UI thread before frame painting.
    ///
    /// The resource adapter validates document generations and requests repaint
    /// after successful installation. Fetch/decode work belongs to the returned
    /// [`crate::resources::ResourceWorker`], never to this method or paint.
    #[must_use]
    pub fn install_resource_completions(
        &mut self,
        document: &Document,
        context: &egui::Context,
    ) -> usize {
        self.resources.install_completions(document, context)
    }

    /// Install native forms, bounded image resources and retained canvas painting.
    ///
    /// Host invocation contract:
    /// * Retain both returned values for the host lifetime.
    /// * Service `worker.try_request()` outside paint with bounded fetch/decode;
    ///   submit success or failure via `worker.complete(completion, context)`.
    /// * Call [`Self::install_resource_completions`] on the UI thread, then
    ///   [`FramePainter::reconcile`] before layout and traversal.
    /// * Apply [`FramePainter::prepare_layout`] only to fresh calculated clones,
    ///   before the shared calculator; filter traversal and event targets with
    ///   [`FramePainter::allows_node`]. Prepare nodes in document preorder even
    ///   when clipped, then paint visible nodes once using projected geometry.
    /// * Route canvas updates with [`FramePainter::apply_canvas`] and drain
    ///   [`FramePainter::take_changes`] once after traversal. Drain native form
    ///   intents/file requests through [`Self::forms_mut`].
    ///
    /// Returns the worker endpoint for the native host's asynchronous loader.
    #[must_use]
    pub fn native(
        limits: crate::resources::ResourceLimits,
    ) -> (Self, crate::resources::ResourceWorker) {
        let (resources, worker) = crate::resources::ImageResources::new(limits);
        (
            Self::new(
                forms::FormsAdapter::new(),
                resources,
                crate::canvas::CanvasAdapter::new(),
            ),
            worker,
        )
    }
}

/// Production structural painter with explicit native-state adapters.
///
/// `F` handles Input/Textarea/Select/Option, `R` handles Image, and `C` handles
/// Canvas. Adapters are mandatory: missing resource support must be diagnosed,
/// not silently replaced by a successful empty paint.
pub struct WidgetPainter<F, R, C> {
    forms: F,
    resources: R,
    canvas: C,
    disclosure: std::collections::BTreeMap<ElementKey, disclosure::Disclosure>,
    summary_owners: std::collections::BTreeMap<ElementKey, ElementKey>,
    suppressed: std::collections::BTreeSet<ElementKey>,
    native_children: std::collections::BTreeSet<ElementKey>,
    list_markers: std::collections::BTreeMap<ElementKey, String>,
    calculated_text_paths: std::collections::BTreeMap<ElementKey, Vec<Container>>,
    text_styles: std::collections::BTreeMap<ElementKey, text::TextStyle>,
    opacity: std::collections::BTreeMap<ElementKey, f32>,
}

impl<F: WidgetAdapter, R, C> WidgetPainter<F, R, C> {
    /// Delegate focus to the forms adapter without inventing a native widget ID.
    ///
    /// # Errors
    /// * Propagates stale-generation, eligibility or unsupported-adapter errors.
    pub fn focus(&mut self, ctx: &egui::Context, stamp: ElementStamp) -> Result<(), String> {
        self.forms.focus(ctx, stamp)
    }

    /// Delegate blur to the forms adapter's current native control.
    ///
    /// # Errors
    /// * Propagates stale-generation, eligibility or unsupported-adapter errors.
    pub fn blur(&mut self, ctx: &egui::Context, stamp: ElementStamp) -> Result<(), String> {
        self.forms.blur(ctx, stamp)
    }

    /// Delegate selection to the forms adapter's authoritative editor state.
    ///
    /// # Errors
    /// * Propagates stale-generation, editability or unsupported-adapter errors.
    pub fn select_all(&mut self, ctx: &egui::Context, stamp: ElementStamp) -> Result<(), String> {
        self.forms.select_all(ctx, stamp)
    }
}

impl<F, R, C> WidgetPainter<F, R, C> {
    /// Access native form actions and asynchronous file-dialog completion hooks.
    #[must_use]
    pub const fn forms_mut(&mut self) -> &mut F {
        &mut self.forms
    }

    /// Access image resource completion hooks.
    #[must_use]
    pub const fn resources_mut(&mut self) -> &mut R {
        &mut self.resources
    }

    /// Access canvas update hooks.
    #[must_use]
    pub const fn canvas_mut(&mut self) -> &mut C {
        &mut self.canvas
    }

    /// Install the forms, image-resource and canvas implementations.
    #[must_use]
    pub const fn new(forms: F, resources: R, canvas: C) -> Self {
        Self {
            forms,
            resources,
            canvas,
            disclosure: std::collections::BTreeMap::new(),
            summary_owners: std::collections::BTreeMap::new(),
            suppressed: std::collections::BTreeSet::new(),
            native_children: std::collections::BTreeSet::new(),
            list_markers: std::collections::BTreeMap::new(),
            calculated_text_paths: std::collections::BTreeMap::new(),
            text_styles: std::collections::BTreeMap::new(),
            opacity: std::collections::BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod installation_tests {
    use super::{FramePainter, NativeWidgetPainter};
    use crate::{document::Document, resources::ResourceLimits};

    #[test]
    fn production_stack_installs_without_starting_resource_work() {
        let (mut painter, worker) = NativeWidgetPainter::native(ResourceLimits::default());
        let document = Document::new();
        painter.reconcile(&document, &[]).unwrap();
        assert_eq!(painter.pending_resources(), 0);
        assert!(worker.try_request().is_none());
        assert_eq!(
            painter.install_resource_completions(&document, &eframe::egui::Context::default()),
            0
        );
        assert_eq!(painter.take_changes(), []);
    }
}
