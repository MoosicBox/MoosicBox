//! Pure, bounded `HyperChad` action execution. No terminal, filesystem, or network IO.
//!
//! The host supplies semantic control values and a source from the current document generation.
//! Dispatch is transactional: errors publish neither tree edits nor effects. Execute returned
//! effects in order outside the document lock. Route responses must retain `RequestEffect::source`,
//! `target`, `strategy`, and `request_id` when passed to the state lane's view application API.
//! Call `release` on hover exit / effect deactivation, `advance` at `next_deadline`, and
//! `reconcile` after document updates. Painting must never dispatch actions.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod admission;
mod eval;
#[cfg(test)]
mod expression_tests;
#[cfg(test)]
mod form_tests;
mod forms;
#[cfg(test)]
mod reconcile_tests;
mod selectors;
#[cfg(test)]
mod target_tests;
#[cfg(test)]
mod tests;

pub use forms::{
    AuthorizedFile, ControlValue, FormPart, FormValues, MultipartSegment, RequestBody,
};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use hyperchad_actions::{
    ActionEffect, ActionTrigger, ActionType, HttpEventContext, InputActionType, Key, LogLevel,
    StyleAction,
    handler::should_trigger_action,
    logic::{Condition, Value},
};
use hyperchad_color::Color;
use hyperchad_transformer::{Container, Element};
use hyperchad_transformer_models::{
    ElementTarget, LinkTarget, Route, Selector, SwapStrategy, Visibility,
};

/// Whether native defaults need a load event before presentation, including hidden nodes.
pub(crate) fn has_load_route(node: &Container) -> bool {
    matches!(
        &node.route,
        Some(Route::Get { trigger, .. }
            | Route::Post { trigger, .. }
            | Route::Put { trigger, .. }
            | Route::Delete { trigger, .. }
            | Route::Patch { trigger, .. }) if trigger.as_deref() == Some("load")
    )
}

/// Checked identity of an event source. Paths are child indices relative to the root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Source {
    /// Canonical `layout::node_key` at `path`.
    pub key: String,
    /// Child-index path; the root is `[]`.
    pub path: Vec<usize>,
    /// State lane document generation. Stale events must not target replacements.
    pub generation: u64,
}

impl Source {
    /// Capture an identity from a current tree.
    ///
    /// # Errors
    /// * Returns `InvalidSource` if the path does not exist.
    pub fn at(root: &Container, path: Vec<usize>, generation: u64) -> Result<Self, ActionError> {
        let node = selectors::at(root, &path).ok_or(ActionError::InvalidSource)?;
        Ok(Self {
            key: crate::layout::node_key(node, &path),
            path,
            generation,
        })
    }
}

/// Semantic input event, independent of a terminal input protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// Pointer click or keyboard activation of a clickable element.
    Click,
    /// Host determined that the pointer click is outside this source's subtree.
    ClickOutside,
    /// Pointer press.
    MouseDown,
    /// Key press (key state comes from `EventContext::keys`).
    KeyDown,
    /// Hover enter. Host must call `release` on exit.
    Hover,
    /// Committed semantic control change.
    Change,
    /// Effective geometry changed.
    Resize,
    /// Once per action registration in this generation.
    Immediate,
    /// Named event delivered by `Renderer::emit_event`.
    Named(String),
    /// One of the six represented request lifecycle triggers.
    Http(HttpPhase),
    /// Submit nearest ancestor form (including the source itself).
    Submit,
    /// Restore nearest ancestor form's declared defaults.
    Reset,
}

/// HTTP lifecycle stages. The host emits exactly one terminal outcome, then `After`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpPhase {
    Before,
    After,
    Success,
    Error,
    Abort,
    Timeout,
}

impl EventKind {
    fn matches(&self, trigger: &ActionTrigger) -> bool {
        let name = match self {
            Self::Click => "click",
            Self::ClickOutside => "click_outside",
            Self::MouseDown => "mouse_down",
            Self::KeyDown => "key_down",
            Self::Hover => "hover",
            Self::Change => "change",
            Self::Resize => "resize",
            Self::Immediate => "immediate",
            Self::Named(name) => {
                return matches!(trigger, ActionTrigger::Event(expected) if expected == name);
            }
            Self::Http(phase) => match phase {
                HttpPhase::Before => "http_before_request",
                HttpPhase::After => "http_after_request",
                HttpPhase::Success => "http_request_success",
                HttpPhase::Error => "http_request_error",
                HttpPhase::Abort => "http_request_abort",
                HttpPhase::Timeout => "http_request_timeout",
            },
            Self::Submit | Self::Reset => return false,
        };
        // Named events must not alias the built-in event namespace.
        !matches!(trigger, ActionTrigger::Event(_)) && should_trigger_action(trigger, name)
    }
}

/// Geometry in logical pixels, matching layout and the presented hit-test snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct Geometry {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// Complete event input. Coordinates are logical pixels, never terminal escape data.
#[derive(Clone, Debug)]
pub struct EventContext {
    /// Source validated against the current root and generation.
    pub source: Source,
    /// Semantic event kind.
    pub kind: EventKind,
    /// Caller-owned monotonic clock in milliseconds.
    pub now_ms: u64,
    /// Optional scalar change/custom-event value.
    pub value: Option<String>,
    /// Currently pressed keys; supports chords without inventing key-up events.
    pub keys: Vec<Key>,
    /// Pointer in root logical coordinates. Keyboard activation leaves this absent.
    pub pointer: Option<(f32, f32)>,
    /// Current presented geometry indexed by canonical node key.
    pub geometry: BTreeMap<String, Geometry>,
    /// Required for HTTP events; available to DSL as `http.*`.
    pub http: Option<HttpEventContext>,
    /// Correlates a request and its response. The host is responsible for uniqueness.
    pub request_id: Option<String>,
    /// Current location for relative form action / anchor resolution by the host.
    pub location: String,
    /// Suppress native anchor/button/form defaults after custom actions.
    pub prevent_default: bool,
}

