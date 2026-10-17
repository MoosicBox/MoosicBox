//! Persistent native action adapter. Evaluation lives in `hyperchad_actions::handler`.
//!
//! Document keys/stamps are opaque identities, never authored numeric IDs. The
//! finder assigns private stable handles; numeric selectors expand `numeric_matches`.
//! Action child-class selectors search descendants, unlike document direct-child
//! selectors. Scalar getters take the first document-order match, mutations take all.
//! CSS grammar is deliberately bounded: `#identifier`, `.identifier`, or a legacy
//! bare identifier. Combinators, attributes, pseudo selectors and lists are errors.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{cell::RefCell, collections::BTreeMap};

use hyperchad_actions::{
    ActionTrigger, ElementTarget, Target,
    handler::{
        ActionContext, ActionHandler, BTreeMapStyleManager, ElementFinder, ExecutionReceipt,
        LogLevel,
    },
    logic::Value,
};
use hyperchad_color::Color;
use hyperchad_transformer::{Container, models::Visibility};

use crate::{
    document::{Document, ElementKey},
    events::{EventPhase, EventValue, UiEvent},
    geometry::GeometrySnapshot,
};

/// Host work produced by action evaluation. Consuming an effect is not proof that
/// the host completed it. Unsupported host operations must surface diagnostics.
#[derive(Clone, Debug, Default)]
pub struct ActionEffects {
    /// Style changes require rebuilding the calculated tree from projected roots.
    pub rebuild: bool,
    /// Ordered navigation URLs, for B's route integration.
    pub navigation: Vec<String>,
    /// Ordered application action requests and evaluated parameters.
    pub custom: Vec<(String, Option<Value>)>,
    /// Native focus requests (false means blur).
    pub focus: Vec<(ElementKey, bool)>,
    /// Native input select-all requests.
    pub select_input: Vec<ElementKey>,
    /// Explicit evaluator/host failures.
    pub diagnostics: Vec<String>,
    /// Requested logs with their original severity.
    pub logs: Vec<(LogLevel, String)>,
}

#[derive(Clone, Debug)]
struct Node {
    key: ElementKey,
    container: Container,
    children: Vec<usize>,
    dimensions: Option<(f32, f32)>,
    position: Option<(f32, f32)>,
}

#[derive(Clone, Debug, Default)]
struct Finder {
    nodes: BTreeMap<usize, Node>,
    order: Vec<usize>,
    numeric: BTreeMap<usize, Vec<usize>>,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-'))
}

impl Finder {
    fn selector(&self, selector: &str) -> Result<Vec<usize>, String> {
        let (prefix, value) = if let Some(value) = selector.strip_prefix('#') {
            ('#', value)
        } else if let Some(value) = selector.strip_prefix('.') {
            ('.', value)
        } else {
            ('#', selector)
        };
        if !identifier(value) {
            return Err(format!("unsupported action selector: {selector:?}"));
        }
        Ok(self
            .order
            .iter()
            .copied()
            .filter(|id| {
                let node = &self.nodes[id].container;
                if prefix == '#' {
                    node.str_id.as_deref() == Some(value)
                } else {
                    node.classes.iter().any(|class| class == value)
                }
            })
            .collect())
    }

    fn descendants(&self, source: usize, result: &mut Vec<usize>) {
        if let Some(node) = self.nodes.get(&source) {
            for child in &node.children {
                result.push(*child);
                self.descendants(*child, result);
            }
        }
    }
}

