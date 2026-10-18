//! Persistent native form controls. No document layout, routing or blocking I/O.
//!
//! The host drains [`FormsAdapter::take_actions`] and
//! [`FormsAdapter::take_file_requests`] after traversal. A native dialog service
//! performs its work outside paint and returns bytes through `complete_file`.
//!
//! Routing integration: reconcile first, seed action state with `control_stamps`
//! and stamped `current_value`, then deliver native scalar changes and trait
//! `WidgetAdapter::take_changes` exactly once. Structural button activation calls
//! `activate_button` once; `take_actions` supplies captured Submit payloads to the
//! routing orchestrator. Reset is already applied and is only a notification.
//! Never collect submission values from authored defaults or encode transport here.
//!
//! Exact pointer bounds also require the frame host to set egui's global
//! `interaction.interact_radius` to zero before `begin_pass`. Native hit candidates
//! are selected from last-frame geometry at pass start; the default five-point
//! nearest-widget search can otherwise activate a control outside its clipped box.
//! A per-control paint-time style override cannot correct that frame-start search.

pub mod native_files;
pub use native_files::{FileLimits, FileServiceError, NativeFileService};

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::text_layout::{FontRegistry, request_job};
use eframe::egui::{self, Id, Key, Sense, WidgetInfo, WidgetType};
use hyperchad_transformer::layout::font::{
    TextFontFamily, TextFontWeight, TextMeasureRequest, resolve_text_weight,
};
use hyperchad_transformer::models::{FontWeight, WhiteSpace};
use hyperchad_transformer::{Container, Element, Input};

use super::WidgetAdapter;
use crate::{
    backend::NodeOutput,
    document::{Document, ElementKey, ElementStamp, MutationSummary},
    geometry::NodeGeometry,
};

/// Selected file data. Deliberately does not implement Debug (names/bytes are private).
#[derive(Clone)]
pub struct FormFile {
    pub name: String,
    pub media_type: Option<String>,
    pub bytes: Arc<[u8]>,
}

/// One successful control value. Never log this type: it can contain passwords.
#[derive(Clone)]
pub enum FormValue {
    Text(String),
    File(FormFile),
}

/// Ordered entry, not a map: empty and repeated names are meaningful.
#[derive(Clone)]
pub struct FormEntry {
    pub name: String,
    pub value: FormValue,
}

/// Complete routing input, captured before any asynchronous work.
/// No HTTP encoding or transport is performed in the widget lane.
#[derive(Clone)]
pub struct FormSubmission {
    pub form: ElementStamp,
    pub submitter: Option<ElementStamp>,
    pub action: Option<String>,
    pub method: Option<String>,
    pub multipart: bool,
    pub entries: Vec<FormEntry>,
}

/// Structured changes that cannot fit `NodeOutput::value` without loss.
/// The coordinator emits these on the owning select/file input, not its option.
#[derive(Clone)]
pub struct FormChange {
    pub source: ElementStamp,
    pub values: Vec<FormValue>,
}

/// Extra output for the routing coordinator; drain exactly once after traversal.
#[derive(Clone)]
pub enum FormAction {
    Submit(FormSubmission),
    Reset {
        form: ElementStamp,
        submitter: Option<ElementStamp>,
    },
}

/// Native dialog work descriptor. Work must not execute on the paint thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileRequest {
    pub source: ElementStamp,
    pub sequence: u64,
    pub multiple: bool,
}

#[derive(Clone, PartialEq, Eq)]
enum Value {
    Text(String),
    Checked(bool),
    Selected(BTreeSet<usize>),
    Files,
}

#[derive(Clone, PartialEq, Eq)]
struct Choice {
    key: ElementKey,
    generation: u64,
    label: String,
    value: String,
    disabled: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Text,
    Password,
    Checkbox,
    Hidden,
    File,
    Textarea,
    Select,
}

#[derive(Clone, PartialEq, Eq)]
struct Spec {
    kind: ControlKind,
    initial: Value,
    choices: Vec<Choice>,
}

#[derive(Clone, Default)]
struct EditorStyle {
    families: Option<Vec<String>>,
    weights: Vec<FontWeight>,
    whitespace: Option<WhiteSpace>,
    color: Option<hyperchad_color::Color>,
    authored_size: bool,
    calculated_path: bool,
}

impl EditorStyle {
    fn inherited(document: &Document, mut key: ElementKey) -> Self {
        let mut ancestors = Vec::new();
        loop {
            ancestors.push(key);
            let Some(parent) = document.parent(key) else {
                break;
            };
            key = parent;
        }
        let mut style = Self::default();
        for key in ancestors.into_iter().rev() {
            let Some(node) = document.get(key) else {
                continue;
            };
            style.apply(node);
        }
        style
    }

    fn apply(&mut self, node: &Container) {
        if let Some(families) = &node.font_family {
            self.families = Some(families.clone());
        }
        if let Some(weight) = node.font_weight {
            self.weights.push(weight);
        }
        self.whitespace = node.white_space.or(self.whitespace);
        self.color = node.color.or(self.color);
        self.authored_size |= node.font_size.is_some();
    }

    fn job(
        &self,
        registry: &FontRegistry,
        text: &str,
        size: f32,
        width: f32,
        whitespace: WhiteSpace,
    ) -> Result<egui::text::LayoutJob, String> {
        let weight = self
            .weights
            .iter()
            .try_fold(
                TextFontWeight::CompatibilityDefault,
                |inherited, authored| {
                    resolve_text_weight(*authored, inherited, registry.compatibility_weight())
                },
            )
            .map_err(|error| error.to_string())?;
        request_job(
            registry,
            &TextMeasureRequest {
                text,
                family: self.families.as_deref().map_or(
                    TextFontFamily::CompatibilityDefault,
                    TextFontFamily::Authored,
                ),
                weight,
                size,
                wrap_width: width,
                whitespace: Some(whitespace),
            },
        )
        .map_err(|error| error.to_string())
    }
}

struct Field {
    style: EditorStyle,
    stamp: ElementStamp,
    spec: Spec,
    value: Value,
    files: Vec<FormFile>,
    name: Option<String>,
    form: Option<ElementKey>,
    disabled: bool,
    readonly: bool,
    multiple: bool,
    native_id: Option<Id>,
    label: String,
    focus: Option<bool>,
    /// Separate editor identity only changes on an intentional value reset.
    editor_revision: u64,
    pending: Option<FileRequest>,
}

impl Field {
    fn reconcile(&mut self, stamp: ElementStamp, spec: Spec) {
        // A changed ancestor generation alone is NOT an authoritative value update.
        // Compare the actual authored value/configuration, not subtree stamps.
        if self.spec.kind != spec.kind
            || self.spec.initial != spec.initial
            || self
                .spec
                .choices
                .iter()
                .map(|choice| (&choice.key, &choice.value))
                .ne(spec
                    .choices
                    .iter()
                    .map(|choice| (&choice.key, &choice.value)))
        {
            self.value = spec.initial.clone();
            self.files.clear();
            self.editor_revision = self.editor_revision.wrapping_add(1);
        }
        if self.stamp != stamp {
            self.pending = None;
            self.native_id = None;
        }
        self.stamp = stamp;
        self.spec = spec;
    }

