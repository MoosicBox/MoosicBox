//! Ordered event routing, independent of action evaluation and network execution.
//!
//! Call `reconcile` after document mutations and `refresh_geometry` after layout or
//! scroll changes. Feed inputs in native order, once. Widgets own activation and
//! editing; mark their pointer input with `native_widget` and report activation
//! separately using the same gesture ID. Raw release routes `Click`/`ClickOutside`;
//! native pointer activation only acknowledges that gesture. Do not report it as
//! an uncorrelated activation. All positions are egui logical screen points.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;

use eframe::egui::{Modifiers, PointerButton, Pos2, Vec2};
use hyperchad_actions::{Action, ActionTrigger};

use crate::document::{Document, ElementKey, ElementStamp};
use crate::geometry::GeometrySnapshot;

/// Structured event payload. No expressions are evaluated by this module.
#[derive(Clone, Debug, PartialEq)]
pub enum EventValue {
    /// Absent value.
    Null,
    /// Checkbox/toggle state.
    Bool(bool),
    /// Numeric value.
    Number(f64),
    /// Text, including browser-compatible logical key names.
    String(String),
    /// Ordered values.
    Array(Vec<Self>),
    /// Named fields, including request metadata.
    Object(BTreeMap<String, Self>),
}

/// Whether an action starts, fires imperatively, or reverses a hover activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    /// One-shot action.
    Fire,
    /// Hover entered.
    Enter,
    /// Hover left, or its binding was removed/replaced.
    Leave,
}

/// One selected binding, ready for the shared action runtime.
#[derive(Clone, Debug)]
pub struct UiEvent {
    /// Monotonic input/lifecycle operation sequence; one operation may select many bindings.
    pub sequence: u64,
    /// Content stamp at dispatch (at activation for teardown of removed bindings).
    pub source: ElementStamp,
    /// Index into the source's authored `actions` array.
    pub action_index: usize,
    /// Exact model trigger; no invented bubbling phases.
    pub trigger: ActionTrigger,
    /// Execution or hover reversal.
    pub phase: EventPhase,
    /// Action value; keyboard events use their logical key string.
    pub value: EventValue,
    /// Full keyboard metadata when dispatched from a key.
    pub keyboard: Option<KeyboardEvent>,
    /// Full pointer metadata when dispatched from a pointer.
    pub pointer: Option<PointerEvent>,
}

/// Full normalized keyboard metadata supplied by the native integration.
///
/// egui's `Key` enum cannot reconstruct arbitrary Unicode logical keys or native
/// physical codes. The adapter must provide these strings rather than debug-print
/// an egui key. Text/IME mutation belongs to the widget, never this router.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent native keyboard facts; retain the normalized widget integration contract"
)]
pub struct KeyboardEvent {
    /// Browser-style logical key (e.g. `ArrowLeft`, `a`, or `é`).
    pub key: String,
    /// Native physical code if available; never guessed from the logical key.
    pub code: Option<String>,
    /// True on keydown, false on keyup. The model currently only has `KeyDown`.
    pub pressed: bool,
    /// Native key repeat.
    pub repeat: bool,
    /// Modifier state at this event, not end-of-frame state.
    pub modifiers: Modifiers,
    /// Whether an IME composition is active.
    pub composing: bool,
    /// Widget consumed this key for editing; do not leak to ancestor shortcuts.
    pub editing_consumed: bool,
}

/// An ordered pointer button event. Gesture IDs must not be reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerEvent {
    /// Correlates down/up and native activation; caller-owned monotonically unique ID.
    pub gesture: u64,
    /// Screen position in egui points.
    pub position: Pos2,
    /// Native button (only primary produces `Click`/`ClickOutside`).
    pub button: PointerButton,
    /// Press or release.
    pub pressed: bool,
    /// Event-time modifiers.
    pub modifiers: Modifiers,
    /// Native widget owns command activation. Raw release still routes `Click` once.
    /// Save this owner on press and supply it unchanged on release, even outside.
    pub native_widget: Option<ElementKey>,
}

/// Native-widget facts, emitted once by the widget integration.
#[derive(Clone, Debug)]
pub struct WidgetEvent {
    /// Current widget content stamp, independent of authored IDs.
    pub source: ElementStamp,
    /// Authoritative widget outcome.
    pub kind: WidgetEventKind,
}

/// Widget results, not requests to mutate the widget's editing state.
#[derive(Clone, Debug)]
pub enum WidgetEventKind {
    /// `Some` acknowledges an already-routed pointer click, without emitting another.
    /// `None` routes keyboard/accessibility activation. Acknowledgements must precede
    /// `EventRouter::end_input_batch` and may occur after pointer departure.
    Activate { gesture: Option<u64> },
    /// New native value; the widget owns text/IME editing and commits.
    Change(EventValue),
    /// Synchronize native focus and whether this is a text-editing widget.
    Focus { focused: bool, editing: bool },
}

/// Network lifecycle only. Transport and exactly-once request settlement live elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestPhase {
    /// Before transport starts.
    Before,
    /// After any terminal result.
    After,
    /// Successful response.
    Success,
    /// Failed response/transport.
    Error,
    /// Explicit cancellation.
    Abort,
    /// Deadline exceeded.
    Timeout,
}

/// Single ordered input boundary. No evaluator or transport is embedded here.
#[derive(Clone, Debug)]
pub enum InputEvent {
    /// Pointer position changed; refreshes hover using geometry, not widget rectangles.
    PointerMoved(Pos2),
    /// Pointer down/up.
    Pointer(PointerEvent),
    /// Pointer left the window/surface (including touch-up's `PointerGone`).
    /// Clears hover, but preserves gestures awaiting release/acknowledgement.
    PointerDeparted,
    /// Platform cancellation, not a click or release outside.
    PointerCancelled,
    /// Full native key metadata. Keyup is recognized but has no model trigger.
    Key(KeyboardEvent),
    /// Authoritative native activation/change/focus.
    Widget(WidgetEvent),
    /// Global named subscription; payload is preserved structurally.
    Named { name: String, value: EventValue },
    /// Dispatch to source and descendants with matching HTTP triggers, like browser code.
    Request {
        source: ElementStamp,
        phase: RequestPhase,
        value: EventValue,
    },
    /// Losing window focus clears hover, capture, presses and native focus.
    WindowFocus(bool),
}