impl ElementFinder for Finder {
    fn find_by_str_id(&self, id: &str) -> Option<usize> {
        self.order
            .iter()
            .copied()
            .find(|key| self.nodes[key].container.str_id.as_deref() == Some(id))
    }
    fn find_by_class(&self, class: &str) -> Option<usize> {
        self.order.iter().copied().find(|key| {
            self.nodes[key]
                .container
                .classes
                .iter()
                .any(|value| value == class)
        })
    }
    fn find_child_by_class(&self, parent: usize, class: &str) -> Option<usize> {
        let mut descendants = Vec::new();
        self.descendants(parent, &mut descendants);
        descendants.into_iter().find(|key| {
            self.nodes[key]
                .container
                .classes
                .iter()
                .any(|value| value == class)
        })
    }
    fn get_last_child(&self, parent: usize) -> Option<usize> {
        self.nodes.get(&parent)?.children.last().copied()
    }
    fn get_data_attr(&self, id: usize, attr: &str) -> Option<String> {
        self.nodes.get(&id)?.container.data.get(attr).cloned()
    }
    fn get_str_id(&self, id: usize) -> Option<String> {
        self.nodes.get(&id)?.container.str_id.clone()
    }
    fn get_dimensions(&self, id: usize) -> Option<(f32, f32)> {
        self.nodes.get(&id)?.dimensions
    }
    fn get_position(&self, id: usize) -> Option<(f32, f32)> {
        self.nodes.get(&id)?.position
    }
    fn get_visibility(&self, id: usize) -> Option<Visibility> {
        self.nodes.get(&id)?.container.visibility
    }
    fn get_display(&self, id: usize) -> Option<bool> {
        Some(!self.nodes.get(&id)?.container.hidden.unwrap_or(false))
    }
    fn find_all(&self, target: &ElementTarget, source: usize) -> Result<Vec<usize>, String> {
        match target {
            ElementTarget::Selector(Target::Literal(value)) => self.selector(value),
            ElementTarget::ById(Target::Literal(value)) => Ok(self
                .order
                .iter()
                .copied()
                .filter(|id| self.nodes[id].container.str_id.as_deref() == Some(value))
                .collect()),
            ElementTarget::Class(Target::Literal(value)) => Ok(self
                .order
                .iter()
                .copied()
                .filter(|id| self.nodes[id].container.classes.contains(value))
                .collect()),
            ElementTarget::ChildClass(Target::Literal(value)) => {
                let mut descendants = Vec::new();
                self.descendants(source, &mut descendants);
                Ok(descendants
                    .into_iter()
                    .filter(|id| self.nodes[id].container.classes.contains(value))
                    .collect())
            }
            ElementTarget::SelfTarget => Ok(self
                .nodes
                .contains_key(&source)
                .then_some(source)
                .into_iter()
                .collect()),
            ElementTarget::LastChild => Ok(self.get_last_child(source).into_iter().collect()),
            ElementTarget::Id(id) => Ok(self.numeric.get(id).cloned().unwrap_or_default()),
            _ => Err("unresolved dynamic action target".into()),
        }
    }
}

type Handler = ActionHandler<
    Finder,
    BTreeMapStyleManager<Option<Visibility>>,
    BTreeMapStyleManager<Option<Color>>,
    BTreeMapStyleManager<bool>,
>;
type BindingKey = (ElementKey, u64, usize);

#[derive(Debug)]
struct Binding {
    owner: u64,
    receipt: ExecutionReceipt,
}

struct Host<'a> {
    finder: &'a Finder,
    effects: &'a RefCell<ActionEffects>,
    name: Option<&'a str>,
    mouse: Option<(f32, f32)>,
    focus: &'a RefCell<Option<ElementKey>>,
    payload: Option<&'a EventValue>,
}

