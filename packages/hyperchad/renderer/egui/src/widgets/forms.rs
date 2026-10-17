//! Persistent native form controls. No document layout, routing or blocking I/O.
//!
//! The host drains [`FormsAdapter::take_actions`] and
//! [`FormsAdapter::take_file_requests`] after traversal. A native dialog service
//! performs its work outside paint and returns bytes through `complete_file`.

pub mod native_files;
pub use native_files::{FileLimits, FileServiceError, NativeFileService};

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use eframe::egui::{self, Id, Key, Sense, WidgetInfo, WidgetType};
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

struct Field {
    stamp: ElementStamp,
    spec: Spec,
    value: Value,
    files: Vec<FormFile>,
    name: Option<String>,
    form: Option<ElementKey>,
    disabled: bool,
    multiple: bool,
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
            let field = self.fields.entry(key).or_insert_with(|| Field {
                stamp,
                value: spec.initial.clone(),
                spec: spec.clone(),
                files: Vec::new(),
                name: None,
                form: None,
                disabled,
                multiple,
                label: String::new(),
                focus: Some(autofocus),
                editor_revision: 0,
                pending: None,
            });
            field.reconcile(stamp, spec);
            field.name = name;
            field.form = enclosing_form(document, key);
            field.disabled = disabled;
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
        _calculated: &Container,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<NodeOutput, String> {
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
                let (response, value) = paint_text(ui, field, authored, rect);
                changed_value = value;
                response
            }
            Value::Checked(checked) => {
                let response = ui
                    .scope_builder(egui::UiBuilder::new().id(id), |ui| {
                        ui.add_enabled_ui(!field.disabled, |ui| {
                            ui.put(rect, egui::Checkbox::new(&mut *checked, &field.label))
                        })
                        .inner
                    })
                    .inner;
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

    fn take_changes(&mut self) -> Vec<(ElementStamp, crate::events::EventValue)> {
        use crate::events::EventValue;
        FormsAdapter::take_changes(self)
            .into_iter()
            .map(|change| {
                let values = change
                    .values
                    .into_iter()
                    .map(|value| match value {
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
                    })
                    .collect();
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

fn paint_text(
    ui: &mut egui::Ui,
    field: &mut Field,
    authored: &Container,
    rect: egui::Rect,
) -> (egui::Response, Option<String>) {
    let id = field.id();
    let Value::Text(value) = &mut field.value else {
        unreachable!()
    };
    let mut changed_value = None;
    let password = field.spec.kind == ControlKind::Password;
    let placeholder = match &authored.element {
        Element::Input {
            input: Input::Text { placeholder, .. } | Input::Password { placeholder, .. },
            ..
        }
        | Element::Textarea { placeholder, .. } => placeholder.as_deref().unwrap_or_default(),
        _ => "",
    };
    // Keep the focus ID stable. Reset only the editor's cursor/undo state.
    let revision_id = id.with("revision");
    let previous = ui.ctx().data_mut(|data| data.get_temp::<u64>(revision_id));
    if previous != Some(field.editor_revision) {
        egui::text_edit::TextEditState::default().store(ui.ctx(), id);
        ui.ctx()
            .data_mut(|data| data.insert_temp(revision_id, field.editor_revision));
    }
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
                    .id(id)
                    .password(password)
                    .hint_text(placeholder)
                    .desired_width(rect.width()),
            )
        })
        .inner;
    // Password values are emitted only as event payloads, never WidgetInfo text.
    if response.changed() {
        changed_value = Some(value.clone());
    }
    (response, changed_value)
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
    ui: &mut egui::Ui,
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
