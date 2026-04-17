use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use hyperchad_actions::{
    Action,
    handler::{
        self, ActionContext, ElementFinder, LogLevel, StyleTrigger,
        utils::{DefaultActionHandler, create_default_handler, process_element_actions},
    },
    logic::Value,
};
use hyperchad_renderer::transformer::{Container, models::Selector};

use crate::{dom::select, renderer::RendererSnapshot};

/// Captured side effects emitted while executing actions.
#[derive(Debug, Clone, Default)]
pub struct ActionEffects {
    pub navigation: Vec<String>,
    pub custom_actions: Vec<(String, Option<Value>)>,
    pub logs: Vec<(LogLevel, String)>,
    pub repaint_requests: usize,
}

#[derive(Debug, Default)]
struct ActionContextState {
    effects: ActionEffects,
    mouse_position: Option<(f32, f32)>,
}

#[derive(Clone)]
struct HarnessActionContext {
    state: Arc<RwLock<ActionContextState>>,
}

impl HarnessActionContext {
    fn take_effects(&self) -> ActionEffects {
        let mut state = self.state.write().unwrap();
        std::mem::take(&mut state.effects)
    }
}

impl ActionContext for HarnessActionContext {
    fn request_repaint(&self) {
        self.state.write().unwrap().effects.repaint_requests += 1;
    }

    fn get_mouse_position(&self) -> Option<(f32, f32)> {
        self.state.read().unwrap().mouse_position
    }

    fn get_mouse_position_relative(&self, _element_id: usize) -> Option<(f32, f32)> {
        self.get_mouse_position()
    }

    fn navigate(&self, url: String) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.state.write().unwrap().effects.navigation.push(url);
        Ok(())
    }

    fn request_custom_action(
        &self,
        action: String,
        value: Option<Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        self.state
            .write()
            .unwrap()
            .effects
            .custom_actions
            .push((action, value));
        Ok(())
    }

    fn log(&self, level: LogLevel, message: &str) {
        self.state
            .write()
            .unwrap()
            .effects
            .logs
            .push((level, message.to_string()));
    }
}

#[derive(Clone)]
struct SharedDomElementFinder {
    snapshot: Arc<RwLock<RendererSnapshot>>,
    positions: Arc<RwLock<BTreeMap<usize, (f32, f32)>>>,
    dimensions: Arc<RwLock<BTreeMap<usize, (f32, f32)>>>,
}

impl SharedDomElementFinder {
    fn with_root<T>(&self, func: impl FnOnce(&Container) -> T) -> Option<T> {
        let snapshot = self.snapshot.read().unwrap();
        let root = snapshot.dom.root()?;
        Some(func(root))
    }
}

impl ElementFinder for SharedDomElementFinder {
    fn find_by_str_id(&self, str_id: &str) -> Option<usize> {
        self.with_root(|root| root.find_element_by_str_id(str_id).map(|x| x.id))?
    }

    fn find_by_class(&self, class: &str) -> Option<usize> {
        self.with_root(|root| root.find_element_by_class(class).map(|x| x.id))?
    }

    fn find_child_by_class(&self, parent_id: usize, class: &str) -> Option<usize> {
        self.with_root(|root| {
            root.find_element_by_id(parent_id)?
                .children
                .iter()
                .find(|x| x.classes.iter().any(|c| c == class))
                .map(|x| x.id)
        })?
    }

    fn get_last_child(&self, parent_id: usize) -> Option<usize> {
        self.with_root(|root| {
            root.find_element_by_id(parent_id)?
                .children
                .last()
                .map(|x| x.id)
        })?
    }

    fn get_data_attr(&self, element_id: usize, attr: &str) -> Option<String> {
        self.with_root(|root| root.find_element_by_id(element_id)?.data.get(attr).cloned())?
    }

    fn get_str_id(&self, element_id: usize) -> Option<String> {
        self.with_root(|root| root.find_element_by_id(element_id)?.str_id.clone())?
    }

    fn get_dimensions(&self, element_id: usize) -> Option<(f32, f32)> {
        self.dimensions.read().unwrap().get(&element_id).copied()
    }

    fn get_position(&self, element_id: usize) -> Option<(f32, f32)> {
        self.positions.read().unwrap().get(&element_id).copied()
    }
}

/// Stateful action runtime used by the harness.
pub struct ActionRuntime {
    snapshot: Arc<RwLock<RendererSnapshot>>,
    handler: DefaultActionHandler<SharedDomElementFinder>,
    context: HarnessActionContext,
}

impl ActionRuntime {
    /// Creates a runtime bound to shared renderer state.
    #[must_use]
    pub fn new(snapshot: Arc<RwLock<RendererSnapshot>>) -> Self {
        let finder = SharedDomElementFinder {
            snapshot: Arc::clone(&snapshot),
            positions: Arc::new(RwLock::new(BTreeMap::new())),
            dimensions: Arc::new(RwLock::new(BTreeMap::new())),
        };
        let context = HarnessActionContext {
            state: Arc::new(RwLock::new(ActionContextState::default())),
        };
        Self {
            snapshot,
            handler: create_default_handler(finder),
            context,
        }
    }