impl ActionContext for Host<'_> {
    fn request_repaint(&self) {
        self.effects.borrow_mut().rebuild = true;
    }
    fn get_mouse_position(&self) -> Option<(f32, f32)> {
        self.mouse
    }
    fn get_mouse_position_relative(&self, id: usize) -> Option<(f32, f32)> {
        let (x, y) = self.mouse?;
        let (left, top) = self.finder.get_position(id)?;
        Some((x - left, y - top))
    }
    fn navigate(&self, url: String) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.effects.borrow_mut().navigation.push(url);
        Ok(())
    }
    fn request_custom_action(
        &self,
        action: String,
        value: Option<Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.effects.borrow_mut().custom.push((action, value));
        Ok(())
    }
    fn log(&self, level: LogLevel, message: &str) {
        let mut effects = self.effects.borrow_mut();
        if level == LogLevel::Error {
            effects.diagnostics.push(message.into());
        }
        effects.logs.push((level, message.into()));
    }
    fn resolve_event_value(&self, fallback: Option<&str>) -> Result<Option<String>, String> {
        self.payload
            .map_or_else(|| Ok(fallback.map(str::to_owned)), event_text)
    }
    fn event_name(&self) -> Option<&str> {
        self.name
    }
    fn is_focused(&self, id: usize) -> Option<bool> {
        Some(*self.focus.borrow() == Some(self.finder.nodes.get(&id)?.key))
    }
    fn set_focus(&self, id: usize, focus: bool) -> Result<(), String> {
        let key = self
            .finder
            .nodes
            .get(&id)
            .ok_or("focus target is no longer live")?
            .key;
        if focus {
            *self.focus.borrow_mut() = Some(key);
        } else if *self.focus.borrow() == Some(key) {
            *self.focus.borrow_mut() = None;
        }
        self.effects.borrow_mut().focus.push((key, focus));
        Ok(())
    }
    fn select_input(&self, id: usize) -> Result<(), String> {
        let key = self
            .finder
            .nodes
            .get(&id)
            .ok_or("input target is no longer live")?
            .key;
        self.effects.borrow_mut().select_input.push(key);
        Ok(())
    }
}

/// UI-thread-owned persistent evaluator. Call `dispatch` in native event order.
/// Drain host effects after dispatch/tick, project styles before layout, refresh
/// geometry afterwards, and feed G's reconcile-generated Leaves even when stale.
pub struct ActionRuntime {
    handler: Handler,
    finder: Finder,
    handles: BTreeMap<ElementKey, usize>,
    next_handle: usize,
    bindings: BTreeMap<BindingKey, Binding>,
    pending: Vec<ExecutionReceipt>,
    next_owner: u64,
    now: u64,
    effects: RefCell<ActionEffects>,
    focus: RefCell<Option<ElementKey>>,
}

