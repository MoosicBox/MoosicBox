//! Persistent native terminal controls for an immutable `HyperChad` document.
//!
//! Reconcile after canonical updates, measure with `measurer`, then `prepare`
//! from the authoritative layout before painting or dispatching input. The host
//! consumes semantic events; it must not repeat the control's default behavior.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use crate::actions::AuthorizedFile;
use bmux_keyboard::KeyCode;
use bmux_text_edit::TextEditBuffer;
use bmux_tui::component::{Constraints, LayoutCx, LayoutNode, LogicalSize};
use bmux_tui::event::{Event, MouseButton, MouseEventKind};
use bmux_tui::geometry::{Point, Rect};
use bmux_tui::hit::{HitRegion, HitRole};
use bmux_tui::paint::{LocalRect, PaintCx};
use bmux_tui::style::{Modifier, Style};
use bmux_tui_components::button::ButtonState;
use bmux_tui_components::checkbox::CheckboxState;
use bmux_tui_components::select_dropdown::SelectDropdownState;
use bmux_tui_components::text_input::TextInputState;
use hyperchad_transformer::Container;

use crate::layout::{CellRect, LayoutTree, LeafMeasure, node_key};

mod events;
mod native;
mod spec;
#[cfg(test)]
mod tests;

use spec::{Kind, Spec};

/// A control value. Secrets are deliberately absent from semantic event payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlValue {
    /// Text, hidden value, or single select value.
    Text(String),
    /// Checked state.
    Checked(bool),
    /// Multi-select values in document option order.
    Multiple(Vec<String>),
    /// A password changed; retrieve successful form values only at submission.
    SecretChanged,
    /// Host-selected file references, not file contents.
    Files(Vec<AuthorizedFile>),
}

/// Ordered successful form controls, preserving duplicate names.
///
/// This type intentionally has no `Debug`: values can contain passwords. The
/// host must not include these values in traces, snapshots, or diagnostics.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct FormValues(pub Vec<(String, String)>);

/// Renderer-neutral semantic effects. No network or navigation is performed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlEvent {
    /// Control activation; run the element's click actions once.
    Activate { key: String },
    /// A user changed a value; run change actions once.
    Change { key: String, value: ControlValue },
    /// Submit a form, with the activating button if any.
    Submit {
        form: String,
        submitter: Option<String>,
    },
    /// A form was reset to its latest document defaults.
    Reset { form: String },
    /// Details changed; feed this state into layout's details overrides.
    Toggle { key: String, open: bool },
    /// An anchor was activated; host applies navigation policy.
    Navigate { key: String, href: String },
    /// Host must request files and call `set_files` on completion.
    FileRequest { key: String, multiple: bool },
    /// Focus changed, including removal of the focused control.
    Focus { key: Option<String> },
    /// Reveal the focused node through its ancestor scroll viewports.
    Reveal { key: String },
    /// Editor viewport scrolled or cursor reached a vertical boundary.
    Scroll { key: String, edge: i8 },
    /// Native popup changed; remeasure before the next paint.
    Popup { key: String, open: bool },
}

/// Dispatch distinguishes consumption from repaint and relayout requirements.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ControlOutcome {
    pub consumed: bool,
    pub redraw: bool,
    pub relayout: bool,
    pub events: Vec<ControlEvent>,
}

impl ControlOutcome {
    fn handled() -> Self {
        Self {
            consumed: true,
            redraw: true,
            ..Self::default()
        }
    }

    fn extend(&mut self, other: Self) {
        self.consumed |= other.consumed;
        self.redraw |= other.redraw;
        self.relayout |= other.relayout;
        self.events.extend(other.events);
    }
}

#[derive(Clone)]
struct Prepared {
    layout: LayoutNode,
    bounds: CellRect,
    clip: CellRect,
    order: usize,
    path: Vec<usize>,
    hit_regions: Vec<CellRect>,
}

impl Prepared {
    fn regions(&self) -> &[CellRect] {
        if self.hit_regions.is_empty() {
            std::slice::from_ref(&self.bounds)
        } else {
            &self.hit_regions
        }
    }