/// Explicit input/integration contract violations. No guessed fallbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventError {
    /// Call reconcile before routing against changed bindings/document membership.
    UnsynchronizedDocument,
    /// Geometry contains an identity not in this document.
    StaleGeometry,
    /// Source stamp is detached or replaced.
    StaleSource,
    /// Widget/target is absent from visible geometry.
    IneligibleTarget,
    /// Native owner is not on the actual pointer hit path.
    InvalidNativeOwner,
    /// Gesture reused, unmatched release, mismatched button/owner, or duplicate activation.
    InvalidGesture,
    /// Native activation was reported for a gesture owned by raw routing.
    DuplicateActivation,
    /// Pointer coordinates must be finite logical screen points.
    InvalidPosition,
    /// Empty logical key cannot be normalized faithfully.
    MissingLogicalKey,
    /// Sequence overflow rather than silently changing event order.
    SequenceExhausted,
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "event routing contract violation: {self:?}")
    }
}
impl std::error::Error for EventError {}

#[derive(Clone, Debug)]
struct Mounted {
    stamp: ElementStamp,
    actions: Vec<Action>,
}

#[derive(Clone, Debug)]
struct Gesture {
    pointer: PointerEvent,
    path: Vec<ElementKey>,
    outside: Vec<(ElementKey, usize, Action)>,
    released: bool,
}

/// UI-thread-owned persistent routing state. Never recreated per frame.
#[derive(Debug, Default)]
pub struct EventRouter {
    sequence: u64,
    mounted: BTreeMap<ElementKey, Mounted>,
    order: Vec<ElementKey>,
    hovered: Vec<ElementKey>,
    position: Option<Pos2>,
    focused: Option<ElementKey>,
    editing: bool,
    capture: Option<ElementKey>,
    gestures: BTreeMap<u64, Gesture>,
    last_gesture: Option<u64>,
    sizes: BTreeMap<ElementKey, Vec2>,
}