impl Default for ActionRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionRuntime {
    /// Construct a runtime. Share one instance across frames of one document.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handler: ActionHandler::new(
                Finder::default(),
                BTreeMapStyleManager::default(),
                BTreeMapStyleManager::default(),
                BTreeMapStyleManager::default(),
            ),
            finder: Finder::default(),
            handles: BTreeMap::new(),
            next_handle: 0,
            bindings: BTreeMap::new(),
            pending: Vec::new(),
            next_owner: 0,
            now: 0,
            effects: RefCell::new(ActionEffects::default()),
            focus: RefCell::new(None),
        }
    }

    fn synchronize(
        &mut self,
        document: &Document,
        geometry: Option<&GeometrySnapshot<ElementKey>>,
    ) -> Result<(), String> {
        let mut finder = Finder::default();
        for key in document.keys() {
            if !self.handles.contains_key(&key) {
                self.next_handle = self
                    .next_handle
                    .checked_add(1)
                    .ok_or("action handle exhausted")?;
                self.handles.insert(key, self.next_handle);
            }
        }
        for key in document.keys() {
            let container = document.get(key).ok_or("document key disappeared")?.clone();
            let id = self.handles[&key];
            let dimensions = geometry
                .and_then(|snapshot| snapshot.node(key))
                .map(|node| (node.border_box.width(), node.border_box.height()));
            let position = geometry
                .and_then(|snapshot| snapshot.node(key))
                .map(|node| (node.border_box.left(), node.border_box.top()));
            finder.numeric.insert(
                container.id,
                document
                    .numeric_matches(container.id)
                    .iter()
                    .map(|key| self.handles[key])
                    .collect(),
            );
            finder.nodes.insert(
                id,
                Node {
                    key,
                    container,
                    children: document
                        .children(key)
                        .iter()
                        .map(|key| self.handles[key])
                        .collect(),
                    dimensions,
                    position,
                },
            );
            finder.order.push(id);
        }
        self.handler.set_finder(finder.clone());
        self.finder = finder;
        Ok(())
    }

    /// Refresh calculated geometry before dispatching actions using size/position.
    /// # Errors
    /// * Identity exhaustion or inconsistent document keys.
    pub fn refresh(
        &mut self,
        document: &Document,
        geometry: &GeometrySnapshot<ElementKey>,
    ) -> Result<(), String> {
        self.synchronize(document, Some(geometry))
    }

    /// Dispatch one selected authored binding. Leaves resolve saved receipts before
    /// checking liveness, so teardown after replacement cannot leak hover overrides.
    /// # Errors
    /// * Regressing clock, stale non-Leave events, missing binding, evaluator failure,
    ///   or exhausted identity counters. Partial effects remain available to drain.
    pub fn dispatch(
        &mut self,
        document: &Document,
        event: &UiEvent,
        now_ms: u64,
    ) -> Result<(), String> {
        self.clock(now_ms)?;
        let key = (
            event.source.key,
            event.source.generation,
            event.action_index,
        );
        if event.phase == EventPhase::Leave {
            if let Some(mut binding) = self.bindings.remove(&key) {
                binding.receipt.begin_undo(now_ms);
                self.pending.push(binding.receipt);
                self.handler.forget_binding(binding.owner);
            }
            self.tick(now_ms)?;
            return Ok(());
        }
        if !document.is_current(event.source) {
            return Err("stale action source".into());
        }
        // Preserve refreshed geometry when the key remains live.
        let previous = self.finder.clone();
        self.synchronize(document, None)?;
        for (id, node) in &mut self.finder.nodes {
            if let Some(old) = previous.nodes.get(id) {
                node.dimensions = old.dimensions;
                node.position = old.position;
            }
        }
        self.handler.set_finder(self.finder.clone());
        let action = document
            .get(event.source.key)
            .and_then(|node| node.actions.get(event.action_index))
            .ok_or("action binding no longer exists")?;
        if action.trigger != event.trigger {
            return Err("action trigger changed".into());
        }
        if !self.bindings.contains_key(&key) {
            self.next_owner = self
                .next_owner
                .checked_add(1)
                .ok_or("binding identity exhausted")?;
            self.bindings.insert(
                key,
                Binding {
                    owner: self.next_owner,
                    receipt: ExecutionReceipt::default(),
                },
            );
        }
        let name = if let ActionTrigger::Event(name) = &event.trigger {
            Some(name.as_str())
        } else {
            None
        };
        let host = Host {
            finder: &self.finder,
            effects: &self.effects,
            focus: &self.focus,
            name,
            payload: Some(&event.value),
            mouse: event
                .pointer
                .map(|pointer| (pointer.position.x, pointer.position.y)),
        };
        let binding = self.bindings.get_mut(&key).ok_or("binding disappeared")?;
        let result = self.handler.execute_binding(
            binding.owner,
            &action.effect,
            self.handles[&event.source.key],
            &host,
            None,
            now_ms,
        );
        binding.receipt.append(result.receipt);
        if result.success {
            Ok(())
        } else {
            Err("action evaluation failed; inspect ActionEffects diagnostics".into())
        }
    }

    fn clock(&mut self, now_ms: u64) -> Result<(), String> {
        if now_ms < self.now {
            return Err("action clock regressed".into());
        }
        self.now = now_ms;
        Ok(())
    }

    /// Process deferred hover undo at deterministic caller time.
    /// # Errors
    /// * Caller time regressed.
    pub fn tick(&mut self, now_ms: u64) -> Result<(), String> {
        self.clock(now_ms)?;
        let host = Host {
            finder: &self.finder,
            effects: &self.effects,
            focus: &self.focus,
            name: None,
            payload: None,
            mouse: None,
        };
        self.pending
            .retain_mut(|receipt| !self.handler.undo_receipt(receipt, now_ms, &host));
        Ok(())
    }

    /// Earliest pending undo wakeup. Active hovers do not schedule undo themselves.
    #[allow(clippy::must_use_candidate)] // Option already carries must_use.
    pub fn next_deadline(&self) -> Option<u64> {
        self.pending
            .iter()
            .filter_map(ExecutionReceipt::next_deadline)
            .min()
    }

    /// Synchronize actual native widget focus before dispatching its events.
    pub fn sync_focus(&mut self, focused: Option<ElementKey>) {
        *self.focus.borrow_mut() = focused;
    }

    /// Drain host work exactly once; the backend must actually service these effects.
    #[must_use]
    pub fn take_effects(&mut self) -> ActionEffects {
        self.effects.take()
    }

    /// Project overrides on a fresh document clone before layout. Traversal uses
    /// key/child order, never numeric IDs (which are allowed to repeat).
    #[must_use]
    pub fn projected_roots(&self, document: &Document) -> Vec<Container> {
        fn project(
            runtime: &ActionRuntime,
            document: &Document,
            key: ElementKey,
        ) -> Option<Container> {
            let mut node = document.get(key)?.clone();
            if let Some(id) = runtime.handles.get(&key) {
                if let Some(value) = runtime.handler.get_visibility_override(*id) {
                    node.visibility = *value;
                }
                if let Some(value) = runtime.handler.get_background_override(*id) {
                    node.background = *value;
                }
                if let Some(value) = runtime.handler.get_display_override(*id) {
                    node.hidden = Some(!value);
                }
            }
            node.children = document
                .children(key)
                .into_iter()
                .filter_map(|key| project(runtime, document, key))
                .collect();
            Some(node)
        }
        document
            .root_keys()
            .into_iter()
            .filter_map(|key| project(self, document, key))
            .collect()
    }
}