    fn reset(&mut self) {
        self.value = self.spec.initial.clone();
        self.files.clear();
        self.pending = None;
        self.editor_revision = self.editor_revision.wrapping_add(1);
    }

    fn id(&self) -> Id {
        Id::new(("hyperchad-form", self.stamp.key))
    }
}

/// UI-owned form state. Never use Debug on form state or submission values.
#[derive(Default)]
pub struct FormsAdapter {
    fields: BTreeMap<ElementKey, Field>,
    order: Vec<ElementKey>,
    forms: BTreeMap<ElementKey, (ElementStamp, Option<String>, Option<String>, bool)>,
    submitters: BTreeMap<ElementKey, (ElementStamp, ElementKey)>,
    changes: Vec<FormChange>,
    actions: Vec<FormAction>,
    file_requests: Vec<FileRequest>,
    next_file_sequence: u64,
    file_service: bool,
}

impl FormsAdapter {
    /// No file picker is assumed. Install a host service before painting file inputs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare that the host drains file requests and completes them asynchronously.
    pub const fn set_file_service_available(&mut self, available: bool) {
        self.file_service = available;
    }

    /// Drain routing outputs. Payloads can contain credentials; do not log them.
    #[must_use]
    pub fn take_actions(&mut self) -> Vec<FormAction> {
        std::mem::take(&mut self.actions)
    }

    /// Drain lossless multi-select and file changes, attributed to the owning input.
    #[must_use]
    pub fn take_changes(&mut self) -> Vec<FormChange> {
        std::mem::take(&mut self.changes)
    }

    /// Supply the actual calculated root-to-control style path for this frame.
    /// Call after reconciliation and before `paint_node` when responsive ancestor
    /// typography differs from authored state. The last entry is this control.
    ///
    /// # Errors
    /// * The control stamp is stale/missing, the path is empty, or its last node
    ///   is not the same kind of control as the reconciled field.
    pub fn set_calculated_text_path(
        &mut self,
        source: ElementStamp,
        path: &[&Container],
    ) -> Result<(), String> {
        let field = self
            .fields
            .get_mut(&source.key)
            .filter(|field| field.stamp == source)
            .ok_or("editor style source is stale or missing")?;
        let last = path.last().ok_or("editor style path is empty")?;
        let matches = matches!(
            (&last.element, field.spec.kind),
            (Element::Textarea { .. }, ControlKind::Textarea)
                | (
                    Element::Input {
                        input: Input::Text { .. },
                        ..
                    },
                    ControlKind::Text
                )
                | (
                    Element::Input {
                        input: Input::Password { .. },
                        ..
                    },
                    ControlKind::Password
                )
        );
        if !matches {
            return Err("editor style path must end at its text control".into());
        }
        let mut style = EditorStyle::default();
        for node in path {
            style.apply(node);
        }
        style.calculated_path = true;
        field.style = style;
        Ok(())
    }

    /// Return the authoritative native focus ID after checking the full stamp.
    /// Text editors and select owners have explicit IDs; checkbox/file IDs are
    /// captured from the native response and require painting this generation.
    /// The host must additionally check current geometry visibility before focus.
    ///
    /// # Errors
    /// * Rejects missing/stale, disabled, or hidden controls.
    /// * Rejects checkbox/file controls not painted in the current generation.
    pub fn focus_id(&self, source: ElementStamp) -> Result<Id, String> {
        let field = self.focus_field(source)?;
        if matches!(field.spec.kind, ControlKind::Checkbox | ControlKind::File) {
            field
                .native_id
                .ok_or_else(|| "control has no current native focus response".into())
        } else {
            Ok(field.id())
        }
    }

    /// Focus an eligible control without repeating first-paint autofocus.
    ///
    /// # Errors
    /// * Returns the same eligibility/stamp errors as `focus_id`.
    pub fn focus(&mut self, context: &egui::Context, source: ElementStamp) -> Result<(), String> {
        let id = self.focus_id(source)?;
        self.fields
            .get_mut(&source.key)
            .ok_or("control is missing")?
            .focus = None;
        context.memory_mut(|memory| memory.request_focus(id));
        context.request_repaint();
        Ok(())
    }

    /// Blur only this control, never a different currently focused widget.
    ///
    /// # Errors
    /// * Returns the same eligibility/stamp errors as `focus_id`.
    pub fn blur(&mut self, context: &egui::Context, source: ElementStamp) -> Result<(), String> {
        let id = self.focus_id(source)?;
        self.fields
            .get_mut(&source.key)
            .ok_or("control is missing")?
            .focus = None;
        context.memory_mut(|memory| memory.surrender_focus(id));
        context.request_repaint();
        Ok(())
    }