impl EventRouter {
    /// Construct empty persistent routing state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Focus owner; loss of window focus/removal clears it.
    #[allow(
        clippy::must_use_candidate,
        reason = "Option is already must_use; repository policy forbids redundant function attributes"
    )]
    pub const fn focused(&self) -> Option<ElementKey> {
        self.focused
    }

    /// Whether the current native focus owner edits text.
    #[must_use]
    pub const fn is_editing(&self) -> bool {
        self.editing
    }

    /// Current explicit pointer capture owner. Capture does not change click hit testing.
    #[allow(
        clippy::must_use_candidate,
        reason = "Option is already must_use; repository policy forbids redundant function attributes"
    )]
    pub const fn captured(&self) -> Option<ElementKey> {
        self.capture
    }

    /// Synchronize imperative host focus, validating native eligibility.
    ///
    /// # Errors
    /// * Rejects an unmounted, stale or invisible target.
    pub fn focus(
        &mut self,
        document: &Document,
        geometry: &GeometrySnapshot<ElementKey>,
        target: Option<ElementKey>,
        editing: bool,
    ) -> Result<(), EventError> {
        self.validate(document, geometry)?;
        if let Some(key) = target {
            self.eligible(geometry, key)?;
        }
        self.focused = target;
        self.editing = target.is_some() && editing;
        Ok(())
    }

    /// Set/release explicit pointer capture. Widgets retain authority over dragging.
    ///
    /// # Errors
    /// * Rejects an unmounted, stale or invisible target.
    pub fn capture(
        &mut self,
        document: &Document,
        geometry: &GeometrySnapshot<ElementKey>,
        target: Option<ElementKey>,
    ) -> Result<(), EventError> {
        self.validate(document, geometry)?;
        if let Some(key) = target {
            self.eligible(geometry, key)?;
        }
        self.capture = target;
        Ok(())
    }

    /// Reconcile actual bindings, not subtree generations. Unchanged actions do not
    /// remount when descendants change. Immediate executes once per new/rebound slot.
    /// Teardown Leave envelopes retain the previous activation's stamp.
    ///
    /// # Errors
    /// * Returns `SequenceExhausted` if operation ordering cannot be represented.
    pub fn reconcile(&mut self, document: &Document) -> Result<Vec<UiEvent>, EventError> {
        self.tick()?;
        let mut out = Vec::new();
        let keys = document.keys();
        let mut next = BTreeMap::new();
        for &key in self.order.iter().rev() {
            let old = &self.mounted[&key];
            let current = document.get(key);
            for (index, action) in old.actions.iter().enumerate() {
                if action.trigger == ActionTrigger::Hover
                    && self.hovered.contains(&key)
                    && current.and_then(|node| node.actions.get(index)) != Some(action)
                {
                    out.push(self.envelope(
                        old.stamp,
                        index,
                        ActionTrigger::Hover,
                        EventPhase::Leave,
                        EventValue::Null,
                    ));
                }
            }
        }
        for &key in &keys {
            let Some(node) = document.get(key) else {
                continue;
            };
            let Some(stamp) = document.stamp(key) else {
                continue;
            };
            for (index, action) in node.actions.iter().enumerate() {
                let unchanged = self
                    .mounted
                    .get(&key)
                    .and_then(|old| old.actions.get(index))
                    == Some(action);
                if !unchanged && action.trigger == ActionTrigger::Immediate {
                    out.push(self.envelope(
                        stamp,
                        index,
                        action.trigger.clone(),
                        EventPhase::Fire,
                        EventValue::Null,
                    ));
                }
                if !unchanged
                    && action.trigger == ActionTrigger::Hover
                    && self.hovered.contains(&key)
                {
                    out.push(self.envelope(
                        stamp,
                        index,
                        ActionTrigger::Hover,
                        EventPhase::Enter,
                        EventValue::Null,
                    ));
                }
            }
            next.insert(
                key,
                Mounted {
                    stamp,
                    actions: node.actions.clone(),
                },
            );
        }
        self.hovered.retain(|key| next.contains_key(key));
        if self.focused.is_some_and(|key| !next.contains_key(&key)) {
            self.focused = None;
            self.editing = false;
        }
        if self.capture.is_some_and(|key| !next.contains_key(&key)) {
            self.capture = None;
        }
        self.gestures.retain(|_, gesture| {
            gesture.path.iter().all(|key| next.contains_key(key))
                && gesture
                    .pointer
                    .native_widget
                    .is_none_or(|key| next.contains_key(&key))
        });
        self.sizes.retain(|key, _| next.contains_key(key));
        // Once a binding changes during a gesture it cannot rejoin that gesture,
        // even if another reconciliation later restores identical authored actions.
        for gesture in self.gestures.values_mut() {
            gesture.outside.retain(|(key, index, action)| {
                next.get(key)
                    .and_then(|mounted| mounted.actions.get(*index))
                    == Some(action)
            });
        }
        self.mounted = next;
        self.order = keys;
        Ok(out)
    }

    /// Refresh hover and local client-size changes after layout/scroll. Initial size
    /// establishes a baseline; scrolling alone never emits Resize.
    ///
    /// # Errors
    /// * Rejects unsynchronized document membership/bindings or stale geometry.
    /// * Rejects exhausted operation ordering.
    pub fn refresh_geometry(
        &mut self,
        document: &Document,
        geometry: &GeometrySnapshot<ElementKey>,
    ) -> Result<Vec<UiEvent>, EventError> {
        self.validate(document, geometry)?;
        self.tick()?;
        let mut out = self.hover(geometry, self.position);
        if self
            .focused
            .is_some_and(|key| self.eligible(geometry, key).is_err())
        {
            self.focused = None;
            self.editing = false;
        }
        if self
            .capture
            .is_some_and(|key| self.eligible(geometry, key).is_err())
        {
            self.capture = None;
        }
        self.gestures.retain(|_, gesture| {
            gesture
                .path
                .iter()
                .all(|key| geometry.node(*key).is_some_and(|node| node.visible))
        });
        for &key in &self.order {
            let Some(node) = geometry.node(key) else {
                self.sizes.remove(&key);
                continue;
            };
            if !node.visible {
                self.sizes.remove(&key);
                continue;
            }
            let size = node.padding_box.size() / node.transform.scale;
            if self.sizes.insert(key, size).is_some_and(|old| old != size) {
                out.extend(self.at(
                    key,
                    &ActionTrigger::Resize,
                    EventPhase::Fire,
                    &EventValue::Null,
                ));
            }
        }
        Ok(out)
    }

    /// Route exactly one normalized input in native order. The returned vector is
    /// already in dispatch order. Feed native widget results once, not as raw clicks.
    ///
    /// `KeyDown` without element focus selects the first root with a handler, in
    /// document order; it never broadcasts to unrelated descendants. Focused keys
    /// use nearest-handler ancestry. Editing-consumed and composing keys are excluded.
    ///
    /// # Errors
    /// * Rejects stale/unsynchronized sources and geometry.
    /// * Rejects invalid gesture ownership, duplicate activation and missing key names.
    /// * Rejects exhausted operation ordering.
    pub fn route(
        &mut self,
        document: &Document,
        geometry: &GeometrySnapshot<ElementKey>,
        input: InputEvent,
    ) -> Result<Vec<UiEvent>, EventError> {
        self.validate(document, geometry)?;
        self.tick()?;
        match input {
            InputEvent::PointerMoved(position) => {
                if !position.is_finite() {
                    return Err(EventError::InvalidPosition);
                }
                self.position = Some(position);
                Ok(self.hover(geometry, Some(position)))
            }
            InputEvent::Pointer(pointer) => self.pointer(geometry, pointer),
            InputEvent::PointerDeparted => {
                self.position = None;
                Ok(self.hover(geometry, None))
            }
            InputEvent::PointerCancelled => {
                self.gestures.clear();
                self.capture = None;
                self.position = None;
                Ok(self.hover(geometry, None))
            }
            InputEvent::WindowFocus(focused) => {
                if focused {
                    return Ok(Vec::new());
                }
                self.focused = None;
                self.editing = false;
                self.capture = None;
                self.gestures.clear();
                self.position = None;
                Ok(self.hover(geometry, None))
            }
            InputEvent::Key(key) => {
                if key.key.is_empty() {
                    return Err(EventError::MissingLogicalKey);
                }
                if !key.pressed || key.editing_consumed || key.composing {
                    return Ok(Vec::new());
                }
                let path = self.focused.map_or_else(
                    || document.root_keys(),
                    |target| Self::path(geometry, target),
                );
                let mut out = self.nearest(
                    &path,
                    &ActionTrigger::KeyDown,
                    &EventValue::String(key.key.clone()),
                );
                for event in &mut out {
                    event.keyboard = Some(key.clone());
                }
                Ok(out)
            }
            InputEvent::Widget(widget) => self.widget(document, geometry, widget),
            InputEvent::Named { name, value } => {
                Ok(self.broadcast(&ActionTrigger::Event(name), &value))
            }
            InputEvent::Request {
                source,
                phase,
                value,
            } => {
                if !document.is_current(source) {
                    return Err(EventError::StaleSource);
                }
                let trigger = match phase {
                    RequestPhase::Before => ActionTrigger::HttpBeforeRequest,
                    RequestPhase::After => ActionTrigger::HttpAfterRequest,
                    RequestPhase::Success => ActionTrigger::HttpRequestSuccess,
                    RequestPhase::Error => ActionTrigger::HttpRequestError,
                    RequestPhase::Abort => ActionTrigger::HttpRequestAbort,
                    RequestPhase::Timeout => ActionTrigger::HttpRequestTimeout,
                };
                let mut out = Vec::new();
                for &key in &self.order {
                    let mut ancestor = Some(key);
                    while let Some(parent) = ancestor {
                        if parent == source.key {
                            out.extend(self.at(key, &trigger, EventPhase::Fire, &value));
                            break;
                        }
                        ancestor = document.parent(parent);
                    }
                }
                Ok(out)
            }
        }
    }

    fn widget(
        &mut self,
        document: &Document,
        geometry: &GeometrySnapshot<ElementKey>,
        widget: WidgetEvent,
    ) -> Result<Vec<UiEvent>, EventError> {
        if !document.is_current(widget.source) {
            return Err(EventError::StaleSource);
        }
        self.eligible(geometry, widget.source.key)?;
        match widget.kind {
            WidgetEventKind::Focus { focused, editing } => {
                if focused {
                    self.focused = Some(widget.source.key);
                    self.editing = editing;
                } else if self.focused == Some(widget.source.key) {
                    self.focused = None;
                    self.editing = false;
                }
                Ok(Vec::new())
            }
            WidgetEventKind::Change(value) => Ok(self.nearest(
                &Self::path(geometry, widget.source.key),
                &ActionTrigger::Change,
                &value,
            )),
            WidgetEventKind::Activate { gesture: None } => Ok(self.nearest(
                &Self::path(geometry, widget.source.key),
                &ActionTrigger::Click,
                &EventValue::Null,
            )),
            WidgetEventKind::Activate { gesture: Some(id) } => {
                let gesture = self.gestures.get(&id).ok_or(EventError::InvalidGesture)?;
                if gesture.pointer.native_widget.is_none() {
                    return Err(EventError::DuplicateActivation);
                }
                if !gesture.released
                    || gesture.pointer.native_widget != Some(widget.source.key)
                    || !gesture.path.contains(&widget.source.key)
                {
                    return Err(EventError::InvalidGesture);
                }
                let _acknowledged = self
                    .gestures
                    .remove(&id)
                    .ok_or(EventError::InvalidGesture)?;
                // The release already emitted Click and ClickOutside. Widget command
                // activation is a separate fact, not a second model click.
                Ok(Vec::new())
            }
        }
    }

    fn pointer(
        &mut self,
        geometry: &GeometrySnapshot<ElementKey>,
        pointer: PointerEvent,
    ) -> Result<Vec<UiEvent>, EventError> {
        if !pointer.position.is_finite() {
            return Err(EventError::InvalidPosition);
        }
        let path = geometry
            .hit_test(pointer.position, |_| true)
            .map_or_else(Vec::new, |key| Self::path(geometry, key));
        if pointer.pressed
            && pointer
                .native_widget
                .is_some_and(|key| !path.contains(&key))
        {
            return Err(EventError::InvalidNativeOwner);
        }
        if pointer.pressed {
            if self
                .last_gesture
                .is_some_and(|last| pointer.gesture <= last)
            {
                return Err(EventError::InvalidGesture);
            }
            let outside = self.outside_snapshot(geometry);
            self.gestures.insert(
                pointer.gesture,
                Gesture {
                    pointer,
                    path: path.clone(),
                    outside,
                    released: false,
                },
            );
            self.last_gesture = Some(pointer.gesture);
            self.position = Some(pointer.position);
            let mut out = self.hover(geometry, Some(pointer.position));
            out.extend(self.nearest(&path, &ActionTrigger::MouseDown, &EventValue::Null));
            for event in &mut out {
                event.pointer = Some(pointer);
            }
            return Ok(out);
        }
        let gesture = self
            .gestures
            .get(&pointer.gesture)
            .ok_or(EventError::InvalidGesture)?;
        if gesture.released
            || gesture.pointer.button != pointer.button
            || gesture.pointer.native_widget != pointer.native_widget
        {
            return Err(EventError::InvalidGesture);
        }
        let mut gesture = self
            .gestures
            .remove(&pointer.gesture)
            .ok_or(EventError::InvalidGesture)?;
        self.capture = None;
        self.position = Some(pointer.position);
        let mut out = self.hover(geometry, Some(pointer.position));
        // Browser click uses the nearest common ancestor of press/release targets.
        // Crossing into the viewport background from an element has no such target.
        // Native widgets are authoritative activation targets, never decorative
        // descendants. Outside the owner, retain browser common-ancestor targeting.
        let click_path = pointer
            .native_widget
            .filter(|owner| path.contains(owner))
            .map(|owner| Self::path(geometry, owner))
            .or_else(|| {
                gesture
                    .path
                    .iter()
                    .position(|key| path.contains(key))
                    .map(|index| gesture.path[index..].to_vec())
            });
        let background = gesture.path.is_empty() && path.is_empty();
        if pointer.button != PointerButton::Primary || (click_path.is_none() && !background) {
            return Ok(out);
        }
        gesture.path = click_path.unwrap_or_default();
        gesture.pointer = pointer;
        gesture.released = true;
        out.extend(self.nearest(&gesture.path, &ActionTrigger::Click, &EventValue::Null));
        out.extend(self.outside(&gesture, geometry));
        if pointer.native_widget.is_some() {
            // Retain only for an optional native command acknowledgement.
            self.gestures.insert(pointer.gesture, gesture);
        }
        for event in &mut out {
            event.pointer = Some(pointer);
        }
        Ok(out)
    }

    /// Discard completed native gestures after all widget facts for an input batch.
    /// Unreleased presses survive frames. Call after processing native activation.
    pub fn end_input_batch(&mut self) {
        self.gestures.retain(|_, gesture| !gesture.released);
    }

    fn outside_snapshot(
        &self,
        geometry: &GeometrySnapshot<ElementKey>,
    ) -> Vec<(ElementKey, usize, Action)> {
        let mut outside = Vec::new();
        for key in &self.order {
            if geometry.node(*key).is_some_and(|node| node.visible) {
                for (index, action) in self.mounted[key].actions.iter().enumerate() {
                    if action.trigger == ActionTrigger::ClickOutside {
                        outside.push((*key, index, action.clone()));
                    }
                }
            }
        }
        outside
    }

    fn outside(&self, gesture: &Gesture, geometry: &GeometrySnapshot<ElementKey>) -> Vec<UiEvent> {
        gesture
            .outside
            .iter()
            .filter_map(|(key, index, binding)| {
                let current = self.mounted.get(key)?;
                // Unrelated subtree generations do not invalidate a binding. A new
                // binding or a listener hidden at press cannot consume its opener.
                (!gesture.path.contains(key)
                    && geometry.node(*key).is_some_and(|node| node.visible)
                    && current.actions.get(*index) == Some(binding))
                .then(|| {
                    self.envelope(
                        current.stamp,
                        *index,
                        ActionTrigger::ClickOutside,
                        EventPhase::Fire,
                        EventValue::Null,
                    )
                })
            })
            .collect()
    }

    fn hover(
        &mut self,
        geometry: &GeometrySnapshot<ElementKey>,
        position: Option<Pos2>,
    ) -> Vec<UiEvent> {
        let next = position
            .and_then(|point| geometry.hit_test(point, |_| true))
            .map_or_else(Vec::new, |key| Self::path(geometry, key));
        let mut out = Vec::new();
        for &key in &self.hovered {
            if !next.contains(&key) {
                out.extend(self.at(
                    key,
                    &ActionTrigger::Hover,
                    EventPhase::Leave,
                    &EventValue::Null,
                ));
            }
        }
        for &key in next.iter().rev() {
            if !self.hovered.contains(&key) {
                out.extend(self.at(
                    key,
                    &ActionTrigger::Hover,
                    EventPhase::Enter,
                    &EventValue::Null,
                ));
            }
        }
        self.hovered = next;
        out
    }

    fn path(geometry: &GeometrySnapshot<ElementKey>, target: ElementKey) -> Vec<ElementKey> {
        let mut path = Vec::new();
        let mut current = Some(target);
        // Geometry is constructed as a tree. The guard also avoids hangs if its
        // representation is extended later with malformed foreign parent links.
        while let Some(key) = current {
            if path.contains(&key) {
                break;
            }
            path.push(key);
            current = geometry.node(key).and_then(|node| node.parent);
        }
        path
    }

    fn nearest(
        &self,
        path: &[ElementKey],
        trigger: &ActionTrigger,
        value: &EventValue,
    ) -> Vec<UiEvent> {
        for &key in path {
            let events = self.at(key, trigger, EventPhase::Fire, value);
            if !events.is_empty() {
                return events;
            }
        }
        Vec::new()
    }

    fn broadcast(&self, trigger: &ActionTrigger, value: &EventValue) -> Vec<UiEvent> {
        self.order
            .iter()
            .flat_map(|&key| self.at(key, trigger, EventPhase::Fire, value))
            .collect()
    }

    fn at(
        &self,
        key: ElementKey,
        trigger: &ActionTrigger,
        phase: EventPhase,
        value: &EventValue,
    ) -> Vec<UiEvent> {
        self.mounted.get(&key).map_or_else(Vec::new, |mounted| {
            mounted
                .actions
                .iter()
                .enumerate()
                .filter(|(_, action)| &action.trigger == trigger)
                .map(|(index, _)| {
                    self.envelope(mounted.stamp, index, trigger.clone(), phase, value.clone())
                })
                .collect()
        })
    }

    const fn envelope(
        &self,
        source: ElementStamp,
        action_index: usize,
        trigger: ActionTrigger,
        phase: EventPhase,
        value: EventValue,
    ) -> UiEvent {
        UiEvent {
            sequence: self.sequence,
            source,
            action_index,
            trigger,
            phase,
            value,
            keyboard: None,
            pointer: None,
        }
    }

    fn eligible(
        &self,
        geometry: &GeometrySnapshot<ElementKey>,
        key: ElementKey,
    ) -> Result<(), EventError> {
        if !self.mounted.contains_key(&key) {
            return Err(EventError::StaleSource);
        }
        if !geometry.node(key).is_some_and(|node| node.visible) {
            return Err(EventError::IneligibleTarget);
        }
        Ok(())
    }

    fn validate(
        &self,
        document: &Document,
        geometry: &GeometrySnapshot<ElementKey>,
    ) -> Result<(), EventError> {
        if document.keys() != self.order
            || self.order.iter().any(|key| {
                let old = &self.mounted[key];
                document.stamp(*key) != Some(old.stamp)
                    || document
                        .get(*key)
                        .is_none_or(|node| node.actions != old.actions)
            })
        {
            return Err(EventError::UnsynchronizedDocument);
        }
        if geometry
            .paint_order()
            .any(|node| document.get(node.key).is_none())
        {
            return Err(EventError::StaleGeometry);
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), EventError> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(EventError::SequenceExhausted)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ViewOrigin;
    use crate::geometry::GeometryViewport;
    use eframe::egui::{Rect, pos2, vec2};
    use hyperchad_actions::ActionType;
    use hyperchad_renderer::View;
    use hyperchad_transformer::{
        Container,
        models::{LayoutOverflow, Selector, SwapStrategy},
    };

    fn node(
        name: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        triggers: Vec<ActionTrigger>,
        children: Vec<Container>,
    ) -> Container {
        Container {
            // Every node deliberately has the same zero authored numeric ID.
            id: 0,
            str_id: Some(name.into()),
            calculated_x: Some(x),
            calculated_y: Some(y),
            calculated_width: Some(width),
            calculated_height: Some(height),
            actions: triggers
                .into_iter()
                .map(|trigger| Action {
                    trigger,
                    effect: ActionType::NoOp.into(),
                })
                .collect(),
            children,
            ..Container::default()
        }
    }

    fn document(root: Container) -> Document {
        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(root),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        document
    }

    fn key(document: &Document, name: &str) -> ElementKey {
        document.select(&Selector::Id(name.into()), None)[0]
    }

    fn geometry(
        document: &Document,
        scroll: &BTreeMap<ElementKey, Vec2>,
    ) -> GeometrySnapshot<ElementKey> {
        geometry_roots(document, document.roots(), scroll)
    }

    fn geometry_roots(
        document: &Document,
        roots: &[Container],
        scroll: &BTreeMap<ElementKey, Vec2>,
    ) -> GeometrySnapshot<ElementKey> {
        GeometrySnapshot::build_roots(
            roots,
            GeometryViewport {
                rect: Rect::from_min_size(Pos2::ZERO, vec2(300.0, 300.0)),
                zoom: 1.0,
            },
            1,
            scroll,
            |path, _| {
                let mut key = document.root_keys()[path[0]];
                for &index in &path[1..] {
                    key = document.children(key)[index];
                }
                Some(key)
            },
        )
        .unwrap()
    }

    fn pointer(
        gesture: u64,
        pressed: bool,
        x: f32,
        y: f32,
        native_widget: Option<ElementKey>,
    ) -> InputEvent {
        InputEvent::Pointer(PointerEvent {
            gesture,
            position: pos2(x, y),
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
            native_widget,
        })
    }

    fn setup(document: &Document) -> (EventRouter, GeometrySnapshot<ElementKey>) {
        let geometry = geometry(document, &BTreeMap::new());
        let mut router = EventRouter::new();
        router.reconcile(document).unwrap();
        router.refresh_geometry(document, &geometry).unwrap();
        (router, geometry)
    }

    #[test]
    fn nested_delegation_is_nearest_and_nonlistener_occludes() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            200.0,
            200.0,
            vec![ActionTrigger::Click],
            vec![
                node(
                    "back",
                    0.0,
                    0.0,
                    80.0,
                    80.0,
                    vec![ActionTrigger::Click],
                    vec![],
                ),
                node("front", 0.0, 0.0, 40.0, 40.0, vec![], vec![]),
            ],
        ));
        let (mut router, geometry) = setup(&document);
        router
            .route(&document, &geometry, pointer(1, true, 10.0, 10.0, None))
            .unwrap();
        let events = router
            .route(&document, &geometry, pointer(1, false, 10.0, 10.0, None))
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.key, key(&document, "root"));
        router
            .route(&document, &geometry, pointer(2, true, 60.0, 60.0, None))
            .unwrap();
        let events = router
            .route(&document, &geometry, pointer(2, false, 60.0, 60.0, None))
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.key, key(&document, "back"));
    }

    #[test]
    fn release_outside_does_not_activate_pressed_element() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::Click],
            vec![],
        ));
        let (mut router, geometry) = setup(&document);
        let root = key(&document, "root");
        router
            .route(&document, &geometry, pointer(1, true, 10.0, 10.0, None))
            .unwrap();
        router.capture(&document, &geometry, Some(root)).unwrap();
        let events = router
            .route(&document, &geometry, pointer(1, false, 250.0, 250.0, None))
            .unwrap();
        assert!(events.is_empty());
        assert_eq!(router.captured(), None);
    }

    #[test]
    fn native_activation_is_authoritative_and_single_use() {
        let document = document(node(
            "widget",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![
                ActionTrigger::Click,
                ActionTrigger::MouseDown,
                ActionTrigger::Change,
            ],
            vec![],
        ));
        let (mut router, geometry) = setup(&document);
        let widget = key(&document, "widget");
        let down = router
            .route(
                &document,
                &geometry,
                pointer(1, true, 10.0, 10.0, Some(widget)),
            )
            .unwrap();
        assert_eq!(down.len(), 1);
        assert_eq!(down[0].trigger, ActionTrigger::MouseDown);
        let released = router
            .route(
                &document,
                &geometry,
                pointer(1, false, 10.0, 10.0, Some(widget)),
            )
            .unwrap();
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].trigger, ActionTrigger::Click);
        // Touch-up commonly sends PointerGone before the native activation fact.
        router
            .route(&document, &geometry, InputEvent::PointerDeparted)
            .unwrap();
        let activation = InputEvent::Widget(WidgetEvent {
            source: document.stamp(widget).unwrap(),
            kind: WidgetEventKind::Activate { gesture: Some(1) },
        });
        let events = router
            .route(&document, &geometry, activation.clone())
            .unwrap();
        assert!(events.is_empty());
        assert_eq!(
            router.route(&document, &geometry, activation).unwrap_err(),
            EventError::InvalidGesture
        );
        let events = router
            .route(
                &document,
                &geometry,
                InputEvent::Widget(WidgetEvent {
                    source: document.stamp(widget).unwrap(),
                    kind: WidgetEventKind::Change(EventValue::String("é".into())),
                }),
            )
            .unwrap();
        assert_eq!(events[0].value, EventValue::String("é".into()));
    }

    #[test]
    fn hover_enters_ancestors_first_and_leaves_children_first_once() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::Hover],
            vec![node(
                "child",
                0.0,
                0.0,
                30.0,
                30.0,
                vec![ActionTrigger::Hover],
                vec![],
            )],
        ));
        let (mut router, geometry) = setup(&document);
        let events = router
            .route(
                &document,
                &geometry,
                InputEvent::PointerMoved(pos2(10.0, 10.0)),
            )
            .unwrap();
        assert_eq!(
            events.iter().map(|e| e.source.key).collect::<Vec<_>>(),
            vec![key(&document, "root"), key(&document, "child")]
        );
        assert!(events.iter().all(|e| e.phase == EventPhase::Enter));
        assert!(
            router
                .refresh_geometry(&document, &geometry)
                .unwrap()
                .is_empty()
        );
        let events = router
            .route(&document, &geometry, InputEvent::WindowFocus(false))
            .unwrap();
        assert_eq!(
            events.iter().map(|e| e.source.key).collect::<Vec<_>>(),
            vec![key(&document, "child"), key(&document, "root")]
        );
        assert!(events.iter().all(|e| e.phase == EventPhase::Leave));
    }

    #[test]
    fn clip_and_nested_scroll_use_projected_geometry() {
        let mut scroller = node(
            "scroll",
            0.0,
            0.0,
            50.0,
            50.0,
            vec![],
            vec![node(
                "child",
                0.0,
                70.0,
                30.0,
                30.0,
                vec![ActionTrigger::Hover],
                vec![],
            )],
        );
        scroller.overflow_y = LayoutOverflow::Scroll;
        let document = document(node("root", 0.0, 0.0, 200.0, 200.0, vec![], vec![scroller]));
        let (mut router, initial) = setup(&document);
        assert!(
            router
                .route(
                    &document,
                    &initial,
                    InputEvent::PointerMoved(pos2(10.0, 40.0))
                )
                .unwrap()
                .is_empty()
        );
        // Child starts outside the ancestor clip. Scrolling projects it under the pointer.
        let projected = geometry(
            &document,
            &BTreeMap::from([(key(&document, "scroll"), vec2(0.0, 50.0))]),
        );
        let events = router.refresh_geometry(&document, &projected).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.key, key(&document, "child"));
        assert_eq!(events[0].phase, EventPhase::Enter);
        assert!(
            router
                .route(
                    &document,
                    &projected,
                    InputEvent::PointerMoved(pos2(10.0, 80.0))
                )
                .unwrap()
                .iter()
                .all(|e| e.phase == EventPhase::Leave)
        );
    }

    #[test]
    fn new_popup_does_not_consume_opener_gesture() {
        let mut document = document(node(
            "opener",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::Click],
            vec![],
        ));
        let (mut router, initial) = setup(&document);
        let opener = key(&document, "opener");
        router
            .route(&document, &initial, pointer(1, true, 10.0, 10.0, None))
            .unwrap();
        // MouseDown can mount a popup before release; it is not in the down snapshot.
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        children: vec![node(
                            "popup",
                            150.0,
                            0.0,
                            50.0,
                            50.0,
                            vec![ActionTrigger::ClickOutside],
                            vec![],
                        )],
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Element {
                    source: opener,
                    target: Selector::SelfTarget,
                    strategy: SwapStrategy::BeforeEnd,
                },
            )
            .unwrap();
        router.reconcile(&document).unwrap();
        let geometry = geometry(&document, &BTreeMap::new());
        let events = router
            .route(&document, &geometry, pointer(1, false, 10.0, 10.0, None))
            .unwrap();
        assert!(
            events
                .iter()
                .all(|e| e.trigger != ActionTrigger::ClickOutside)
        );
        router
            .route(&document, &geometry, pointer(2, true, 10.0, 10.0, None))
            .unwrap();
        let events = router
            .route(&document, &geometry, pointer(2, false, 10.0, 10.0, None))
            .unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.trigger == ActionTrigger::ClickOutside)
        );
    }

    #[test]
    fn removal_and_window_blur_clear_focus_capture_and_presses() {
        let mut document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![],
            vec![node("child", 0.0, 0.0, 30.0, 30.0, vec![], vec![])],
        ));
        let (mut router, geometry) = setup(&document);
        let child = key(&document, "child");
        router
            .focus(&document, &geometry, Some(child), true)
            .unwrap();
        router.capture(&document, &geometry, Some(child)).unwrap();
        router
            .route(
                &document,
                &geometry,
                pointer(1, true, 10.0, 10.0, Some(child)),
            )
            .unwrap();
        document
            .apply_view(
                View {
                    delete_selectors: vec![Selector::Id("child".into())],
                    ..View::default()
                },
                ViewOrigin::Element {
                    source: child,
                    target: Selector::SelfTarget,
                    strategy: SwapStrategy::None,
                },
            )
            .unwrap();
        router.reconcile(&document).unwrap();
        assert_eq!(router.focused(), None);
        assert_eq!(router.captured(), None);
        assert!(!router.is_editing());
        assert!(router.gestures.is_empty());
        let geometry = self::geometry(&document, &BTreeMap::new());
        let root = key(&document, "root");
        router
            .focus(&document, &geometry, Some(root), false)
            .unwrap();
        router.capture(&document, &geometry, Some(root)).unwrap();
        router
            .route(&document, &geometry, InputEvent::WindowFocus(false))
            .unwrap();
        assert_eq!(router.focused(), None);
        assert_eq!(router.captured(), None);
    }

    #[test]
    fn keys_preserve_repeat_modifiers_and_do_not_escape_editing() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::KeyDown],
            vec![node("edit", 0.0, 0.0, 30.0, 30.0, vec![], vec![])],
        ));
        let (mut router, geometry) = setup(&document);
        router
            .focus(&document, &geometry, Some(key(&document, "edit")), true)
            .unwrap();
        let mut keyboard = KeyboardEvent {
            key: "é".into(),
            code: Some("KeyE".into()),
            pressed: true,
            repeat: true,
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            composing: false,
            editing_consumed: true,
        };
        assert!(
            router
                .route(&document, &geometry, InputEvent::Key(keyboard.clone()))
                .unwrap()
                .is_empty()
        );
        keyboard.editing_consumed = false;
        let events = router
            .route(&document, &geometry, InputEvent::Key(keyboard.clone()))
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, EventValue::String("é".into()));
        assert_eq!(events[0].keyboard.as_ref(), Some(&keyboard));
        keyboard.pressed = false;
        assert!(
            router
                .route(&document, &geometry, InputEvent::Key(keyboard))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn immediate_named_and_request_lifecycle_account_for_hidden_live_bindings() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![
                ActionTrigger::Immediate,
                ActionTrigger::Event("hello".into()),
            ],
            vec![node(
                "child",
                0.0,
                0.0,
                20.0,
                20.0,
                vec![
                    ActionTrigger::HttpBeforeRequest,
                    ActionTrigger::HttpAfterRequest,
                    ActionTrigger::HttpRequestSuccess,
                    ActionTrigger::HttpRequestError,
                    ActionTrigger::HttpRequestAbort,
                    ActionTrigger::HttpRequestTimeout,
                ],
                vec![],
            )],
        ));
        let geometry = geometry(&document, &BTreeMap::new());
        let mut router = EventRouter::new();
        assert_eq!(router.reconcile(&document).unwrap().len(), 1);
        assert!(router.reconcile(&document).unwrap().is_empty());
        let payload = EventValue::Object(BTreeMap::from([(
            "name".into(),
            EventValue::String("value".into()),
        )]));
        let events = router
            .route(
                &document,
                &geometry,
                InputEvent::Named {
                    name: "hello".into(),
                    value: payload.clone(),
                },
            )
            .unwrap();
        assert_eq!(events[0].value, payload);
        let source = document.stamp(key(&document, "root")).unwrap();
        for phase in [
            RequestPhase::Before,
            RequestPhase::After,
            RequestPhase::Success,
            RequestPhase::Error,
            RequestPhase::Abort,
            RequestPhase::Timeout,
        ] {
            let events = router
                .route(
                    &document,
                    &geometry,
                    InputEvent::Request {
                        source,
                        phase,
                        value: EventValue::Null,
                    },
                )
                .unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].source.key, key(&document, "child"));
        }
    }

    #[test]
    fn resize_uses_local_size_not_scroll_or_position() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::Resize],
            vec![],
        ));
        let (mut router, initial) = setup(&document);
        assert!(
            router
                .refresh_geometry(&document, &initial)
                .unwrap()
                .is_empty()
        );
        let mut roots = document.roots().to_vec();
        roots[0].calculated_width = Some(120.0);
        let resized = geometry_roots(&document, &roots, &BTreeMap::new());
        let events = router.refresh_geometry(&document, &resized).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trigger, ActionTrigger::Resize);
        assert!(
            router
                .refresh_geometry(&document, &resized)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn duplicate_string_and_zero_numeric_ids_never_alias() {
        let document = document(node(
            "same",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::Event("test".into())],
            vec![node(
                "same",
                0.0,
                0.0,
                20.0,
                20.0,
                vec![ActionTrigger::Event("test".into())],
                vec![],
            )],
        ));
        let (mut router, geometry) = setup(&document);
        let events = router
            .route(
                &document,
                &geometry,
                InputEvent::Named {
                    name: "test".into(),
                    value: EventValue::Null,
                },
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_ne!(events[0].source.key, events[1].source.key);
        assert_eq!(events[0].sequence, events[1].sequence);
    }

    #[test]
    fn unrelated_mutation_does_not_remount_immediate_and_removal_reverses_hover() {
        let mut document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::Immediate, ActionTrigger::Hover],
            vec![node(
                "child",
                0.0,
                0.0,
                20.0,
                20.0,
                vec![ActionTrigger::Hover],
                vec![],
            )],
        ));
        let (mut router, initial) = setup(&document);
        router
            .route(
                &document,
                &initial,
                InputEvent::PointerMoved(pos2(10.0, 10.0)),
            )
            .unwrap();
        let root = key(&document, "root");
        let child = key(&document, "child");
        document
            .apply_view(
                View {
                    delete_selectors: vec![Selector::Id("child".into())],
                    ..View::default()
                },
                ViewOrigin::Element {
                    source: root,
                    target: Selector::SelfTarget,
                    strategy: SwapStrategy::None,
                },
            )
            .unwrap();
        let events = router.reconcile(&document).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.key, child);
        assert_eq!(events[0].phase, EventPhase::Leave);
        assert!(
            events
                .iter()
                .all(|event| event.trigger != ActionTrigger::Immediate)
        );
    }

    #[test]
    fn nested_scroll_offsets_accumulate_before_hit_testing() {
        let mut inner = node(
            "inner",
            0.0,
            60.0,
            50.0,
            50.0,
            vec![],
            vec![node(
                "target",
                0.0,
                70.0,
                20.0,
                20.0,
                vec![ActionTrigger::Click],
                vec![],
            )],
        );
        inner.overflow_y = LayoutOverflow::Scroll;
        let mut root = node("root", 0.0, 0.0, 100.0, 100.0, vec![], vec![inner]);
        root.overflow_y = LayoutOverflow::Scroll;
        let document = document(root);
        let mut router = EventRouter::new();
        router.reconcile(&document).unwrap();
        let geometry = geometry(
            &document,
            &BTreeMap::from([
                (key(&document, "root"), vec2(0.0, 10.0)),
                (key(&document, "inner"), vec2(0.0, 40.0)),
            ]),
        );
        let target = key(&document, "target");
        let bounds = geometry.node(target).unwrap();
        // Both scroll offsets participate; the child's visible box starts at y=80.
        assert_eq!(bounds.border_box.min.y, 80.0);
        router
            .route(&document, &geometry, pointer(1, true, 10.0, 85.0, None))
            .unwrap();
        let events = router
            .route(&document, &geometry, pointer(1, false, 10.0, 85.0, None))
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.key, target);
    }

    #[test]
    fn native_release_outside_never_activates_owner() {
        let document = document(node(
            "widget",
            0.0,
            0.0,
            50.0,
            50.0,
            vec![ActionTrigger::Click],
            vec![],
        ));
        let (mut router, geometry) = setup(&document);
        let owner = key(&document, "widget");
        router
            .route(
                &document,
                &geometry,
                pointer(1, true, 10.0, 10.0, Some(owner)),
            )
            .unwrap();
        assert!(
            router
                .route(
                    &document,
                    &geometry,
                    pointer(1, false, 200.0, 200.0, Some(owner))
                )
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            router
                .route(
                    &document,
                    &geometry,
                    InputEvent::Widget(WidgetEvent {
                        source: document.stamp(owner).unwrap(),
                        kind: WidgetEventKind::Activate { gesture: Some(1) }
                    })
                )
                .unwrap_err(),
            EventError::InvalidGesture
        );
    }

    #[test]
    fn decorated_native_owner_routes_once_and_editor_click_closes_outside() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            200.0,
            200.0,
            vec![],
            vec![
                node(
                    "editor",
                    0.0,
                    0.0,
                    80.0,
                    80.0,
                    vec![ActionTrigger::Click],
                    vec![node(
                        "decoration",
                        0.0,
                        0.0,
                        20.0,
                        20.0,
                        vec![ActionTrigger::Click],
                        vec![],
                    )],
                ),
                node(
                    "popup",
                    100.0,
                    0.0,
                    50.0,
                    50.0,
                    vec![ActionTrigger::ClickOutside],
                    vec![],
                ),
            ],
        ));
        let (mut router, geometry) = setup(&document);
        let owner = key(&document, "editor");
        router
            .route(
                &document,
                &geometry,
                pointer(1, true, 10.0, 10.0, Some(owner)),
            )
            .unwrap();
        let events = router
            .route(
                &document,
                &geometry,
                pointer(1, false, 10.0, 10.0, Some(owner)),
            )
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].source.key, owner);
        assert_eq!(events[1].trigger, ActionTrigger::ClickOutside);
        // Editors need not emit command activation. Outside delivery is complete now.
        router.end_input_batch();
        assert!(router.gestures.is_empty());
    }

    #[test]
    fn global_escape_works_unfocused_and_from_unconsuming_editor() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::KeyDown],
            vec![node("editor", 0.0, 0.0, 30.0, 30.0, vec![], vec![])],
        ));
        let (mut router, geometry) = setup(&document);
        let mut keyboard = KeyboardEvent {
            key: "Escape".into(),
            code: Some("Escape".into()),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
            composing: false,
            editing_consumed: false,
        };
        let root = key(&document, "root");
        let events = router
            .route(&document, &geometry, InputEvent::Key(keyboard.clone()))
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.key, root);
        router
            .focus(&document, &geometry, Some(key(&document, "editor")), true)
            .unwrap();
        let events = router
            .route(&document, &geometry, InputEvent::Key(keyboard.clone()))
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source.key, root);
        keyboard.editing_consumed = true;
        assert!(
            router
                .route(&document, &geometry, InputEvent::Key(keyboard))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn popup_becoming_visible_during_gesture_is_not_outside_eligible() {
        let mut document = document(node(
            "root",
            0.0,
            0.0,
            200.0,
            200.0,
            vec![],
            vec![node(
                "popup",
                100.0,
                0.0,
                50.0,
                50.0,
                vec![ActionTrigger::ClickOutside],
                vec![],
            )],
        ));
        let mut roots = document.roots().to_vec();
        roots[0].children[0].visibility = Some(hyperchad_transformer::models::Visibility::Hidden);
        let hidden = geometry_roots(&document, &roots, &BTreeMap::new());
        let mut router = EventRouter::new();
        router.reconcile(&document).unwrap();
        router.refresh_geometry(&document, &hidden).unwrap();
        router
            .route(&document, &hidden, pointer(1, true, 10.0, 10.0, None))
            .unwrap();
        let visible = geometry(&document, &BTreeMap::new());
        router.refresh_geometry(&document, &visible).unwrap();
        let events = router
            .route(&document, &visible, pointer(1, false, 10.0, 10.0, None))
            .unwrap();
        assert!(
            events
                .iter()
                .all(|event| event.trigger != ActionTrigger::ClickOutside)
        );
        // An unrelated authored descendant mutation must not suppress the same binding
        // on the next gesture merely because its content stamp changes.
        router
            .route(&document, &visible, pointer(2, true, 10.0, 10.0, None))
            .unwrap();
        let popup = key(&document, "popup");
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        children: vec![node("new", 0.0, 0.0, 10.0, 10.0, vec![], vec![])],
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Element {
                    source: popup,
                    target: Selector::SelfTarget,
                    strategy: SwapStrategy::BeforeEnd,
                },
            )
            .unwrap();
        router.reconcile(&document).unwrap();
        let current = geometry(&document, &BTreeMap::new());
        let events = router
            .route(&document, &current, pointer(2, false, 10.0, 10.0, None))
            .unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.trigger == ActionTrigger::ClickOutside)
                .count(),
            1
        );
        assert_eq!(events[0].source, document.stamp(popup).unwrap());
    }

    #[test]
    fn departure_preserves_press_but_cancel_invalidates_it() {
        let document = document(node(
            "widget",
            0.0,
            0.0,
            50.0,
            50.0,
            vec![ActionTrigger::Click, ActionTrigger::Hover],
            vec![],
        ));
        let (mut router, geometry) = setup(&document);
        let owner = key(&document, "widget");
        router
            .route(
                &document,
                &geometry,
                pointer(1, true, 10.0, 10.0, Some(owner)),
            )
            .unwrap();
        let departed = router
            .route(&document, &geometry, InputEvent::PointerDeparted)
            .unwrap();
        assert_eq!(departed.len(), 1);
        assert_eq!(departed[0].phase, EventPhase::Leave);
        let events = router
            .route(
                &document,
                &geometry,
                pointer(1, false, 10.0, 10.0, Some(owner)),
            )
            .unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.trigger == ActionTrigger::Click)
                .count(),
            1
        );
        router
            .route(&document, &geometry, InputEvent::PointerCancelled)
            .unwrap();
        assert_eq!(
            router
                .route(
                    &document,
                    &geometry,
                    InputEvent::Widget(WidgetEvent {
                        source: document.stamp(owner).unwrap(),
                        kind: WidgetEventKind::Activate { gesture: Some(1) }
                    })
                )
                .unwrap_err(),
            EventError::InvalidGesture
        );
    }

    #[test]
    fn invalid_input_is_explicit_and_cancel_never_clicks() {
        let document = document(node(
            "root",
            0.0,
            0.0,
            100.0,
            100.0,
            vec![ActionTrigger::Click],
            vec![],
        ));
        let (mut router, geometry) = setup(&document);
        assert_eq!(
            router
                .route(&document, &geometry, pointer(9, false, 10.0, 10.0, None))
                .unwrap_err(),
            EventError::InvalidGesture
        );
        router
            .route(&document, &geometry, pointer(1, true, 10.0, 10.0, None))
            .unwrap();
        assert!(
            router
                .route(&document, &geometry, InputEvent::PointerCancelled)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            router
                .route(&document, &geometry, pointer(1, false, 10.0, 10.0, None))
                .unwrap_err(),
            EventError::InvalidGesture
        );
        assert_eq!(
            router
                .route(
                    &document,
                    &geometry,
                    InputEvent::PointerMoved(pos2(f32::NAN, 0.0))
                )
                .unwrap_err(),
            EventError::InvalidPosition
        );
    }
}