/// Event expressions preserve strings verbatim and expose arrays/objects as compact
/// JSON strings, since shared `logic::Value` has no structured variant. Conversion
/// is lazy: invalid numbers/depth fail only expressions that read the payload.
fn event_text(value: &EventValue) -> Result<Option<String>, String> {
    match value {
        EventValue::Null => Ok(None),
        EventValue::Bool(value) => Ok(Some(value.to_string())),
        EventValue::Number(value) if value.is_finite() => Ok(Some(value.to_string())),
        EventValue::String(value) => Ok(Some(value.clone())),
        EventValue::Array(_) | EventValue::Object(_) => event_json(value, 0).map(Some),
        EventValue::Number(_) => Err("non-finite event number".into()),
    }
}

fn json_string(value: &str) -> String {
    let mut encoded = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            ch if ch <= '\u{1f}' => {
                use std::fmt::Write as _;
                // Writing to a String cannot fail.
                write!(encoded, "\\u{:04x}", u32::from(ch)).expect("String formatting");
            }
            ch => encoded.push(ch),
        }
    }
    encoded.push('"');
    encoded
}

fn event_json(value: &EventValue, depth: usize) -> Result<String, String> {
    if depth > 128 {
        return Err("event payload exceeds JSON nesting limit".into());
    }
    match value {
        EventValue::Null => Ok("null".into()),
        EventValue::Bool(value) => Ok(value.to_string()),
        EventValue::Number(value) if value.is_finite() => Ok(value.to_string()),
        EventValue::Number(_) => Err("non-finite event number".into()),
        EventValue::String(value) => Ok(json_string(value)),
        EventValue::Array(values) => {
            let values = values
                .iter()
                .map(|value| event_json(value, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", values.join(",")))
        }
        EventValue::Object(values) => {
            let values = values
                .iter()
                .map(|(key, value)| {
                    event_json(value, depth + 1)
                        .map(|value| format!("{}:{value}", json_string(key)))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{{{}}}", values.join(",")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ViewOrigin;
    use hyperchad_actions::{Action, ActionEffect, ActionType, StyleAction};
    use hyperchad_renderer::View;

    fn fixture(actions: Vec<Action>) -> Document {
        let mut document = Document::new();
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        id: 7,
                        str_id: Some("root".into()),
                        actions,
                        children: vec![Container {
                            id: 7,
                            str_id: Some("a".into()),
                            classes: vec!["item".into()],
                            children: vec![Container {
                                id: 8,
                                str_id: Some("b".into()),
                                classes: vec!["item".into()],
                                ..Container::default()
                            }],
                            ..Container::default()
                        }],
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        document
    }

    fn binding(effect: ActionEffect, trigger: ActionTrigger) -> Action {
        Action { trigger, effect }
    }
    fn event(
        document: &Document,
        index: usize,
        trigger: ActionTrigger,
        phase: EventPhase,
    ) -> UiEvent {
        UiEvent {
            sequence: 1,
            source: document.stamp(document.root_keys()[0]).unwrap(),
            action_index: index,
            trigger,
            phase,
            value: EventValue::Null,
            keyboard: None,
            pointer: None,
        }
    }

    #[test]
    fn two_bindings_same_node_and_stale_leave_keep_exact_ownership() {
        let mut hidden: ActionEffect = ActionType::hide_by_id("a").into();
        hidden.throttle = Some(100);
        let shown: ActionEffect = ActionType::show_by_id("a").into();
        let mut document = fixture(vec![
            binding(hidden, ActionTrigger::Hover),
            binding(shown, ActionTrigger::Hover),
        ]);
        let mut runtime = ActionRuntime::new();
        let first = event(&document, 0, ActionTrigger::Hover, EventPhase::Enter);
        let second = event(&document, 1, ActionTrigger::Hover, EventPhase::Enter);
        runtime.dispatch(&document, &first, 0).unwrap();
        runtime.dispatch(&document, &second, 0).unwrap();
        let mut leave = second;
        leave.phase = EventPhase::Leave;
        runtime.dispatch(&document, &leave, 0).unwrap();
        assert_eq!(
            runtime.projected_roots(&document)[0].children[0].visibility,
            Some(Visibility::Hidden)
        );
        document
            .apply_view(
                View {
                    primary: Some(Container::default()),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        let mut stale = first;
        stale.phase = EventPhase::Leave;
        runtime.dispatch(&document, &stale, 0).unwrap();
        assert!(runtime.pending.is_empty());
    }

    #[test]
    fn child_class_is_descendant_and_mutations_hit_all_numeric_ids() {
        let document = fixture(vec![binding(
            ActionType::Style {
                target: ElementTarget::ChildClass(Target::Literal("item".into())),
                action: StyleAction::SetDisplay(false),
            }
            .into(),
            ActionTrigger::Click,
        )]);
        let mut runtime = ActionRuntime::new();
        runtime
            .dispatch(
                &document,
                &event(&document, 0, ActionTrigger::Click, EventPhase::Fire),
                0,
            )
            .unwrap();
        let projected = runtime.projected_roots(&document);
        assert_eq!(projected[0].children[0].hidden, Some(true));
        assert_eq!(projected[0].children[0].children[0].hidden, Some(true));
        assert_eq!(
            runtime
                .finder
                .find_all(&ElementTarget::Id(7), 0)
                .unwrap()
                .len(),
            2
        );
        let source = runtime.handles[&document.root_keys()[0]];
        assert_eq!(
            runtime.handler.get_element_id(
                &ElementTarget::Selector(Target::Literal(".item".into())),
                source
            ),
            Some(runtime.finder.order[1])
        );
    }

    #[test]
    fn effects_focus_input_navigation_custom_and_event_wrapper_are_not_dropped() {
        let actions = ActionType::Multi(vec![
            ActionType::set_focus_by_id(true, "a"),
            ActionType::select_by_id("a"),
            ActionType::Navigate {
                url: "/next".into(),
            },
            ActionType::Custom {
                action: "save".into(),
            },
        ]);
        let document = fixture(vec![binding(
            ActionType::on_event("save", actions).into(),
            ActionTrigger::Event("save".into()),
        )]);
        let mut runtime = ActionRuntime::new();
        let mut input = event(
            &document,
            0,
            ActionTrigger::Event("save".into()),
            EventPhase::Fire,
        );
        input.value = EventValue::String("value".into());
        runtime.dispatch(&document, &input, 0).unwrap();
        let effects = runtime.take_effects();
        assert_eq!(effects.focus.len(), 1);
        assert_eq!(effects.select_input.len(), 1);
        assert_eq!(effects.navigation, ["/next"]);
        assert_eq!(effects.custom.len(), 1);
        assert!(effects.diagnostics.is_empty());
        assert!(runtime.take_effects().navigation.is_empty());
    }

    #[test]
    fn unsupported_selectors_and_regressing_clock_propagate_errors() {
        let document = fixture(vec![binding(
            ActionType::Style {
                target: ElementTarget::Selector(Target::Literal("div > .item".into())),
                action: StyleAction::SetDisplay(false),
            }
            .into(),
            ActionTrigger::Click,
        )]);
        let mut runtime = ActionRuntime::new();
        let input = event(&document, 0, ActionTrigger::Click, EventPhase::Fire);
        assert!(runtime.dispatch(&document, &input, 10).is_err());
        assert!(!runtime.take_effects().diagnostics.is_empty());
        assert!(runtime.tick(9).is_err());
    }

    #[test]
    fn delay_off_starts_at_leave_and_deadline_is_deterministic() {
        let mut effect: ActionEffect = ActionType::hide_by_id("a").into();
        effect.delay_off = Some(20);
        let document = fixture(vec![binding(effect, ActionTrigger::Hover)]);
        let mut runtime = ActionRuntime::new();
        let mut input = event(&document, 0, ActionTrigger::Hover, EventPhase::Enter);
        runtime.dispatch(&document, &input, 0).unwrap();
        assert_eq!(runtime.next_deadline(), None);
        input.phase = EventPhase::Leave;
        runtime.dispatch(&document, &input, 100).unwrap();
        assert_eq!(runtime.next_deadline(), Some(120));
        runtime.tick(119).unwrap();
        assert_eq!(
            runtime.projected_roots(&document)[0].children[0].visibility,
            Some(Visibility::Hidden)
        );
        runtime.tick(120).unwrap();
        assert_eq!(runtime.next_deadline(), None);
        assert_eq!(
            runtime.projected_roots(&document)[0].children[0].visibility,
            None
        );
    }
    #[test]
    fn multi_select_change_plain_actions_ignore_structured_payload() {
        let document = fixture(vec![binding(
            ActionType::Multi(vec![
                ActionType::show_by_id("a"),
                ActionType::Log {
                    message: "selection changed".into(),
                    level: hyperchad_actions::LogLevel::Info,
                },
            ])
            .into(),
            ActionTrigger::Change,
        )]);
        let mut runtime = ActionRuntime::new();
        let mut input = event(&document, 0, ActionTrigger::Change, EventPhase::Fire);
        input.value = EventValue::Array(vec![
            EventValue::String("a".into()),
            EventValue::String("b".into()),
        ]);
        runtime.dispatch(&document, &input, 0).unwrap();
        assert_eq!(
            runtime.projected_roots(&document)[0].children[0].visibility,
            Some(Visibility::Visible)
        );
        let effects = runtime.take_effects();
        assert_eq!(effects.logs, [(LogLevel::Info, "selection changed".into())]);
        assert!(effects.diagnostics.is_empty());
    }

    #[test]
    fn http_success_object_log_executes_without_payload_conversion() {
        let document = fixture(vec![binding(
            ActionType::Log {
                message: "request succeeded".into(),
                level: hyperchad_actions::LogLevel::Info,
            }
            .into(),
            ActionTrigger::HttpRequestSuccess,
        )]);
        let mut runtime = ActionRuntime::new();
        let mut input = event(
            &document,
            0,
            ActionTrigger::HttpRequestSuccess,
            EventPhase::Fire,
        );
        input.value = EventValue::Object(BTreeMap::from([(
            "status".into(),
            EventValue::Number(200.0),
        )]));
        runtime.dispatch(&document, &input, 0).unwrap();
        assert_eq!(
            runtime.take_effects().logs,
            [(LogLevel::Info, "request succeeded".into())]
        );
        // Invalid scalar leaves are irrelevant to an action that never reads them.
        input.value = EventValue::Object(BTreeMap::from([(
            "invalid".into(),
            EventValue::Number(f64::NAN),
        )]));
        runtime.dispatch(&document, &input, 1).unwrap();
    }

    #[test]
    fn structured_event_expression_is_compact_json_and_failures_are_lazy() {
        let effect: ActionEffect = ActionType::Parameterized {
            action: Box::new(ActionType::Custom {
                action: "selection".into(),
            }),
            value: Value::Calc(hyperchad_actions::logic::CalcValue::EventValue),
        }
        .into();
        let document = fixture(vec![binding(effect, ActionTrigger::Change)]);
        let mut runtime = ActionRuntime::new();
        let mut input = event(&document, 0, ActionTrigger::Change, EventPhase::Fire);
        input.value = EventValue::Array(vec![
            EventValue::String("a".into()),
            EventValue::String("b".into()),
        ]);
        runtime.dispatch(&document, &input, 0).unwrap();
        assert_eq!(
            runtime.take_effects().custom,
            [(
                "selection".into(),
                Some(Value::String(r#"["a","b"]"#.into()))
            )]
        );
        input.value = EventValue::Object(BTreeMap::from([
            ("status".into(), EventValue::Number(200.0)),
            ("text".into(), EventValue::String("line\n\"quoted\"".into())),
        ]));
        runtime.dispatch(&document, &input, 1).unwrap();
        assert_eq!(
            runtime.take_effects().custom,
            [(
                "selection".into(),
                Some(Value::String(
                    r#"{"status":200,"text":"line\n\"quoted\""}"#.into()
                ))
            )]
        );
        input.value = EventValue::Array(vec![EventValue::Number(f64::NAN)]);
        assert!(runtime.dispatch(&document, &input, 2).is_err());
        let effects = runtime.take_effects();
        assert!(effects.custom.is_empty());
        assert!(
            effects
                .diagnostics
                .iter()
                .any(|error| error.contains("non-finite"))
        );
    }

    #[test]
    fn structured_dsl_event_variable_uses_same_serialization() {
        let document = fixture(vec![binding(
            ActionType::Multi(vec![
                ActionType::Let {
                    name: "payload".into(),
                    value: hyperchad_actions::dsl::Expression::Variable("event_value".into()),
                },
                ActionType::hide_by_id(Target::Ref("payload".into())),
            ])
            .into(),
            ActionTrigger::Change,
        )]);
        let mut runtime = ActionRuntime::new();
        let mut input = event(&document, 0, ActionTrigger::Change, EventPhase::Fire);
        input.value = EventValue::Array(vec![EventValue::String("a".into())]);
        // The serialized string is a literal ID here, not a selector or executable code.
        let mut document = document;
        document
            .apply_view(
                View {
                    primary: Some(Container {
                        str_id: Some(r#"["a"]"#.into()),
                        actions: document.roots()[0].actions.clone(),
                        ..Container::default()
                    }),
                    ..View::default()
                },
                ViewOrigin::Navigation,
            )
            .unwrap();
        input.source = document.stamp(document.root_keys()[0]).unwrap();
        runtime.dispatch(&document, &input, 0).unwrap();
        assert_eq!(
            runtime.projected_roots(&document)[0].visibility,
            Some(Visibility::Hidden)
        );
    }
}