    /// Select all Unicode character positions in a live editable text control.
    /// Does not implicitly focus it; call `focus` separately when desired.
    /// Applies a pending authoritative editor reset first, so next paint retains
    /// this selection rather than clearing it. Password values are never logged.
    ///
    /// # Errors
    /// * Rejects missing/stale, hidden, disabled, read-only or non-text controls.
    pub fn select_all(&self, context: &egui::Context, source: ElementStamp) -> Result<(), String> {
        let field = self.focus_field(source)?;
        if field.readonly
            || !matches!(
                field.spec.kind,
                ControlKind::Text | ControlKind::Password | ControlKind::Textarea
            )
        {
            return Err("control does not support editable text selection".into());
        }
        let Value::Text(value) = &field.value else {
            return Err("control has no text editor".into());
        };
        let id = field.id();
        let end = value.chars().count();
        prepare_editor(context, id, field.editor_revision);
        let mut editor = egui::text_edit::TextEditState::load(context, id).unwrap_or_default();
        editor
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(end),
            )));
        editor.store(context, id);
        context.request_repaint();
        Ok(())
    }

    fn focus_field(&self, source: ElementStamp) -> Result<&Field, String> {
        let field = self
            .fields
            .get(&source.key)
            .filter(|field| field.stamp == source)
            .ok_or("control focus source is stale or missing")?;
        if field.disabled || field.spec.kind == ControlKind::Hidden {
            return Err("control is not focus eligible".into());
        }
        Ok(field)
    }

    /// Snapshot a live control's effective native value for action/routing state.
    /// Checkbox values are booleans; selections and files are ordered arrays.
    /// Password text is sensitive payload, never a diagnostic. Disabled controls
    /// remain readable here but are omitted by `collect_submission`.
    ///
    /// # Errors
    /// * Returns a value-free error for a missing or stale control stamp.
    pub fn current_value(&self, source: ElementStamp) -> Result<crate::events::EventValue, String> {
        use crate::events::EventValue;
        let field = self
            .fields
            .get(&source.key)
            .filter(|field| field.stamp == source)
            .ok_or("control value source is stale or missing")?;
        Ok(match &field.value {
            Value::Text(value) => EventValue::String(value.clone()),
            Value::Checked(value) => EventValue::Bool(*value),
            Value::Selected(selected) => EventValue::Array(
                selected
                    .iter()
                    .filter_map(|index| field.spec.choices.get(*index))
                    .map(|choice| EventValue::String(choice.value.clone()))
                    .collect(),
            ),
            Value::Files => EventValue::Array(
                field
                    .files
                    .iter()
                    .cloned()
                    .map(|file| event_value(FormValue::File(file)))
                    .collect(),
            ),
        })
    }

    /// Live control stamps in document order, including hidden/disabled controls.
    /// Use with `current_value` to seed state after reconciliation, before events.
    #[must_use]
    pub fn control_stamps(&self) -> Vec<ElementStamp> {
        self.order
            .iter()
            .filter_map(|key| self.fields.get(key).map(|field| field.stamp))
            .collect()
    }

    /// Drain native dialog requests, to be processed outside the paint thread.
    #[must_use]
    pub fn take_file_requests(&mut self) -> Vec<FileRequest> {
        std::mem::take(&mut self.file_requests)
    }

    /// Complete a dialog/read operation. `None` is cancellation, retaining old files.
    /// Stale generations and superseded requests are ignored, including after reset.
    ///
    /// # Errors
    /// * Returns a value-free diagnostic if a single-file control receives many files.
    pub fn complete_file(
        &mut self,
        request: FileRequest,
        files: Option<Vec<FormFile>>,
    ) -> Result<(), String> {
        let Some(field) = self.fields.get_mut(&request.source.key) else {
            return Ok(());
        };
        if field.stamp != request.source || field.pending != Some(request) {
            return Ok(());
        }
        field.pending = None;
        if let Some(files) = files {
            if !field.multiple && files.len() > 1 {
                return Err("multiple files returned for a single-file input".into());
            }
            field.files = files;
            self.changes.push(FormChange {
                source: field.stamp,
                values: field.files.iter().cloned().map(FormValue::File).collect(),
            });
        }
        Ok(())
    }

    /// Snapshot successful fields in document order; names are neither filtered nor deduplicated.
    /// The submitter must be validated with `activate_button` or by the coordinator.
    ///
    /// # Errors
    /// * Returns an error for a missing form or a stale/foreign submitter.
    pub fn collect_submission(
        &self,
        form: ElementKey,
        submitter: Option<ElementStamp>,
    ) -> Result<FormSubmission, String> {
        let (stamp, action, method, multipart) =
            self.forms.get(&form).ok_or("form is no longer live")?;
        // Full submitter membership is checked by activate_button, which has the document.
        if let Some(submitter) = submitter
            && self.submitters.get(&submitter.key) != Some(&(submitter, form))
        {
            return Err("submitter is stale or belongs to another form".into());
        }
        let mut entries = Vec::new();
        for key in &self.order {
            let field = &self.fields[key];
            if field.form != Some(form) || field.disabled {
                continue;
            }
            let Some(name) = &field.name else {
                continue;
            };
            match &field.value {
                Value::Text(value) => entries.push(FormEntry {
                    name: name.clone(),
                    value: FormValue::Text(value.clone()),
                }),
                Value::Checked(true) => entries.push(FormEntry {
                    name: name.clone(),
                    value: FormValue::Text("on".into()),
                }),
                Value::Checked(false) => {}
                Value::Selected(selected) => {
                    for index in selected {
                        if let Some(choice) = field
                            .spec
                            .choices
                            .get(*index)
                            .filter(|choice| !choice.disabled)
                        {
                            entries.push(FormEntry {
                                name: name.clone(),
                                value: FormValue::Text(choice.value.clone()),
                            });
                        }
                    }
                }
                Value::Files => {
                    if field.files.is_empty() {
                        entries.push(FormEntry {
                            name: name.clone(),
                            value: FormValue::File(FormFile {
                                name: String::new(),
                                media_type: Some("application/octet-stream".into()),
                                bytes: Arc::from([]),
                            }),
                        });
                    }
                    for file in &field.files {
                        entries.push(FormEntry {
                            name: name.clone(),
                            value: FormValue::File(file.clone()),
                        });
                    }
                }
            }
        }
        Ok(FormSubmission {
            form: *stamp,
            submitter,
            action: action.clone(),
            method: method.clone(),
            multipart: *multipart,
            entries,
        })
    }

    /// Explicit authoritative reset, including a same-value authored write. This is
    /// necessary when the document mutation format cannot distinguish reasserting a
    /// default from an unrelated update. Retains focus, clears selection/undo.
    ///
    /// # Errors
    /// * Returns an error if the field is missing or its generation is stale.
    pub fn reset_control(&mut self, source: ElementStamp) -> Result<(), String> {
        let field = self
            .fields
            .get_mut(&source.key)
            .ok_or("control is no longer live")?;
        if field.stamp != source {
            return Err("control reset is stale".into());
        }
        field.reset();
        self.changes
            .retain(|change| change.source.key != source.key);
        self.file_requests
            .retain(|request| request.source.key != source.key);
        Ok(())
    }

    /// Restore the latest authored defaults, clearing editor selection/undo on next paint.
    ///
    /// # Errors
    /// * Returns an error if the form is no longer live.
    pub fn reset_form(&mut self, form: ElementKey) -> Result<(), String> {
        if !self.forms.contains_key(&form) {
            return Err("form is no longer live".into());
        }
        for field in self
            .fields
            .values_mut()
            .filter(|field| field.form == Some(form))
        {
            field.reset();
        }
        self.changes.retain(|change| {
            self.fields
                .get(&change.source.key)
                .is_some_and(|field| field.form != Some(form))
        });
        self.file_requests.retain(|request| {
            self.fields
                .get(&request.source.key)
                .is_some_and(|field| field.pending == Some(*request))
        });
        Ok(())
    }

    /// Coordinator hook for the structural button's native activation (including keyboard).
    /// Call once per activation, not again for the corresponding pointer gesture.
    ///
    /// # Errors
    /// * Returns an error for stale buttons or invalid form state.
    pub fn activate_button(
        &mut self,
        document: &Document,
        submitter: ElementStamp,
    ) -> Result<(), String> {
        if !document.is_current(submitter) {
            return Err("button activation is stale".into());
        }
        let Some(container) = document.get(submitter.key) else {
            return Err("button is no longer live".into());
        };
        let Element::Button { r#type } = &container.element else {
            return Err("form activation requires a button".into());
        };
        if flag(container, "disabled") {
            return Ok(());
        }
        let Some(form) = enclosing_form(document, submitter.key) else {
            return Ok(());
        };
        match r#type.as_deref().unwrap_or("submit") {
            "button" => {}
            "reset" => {
                self.reset_form(form)?;
                self.actions.push(FormAction::Reset {
                    form: self.forms[&form].0,
                    submitter: Some(submitter),
                });
            }
            _ => self.actions.push(FormAction::Submit(
                self.collect_submission(form, Some(submitter))?,
            )),
        }
        Ok(())
    }
}