impl EventContext {
    /// Construct a minimal semantic event; fill optional metadata before dispatch.
    #[must_use]
    pub const fn new(source: Source, kind: EventKind, now_ms: u64) -> Self {
        Self {
            source,
            kind,
            now_ms,
            value: None,
            keys: Vec::new(),
            pointer: None,
            geometry: BTreeMap::new(),
            http: None,
            request_id: None,
            location: String::new(),
            prevent_default: false,
        }
    }
}

/// Explicit, IO-free request for the runtime's authorized transport.
#[derive(Clone, Debug)]
pub struct RequestEffect {
    /// Triggering identity, including the generation for stale-response rejection.
    pub source: Source,
    /// Original URL. The host resolves it against `base_url` and applies URL policy.
    pub url: String,
    /// Base location at dispatch time.
    pub base_url: String,
    /// Uppercase HTTP method.
    pub method: String,
    /// Ordered body, retaining duplicate form names and opaque file capabilities.
    pub body: RequestBody,
    /// Request headers generated by form serialization; host adds cookies/auth/CSRF.
    pub headers: BTreeMap<String, String>,
    /// Response primary's selector, relative to `source` where appropriate.
    pub target: Selector,
    /// Exact requested response swap strategy, including `None` and `Delete`.
    pub strategy: SwapStrategy,
    /// Host correlation identity.
    pub request_id: Option<String>,
    /// A normal form `action` response replaces the page instead of a route target.
    pub navigation: bool,
}

/// Work to execute outside the action engine. No effect performs IO when constructed.
#[derive(Clone, Debug)]
pub enum Effect {
    /// Existing `AppBuilder` custom-action bridge.
    Custom {
        action: String,
        value: Option<Value>,
        source: Source,
    },
    /// Host navigation with safe URL resolution and target policy.
    Navigate {
        url: String,
        base_url: String,
        target: Option<LinkTarget>,
        source: Source,
    },
    /// Local router or authorized HTTP transport request.
    Request(RequestEffect),
    /// Host logging. Do not add passwords/file capabilities to log output.
    Log { level: LogLevel, message: String },
    /// Focus or blur the specified control.
    Focus { source: Source, focused: bool },
    /// Select all editable text in this control (and focus it).
    Select { source: Source },
    /// Replace semantic values with defaults. Host resets caret/dirty state too.
    Reset { source: Source, values: FormValues },
    /// Invalidate style/layout/paint after effective tree styles changed.
    StylesChanged { keys: Vec<String> },
}

/// Bounds apply before allocating persistent state and on every interpreter step.
#[derive(Clone, Debug)]
pub struct ActionLimits {
    /// Maximum document nodes visited per dispatch.
    pub max_nodes: usize,
    /// Maximum tree/AST nesting.
    pub max_depth: usize,
    /// Maximum interpreter operations, including loop iterations and selectors.
    pub max_steps: usize,
    /// Maximum effect/state/variable/collection entries per operation or engine map.
    pub max_entries: usize,
    /// Maximum individual string and aggregate semantic input bytes.
    pub max_value_bytes: usize,
    /// Maximum encoded request size (including authorized file lengths).
    pub max_request_bytes: u64,
}