    fn contains(&self, x: i64, y: i64) -> bool {
        self.clip.contains(x, y) && self.regions().iter().any(|region| region.contains(x, y))
    }
}

enum NativeState {
    Text(Box<RefCell<TextInputState>>),
    Checkbox(Cell<CheckboxState>),
    Select(Cell<SelectDropdownState>),
    Multiple(Vec<Cell<CheckboxState>>),
    Button(Cell<ButtonState>),
    Details(bool),
    Hidden,
}

struct Control {
    spec: Spec,
    state: NativeState,
    prepared: Option<Prepared>,
    files: Vec<AuthorizedFile>,
    secret_pointer: RefCell<TextInputState>,
    active_option: Cell<usize>,
}

/// Caller-owned state keyed exclusively by `layout::node_key`.
///
/// Native editor buffers can contain secrets, so this store deliberately does
/// not implement `Debug` or serialization.
#[derive(Default)]
pub struct Controls {
    nodes: BTreeMap<String, Control>,
    order: Vec<String>,
    focused: Option<String>,
    captured: Option<String>,
}

impl Controls {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconcile without modifying the canonical tree. Unchanged external values
    /// preserve local edits, selection, undo history, and popup state. A changed
    /// external value replaces that value; incompatible kinds reset all state.
    pub fn reconcile(&mut self, root: &Container) -> ControlOutcome {
        let specs = spec::collect(root);
        let mut old = std::mem::take(&mut self.nodes);
        self.order.clear();
        for (key, mut spec) in specs {
            if let Some(previous) = old.get(&key) {
                // Absent external values mean uncontrolled, not an instruction
                // to clear the user's value.
                if spec.checked_external.is_none() && previous.spec.kind == spec.kind {
                    spec.checked = previous.spec.checked;
                    spec.checked_external = previous.spec.checked_external;
                }
                if spec.external.is_none() && previous.spec.kind == spec.kind {
                    spec.external.clone_from(&previous.spec.external);
                }
            }
            let mut control = match old.remove(&key) {
                Some(mut old) if old.spec.kind == spec.kind && old.spec.name == spec.name => {
                    old.reconcile(&spec);
                    old.spec = spec;
                    old
                }
                _ => Control::new(spec),
            };
            control.prepared = None;
            self.order.push(key.clone());
            self.nodes.insert(key, control);
        }
        self.captured = None;
        let mut outcome = ControlOutcome::default();
        if self.focused.as_ref().is_some_and(|key| {
            !self.nodes.contains_key(key)
                || self.nodes[key].spec.disabled
                || !self.nodes[key].spec.focusable()
        }) {
            outcome.extend(self.focus(None));
        }
        if self.focused.is_none()
            && let Some(key) = self
                .order
                .iter()
                .find(|key| {
                    let spec = &self.nodes[*key].spec;
                    spec.autofocus && !spec.disabled && spec.focusable()
                })
                .cloned()
        {
            outcome.extend(self.focus(Some(&key)));
        }
        outcome
    }

    /// Apply the canonical state lane's compatibility decisions. Removed/replaced
    /// identities reset even when a replacement happens to have the same kind.
    pub fn reconcile_report(
        &mut self,
        root: &Container,
        report: &crate::state::ApplyReport,
    ) -> ControlOutcome {
        for key in &report.removed {
            self.nodes.remove(key);
        }
        for update in &report.controls {
            if !update.compatible {
                self.nodes.remove(&update.key);
            }
        }
        self.reconcile(root)
    }

    /// A source-bound measurer resolves structural-path identities correctly.
    /// Use this instead of guessing a key inside `LeafMeasure::measure`.
    #[must_use]
    pub fn measurer<'a>(&'a self, root: &'a Container) -> impl LeafMeasure + 'a {
        fn visit(node: &Container, path: &mut Vec<usize>, keys: &mut BTreeMap<usize, String>) {
            keys.insert(std::ptr::from_ref(node) as usize, node_key(node, path));
            for (index, child) in node.children.iter().enumerate() {
                path.push(index);
                visit(child, path, keys);
                path.pop();
            }
        }
        let mut keys = BTreeMap::new();
        visit(root, &mut Vec::new(), &mut keys);
        move |node: &Container, constraints: Constraints| {
            keys.get(&(std::ptr::from_ref(node) as usize))
                .and_then(|key| self.measure(key, constraints))
                .unwrap_or_else(|| crate::layout::DefaultLeafMeasure.measure(node, constraints))
        }
    }