fn flag(container: &Container, name: &str) -> bool {
    container
        .data
        .get(name)
        .is_some_and(|value| value != "false")
}

fn text(container: &Container) -> String {
    match &container.element {
        Element::Text { value } | Element::Raw { value } => value.clone(),
        _ => container.children.iter().map(text).collect(),
    }
}

fn enclosing_form(document: &Document, mut key: ElementKey) -> Option<ElementKey> {
    while let Some(parent) = document.parent(key) {
        if matches!(document.get(parent)?.element, Element::Form { .. }) {
            return Some(parent);
        }
        key = parent;
    }
    None
}

fn choices(document: &Document, key: ElementKey) -> Vec<Choice> {
    document
        .children(key)
        .into_iter()
        .filter_map(|key| {
            let container = document.get(key)?;
            let Element::Option { value, disabled } = &container.element else {
                return None;
            };
            let label = text(container);
            Some(Choice {
                key,
                generation: document.stamp(key)?.generation,
                value: value.clone().unwrap_or_else(|| label.clone()),
                label,
                disabled: disabled.unwrap_or(false),
            })
        })
        .collect()
}

fn specification(document: &Document, key: ElementKey, container: &Container) -> Option<Spec> {
    let (kind, initial) = match &container.element {
        Element::Input { input, .. } => match input {
            Input::Text { value, .. } => (
                ControlKind::Text,
                Value::Text(value.clone().unwrap_or_default()),
            ),
            Input::Password { value, .. } => (
                ControlKind::Password,
                Value::Text(value.clone().unwrap_or_default()),
            ),
            Input::Checkbox { checked } => (
                ControlKind::Checkbox,
                Value::Checked(checked.unwrap_or(false)),
            ),
            Input::Hidden { value } => (
                ControlKind::Hidden,
                Value::Text(value.clone().unwrap_or_default()),
            ),
            Input::File => (ControlKind::File, Value::Files),
        },
        Element::Textarea { value, .. } => (ControlKind::Textarea, Value::Text(value.clone())),
        Element::Select {
            selected, multiple, ..
        } => {
            let choices = choices(document, key);
            let mut indices: BTreeSet<usize> = choices
                .iter()
                .enumerate()
                .filter(|(_, choice)| selected.as_ref() == Some(&choice.value))
                .map(|(index, _)| index)
                .collect();
            if !multiple.unwrap_or(false) {
                if let Some(first) = indices.first().copied() {
                    indices = BTreeSet::from([first]);
                } else if selected.is_none()
                    && let Some(first) = choices.iter().position(|choice| !choice.disabled)
                {
                    indices.insert(first);
                }
            }
            return Some(Spec {
                kind: ControlKind::Select,
                initial: Value::Selected(indices),
                choices,
            });
        }
        _ => return None,
    };
    Some(Spec {
        kind,
        initial,
        choices: Vec::new(),
    })
}

impl WidgetAdapter for FormsAdapter {
    fn set_calculated_text_path(
        &mut self,
        source: ElementStamp,
        path: &[&Container],
    ) -> Result<(), String> {
        Self::set_calculated_text_path(self, source, path)
    }

    fn reconcile(
        &mut self,
        document: &Document,
        _mutations: &[MutationSummary],
    ) -> Result<(), String> {
        self.forms.clear();
        self.submitters.clear();
        self.order.clear();
        for key in document.keys() {
            let container = document.get(key).ok_or("document key is missing")?;
            let stamp = document.stamp(key).ok_or("document stamp is missing")?;
            self.register_form(document, key, stamp, container);
            let Some(spec) = specification(document, key, container) else {
                continue;
            };
            let (name, autofocus, multiple, disabled) = match &container.element {
                Element::Input {
                    name, autofocus, ..
                } => (
                    name.clone(),
                    autofocus.unwrap_or(false),
                    flag(container, "multiple"),
                    flag(container, "disabled"),
                ),
                Element::Textarea { name, .. } => (
                    name.clone(),
                    flag(container, "autofocus"),
                    false,
                    flag(container, "disabled"),
                ),
                Element::Select {
                    name,
                    autofocus,
                    multiple,
                    disabled,
                    ..
                } => (
                    name.clone(),
                    autofocus.unwrap_or(false),
                    multiple.unwrap_or(false),
                    disabled.unwrap_or(false),
                ),
                _ => unreachable!(),
            };
            let style = EditorStyle::inherited(document, key);
            let field = self.fields.entry(key).or_insert_with(|| Field {
                style: style.clone(),
                stamp,
                value: spec.initial.clone(),
                spec: spec.clone(),
                files: Vec::new(),
                name: None,
                form: None,
                disabled,
                readonly: flag(container, "readonly"),
                multiple,
                native_id: None,
                label: String::new(),
                focus: Some(autofocus),
                editor_revision: 0,
                pending: None,
            });
            field.style = style;
            field.reconcile(stamp, spec);
            field.name = name;
            field.form = enclosing_form(document, key);
            field.disabled = disabled;
            field.readonly = flag(container, "readonly");
            if field.multiple != multiple {
                field.reset();
            }
            field.multiple = multiple;
            field.label = container
                .data
                .get("aria-label")
                .or_else(|| container.data.get("label"))
                .cloned()
                .or_else(|| field.name.clone())
                .unwrap_or_default();
            if field.focus.is_some() {
                field.focus = Some(autofocus);
            }
            self.order.push(key);
        }
        self.fields.retain(|key, _| self.order.contains(key));
        self.changes
            .retain(|change| document.is_current(change.source));
        self.actions.retain(|action| match action {
            FormAction::Submit(submission) => {
                document.is_current(submission.form)
                    && submission
                        .submitter
                        .is_none_or(|submitter| document.is_current(submitter))
            }
            FormAction::Reset { form, submitter } => {
                document.is_current(*form)
                    && submitter.is_none_or(|submitter| document.is_current(submitter))
            }
        });
        self.file_requests.retain(|request| {
            self.fields
                .get(&request.source.key)
                .is_some_and(|field| field.pending == Some(*request))
        });
        Ok(())
    }