impl Default for ActionLimits {
    fn default() -> Self {
        Self {
            max_nodes: 65_536,
            max_depth: 96,
            max_steps: 16_384,
            max_entries: 4_096,
            max_value_bytes: 1_048_576,
            max_request_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Execution failure. A failed dispatch commits neither style changes nor effects/state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionError {
    /// Source path/key does not identify a current node.
    InvalidSource,
    /// Source belongs to an earlier document generation.
    StaleSource,
    /// Caller clock moved backwards.
    ClockReversed,
    /// Missing typed HTTP event context.
    MissingHttpContext,
    /// Resource or loop budget exhausted.
    Limit(&'static str),
    /// Unsupported or malformed selector (never silently treated as an ID).
    Selector(String),
    /// Missing target, variable, or semantic value.
    Missing(String),
    /// Invalid expression type, arithmetic, URL, method, or form data.
    Invalid(String),
    /// Backend-specific executable DSL requires an explicit host integration.
    Unsupported(String),
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "action execution failed: {self:?}")
    }
}
impl std::error::Error for ActionError {}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Owner {
    source: Source,
    action: Vec<usize>,
}
#[derive(Clone, Debug)]
struct Timing {
    last: u64,
    unique: bool,
    // Nested effects share the top-level immediate definition's lifecycle.
    immediate_definition: Option<ActionEffect>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Style {
    hidden: Option<bool>,
    visibility: Option<Visibility>,
    background: Option<Color>,
}
impl Style {
    const fn read(node: &Container) -> Self {
        Self {
            hidden: node.hidden,
            visibility: node.visibility,
            background: node.background,
        }
    }
    const fn write(self, node: &mut Container) {
        node.hidden = self.hidden;
        node.visibility = self.visibility;
        node.background = self.background;
    }
}
#[derive(Clone, Debug)]
struct Layer {
    owner: Owner,
    action: StyleAction,
    order: u64,
    delay: u64,
    expires: Option<u64>,
}
#[derive(Clone, Debug)]
struct StyledNode {
    source: Source,
    base: Style,
    layers: Vec<Layer>,
}
impl StyledNode {
    fn effective(&self) -> Style {
        let mut style = self.base;
        for layer in &self.layers {
            match &layer.action {
                StyleAction::SetVisibility(value) => style.visibility = Some(*value),
                StyleAction::SetDisplay(value) => style.hidden = Some(!value),
                StyleAction::SetBackground(value) => {
                    style.background = value.as_deref().and_then(|v| Color::try_from_hex(v).ok());
                }
                StyleAction::SetFocus(_) => {} // focus is a host effect, never stored here
            }
        }
        style
    }
}
#[derive(Clone, Debug)]
struct Registration {
    source: Source,
    name: String,
    action: ActionType,
    owner: Owner,
}

/// Retained interpreter state. Does not own the document, controls, clock, or IO.
#[derive(Clone, Debug)]
pub struct ActionEngine {
    generation: u64,
    limits: ActionLimits,
    clock: u64,
    sequence: u64,
    timing: BTreeMap<Owner, Timing>,
    loaded_routes: BTreeMap<String, Route>,
    styles: BTreeMap<String, StyledNode>,
    variables: BTreeMap<(String, String), eval::Datum>,
    registrations: BTreeMap<Owner, Registration>,
}

impl Default for ActionEngine {
    fn default() -> Self {
        Self::new(0, ActionLimits::default())
    }
}
#[cfg(test)]
thread_local! {
    pub(crate) static NAMED_WORK_COUNTS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

impl ActionEngine {
    /// Whether a normal transaction is required for style expiry or recommit.
    /// This does not advance time or mutate effects; the original dispatch owns that work.
    #[must_use]
    pub(crate) fn named_maintenance_due(&self, root: &Container, now_ms: u64) -> bool {
        self.next_deadline()
            .is_some_and(|deadline| deadline <= now_ms)
            || self.styles.values().any(|styled| {
                selectors::at(root, &styled.source.path)
                    .is_some_and(|node| Style::read(node) != styled.effective())
            })
    }

    /// Whether this source has an exact-name action, registration or route default.
    /// Re-evaluate for every original source: earlier handlers may change registrations.
    pub(crate) fn accepts_named(&self, key: &str, node: &Container, name: &str) -> bool {
        if node.actions.iter().any(
            |action| matches!(&action.trigger, ActionTrigger::Event(expected) if expected == name),
        ) || self
            .registrations
            .values()
            .any(|registration| registration.source.key == key && registration.name == name)
        {
            return true;
        }
        node.route.as_ref().is_some_and(|route| {
            use hyperchad_transformer_models::Route;
            let trigger = match route {
                Route::Get { trigger, .. }
                | Route::Post { trigger, .. }
                | Route::Put { trigger, .. }
                | Route::Patch { trigger, .. }
                | Route::Delete { trigger, .. } => trigger,
            };
            trigger.as_deref() == Some(name)
        })
    }
    /// Create a pure engine for one state lane generation.
    #[must_use]
    pub const fn new(generation: u64, limits: ActionLimits) -> Self {
        Self {
            generation,
            limits,
            clock: 0,
            sequence: 0,
            timing: BTreeMap::new(),
            loaded_routes: BTreeMap::new(),
            styles: BTreeMap::new(),
            variables: BTreeMap::new(),
            registrations: BTreeMap::new(),
        }
    }

    /// Execute matching actions and native route/form/anchor defaults transactionally.
    ///
    /// # Errors
    /// * Rejects stale/missing source identities, reversed clocks and missing HTTP context.
    /// * Rejects invalid/unsupported expressions, targets and forms, or exhausted budgets.
    pub fn dispatch(
        &mut self,
        root: &mut Container,
        event: &EventContext,
        values: &FormValues,
    ) -> Result<Vec<Effect>, ActionError> {
        let effects = self.dispatch_readonly(root, event, values)?;
        self.publish_styles(root);
        Ok(effects)
    }

    /// Evaluate atomically without obtaining mutable access to the source tree.
    /// Publish styles before handling effects if `StylesChanged` is returned.
    pub(crate) fn dispatch_readonly(
        &mut self,
        root: &Container,
        event: &EventContext,
        values: &FormValues,
    ) -> Result<Vec<Effect>, ActionError> {
        #[cfg(test)]
        if matches!(event.kind, EventKind::Named(_)) {
            NAMED_WORK_COUNTS.with(|counts| {
                let (dispatch, index) = counts.get();
                counts.set((dispatch + 1, index));
            });
        }
        self.validate(root, event)?;
        let mut next = self.clone();
        let mut work = Work::new(root, event, values, &mut next)?;
        work.expire();
        let node = selectors::at(root, &event.source.path).ok_or(ActionError::InvalidSource)?;
        for (index, action) in node.actions.iter().enumerate() {
            if event.kind.matches(&action.trigger) {
                admission::action(&action.effect.action, &work.engine.limits)?;
                let owner = Owner {
                    source: event.source.clone(),
                    action: vec![index],
                };
                work.run_effect(&action.effect, owner, None, 0)?;
            }
        }
        if let EventKind::Named(name) = &event.kind {
            let registrations: Vec<_> = work
                .engine
                .registrations
                .values()
                .filter(|r| r.source.key == event.source.key && &r.name == name)
                .cloned()
                .collect();
            for registration in registrations {
                work.run(&registration.action, registration.owner, None, 0, 0)?;
            }
        }
        if !event.prevent_default {
            work.defaults(node)?;
        }
        let effects = work.finish()?;
        next.finish_evaluation(&effects);
        *self = next;
        Ok(effects)
    }

    /// Reverse effects belonging to registrations matching an event (e.g. hover exit).
    /// Delayed reversals remain active until `advance` reaches their deadline.
    ///
    /// # Errors
    /// * Rejects stale/missing sources, backwards time, or resource limits.
    pub fn release(
        &mut self,
        root: &mut Container,
        event: &EventContext,
    ) -> Result<Vec<Effect>, ActionError> {
        let effects = self.release_readonly(root, event)?;
        self.publish_styles(root);
        Ok(effects)
    }

    /// Evaluate release atomically; publish returned style changes before effects.
    pub(crate) fn release_readonly(
        &mut self,
        root: &Container,
        event: &EventContext,
    ) -> Result<Vec<Effect>, ActionError> {
        self.validate(root, event)?;
        let mut next = self.clone();
        let values = FormValues::new();
        let mut work = Work::new(root, event, &values, &mut next)?;
        let node = selectors::at(root, &event.source.path).ok_or(ActionError::InvalidSource)?;
        let indices: BTreeSet<_> = node
            .actions
            .iter()
            .enumerate()
            .filter(|(_, a)| event.kind.matches(&a.trigger))
            .map(|(i, _)| i)
            .collect();
        for styled in work.engine.styles.values_mut() {
            for layer in &mut styled.layers {
                if layer.owner.source == event.source
                    && layer
                        .owner
                        .action
                        .first()
                        .is_some_and(|i| indices.contains(i))
                {
                    layer.expires = Some(event.now_ms.saturating_add(layer.delay));
                }
            }
        }
        work.expire();
        let effects = work.finish()?;
        next.finish_evaluation(&effects);
        *self = next;
        Ok(effects)
    }

    /// Expire released style effects without dispatching user actions.
    ///
    /// # Errors
    /// * Rejects backwards time or tree/resource limit violations.
    pub fn advance(
        &mut self,
        root: &mut Container,
        now_ms: u64,
    ) -> Result<Vec<Effect>, ActionError> {
        let effects = self.advance_readonly(root, now_ms)?;
        self.publish_styles(root);
        Ok(effects)
    }

    /// Evaluate expiration atomically; publish returned style changes before effects.
    pub(crate) fn advance_readonly(
        &mut self,
        root: &Container,
        now_ms: u64,
    ) -> Result<Vec<Effect>, ActionError> {
        let source = Source::at(root, Vec::new(), self.generation)?;
        let event = EventContext::new(source, EventKind::Immediate, now_ms);
        self.validate(root, &event)?;
        let mut next = self.clone();
        let values = FormValues::new();
        let mut work = Work::new(root, &event, &values, &mut next)?;
        work.expire();
        let effects = work.finish()?;
        next.finish_evaluation(&effects);
        *self = next;
        Ok(effects)
    }

    /// Earliest released-effect deadline in the caller's clock units.
    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        self.styles
            .values()
            .flat_map(|s| &s.layers)
            .filter_map(|l| l.expires)
            .min()
    }

    /// Rebind retained identities after structural updates without resetting timers or baselines.
    /// Use `reconcile_removed` with `ApplyReport::removed` at a committed view boundary to
    /// distinguish same-key replacements from retained nodes. This convenience method assumes
    /// all keys still present are retained; it cannot infer replacement identity from a tree.
    ///
    /// # Errors
    /// * Rejects excessive tree size/depth or duplicate canonical keys.
    pub fn reconcile(&mut self, root: &Container, generation: u64) -> Result<(), ActionError> {
        self.reconcile_removed(root, generation, &BTreeSet::new())
    }

    /// Reconcile using the state lane's exact removed/reset key set, including reused IDs.
    /// Timers, variables and event registrations survive unrelated updates and path moves.
    /// Removed targets and effects owned by removed sources are disposed, never transferred
    /// to replacements. Retained style baselines remain reversible across generations.
    ///
    /// # Errors
    /// * Rejects excessive tree size/depth or duplicate canonical keys before mutating state.
    pub fn reconcile_removed(
        &mut self,
        root: &Container,
        generation: u64,
        removed: &BTreeSet<String>,
    ) -> Result<(), ActionError> {
        let index = selectors::index(root, &self.limits)?;
        self.loaded_routes.retain(|key, route| {
            !removed.contains(key)
                && index
                    .get(key)
                    .and_then(|path| selectors::at(root, path))
                    .is_some_and(|node| node.route.as_ref() == Some(route))
        });
        let rebind = |source: &mut Source| {
            if removed.contains(&source.key) {
                return false;
            }
            let Some(path) = index.get(&source.key) else {
                return false;
            };
            source.path.clone_from(path);
            source.generation = generation;
            true
        };
        self.timing = std::mem::take(&mut self.timing)
            .into_iter()
            .filter_map(|(mut owner, timing)| {
                if !rebind(&mut owner.source) {
                    return None;
                }
                if let Some(definition) = &timing.immediate_definition {
                    let action = selectors::at(root, &owner.source.path)
                        .and_then(|node| owner.action.first().and_then(|i| node.actions.get(*i)));
                    if !action.is_some_and(|action| {
                        matches!(action.trigger, ActionTrigger::Immediate)
                            && &action.effect == definition
                    }) {
                        return None;
                    }
                }
                Some((owner, timing))
            })
            .collect();
        self.registrations = std::mem::take(&mut self.registrations)
            .into_iter()
            .filter_map(|(mut owner, mut registration)| {
                if !rebind(&mut owner.source)
                    || !rebind(&mut registration.source)
                    || !rebind(&mut registration.owner.source)
                {
                    return None;
                }
                Some((owner, registration))
            })
            .collect();
        for styled in self.styles.values_mut() {
            if !rebind(&mut styled.source) {
                continue;
            }
            // Removed owners may have styled a retained target. Keep a zero-deadline layer
            // until advance/dispatch so the previous baseline is actually restored in root.
            for layer in &mut styled.layers {
                if !rebind(&mut layer.owner.source) {
                    layer.expires = Some(self.clock);
                }
            }
            if let Some(node) = selectors::at(root, &styled.source.path) {
                let actual = Style::read(node);
                let effective = styled.effective();
                // A retained node may have received authoritative base style changes.
                if actual.hidden != effective.hidden {
                    styled.base.hidden = actual.hidden;
                }
                if actual.visibility != effective.visibility {
                    styled.base.visibility = actual.visibility;
                }
                if actual.background != effective.background {
                    styled.base.background = actual.background;
                }
            }
        }
        self.styles
            .retain(|key, _| !removed.contains(key) && index.contains_key(key));
        self.variables
            .retain(|(key, _), _| !removed.contains(key) && index.contains_key(key));
        self.generation = generation;
        Ok(())
    }

    fn finish_evaluation(&mut self, effects: &[Effect]) {
        // Empty layers may still carry a baseline that must be restored. Keep
        // them until publication only when finish detected actual source changes.
        if !effects
            .iter()
            .any(|effect| matches!(effect, Effect::StylesChanged { .. }))
        {
            self.styles.retain(|_, styled| !styled.layers.is_empty());
        }
    }

    /// Publish staged styles immediately after successful readonly evaluation.
    pub(crate) fn publish_styles(&mut self, root: &mut Container) {
        for styled in self.styles.values() {
            if let Some(node) = selectors::at_mut(root, &styled.source.path) {
                styled.effective().write(node);
            }
        }
        self.styles.retain(|_, s| !s.layers.is_empty());
    }

    fn validate(&self, root: &Container, event: &EventContext) -> Result<(), ActionError> {
        if event.source.generation != self.generation {
            return Err(ActionError::StaleSource);
        }
        if event.now_ms < self.clock {
            return Err(ActionError::ClockReversed);
        }
        let node = selectors::at(root, &event.source.path).ok_or(ActionError::InvalidSource)?;
        if crate::layout::node_key(node, &event.source.path) != event.source.key {
            return Err(ActionError::InvalidSource);
        }
        if matches!(event.kind, EventKind::Http(_)) && event.http.is_none() {
            return Err(ActionError::MissingHttpContext);
        }
        Ok(())
    }
}

struct Work<'a> {
    root: &'a Container,
    event: &'a EventContext,
    values: &'a FormValues,
    engine: &'a mut ActionEngine,
    index: BTreeMap<String, Vec<usize>>,
    effects: Vec<Effect>,
    steps: usize,
}

impl<'a> Work<'a> {
    fn new(
        root: &'a Container,
        event: &'a EventContext,
        values: &'a FormValues,
        engine: &'a mut ActionEngine,
    ) -> Result<Self, ActionError> {
        #[cfg(test)]
        if matches!(event.kind, EventKind::Named(_)) {
            NAMED_WORK_COUNTS.with(|counts| {
                let (dispatch, index) = counts.get();
                counts.set((dispatch, index + 1));
            });
        }
        let index = selectors::index(root, &engine.limits)?;
        forms::validate_values(values, &engine.limits)?;
        engine.clock = event.now_ms;
        let work = Self {
            root,
            event,
            values,
            engine,
            index,
            effects: Vec::new(),
            steps: 0,
        };
        work.text(&event.location)?;
        if let Some(value) = &event.value {
            work.text(value)?;
        }
        if let Some(http) = &event.http {
            work.text(&http.url)?;
            work.text(&http.method)?;
            if let Some(error) = &http.error {
                work.text(error)?;
            }
            if let Some(headers) = &http.headers {
                if headers.len() > work.engine.limits.max_entries {
                    return Err(ActionError::Limit("HTTP headers"));
                }
                for (key, value) in headers {
                    work.text(key)?;
                    work.text(value)?;
                }
            }
        }
        if event.keys.len() > work.engine.limits.max_entries
            || event.geometry.len() > work.engine.limits.max_nodes
        {
            return Err(ActionError::Limit("event metadata"));
        }
        Ok(work)
    }
    fn step(&mut self, depth: usize) -> Result<(), ActionError> {
        self.steps = self
            .steps
            .checked_add(1)
            .ok_or(ActionError::Limit("steps"))?;
        if self.steps > self.engine.limits.max_steps || depth > self.engine.limits.max_depth {
            return Err(ActionError::Limit("steps/depth"));
        }
        Ok(())
    }
    const fn text(&self, value: &str) -> Result<(), ActionError> {
        if value.len() > self.engine.limits.max_value_bytes {
            Err(ActionError::Limit("value bytes"))
        } else {
            Ok(())
        }
    }
    fn emit(&mut self, effect: Effect) -> Result<(), ActionError> {
        if self.effects.len() >= self.engine.limits.max_entries {
            return Err(ActionError::Limit("effects"));
        }
        self.effects.push(effect);
        Ok(())
    }
    fn source(&self, path: &[usize]) -> Result<Source, ActionError> {
        Source::at(self.root, path.to_vec(), self.engine.generation)
    }
    fn expire(&mut self) {
        for style in self.engine.styles.values_mut() {
            style
                .layers
                .retain(|l| l.expires.is_none_or(|at| at > self.event.now_ms));
        }
    }
    fn finish(self) -> Result<Vec<Effect>, ActionError> {
        let mut changes = Vec::new();
        for (key, styled) in &self.engine.styles {
            if let Some(node) = selectors::at(self.root, &styled.source.path)
                && Style::read(node) != styled.effective()
            {
                changes.push(key.clone());
            }
        }
        let mut effects = self.effects;
        if !changes.is_empty() {
            if effects.len() >= self.engine.limits.max_entries {
                return Err(ActionError::Limit("effects"));
            }
            effects.push(Effect::StylesChanged { keys: changes });
        }
        Ok(effects)
    }
    fn run_effect(
        &mut self,
        effect: &ActionEffect,
        owner: Owner,
        value: Option<Value>,
        depth: usize,
    ) -> Result<(), ActionError> {
        self.step(depth)?;
        if let Some(timing) = self.engine.timing.get(&owner)
            && (timing.unique
                || matches!(self.event.kind, EventKind::Immediate)
                || effect
                    .throttle
                    .is_some_and(|ms| self.event.now_ms.saturating_sub(timing.last) < ms))
        {
            return Ok(());
        }
        if !self.engine.timing.contains_key(&owner)
            && self.engine.timing.len() >= self.engine.limits.max_entries
        {
            return Err(ActionError::Limit("timers"));
        }
        self.engine.timing.insert(
            owner.clone(),
            Timing {
                last: self.event.now_ms,
                unique: effect.unique == Some(true),
                immediate_definition: if matches!(self.event.kind, EventKind::Immediate) {
                    selectors::at(self.root, &owner.source.path)
                        .and_then(|node| owner.action.first().and_then(|i| node.actions.get(*i)))
                        .map(|action| action.effect.clone())
                } else {
                    None
                },
            },
        );
        self.run(
            &effect.action,
            owner,
            value,
            effect.delay_off.unwrap_or(0),
            depth + 1,
        )
    }
    // Keep exhaustive sequencing and parameter propagation in one interpreter match.
    #[allow(clippy::too_many_lines)]
    fn run(
        &mut self,
        action: &ActionType,
        owner: Owner,
        value: Option<Value>,
        delay: u64,
        depth: usize,
    ) -> Result<(), ActionError> {
        self.step(depth)?;
        match action {
            ActionType::NoOp => Ok(()),
            ActionType::Let { name, value } => {
                let datum = self.expression(value, &owner, depth + 1)?;
                self.set_variable(&owner.source.key, name, datum)
            }
            ActionType::Style { target, action } => self.style(target, action, &owner, delay),
            ActionType::Input(InputActionType::Select { target }) => {
                for path in self.targets(target, &owner.source, depth + 1)? {
                    let node = selectors::at(self.root, &path).ok_or(ActionError::InvalidSource)?;
                    if !matches!(
                        node.element,
                        Element::Input {
                            input: hyperchad_transformer::Input::Text { .. }
                                | hyperchad_transformer::Input::Password { .. },
                            ..
                        } | Element::Textarea { .. }
                    ) {
                        return Err(ActionError::Invalid(
                            "select target is not editable text".into(),
                        ));
                    }
                    self.emit(Effect::Select {
                        source: self.source(&path)?,
                    })?;
                }
                Ok(())
            }
            ActionType::Navigate { url } => {
                forms::validate_url(url, &self.engine.limits)?;
                self.emit(Effect::Navigate {
                    url: url.clone(),
                    base_url: self.event.location.clone(),
                    target: None,
                    source: owner.source,
                })
            }
            ActionType::Log { message, level } => {
                self.text(message)?;
                self.emit(Effect::Log {
                    level: *level,
                    message: message.clone(),
                })
            }
            ActionType::Custom { action } => {
                self.text(action)?;
                self.emit(Effect::Custom {
                    action: action.clone(),
                    value,
                    source: owner.source,
                })
            }
            ActionType::Event { name, action } => {
                self.text(name)?;
                // Validate before cloning retained AST. This also bounds destruction/clone depth.
                eval::validate_action(action, &self.engine.limits)?;
                if !self.engine.registrations.contains_key(&owner)
                    && self.engine.registrations.len() >= self.engine.limits.max_entries
                {
                    return Err(ActionError::Limit("event registrations"));
                }
                self.engine.registrations.insert(
                    owner.clone(),
                    Registration {
                        source: owner.source.clone(),
                        name: name.clone(),
                        action: (**action).clone(),
                        owner,
                    },
                );
                Ok(())
            }
            ActionType::Multi(actions) => {
                for (i, action) in actions.iter().enumerate() {
                    let mut child = owner.clone();
                    child.action.push(i);
                    self.run(action, child, value.clone(), delay, depth + 1)?;
                }
                Ok(())
            }
            ActionType::MultiEffect(effects) => {
                for (i, effect) in effects.iter().enumerate() {
                    let mut child = owner.clone();
                    child.action.push(i);
                    self.run_effect(effect, child, value.clone(), depth + 1)?;
                }
                Ok(())
            }
            ActionType::Parameterized { action, value } => {
                let value = self.value(value, &owner, depth + 1)?;
                self.run(action, owner, Some(value), delay, depth + 1)
            }
            ActionType::Logic(condition) => {
                let matched = match &condition.condition {
                    Condition::Bool(value) => *value,
                    Condition::Eq(a, b) => {
                        self.value(a, &owner, depth + 1)? == self.value(b, &owner, depth + 1)?
                    }
                };
                for (i, effect) in (if matched {
                    &condition.actions
                } else {
                    &condition.else_actions
                })
                .iter()
                .enumerate()
                {
                    let mut child = owner.clone();
                    child.action.extend([usize::from(!matched), i]);
                    self.run_effect(effect, child, value.clone(), depth + 1)?;
                }
                Ok(())
            }
        }
    }
    fn style(
        &mut self,
        target: &ElementTarget,
        action: &StyleAction,
        owner: &Owner,
        delay: u64,
    ) -> Result<(), ActionError> {
        if let StyleAction::SetBackground(Some(color)) = action {
            self.text(color)?;
            Color::try_from_hex(color)
                .map_err(|_| ActionError::Invalid("invalid background color".into()))?;
        }
        for path in self.targets(target, &owner.source, 0)? {
            let source = self.source(&path)?;
            if let StyleAction::SetFocus(focused) = action {
                self.emit(Effect::Focus {
                    source,
                    focused: *focused,
                })?;
                continue;
            }
            if !self.engine.styles.contains_key(&source.key)
                && self.engine.styles.len() >= self.engine.limits.max_entries
            {
                return Err(ActionError::Limit("styled nodes"));
            }
            self.engine.sequence = self
                .engine
                .sequence
                .checked_add(1)
                .ok_or(ActionError::Limit("style sequence"))?;
            let order = self.engine.sequence;
            let node = selectors::at(self.root, &path).ok_or(ActionError::InvalidSource)?;
            let styled = self
                .engine
                .styles
                .entry(source.key.clone())
                .or_insert_with(|| StyledNode {
                    source,
                    base: Style::read(node),
                    layers: Vec::new(),
                });
            // Reactivation updates the owner's layer instead of growing a hover stack every frame.
            styled.layers.retain(|l| {
                !(l.owner == *owner
                    && std::mem::discriminant(&l.action) == std::mem::discriminant(action))
            });
            if styled.layers.len() >= self.engine.limits.max_entries {
                return Err(ActionError::Limit("style layers"));
            }
            styled.layers.push(Layer {
                owner: owner.clone(),
                action: action.clone(),
                order,
                delay,
                expires: None,
            });
            styled.layers.sort_by_key(|l| l.order);
        }
        Ok(())
    }
}

#[cfg(test)]
mod named_admission_tests {
    use super::*;
    use hyperchad_actions::Action;

    fn action(trigger: ActionTrigger, action: ActionType) -> Action {
        Action {
            trigger,
            effect: ActionEffect {
                action,
                delay_off: None,
                throttle: None,
                unique: None,
            },
        }
    }
    fn custom(name: &str) -> ActionType {
        ActionType::Custom {
            action: name.into(),
        }
    }
    fn named(root: &Container, path: Vec<usize>, generation: u64, now: u64) -> EventContext {
        EventContext::new(
            Source::at(root, path, generation).unwrap(),
            EventKind::Named("load".into()),
            now,
        )
    }
    fn run(root: &mut Container, engine: &mut ActionEngine, prune: bool, now: u64) -> Vec<Effect> {
        let mut effects = Vec::new();
        for path in [vec![], vec![0], vec![1]] {
            let context = named(root, path.clone(), engine.generation, now);
            let node = selectors::at(root, &path).unwrap();
            if !prune
                || path.is_empty()
                || engine.named_maintenance_due(root, now)
                || engine.accepts_named(&context.source.key, node, "load")
            {
                effects.extend(engine.dispatch(root, &context, &FormValues::new()).unwrap());
            }
        }
        effects
    }

    #[test]
    fn named_admission_dynamic_rebinding_order_defaults_and_namespace_parity() {
        use hyperchad_transformer_models::Route;
        let mut root = Container {
            str_id: Some("root".into()),
            children: vec![
                Container {
                    str_id: Some("listener".into()),
                    actions: vec![
                        action(
                            ActionTrigger::Click,
                            ActionType::Event {
                                name: "load".into(),
                                action: Box::new(custom("retained")),
                            },
                        ),
                        action(ActionTrigger::Event("load".into()), custom("static")),
                        action(
                            ActionTrigger::Event("load".into()),
                            ActionType::Event {
                                name: "load".into(),
                                action: Box::new(custom("installed")),
                            },
                        ),
                        action(ActionTrigger::Immediate, custom("must-not-alias")),
                    ],
                    ..Default::default()
                },
                Container {
                    str_id: Some("route".into()),
                    route: Some(Route::Get {
                        route: "/named".into(),
                        trigger: Some("load".into()),
                        target: hyperchad_transformer_models::Selector::default(),
                        strategy: hyperchad_transformer_models::SwapStrategy::default(),
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut engine = ActionEngine::default();
        let click = EventContext::new(Source::at(&root, vec![0], 0).unwrap(), EventKind::Click, 0);
        engine
            .dispatch(&mut root, &click, &FormValues::new())
            .unwrap();
        // Rebind the dynamically registered keyed source before delivery.
        root.children.swap(0, 1);
        engine.reconcile(&root, 1).unwrap();
        let mut expected_root = root.clone();
        let mut expected = engine.clone();
        let effects = run(&mut root, &mut engine, true, 1);
        let unfiltered = run(&mut expected_root, &mut expected, false, 1);
        assert_eq!(format!("{effects:?}"), format!("{unfiltered:?}"));
        assert_eq!(format!("{engine:?}"), format!("{expected:?}"));
        assert_eq!(root, expected_root);
        let names: Vec<_> = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Custom { action, .. } => Some(action.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["static", "retained", "installed"]);
        assert!(matches!(effects.first(), Some(Effect::Request(_))));
        // Registrations outlive their declaring AST while the keyed owner is retained.
        root.children[1].actions.clear();
        engine.reconcile(&root, 2).unwrap();
        assert_eq!(root.children[1].actions.len(), 0);
        assert!(engine.accepts_named("listener", &root.children[1], "load"));
        let context = named(&root, vec![1], 2, 2);
        let dynamic = engine
            .dispatch(&mut root, &context, &FormValues::new())
            .unwrap();
        let names: Vec<_> = dynamic
            .iter()
            .filter_map(|effect| match effect {
                Effect::Custom { action, .. } => Some(action.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["retained", "installed"]);
        assert!(!engine.accepts_named("root", &root, "load"));
        assert!(engine.accepts_named("listener", &root.children[1], "load"));
        assert!(!engine.accepts_named("listener", &root.children[1], "other"));
        root.children.remove(1);
        engine.reconcile(&root, 2).unwrap();
        assert!(
            !engine
                .registrations
                .values()
                .any(|r| r.source.key == "listener")
        );
    }

    #[test]
    fn named_admission_get_post_defaults_without_actions_repeat_exact_name() {
        for route in [
            Route::Get {
                route: "/named".into(),
                trigger: Some("load".into()),
                target: hyperchad_transformer_models::Selector::default(),
                strategy: hyperchad_transformer_models::SwapStrategy::default(),
            },
            Route::Post {
                route: "/named".into(),
                trigger: Some("load".into()),
                target: hyperchad_transformer_models::Selector::default(),
                strategy: hyperchad_transformer_models::SwapStrategy::default(),
            },
        ] {
            let mut root = Container {
                str_id: Some("route".into()),
                route: Some(route),
                ..Default::default()
            };
            let mut engine = ActionEngine::default();
            assert_eq!(root.actions.len(), 0);
            assert!(engine.accepts_named("route", &root, "load"));
            assert!(!engine.accepts_named("route", &root, "other"));
            for now in [0, 1] {
                let context = named(&root, vec![], 0, now);
                assert!(matches!(
                    engine
                        .dispatch(&mut root, &context, &FormValues::new())
                        .unwrap()
                        .as_slice(),
                    [Effect::Request(_)]
                ));
            }
        }
    }

    #[test]
    fn named_admission_rechecks_registration_installed_before_later_key() {
        let mut root = Container {
            str_id: Some("root".into()),
            children: vec![Container {
                str_id: Some("later".into()),
                actions: vec![action(
                    ActionTrigger::Click,
                    ActionType::Event {
                        name: "load".into(),
                        action: Box::new(custom("new-listener")),
                    },
                )],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut engine = ActionEngine::default();
        assert!(!engine.accepts_named("later", &root.children[0], "load"));
        // A previous key's host effects may synchronously install registrations.
        let context =
            EventContext::new(Source::at(&root, vec![0], 0).unwrap(), EventKind::Click, 0);
        engine
            .dispatch(&mut root, &context, &FormValues::new())
            .unwrap();
        assert!(engine.accepts_named("later", &root.children[0], "load"));
        let event = named(&root, vec![0], 0, 1);
        assert!(
            matches!(engine.dispatch(&mut root, &event, &FormValues::new()).unwrap().as_slice(), [Effect::Custom { action, .. }] if action == "new-listener")
        );
    }

    #[test]
    fn readonly_request_preserves_source_and_errors_are_atomic() {
        use hyperchad_transformer_models::Route;
        let mut state = crate::state::ViewState::from_root(Container {
            str_id: Some("root".into()),
            route: Some(Route::Get {
                route: "/page".into(),
                trigger: Some("load".into()),
                target: hyperchad_transformer_models::Selector::default(),
                strategy: hyperchad_transformer_models::SwapStrategy::default(),
            }),
            ..Default::default()
        })
        .unwrap();
        let snapshot = state.source_snapshot();
        let mut engine = ActionEngine::default();
        let event = named(state.root(), vec![], 0, 5);
        let effects = engine
            .dispatch_readonly(state.root(), &event, &FormValues::new())
            .unwrap();
        assert!(matches!(effects.as_slice(), [Effect::Request(_)]));
        assert!(state.matches_source(&snapshot));
        let root = state.root().clone();
        let before = format!("{engine:?}");
        let backwards = named(state.root(), vec![], 0, 4);
        assert!(
            engine
                .dispatch_readonly(state.root(), &backwards, &FormValues::new())
                .is_err()
        );
        assert!(engine.release_readonly(state.root(), &backwards).is_err());
        assert!(engine.advance_readonly(state.root(), 4).is_err());
        assert_eq!(format!("{engine:?}"), before);
        assert_eq!(state.root(), &root);
        assert!(state.matches_source(&snapshot));
    }

    #[test]
    fn readonly_style_publication_release_and_expiry_match_public_wrappers() {
        use hyperchad_transformer_models::ElementTarget;
        let mut hover = action(
            ActionTrigger::Hover,
            ActionType::Style {
                target: ElementTarget::SelfTarget,
                action: StyleAction::SetDisplay(false),
            },
        );
        hover.effect.delay_off = Some(10);
        let root = Container {
            str_id: Some("root".into()),
            actions: vec![hover],
            ..Default::default()
        };
        let mut state = crate::state::ViewState::from_root(root.clone()).unwrap();
        let mut expected_root = root;
        let mut engine = ActionEngine::default();
        let mut expected = engine.clone();
        for (step, now) in [(0, 0), (1, 1), (2, 5), (2, 11), (2, 12)] {
            let snapshot = state.source_snapshot();
            let event = EventContext::new(
                Source::at(state.root(), vec![], 0).unwrap(),
                EventKind::Hover,
                now,
            );
            let before_root = state.root().clone();
            let effects = match step {
                0 => engine.dispatch_readonly(state.root(), &event, &FormValues::new()),
                1 => engine.release_readonly(state.root(), &event),
                _ => engine.advance_readonly(state.root(), now),
            }
            .unwrap();
            assert_eq!(state.root(), &before_root, "evaluation never publishes");
            assert!(state.matches_source(&snapshot));
            let changed = effects
                .iter()
                .any(|effect| matches!(effect, Effect::StylesChanged { .. }));
            if changed {
                engine.publish_styles(state.root_mut());
                assert!(!state.matches_source(&snapshot));
            }
            let expected_effects = match step {
                0 => expected.dispatch(&mut expected_root, &event, &FormValues::new()),
                1 => expected.release(&mut expected_root, &event),
                _ => expected.advance(&mut expected_root, now),
            }
            .unwrap();
            assert_eq!(format!("{effects:?}"), format!("{expected_effects:?}"));
            assert_eq!(format!("{engine:?}"), format!("{expected:?}"));
            assert_eq!(state.root(), &expected_root);
            assert_eq!(state.generation(), 0);
        }
        assert!(
            engine.styles.is_empty(),
            "expired styles pruned after restoration"
        );
    }

    #[test]
    fn readonly_evaluation_failure_does_not_commit_partial_styles_or_engine() {
        use hyperchad_transformer_models::ElementTarget;
        let root = Container {
            str_id: Some("root".into()),
            actions: vec![
                action(
                    ActionTrigger::Click,
                    ActionType::Style {
                        target: ElementTarget::SelfTarget,
                        action: StyleAction::SetDisplay(false),
                    },
                ),
                action(ActionTrigger::Click, custom("overflow")),
            ],
            ..Default::default()
        };
        let mut engine = ActionEngine::default();
        engine.limits.max_entries = 1;
        let before = format!("{engine:?}");
        let event = EventContext::new(Source::at(&root, vec![], 0).unwrap(), EventKind::Click, 1);
        assert!(
            engine
                .dispatch_readonly(&root, &event, &FormValues::new())
                .is_err()
        );
        assert_eq!(format!("{engine:?}"), before);
        assert_eq!(root.hidden, None);
    }

    #[test]
    fn named_admission_retained_style_expiry_and_timer_parity() {
        use hyperchad_transformer_models::ElementTarget;
        let mut hover = action(
            ActionTrigger::Hover,
            ActionType::Style {
                target: ElementTarget::SelfTarget,
                action: StyleAction::SetDisplay(false),
            },
        );
        hover.effect.delay_off = Some(10);
        hover.effect.throttle = Some(5);
        let mut root = Container {
            str_id: Some("root".into()),
            actions: vec![hover],
            children: vec![Container::default(), Container::default()],
            ..Default::default()
        };
        let mut engine = ActionEngine::default();
        let enter = EventContext::new(Source::at(&root, vec![], 0).unwrap(), EventKind::Hover, 0);
        engine
            .dispatch(&mut root, &enter, &FormValues::new())
            .unwrap();
        let leave = EventContext::new(Source::at(&root, vec![], 0).unwrap(), EventKind::Hover, 1);
        engine.release(&mut root, &leave).unwrap();
        assert_eq!(engine.next_deadline(), Some(11));
        assert!(
            !engine.named_maintenance_due(&root, 5),
            "future expiry permits pruning"
        );
        assert!(
            engine.named_maintenance_due(&root, 11),
            "due expiry admits every original key"
        );
        root.hidden = None;
        assert!(
            engine.named_maintenance_due(&root, 5),
            "external style drift needs recommit even before expiry"
        );
        let mut expected_root = root.clone();
        let mut expected = engine.clone();
        for now in [5, 11, 12] {
            let effects = run(&mut root, &mut engine, true, now);
            let unfiltered = run(&mut expected_root, &mut expected, false, now);
            assert_eq!(format!("{effects:?}"), format!("{unfiltered:?}"));
            assert_eq!(format!("{engine:?}"), format!("{expected:?}"));
            assert_eq!(root, expected_root);
            assert!(
                !engine.named_maintenance_due(&root, now),
                "normal transaction clears due maintenance"
            );
        }
        assert_eq!(root.hidden, None);
    }
}
