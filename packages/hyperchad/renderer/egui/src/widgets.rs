//! Native painting of projected HyperChad nodes, without child layout or traversal.
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
/// not arrange or recursively paint HyperChad children. Identity is `stamp.key`;
/// asynchronous completion must additionally validate the complete stamp.
pub trait WidgetAdapter {
    /// Retire removed generations and reconcile retained native state.
    ///
    /// # Errors
    /// * Returns a diagnostic if reconciliation cannot safely complete.
    fn reconcile(
        &mut self,
        document: &Document,
        mutations: &[MutationSummary],
    ) -> Result<(), String>;

    /// Paint one node; never perform HyperChad child layout.
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
    /// Install native forms, bounded image resources and retained canvas painting.
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
    text_styles: std::collections::BTreeMap<ElementKey, text::TextStyle>,
    opacity: std::collections::BTreeMap<ElementKey, f32>,
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
            text_styles: std::collections::BTreeMap::new(),
            opacity: std::collections::BTreeMap::new(),
        }
    }
}