    fn paint_node(
        &mut self,
        ui: &mut egui::Ui,
        stamp: ElementStamp,
        authored: &Container,
        calculated: &Container,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<NodeOutput, String> {
        // Native controls may have intrinsic minima larger than E's allocated
        // box. The local UI confines paint and registered pointer geometry;
        // it never measures/reflows any HyperChad descendants.
        let mut slot = ui.new_child(
            egui::UiBuilder::new()
                .id(("hyperchad-form-slot", stamp.key))
                .max_rect(geometry.content_box)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        slot.set_clip_rect(
            ui.clip_rect()
                .intersect(geometry.paint_clip)
                .intersect(geometry.content_box),
        );
        let ui = &mut slot;
        if matches!(authored.element, Element::Option { .. }) {
            return self.paint_option(ui, stamp, geometry);
        }
        let field = self
            .fields
            .get_mut(&stamp.key)
            .ok_or("form control was not reconciled")?;
        if geometry.key != stamp.key {
            return Err("form geometry identity does not match".into());
        }
        if field.stamp != stamp {
            return Err("form control stamp is stale".into());
        }
        let id = field.id();
        let rect = geometry.content_box;
        if field.spec.kind == ControlKind::Hidden {
            // Hidden controls remain successful without focus, pointer or accessibility nodes.
            return Ok(NodeOutput {
                response: ui.interact(egui::Rect::NOTHING, id, Sense::hover()),
                value: None,
            });
        }
        if field.spec.kind == ControlKind::File && !self.file_service {
            return Err("file input requires an asynchronous native dialog service".into());
        }
        let mut changed_value = None;
        let mut response = match &mut field.value {
            Value::Text(_) => {
                let (response, value) = paint_text(ui, field, authored, calculated, geometry)?;
                changed_value = value;
                response
            }
            Value::Checked(checked) => {
                let response = paint_checkbox(ui, id, rect, checked, &field.label, field.disabled);
                if response.changed() {
                    changed_value = Some(checked.to_string());
                }
                response
            }
            Value::Selected(_) => paint_select(ui, field, rect, &mut self.changes),
            Value::Files => paint_file(
                ui,
                field,
                rect,
                &mut self.next_file_sequence,
                &mut self.file_requests,
            )?,
        };
        response.rect = response.rect.intersect(rect);
        response.interact_rect = response
            .interact_rect
            .intersect(ui.clip_rect())
            .intersect(rect);
        field.native_id = Some(response.id);
        response = label_and_focus(ui, field, response, rect);
        if response.clicked() {
            response.request_focus();
        }
        if changed_value.is_some() {
            response.mark_changed();
        }
        let implicit_form = if matches!(field.spec.kind, ControlKind::Text | ControlKind::Password)
        {
            if response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                field.form
            } else {
                None
            }
        } else {
            None
        };
        if let Some(form) = implicit_form {
            self.actions
                .push(FormAction::Submit(self.collect_submission(form, None)?));
        }
        Ok(NodeOutput {
            response,
            value: changed_value,
        })
    }

    fn focus(&mut self, context: &egui::Context, source: ElementStamp) -> Result<(), String> {
        Self::focus(self, context, source)
    }

    fn blur(&mut self, context: &egui::Context, source: ElementStamp) -> Result<(), String> {
        Self::blur(self, context, source)
    }

    fn select_all(&mut self, context: &egui::Context, source: ElementStamp) -> Result<(), String> {
        Self::select_all(self, context, source)
    }

    fn take_changes(&mut self) -> Vec<(ElementStamp, crate::events::EventValue)> {
        use crate::events::EventValue;
        Self::take_changes(self)
            .into_iter()
            .map(|change| {
                let values = change.values.into_iter().map(event_value).collect();
                (change.source, EventValue::Array(values))
            })
            .collect()
    }

    fn pending_resources(&self) -> usize {
        self.fields
            .values()
            .filter(|field| field.pending.is_some())
            .count()
    }
}

#[cfg(test)]
mod calculated_style_dispatch_tests {
    use super::*;
    use crate::document::ViewOrigin;
    use hyperchad_renderer::View;

    fn set_path<C: WidgetAdapter>(
        adapter: &mut C,
        source: ElementStamp,
        path: &[&Container],
    ) -> Result<(), String> {
        adapter.set_calculated_text_path(source, path)
    }

    #[test]
    fn generic_adapter_forwards_calculated_style_and_validation_errors() {
        let mut document = Document::new();
        let control = Container {
            id: 2,
            element: Element::Input {
                input: Input::Text {
                    value: Some("inherited".into()),
                    placeholder: None,
                },
                name: None,
                autofocus: None,
            },
            ..Container::default()
        };
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        id: 1,
                        font_weight: Some(FontWeight::Bold),
                        children: vec![control.clone()],
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let source = document.stamp(document.numeric_matches(2)[0]).unwrap();
        let mut forms = FormsAdapter::new();
        forms.reconcile(&document, &[]).unwrap();
        let mut calculated_parent = document.roots()[0].clone();
        calculated_parent.font_family = Some(vec!["Ubuntu Light".into()]);
        calculated_parent.font_weight = Some(FontWeight::Light);
        set_path(&mut forms, source, &[&calculated_parent, &control]).unwrap();
        let style = &forms.fields[&source.key].style;
        assert!(style.calculated_path);
        let job = style
            .job(
                &FontRegistry::compatibility(),
                "inherited",
                24.0,
                f32::INFINITY,
                WhiteSpace::Normal,
            )
            .unwrap();
        assert_eq!(
            job.sections[0].format.font_id,
            FontRegistry::compatibility()
                .resolve(Some(&["Ubuntu Light".into()]), Some(300), 24.0)
                .unwrap()
        );
        let expired_source = ElementStamp {
            generation: source.generation + 1,
            ..source
        };
        for (stamp, path) in [
            (expired_source, vec![&calculated_parent, &control]),
            (source, vec![]),
            (source, vec![&calculated_parent]),
        ] {
            let expected = FormsAdapter::set_calculated_text_path(&mut forms, stamp, &path);
            assert!(expected.is_err());
            assert_eq!(set_path(&mut forms, stamp, &path), expected);
        }
    }
}

#[cfg(test)]
mod allocation_tests;

#[cfg(test)]
mod typography_tests;

#[cfg(test)]
mod tests;

impl FormsAdapter {
    fn paint_option(
        &mut self,
        ui: &mut egui::Ui,
        stamp: ElementStamp,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<NodeOutput, String> {
        let Some(field) = self.fields.values_mut().find(|field| {
            field
                .spec
                .choices
                .iter()
                .any(|choice| choice.key == stamp.key)
        }) else {
            return Err("option must belong to a reconciled select".into());
        };
        let index = field
            .spec
            .choices
            .iter()
            .position(|choice| choice.key == stamp.key)
            .ok_or("option is missing")?;
        let choice = &field.spec.choices[index];
        if choice.generation != stamp.generation {
            return Err("option stamp is stale".into());
        }
        let Value::Selected(selected) = &mut field.value else {
            return Err("option owner is not a select".into());
        };
        let enabled = !field.disabled && !choice.disabled;
        // Options use E's geometry, never ComboBox child layout or a recursive UI.
        let response = ui
            .scope_builder(
                egui::UiBuilder::new().id(("hyperchad-option", stamp.key)),
                |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.put(
                            geometry.content_box,
                            egui::Button::selectable(selected.contains(&index), &choice.label),
                        )
                    })
                    .inner
                },
            )
            .inner;
        response.widget_info(|| {
            WidgetInfo::selected(
                if field.multiple {
                    WidgetType::Checkbox
                } else {
                    WidgetType::RadioButton
                },
                enabled,
                selected.contains(&index),
                &choice.label,
            )
        });
        if response.clicked() && enabled {
            if field.multiple {
                if !selected.remove(&index) {
                    selected.insert(index);
                }
            } else {
                *selected = BTreeSet::from([index]);
            }
            // Do not report a scalar Change on the option: the owner receives the
            // complete ordered selection, preserving empty and repeated values.
            self.changes.push(FormChange {
                source: field.stamp,
                values: selected
                    .iter()
                    .filter_map(|index| field.spec.choices.get(*index))
                    .map(|choice| FormValue::Text(choice.value.clone()))
                    .collect(),
            });
        }
        Ok(NodeOutput {
            response,
            value: None,
        })
    }
}

