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
use hyperchad_transformer_models::{ElementTarget, LinkTarget, Selector, SwapStrategy, Visibility};

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
    styles: BTreeMap<String, StyledNode>,
    variables: BTreeMap<(String, String), eval::Datum>,
    registrations: BTreeMap<Owner, Registration>,
}

impl Default for ActionEngine {
    fn default() -> Self {
        Self::new(0, ActionLimits::default())
    }
}
impl ActionEngine {
    /// Create a pure engine for one state lane generation.
    #[must_use]
    pub const fn new(generation: u64, limits: ActionLimits) -> Self {
        Self {
            generation,
            limits,
            clock: 0,
            sequence: 0,
            timing: BTreeMap::new(),
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
        next.commit_styles(root);
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
        next.commit_styles(root);
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
        let source = Source::at(root, Vec::new(), self.generation)?;
        let event = EventContext::new(source, EventKind::Immediate, now_ms);
        self.validate(root, &event)?;
        let mut next = self.clone();
        let values = FormValues::new();
        let mut work = Work::new(root, &event, &values, &mut next)?;
        work.expire();
        let effects = work.finish()?;
        next.commit_styles(root);
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
            .filter_map(|(mut owner, timing)| rebind(&mut owner.source).then_some((owner, timing)))
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

    fn commit_styles(&mut self, root: &mut Container) {
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