    /// Measure real native component geometry using retained control state.
    #[expect(
        clippy::must_use_candidate,
        reason = "Option is already must-use; repository policy forbids redundant getter attributes"
    )]
    pub fn measure(&self, key: &str, constraints: Constraints) -> Option<LogicalSize> {
        let control = self.nodes.get(key)?;
        if matches!(control.spec.kind, Kind::Hidden | Kind::Details) {
            return Some(LogicalSize::new(0, 0));
        }
        Some(
            control
                .component(key, self.focused.as_deref() == Some(key))
                .layout(constraints, &mut LayoutCx::new())
                .size,
        )
    }

    /// Resolve and retain native layouts inside the layout lane's assigned content
    /// rectangles. Hidden/closed descendants never gain paint or input geometry.
    pub fn prepare(&mut self, tree: &LayoutTree) {
        for control in self.nodes.values_mut() {
            control.prepared = None;
        }
        for node in &tree.nodes {
            if !node.visible {
                continue;
            }
            if let Some(control) = self.nodes.get_mut(&node.key) {
                if matches!(control.spec.kind, Kind::Hidden | Kind::Details) {
                    continue;
                }
                let bounds = node.content_bounds;
                let constraints = Constraints::new(
                    bounds.width,
                    bounds.width,
                    bounds.height,
                    Some(bounds.height),
                );
                let layout = control
                    .component(&node.key, self.focused.as_deref() == Some(&node.key))
                    .layout(constraints, &mut LayoutCx::new());
                control.prepared = Some(Prepared {
                    layout,
                    bounds,
                    clip: node.clip,
                    order: node.paint_order,
                    path: node.path.clone(),
                    hit_regions: node.hit_regions.clone(),
                });
            }
        }
    }

    /// Paint one control using its retained native layout. `cx` is viewport-root
    /// relative; this method applies placement and clipping exactly once.
    /// Returns false when this key has no painted native control.
    pub fn paint(&self, key: &str, cx: &mut PaintCx<'_, '_>) -> bool {
        let Some(control) = self.nodes.get(key) else {
            return false;
        };
        let Some(prepared) = &control.prepared else {
            return false;
        };
        if control.spec.kind == Kind::Anchor {
            // Inline formatting owns glyphs and descendant styles; links add
            // interaction metadata only, never chrome or replacement text.
            for region in prepared.regions() {
                let visible = region.intersection(prepared.clip);
                if visible.width == 0 || visible.height == 0 {
                    continue;
                }
                cx.push_hit(
                    HitRegion::new(key, visible.terminal())
                        .role(HitRole::Action)
                        .focusable(true)
                        .hoverable(true)
                        .enabled(!control.spec.disabled),
                );
            }
            return false;
        }
        let clip = prepared.clip.intersection(prepared.bounds);
        if clip.width == 0 || clip.height == 0 {
            return false;
        }
        let x = i32::try_from(prepared.bounds.x).unwrap_or(if prepared.bounds.x < 0 {
            i32::MIN
        } else {
            i32::MAX
        });
        let y = prepared.bounds.y;
        let local = LocalRect::new(
            i32::try_from(clip.x.saturating_sub(prepared.bounds.x)).unwrap_or(i32::MAX),
            clip.y.saturating_sub(y),
            u16::try_from(clip.width).unwrap_or(u16::MAX),
            u16::try_from(clip.height).unwrap_or(u16::MAX),
        );
        cx.with_child(x, y, local, |cx| {
            control
                .component(key, self.focused.as_deref() == Some(key))
                .paint(&prepared.layout, cx);
        });
        true
    }

    /// Decorate a focused link's source fragment without changing text/geometry.
    /// The inline painter should pass each fragment source path through this hook.
    #[must_use]
    pub fn inline_style(&self, source_path: &[usize], mut style: Style) -> Style {
        if let Some(control) = self.focused.as_ref().and_then(|key| self.nodes.get(key))
            && control.spec.kind == Kind::Anchor
            && control
                .prepared
                .as_ref()
                .is_some_and(|prepared| source_path.starts_with(&prepared.path))
        {
            style.modifiers |= Modifier::UNDERLINE | Modifier::REVERSED;
        }
        style
    }

    #[expect(
        clippy::must_use_candidate,
        reason = "Option is already must-use; repository policy forbids redundant getter attributes"
    )]
    pub fn focused(&self) -> Option<&str> {
        self.focused.as_deref()
    }

    /// Set focus programmatically. The host uses `Reveal` to update scroll offsets.
    pub fn focus(&mut self, key: Option<&str>) -> ControlOutcome {
        if key.is_some_and(|key| {
            self.nodes
                .get(key)
                .is_none_or(|control| control.spec.disabled || !control.spec.focusable())
        }) {
            return ControlOutcome::default();
        }
        if self.focused.as_deref() == key {
            return ControlOutcome::default();
        }
        self.focused = key.map(str::to_owned);
        let mut outcome = ControlOutcome::handled();
        outcome.events.push(ControlEvent::Focus {
            key: self.focused.clone(),
        });
        if let Some(key) = key {
            outcome.events.push(ControlEvent::Reveal {
                key: key.to_owned(),
            });
        }
        outcome
    }

    /// Traverse only prepared, enabled controls, in document order. The host can
    /// instead supply focus from a broader document focus router.
    pub fn traverse(&mut self, backwards: bool) -> ControlOutcome {
        let keys: Vec<_> = self
            .order
            .iter()
            .filter(|key| {
                let control = &self.nodes[*key];
                control.spec.focusable() && !control.spec.disabled && control.prepared.is_some()
            })
            .cloned()
            .collect();
        if keys.is_empty() {
            return self.focus(None);
        }
        let index = self
            .focused
            .as_ref()
            .and_then(|key| keys.iter().position(|candidate| candidate == key));
        let next = match (index, backwards) {
            (Some(0) | None, true) => keys.len() - 1,
            (Some(i), true) => i - 1,
            (Some(i), false) => (i + 1) % keys.len(),
            (None, false) => 0,
        };
        self.focus(Some(&keys[next]))
    }

    /// Route native controls without a separate host interaction router.
    pub fn event(&mut self, event: &Event) -> ControlOutcome {
        if let Event::Key(stroke) = event
            && stroke.key == KeyCode::Tab
            && !stroke.modifiers.ctrl
            && !stroke.modifiers.alt
        {
            return self.traverse(stroke.modifiers.shift);
        }
        let target = match event {
            Event::Mouse(mouse) => self.captured.clone().or_else(|| {
                self.nodes
                    .iter()
                    .filter(|(_, control)| {
                        !control.spec.disabled
                            && control.prepared.as_ref().is_some_and(|p| {
                                p.contains(i64::from(mouse.position.x), i64::from(mouse.position.y))
                            })
                    })
                    .max_by_key(|(_, control)| control.prepared.as_ref().map(|p| p.order))
                    .map(|(key, _)| key.clone())
            }),
            _ => self.focused.clone(),
        };
        let Some(target) = target else {
            return ControlOutcome::default();
        };
        let mut outcome = ControlOutcome::default();
        if matches!(event, Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left))
        {
            outcome.extend(self.focus(Some(&target)));
            self.captured = Some(target.clone());
        }
        outcome.extend(self.dispatch_to(&target, event));
        if matches!(event, Event::Mouse(mouse) if mouse.kind == MouseEventKind::Up(MouseButton::Left))
        {
            self.captured = None;
        }
        outcome
    }

    /// Dispatch a host-router-resolved event. Pointer coordinates remain viewport
    /// relative; this method translates them into retained native layout space.
    pub fn dispatch_to(&mut self, key: &str, event: &Event) -> ControlOutcome {
        let focused = self.focused.as_deref() == Some(key);
        let Some(control) = self.nodes.get_mut(key) else {
            return ControlOutcome::default();
        };
        if control.spec.disabled {
            return ControlOutcome::default();
        }
        let Some(prepared) = control.prepared.clone() else {
            return ControlOutcome::default();
        };
        let mut local_event = event.clone();
        if let Event::Mouse(mouse) = &mut local_event {
            let x = i64::from(mouse.position.x).saturating_sub(prepared.bounds.x);
            let y = i64::from(mouse.position.y).saturating_sub(prepared.bounds.y);
            // Outside release must reach native button state without becoming an
            // inside activation. Native events have unsigned terminal coordinates.
            mouse.position = Point::new(
                u16::try_from(x).unwrap_or(u16::MAX),
                u16::try_from(y).unwrap_or(u16::MAX),
            );
        } else if matches!(event, Event::Key(_) | Event::Paste(_)) && !focused {
            return ControlOutcome::default();
        }
        if let Event::Paste(text) = &mut local_event {
            *text = text
                .chars()
                .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
                .collect();
            if matches!(control.spec.kind, Kind::Text | Kind::Password) {
                *text = text.chars().filter(|ch| !ch.is_control()).collect();
            }
        }
        let size = prepared.layout.size;
        let area = Rect::new(
            0,
            0,
            u16::try_from(size.width).unwrap_or(u16::MAX),
            u16::try_from(size.height).unwrap_or(u16::MAX),
        );
        let (mut outcome, activate) =
            events::dispatch(control, key, event, &local_event, focused, area, &prepared);
        if activate {
            outcome.extend(self.activate(key));
        }
        outcome
    }

    fn activate(&mut self, key: &str) -> ControlOutcome {
        let spec = self.nodes[key].spec.clone();
        let mut outcome = ControlOutcome::handled();
        outcome.events.push(ControlEvent::Activate {
            key: key.to_owned(),
        });
        match spec.kind {
            Kind::File => outcome.events.push(ControlEvent::FileRequest {
                key: key.to_owned(),
                multiple: false,
            }),
            Kind::Anchor => {
                if let Some(href) = spec.href {
                    outcome.events.push(ControlEvent::Navigate {
                        key: key.to_owned(),
                        href,
                    });
                }
            }
            Kind::Summary => {
                if let Some(details) = spec.details
                    && let Some(Control {
                        state: NativeState::Details(open),
                        ..
                    }) = self.nodes.get_mut(&details)
                {
                    *open = !*open;
                    outcome.relayout = true;
                    outcome.events.push(ControlEvent::Toggle {
                        key: details,
                        open: *open,
                    });
                }
            }
            Kind::Button => {
                if let Some(form) = spec.form {
                    if spec.button_type == "reset" {
                        outcome.extend(self.reset(&form));
                    } else if spec.button_type != "button" {
                        outcome.events.push(ControlEvent::Submit {
                            form,
                            submitter: Some(key.to_owned()),
                        });
                    }
                }
            }
            _ => {}
        }
        outcome
    }

    /// Select all native editable text without replacing its undo history.
    pub fn select_all(&mut self, key: &str) -> ControlOutcome {
        if let Some(Control {
            state: NativeState::Text(state),
            ..
        }) = self.nodes.get_mut(key)
        {
            state.borrow_mut().buffer_mut().select_all();
            return ControlOutcome::handled();
        }
        ControlOutcome::default()
    }

    /// Latest visible value; password contents are never returned here.
    #[expect(
        clippy::must_use_candidate,
        reason = "Option is already must-use; repository policy forbids redundant getter attributes"
    )]
    pub fn value(&self, key: &str) -> Option<ControlValue> {
        self.nodes.get(key)?.value()
    }

    /// Values in the actions lane's exact key-based form serialization contract.
    /// Contains secrets: pass directly to `ActionEngine`, never log this map.
    #[must_use]
    pub fn action_values(&self) -> crate::actions::FormValues {
        use crate::actions::ControlValue as Value;
        self.nodes
            .iter()
            .filter_map(|(key, control)| {
                let value = match &control.state {
                    NativeState::Text(state) => {
                        Value::Text(state.borrow().buffer().text().to_owned())
                    }
                    NativeState::Checkbox(state) => Value::Checked(state.get().checked()),
                    NativeState::Select(state) => Value::Selected(
                        state
                            .get()
                            .selected()
                            .and_then(|i| control.spec.options.get(i))
                            .map(|o| vec![o.id.clone()])
                            .unwrap_or_default(),
                    ),
                    NativeState::Multiple(states) => Value::Selected(
                        control
                            .spec
                            .options
                            .iter()
                            .zip(states)
                            .filter(|(option, state)| !option.disabled && state.get().checked())
                            .map(|(option, _)| option.id.clone())
                            .collect(),
                    ),
                    NativeState::Hidden => {
                        Value::Text(control.spec.external.clone().unwrap_or_default())
                    }
                    NativeState::Button(_) if control.spec.kind == Kind::File => {
                        Value::Files(control.files.clone())
                    }
                    _ => return None,
                };
                Some((key.clone(), value))
            })
            .collect()
    }

    /// Details state for the next authoritative layout pass.
    #[must_use]
    pub fn details_open(&self) -> BTreeMap<String, bool> {
        self.nodes
            .iter()
            .filter_map(|(key, control)| match control.state {
                NativeState::Details(open) => Some((key.clone(), open)),
                _ => None,
            })
            .collect()
    }

    /// Complete a host file request. References are opaque to this layer.
    pub fn set_files(&mut self, key: &str, files: Vec<AuthorizedFile>) -> ControlOutcome {
        let Some(control) = self.nodes.get_mut(key) else {
            return ControlOutcome::default();
        };
        if control.spec.kind != Kind::File || control.spec.disabled || control.files == files {
            return ControlOutcome::default();
        }
        control.files.clone_from(&files);
        let mut outcome = ControlOutcome::handled();
        outcome.events.push(ControlEvent::Change {
            key: key.to_owned(),
            value: ControlValue::Files(files),
        });
        outcome
    }

    /// Reset a form to current external defaults, including file and edit history.
    pub fn reset(&mut self, form: &str) -> ControlOutcome {
        for control in self
            .nodes
            .values_mut()
            .filter(|control| control.spec.form.as_deref() == Some(form))
        {
            let prepared = control.prepared.take();
            *control = Control::new(control.spec.clone());
            control.prepared = prepared;
        }
        let mut outcome = ControlOutcome::handled();
        outcome.relayout = true;
        outcome.events.push(ControlEvent::Reset {
            form: form.to_owned(),
        });
        outcome
    }

    /// Collect successful named fields in document order, preserving repeated names.
    /// File references are emitted individually; multipart encoding belongs to the host.
    #[must_use]
    pub fn form_values(&self, form: &str) -> FormValues {
        let mut values = Vec::new();
        for key in &self.order {
            let control = &self.nodes[key];
            if control.spec.disabled || control.spec.form.as_deref() != Some(form) {
                continue;
            }
            let Some(name) = &control.spec.name else {
                continue;
            };
            match &control.state {
                NativeState::Text(state) => {
                    values.push((name.clone(), state.borrow().buffer().text().to_owned()));
                }
                NativeState::Checkbox(state) if state.get().checked() => {
                    values.push((name.clone(), "on".into()));
                }
                NativeState::Select(state) => {
                    if let Some(option) = state
                        .get()
                        .selected()
                        .and_then(|i| control.spec.options.get(i))
                        .filter(|o| !o.disabled)
                    {
                        values.push((name.clone(), option.id.clone()));
                    }
                }
                NativeState::Multiple(states) => {
                    for (option, state) in control.spec.options.iter().zip(states) {
                        let disabled = option.disabled || control.spec.disabled;
                        if !disabled && state.get().checked() {
                            values.push((name.clone(), option.id.clone()));
                        }
                    }
                }
                NativeState::Hidden => values.push((
                    name.clone(),
                    control.spec.external.clone().unwrap_or_default(),
                )),
                NativeState::Button(_) if control.spec.kind == Kind::File => {
                    for file in &control.files {
                        values.push((name.clone(), file.name.clone()));
                    }
                }
                _ => {}
            }
        }
        FormValues(values)
    }
}