    /// Runs immediate actions for every element currently in the DOM.
    #[must_use]
    pub fn run_immediate_actions(&mut self) -> ActionEffects {
        let ids = self.collect_ids();
        for id in ids {
            self.dispatch_actions_for_id(
                id,
                handler::StyleTrigger::UiEvent,
                "immediate",
                None,
                None,
                None,
            );
        }
        self.apply_style_overrides();
        self.context.take_effects()
    }

    /// Dispatches an event against the first element matching `selector`.
    #[must_use]
    pub fn dispatch_event(
        &mut self,
        selector: &Selector,
        event_type: &str,
        event_name: Option<&str>,
        event_value: Option<&str>,
        value: Option<&Value>,
    ) -> ActionEffects {
        let Some(target_id) = self.target_id_for_selector(selector) else {
            return self.context.take_effects();
        };

        for id in self.path_to_root(target_id) {
            let trigger_type = if event_name.is_some() || event_type.starts_with("http_") {
                StyleTrigger::CustomEvent
            } else {
                StyleTrigger::UiEvent
            };
            self.dispatch_actions_for_id(
                id,
                trigger_type,
                event_type,
                event_name,
                event_value,
                value,
            );
        }

        self.apply_style_overrides();
        self.context.take_effects()
    }

    fn dispatch_actions_for_id(
        &mut self,
        element_id: usize,
        trigger_type: StyleTrigger,
        event_type: &str,
        event_name: Option<&str>,
        event_value: Option<&str>,
        value: Option<&Value>,
    ) {
        let actions = {
            let snapshot = self.snapshot.read().unwrap();
            snapshot
                .dom
                .root()
                .and_then(|root| {
                    root.find_element_by_id(element_id)
                        .map(|x| x.actions.clone())
                })
                .unwrap_or_default()
        };

        if let Some(value) = value {
            for action in &actions {
                if handler::utils::matches_trigger(&action.trigger, event_type, event_name) {
                    let _ = self.handler.handle_action(
                        &action.effect.action,
                        Some(&action.effect),
                        trigger_type,
                        element_id,
                        &self.context,
                        event_value,
                        Some(value),
                    );
                }
            }
        } else {
            let _success = process_element_actions(
                &mut self.handler,
                &actions,
                element_id,
                event_type,
                event_name,
                event_value,
                &self.context,
                trigger_type,
            );
        }
    }

    fn collect_ids(&self) -> Vec<usize> {
        self.snapshot.read().unwrap().dom.collect_ids()
    }

    fn path_to_root(&self, target_id: usize) -> Vec<usize> {
        self.snapshot
            .read()
            .unwrap()
            .dom
            .path_to_root(target_id)
            .unwrap_or_default()
    }

    fn target_id_for_selector(&self, selector: &Selector) -> Option<usize> {
        let snapshot = self.snapshot.read().unwrap();
        let root = snapshot.dom.root()?;
        match selector {
            Selector::SelfTarget => Some(root.id),
            Selector::Id(id) => root.find_element_by_str_id(id).map(|x| x.id),
            Selector::Class(class) => root.find_element_by_class(class).map(|x| x.id),
            Selector::ChildClass(class) => root
                .children
                .iter()
                .find(|x| x.classes.iter().any(|c| c == class))
                .map(|x| x.id),
        }
    }

    fn apply_style_overrides(&mut self) {
        let ids = self.collect_ids();
        let mut visibility = BTreeMap::new();
        let mut background = BTreeMap::new();
        let mut display = BTreeMap::new();

        for id in ids {
            if let Some(value) = self.handler.get_visibility_override(id) {
                visibility.insert(id, value.clone());
            }
            if let Some(value) = self.handler.get_background_override(id) {
                background.insert(id, value.clone());
            }
            if let Some(value) = self.handler.get_display_override(id) {
                display.insert(id, *value);
            }
        }

        let mut snapshot = self.snapshot.write().unwrap();
        let Some(root) = snapshot.dom.root_mut() else {
            return;
        };

        for (id, value) in visibility {
            if let Some(container) = root.find_element_by_id_mut(id) {
                container.visibility = value;
            }
        }
        for (id, value) in background {
            if let Some(container) = root.find_element_by_id_mut(id) {
                container.background = value;
            }
        }
        for (id, value) in display {
            if let Some(container) = root.find_element_by_id_mut(id) {
                container.hidden = Some(!value);
            }
        }
    }
}

/// Converts a selector string into a selector enum.
pub fn parse_selector_str(selector: &str) -> Result<Selector, crate::harness::HarnessError> {
    select::parse_selector(selector).map_err(|_e| crate::harness::HarnessError::InvalidSelector {
        selector: selector.to_string(),
    })
}

/// Finds actions attached to a selector.
#[must_use]
pub fn actions_for_selector(snapshot: &RendererSnapshot, selector: &Selector) -> Vec<Action> {
    let Some(root) = snapshot.dom.root() else {
        return vec![];
    };
    let Some(id) = select::find_first_id(root, selector) else {
        return vec![];
    };

    root.find_element_by_id(id)
        .map_or_else(Vec::new, |x| x.actions.clone())
}