fn paint_checkbox(
    ui: &mut egui::Ui,
    id: Id,
    rect: egui::Rect,
    checked: &mut bool,
    label: &str,
    disabled: bool,
) -> egui::Response {
    ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
        let side = rect.width().min(rect.height()).max(0.0);
        let spacing = ui.spacing_mut();
        spacing.interact_size = egui::Vec2::splat(side);
        spacing.icon_width = side;
        spacing.icon_width_inner = side * 0.75;
        spacing.button_padding = egui::Vec2::ZERO;
        let response = ui
            .add_enabled_ui(!disabled, |ui| {
                ui.put(rect, egui::Checkbox::without_text(&mut *checked))
            })
            .inner;
        // The accessible name does not consume shared child layout space.
        response
            .widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, !disabled, *checked, label));
        response
    })
    .inner
}

fn prepare_editor(context: &egui::Context, id: Id, revision: u64) {
    let revision_id = id.with("revision");
    let previous = context.data_mut(|data| data.get_temp::<u64>(revision_id));
    if previous != Some(revision) {
        egui::text_edit::TextEditState::default().store(context, id);
        context.data_mut(|data| data.insert_temp(revision_id, revision));
    }
}

fn paint_text(
    ui: &mut egui::Ui,
    field: &mut Field,
    authored: &Container,
    calculated: &Container,
    geometry: &NodeGeometry<ElementKey>,
) -> Result<(egui::Response, Option<String>), String> {
    let rect = geometry.content_box;
    let mut style = field.style.clone();
    style.families = calculated.font_family.clone().or(style.families);
    style.whitespace = calculated.white_space.or(style.whitespace);
    style.color = calculated.color.or(style.color);
    if !style.calculated_path && calculated.font_weight != authored.font_weight {
        if authored.font_weight.is_some() {
            style.weights.pop();
        }
        if let Some(weight) = calculated.font_weight {
            style.weights.push(weight);
        }
    }
    let size = calculated
        .calculated_font_size
        .or_else(|| (!style.authored_size).then_some(14.0))
        .ok_or("editor requires calculated inherited font size")?
        * geometry.transform.scale;
    let registry = FontRegistry::for_context(ui.ctx());
    let multiline = field.spec.kind == ControlKind::Textarea;
    let whitespace = style.whitespace.unwrap_or_default();
    let mut template = style.job(
        &registry,
        "",
        size,
        if multiline {
            rect.width()
        } else {
            f32::INFINITY
        },
        whitespace,
    )?;
    let font = template.sections[0].format.font_id.clone();
    let foreground = style
        .color
        .map_or_else(|| ui.visuals().text_color(), super::paint::color);
    template.sections[0].format.color = foreground;
    let password = field.spec.kind == ControlKind::Password;
    let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| {
        let mut job = template.clone();
        job.text = editor_display(buffer.as_str(), whitespace, password);
        job.sections[0].byte_range = 0..job.text.len();
        if multiline && whitespace != WhiteSpace::Preserve {
            job.wrap.max_width = width;
        }
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let id = field.id();
    prepare_editor(ui.ctx(), id, field.editor_revision);
    let Value::Text(value) = &mut field.value else {
        unreachable!()
    };
    let mut changed_value = None;
    let placeholder = match &authored.element {
        Element::Input {
            input: Input::Text { placeholder, .. } | Input::Password { placeholder, .. },
            ..
        }
        | Element::Textarea { placeholder, .. } => placeholder.as_deref().unwrap_or_default(),
        _ => "",
    };
    let editor = if field.spec.kind == ControlKind::Textarea {
        egui::TextEdit::multiline(&mut *value)
    } else {
        egui::TextEdit::singleline(&mut *value)
    };
    let response = ui
        .add_enabled_ui(!field.disabled, |ui| {
            ui.put(
                rect,
                editor
                    .font(font)
                    .layouter(&mut layouter)
                    .text_color(foreground)
                    .id(id)
                    .password(password)
                    .interactive(!field.readonly)
                    .hint_text(placeholder)
                    .margin(egui::Margin::ZERO)
                    .min_size(rect.size())
                    .desired_rows(1)
                    .desired_width(rect.width()),
            )
        })
        .inner;
    // Password values are emitted only as event payloads, never WidgetInfo text.
    if response.changed() {
        changed_value = Some(value.clone());
    }
    Ok((response, changed_value))
}

/// Preserve one galley character per buffer character. Collapsed ASCII whitespace
/// is represented by zero-advance, nonbreaking glyphs, never removed from the
/// editor buffer: egui's Unicode cursor indices remain authoritative. NBSP stays.
fn editor_display(text: &str, whitespace: WhiteSpace, password: bool) -> String {
    if password {
        return std::iter::repeat_n(
            egui::epaint::text::PASSWORD_REPLACEMENT_CHAR,
            text.chars().count(),
        )
        .collect();
    }
    if whitespace != WhiteSpace::Normal {
        return text.to_owned();
    }
    let mut previous_space = false;
    text.chars()
        .map(|character| {
            let space = matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{000c}');
            let rendered = if space {
                if previous_space { '\u{2060}' } else { ' ' }
            } else {
                character
            };
            previous_space = space;
            rendered
        })
        .collect()
}

fn paint_select(
    ui: &mut egui::Ui,
    field: &mut Field,
    rect: egui::Rect,
    changes: &mut Vec<FormChange>,
) -> egui::Response {
    let id = field.id();
    let Value::Selected(selected) = &mut field.value else {
        unreachable!()
    };
    let mut response = ui
        .add_enabled_ui(!field.disabled, |ui| ui.interact(rect, id, Sense::click()))
        .inner;
    if !field.disabled && response.has_focus() {
        let direction = ui.input(|input| {
            if input.key_pressed(Key::ArrowDown) {
                1
            } else if input.key_pressed(Key::ArrowUp) {
                -1
            } else {
                0
            }
        });
        if direction != 0 {
            let start = selected.first().copied();
            let indices: Vec<usize> = field
                .spec
                .choices
                .iter()
                .enumerate()
                .filter(|(_, choice)| !choice.disabled)
                .map(|(index, _)| index)
                .collect();
            let next = if direction > 0 {
                indices
                    .iter()
                    .copied()
                    .find(|index| start.is_none_or(|start| *index > start))
                    .or_else(|| indices.first().copied())
            } else {
                indices
                    .iter()
                    .rev()
                    .copied()
                    .find(|index| start.is_none_or(|start| *index < start))
                    .or_else(|| indices.last().copied())
            };
            if let Some(next) = next {
                if field.multiple && ui.input(|input| input.modifiers.shift) {
                    selected.insert(next);
                } else {
                    *selected = BTreeSet::from([next]);
                }
                response.mark_changed();
            }
        }
    }
    let label = selected
        .iter()
        .filter_map(|index| field.spec.choices.get(*index))
        .map(|choice| choice.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        &label,
        egui::FontId::default(),
        ui.visuals().text_color(),
    );
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(WidgetType::ComboBox, !field.disabled, &field.label);
        info.current_text_value = Some(label.clone());
        info
    });
    if response.changed() {
        changes.push(FormChange {
            source: field.stamp,
            values: selected
                .iter()
                .filter_map(|index| field.spec.choices.get(*index))
                .map(|choice| FormValue::Text(choice.value.clone()))
                .collect(),
        });
    }
    response
}