impl Control {
    fn new(spec: Spec) -> Self {
        let state = match spec.kind {
            Kind::Text | Kind::Password | Kind::Textarea => {
                NativeState::Text(Box::new(RefCell::new(TextInputState::new(
                    TextEditBuffer::from_text(spec.external.clone().unwrap_or_default()),
                ))))
            }
            Kind::Checkbox => NativeState::Checkbox(Cell::new(CheckboxState::new(spec.checked))),
            Kind::Select => {
                NativeState::Select(Cell::new(SelectDropdownState::new(spec.selected_index())))
            }
            Kind::Multiple => NativeState::Multiple(
                spec.options
                    .iter()
                    .map(|option| {
                        Cell::new(CheckboxState::new(
                            spec.external.as_deref() == Some(option.id.as_str()),
                        ))
                    })
                    .collect(),
            ),
            Kind::Details => NativeState::Details(spec.checked),
            Kind::Hidden => NativeState::Hidden,
            _ => NativeState::Button(Cell::new(ButtonState::new())),
        };
        Self {
            spec,
            state,
            prepared: None,
            files: Vec::new(),
            secret_pointer: RefCell::new(TextInputState::default()),
            active_option: Cell::new(0),
        }
    }

    fn reconcile(&mut self, spec: &Spec) {
        match &mut self.state {
            NativeState::Text(state) if self.spec.external != spec.external => {
                // Unrelated updates retain the original buffer and native history.
                // Explicit external changes establish a new editing baseline.
                *state.get_mut() = TextInputState::new(TextEditBuffer::from_text(
                    spec.external.clone().unwrap_or_default(),
                ));
            }
            NativeState::Checkbox(state)
                if spec.checked_external.is_some()
                    && self.spec.checked_external != spec.checked_external =>
            {
                let mut value = state.get();
                value.set_checked(spec.checked);
                state.set(value);
            }
            NativeState::Details(open)
                if spec.checked_external.is_some()
                    && self.spec.checked_external != spec.checked_external =>
            {
                *open = spec.checked;
            }
            NativeState::Select(state) => {
                let old = state.get();
                let selected = if self.spec.external == spec.external {
                    old.selected()
                        .and_then(|i| self.spec.options.get(i))
                        .and_then(|option| {
                            spec.options.iter().position(|candidate| {
                                candidate.id == option.id && !candidate.disabled
                            })
                        })
                } else {
                    spec.selected_index()
                };
                let mut value = old;
                value.set_selected(selected);
                value.set_disabled(spec.disabled);
                state.set(value);
            }
            NativeState::Multiple(states) => {
                let selected: BTreeSet<_> = self
                    .spec
                    .options
                    .iter()
                    .zip(states.iter())
                    .filter(|(_, state)| state.get().checked())
                    .map(|(option, _)| option.id.clone())
                    .collect();
                *states = spec
                    .options
                    .iter()
                    .map(|option| {
                        Cell::new(CheckboxState::new(if self.spec.external == spec.external {
                            selected.contains(&option.id)
                        } else {
                            spec.external.as_deref() == Some(option.id.as_str())
                        }))
                    })
                    .collect();
            }
            _ => {}
        }
    }

    fn value(&self) -> Option<ControlValue> {
        match &self.state {
            NativeState::Text(_) if self.spec.kind == Kind::Password => {
                Some(ControlValue::SecretChanged)
            }
            NativeState::Text(state) => Some(ControlValue::Text(
                state.borrow().buffer().text().to_owned(),
            )),
            NativeState::Checkbox(state) => Some(ControlValue::Checked(state.get().checked())),
            NativeState::Select(state) => state
                .get()
                .selected()
                .and_then(|i| self.spec.options.get(i))
                .map(|option| ControlValue::Text(option.id.clone())),
            NativeState::Multiple(states) => Some(ControlValue::Multiple(
                self.spec
                    .options
                    .iter()
                    .zip(states)
                    .filter(|(_, state)| state.get().checked())
                    .map(|(o, _)| o.id.clone())
                    .collect(),
            )),
            NativeState::Hidden => Some(ControlValue::Text(
                self.spec.external.clone().unwrap_or_default(),
            )),
            NativeState::Button(_) if self.spec.kind == Kind::File => {
                Some(ControlValue::Files(self.files.clone()))
            }
            _ => None,
        }
    }
}