impl FormsAdapter {
    fn register_form(
        &mut self,
        document: &Document,
        key: ElementKey,
        stamp: ElementStamp,
        container: &Container,
    ) {
        if let Element::Form {
            action,
            method,
            multipart,
        } = &container.element
        {
            self.forms
                .insert(key, (stamp, action.clone(), method.clone(), *multipart));
        }
        if let Element::Button { r#type } = &container.element
            && !flag(container, "disabled")
            && r#type
                .as_deref()
                .is_none_or(|kind| kind != "button" && kind != "reset")
            && let Some(form) = enclosing_form(document, key)
        {
            self.submitters.insert(key, (stamp, form));
        }
    }
}

fn paint_file(
    ui: &mut egui::Ui,
    field: &mut Field,
    rect: egui::Rect,
    next_file_sequence: &mut u64,
    requests: &mut Vec<FileRequest>,
) -> Result<egui::Response, String> {
    let id = field.id();
    let response = ui
        .scope_builder(egui::UiBuilder::new().id(id), |ui| {
            ui.add_enabled_ui(!field.disabled, |ui| {
                ui.put(
                    rect,
                    egui::Button::new(if field.label.is_empty() {
                        "Choose file"
                    } else {
                        &field.label
                    }),
                )
            })
            .inner
        })
        .inner;
    if response.clicked() {
        *next_file_sequence = next_file_sequence
            .checked_add(1)
            .ok_or("file request sequence exhausted")?;
        let request = FileRequest {
            source: field.stamp,
            sequence: *next_file_sequence,
            multiple: field.multiple,
        };
        field.pending = Some(request);
        requests.push(request);
    }
    Ok(response)
}

fn label_and_focus(
    ui: &egui::Ui,
    field: &mut Field,
    mut response: egui::Response,
    rect: egui::Rect,
) -> egui::Response {
    let id = field.id();
    if let Some(autofocus) = field.focus.take()
        && autofocus
        && !field.disabled
    {
        response.request_focus();
    }
    if !field.label.is_empty()
        && matches!(
            field.spec.kind,
            ControlKind::Text | ControlKind::Password | ControlKind::Textarea
        )
    {
        // Never call widget_info again on the editor: doing so overwrites
        // native PasswordInput/MultilineTextInput roles with TextInput.
        // A semantic-only label preserves TextEdit's role/selection/value.
        let label_id = id.with("accessible-label");
        let label = ui.interact(
            egui::Rect::from_min_size(rect.min, egui::Vec2::ZERO),
            label_id,
            Sense::hover(),
        );
        label.widget_info(|| WidgetInfo::labeled(WidgetType::Label, !field.disabled, &field.label));
        response = response.labelled_by(label_id);
    }
    response
}

fn event_value(value: FormValue) -> crate::events::EventValue {
    use crate::events::EventValue;
    match value {
        FormValue::Text(value) => EventValue::String(value),
        FormValue::File(file) => EventValue::Object(BTreeMap::from([
            ("name".into(), EventValue::String(file.name)),
            (
                "mediaType".into(),
                file.media_type.map_or(EventValue::Null, EventValue::String),
            ),
            (
                "bytes".into(),
                EventValue::Array(
                    file.bytes
                        .iter()
                        .map(|byte| EventValue::Number(f64::from(*byte)))
                        .collect(),
                ),
            ),
        ])),
    }
}

#[cfg(test)]
mod focus_effect_tests {
    use super::*;
    use crate::document::ViewOrigin;
    use hyperchad_renderer::View;

    fn fixture(
        element: Element,
        attributes: &[(&str, &str)],
    ) -> (Document, FormsAdapter, ElementStamp) {
        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        id: 1,
                        element,
                        data: attributes
                            .iter()
                            .map(|(key, value)| ((*key).into(), (*value).into()))
                            .collect(),
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let mut forms = FormsAdapter::new();
        forms.reconcile(&document, &[]).unwrap();
        let source = document.stamp(document.root_keys()[0]).unwrap();
        (document, forms, source)
    }

    fn input(input: Input) -> Element {
        Element::Input {
            input,
            name: Some("field".into()),
            autofocus: Some(true),
        }
    }

    fn paint(
        context: &egui::Context,
        forms: &mut FormsAdapter,
        document: &Document,
        source: ElementStamp,
    ) -> Id {
        let mut response_id = None;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let rect =
                    egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(220.0, 32.0));
                let geometry = NodeGeometry {
                    key: source.key,
                    container_id: 1,
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
                    scroll_max: egui::Vec2::ZERO,
                    scroll_offset: egui::Vec2::ZERO,
                    visible: true,
                    fixed: false,
                };
                let authored = document.get(source.key).unwrap();
                response_id = Some(
                    forms
                        .paint_node(ui, source, authored, authored, &geometry)
                        .unwrap()
                        .response
                        .id,
                );
            });
        });
        response_id.unwrap()
    }

    #[test]
    fn generic_adapter_dispatches_native_focus_selection_blur_and_rejects_stale() {
        fn exercise<C: WidgetAdapter>(
            adapter: &mut C,
            context: &egui::Context,
            source: ElementStamp,
            id: Id,
        ) {
            adapter.focus(context, source).unwrap();
            adapter.select_all(context, source).unwrap();
            assert_eq!(context.memory(egui::Memory::focused), Some(id));
            let selection = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(4),
            );
            assert_eq!(
                egui::text_edit::TextEditState::load(context, id)
                    .unwrap()
                    .cursor
                    .char_range(),
                Some(selection)
            );
            let stale = ElementStamp {
                generation: source.generation + 1,
                ..source
            };
            assert!(adapter.focus(context, stale).is_err());
            assert!(adapter.select_all(context, stale).is_err());
            assert!(adapter.blur(context, stale).is_err());
            assert_eq!(context.memory(egui::Memory::focused), Some(id));
            assert_eq!(
                egui::text_edit::TextEditState::load(context, id)
                    .unwrap()
                    .cursor
                    .char_range(),
                Some(selection)
            );
            adapter.blur(context, source).unwrap();
            assert_eq!(context.memory(egui::Memory::focused), None);
        }
        let (document, mut forms, source) = fixture(
            input(Input::Text {
                value: Some("aé🦀z".into()),
                placeholder: None,
            }),
            &[],
        );
        let context = egui::Context::default();
        let id = paint(&context, &mut forms, &document, source);
        assert_eq!(forms.focus_id(source).unwrap(), id);
        exercise(&mut forms, &context, source, id);
    }

    #[test]
    fn native_focus_selection_and_blur_use_authoritative_ids_across_paint_and_reset() {
        for element in [
            input(Input::Text {
                value: Some("aé🦀z".into()),
                placeholder: None,
            }),
            input(Input::Password {
                value: Some("aé🦀z".into()),
                placeholder: None,
            }),
            Element::Textarea {
                value: "aé🦀z".into(),
                placeholder: None,
                name: None,
                rows: None,
                cols: None,
            },
        ] {
            let (document, mut forms, source) = fixture(element, &[]);
            let context = egui::Context::default();
            let id = forms.focus_id(source).unwrap();
            forms.focus(&context, source).unwrap();
            forms.select_all(&context, source).unwrap();
            assert_eq!(context.memory(egui::Memory::focused), Some(id));
            assert_eq!(paint(&context, &mut forms, &document, source), id);
            let selection = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(4),
            );
            assert_eq!(
                egui::text_edit::TextEditState::load(&context, id)
                    .unwrap()
                    .cursor
                    .char_range(),
                Some(selection)
            );
            forms.reset_control(source).unwrap();
            forms.select_all(&context, source).unwrap();
            paint(&context, &mut forms, &document, source);
            assert_eq!(
                egui::text_edit::TextEditState::load(&context, id)
                    .unwrap()
                    .cursor
                    .char_range(),
                Some(selection)
            );
            forms.blur(&context, source).unwrap();
            assert_eq!(context.memory(egui::Memory::focused), None);
            let other = Id::new("different native control");
            context.memory_mut(|memory| memory.request_focus(other));
            forms.blur(&context, source).unwrap();
            assert_eq!(context.memory(egui::Memory::focused), Some(other));
        }
    }

    #[test]
    fn rejects_stale_hidden_disabled_readonly_and_nontext_selection_without_side_effects() {
        let (_, mut forms, source) = fixture(
            input(Input::Text {
                value: Some("text".into()),
                placeholder: None,
            }),
            &[],
        );
        let context = egui::Context::default();
        let stale = ElementStamp {
            generation: source.generation + 1,
            ..source
        };
        assert!(forms.focus_id(stale).is_err());
        assert!(forms.focus(&context, stale).is_err());
        assert!(forms.blur(&context, stale).is_err());
        assert!(forms.select_all(&context, stale).is_err());
        assert_eq!(context.memory(egui::Memory::focused), None);
        for (element, attributes) in [
            (input(Input::Hidden { value: None }), vec![]),
            (
                input(Input::Text {
                    value: None,
                    placeholder: None,
                }),
                vec![("disabled", "true")],
            ),
        ] {
            let (_, mut forms, source) = fixture(element, &attributes);
            assert!(forms.focus_id(source).is_err());
            assert!(forms.focus(&context, source).is_err());
            assert!(forms.select_all(&context, source).is_err());
        }
        let (_, forms, source) = fixture(
            input(Input::Text {
                value: Some("readonly".into()),
                placeholder: None,
            }),
            &[("readonly", "true")],
        );
        assert!(forms.focus_id(source).is_ok());
        assert!(forms.select_all(&context, source).is_err());
        let (document, mut forms, source) = fixture(
            input(Input::Checkbox {
                checked: Some(true),
            }),
            &[],
        );
        assert!(forms.focus_id(source).is_err());
        assert!(forms.select_all(&context, source).is_err());
        let actual_id = paint(&context, &mut forms, &document, source);
        assert_eq!(forms.focus_id(source).unwrap(), actual_id);
        forms.focus(&context, source).unwrap();
        assert_eq!(context.memory(egui::Memory::focused), Some(actual_id));
    }
}

#[cfg(test)]
mod integration_state_tests {
    use super::*;
    use crate::{document::ViewOrigin, events::EventValue};
    use hyperchad_renderer::View;

    #[test]
    fn effective_state_is_stamped_and_reset_retires_queued_changes() {
        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        id: 1,
                        element: Element::Form {
                            action: None,
                            method: None,
                            multipart: false,
                        },
                        children: vec![Container {
                            id: 2,
                            element: Element::Input {
                                input: Input::Text {
                                    value: Some("default".into()),
                                    placeholder: None,
                                },
                                name: Some(String::new()),
                                autofocus: None,
                            },
                            ..Container::default()
                        }],
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let mut forms = FormsAdapter::new();
        forms.reconcile(&document, &[]).unwrap();
        let source = forms.control_stamps()[0];
        let field = forms.fields.get_mut(&source.key).unwrap();
        field.value = Value::Text("effective local edit".into());
        field.focus = None;
        let id = field.id();
        assert_eq!(
            forms.current_value(source).unwrap(),
            EventValue::String("effective local edit".into())
        );
        let stale = ElementStamp {
            generation: source.generation + 1,
            ..source
        };
        assert!(forms.current_value(stale).is_err());
        assert!(forms.reset_control(stale).is_err());
        assert_eq!(
            forms.current_value(source).unwrap(),
            EventValue::String("effective local edit".into())
        );
        forms.changes.push(FormChange {
            source,
            values: vec![FormValue::Text("obsolete".into())],
        });
        forms.reset_form(document.root_keys()[0]).unwrap();
        assert_eq!(WidgetAdapter::take_changes(&mut forms), []);
        assert_eq!(
            forms.current_value(source).unwrap(),
            EventValue::String("default".into())
        );
        assert_eq!(forms.fields[&source.key].id(), id);
        assert!(forms.fields[&source.key].focus.is_none());
        let submission = forms
            .collect_submission(document.root_keys()[0], None)
            .unwrap();
        assert_eq!(submission.entries[0].name, "");
        assert!(
            matches!(&submission.entries[0].value, FormValue::Text(value) if value == "default")
        );
    }
}
