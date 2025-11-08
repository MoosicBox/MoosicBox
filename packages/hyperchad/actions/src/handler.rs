//! Action handler implementation for processing and executing actions
//!
//! This module provides traits and implementations for handling action execution in UI frameworks.
//! It includes style management, element finding, action contexts, and a complete action handler
//! that coordinates all action processing.
//!
//! # Core Traits
//!
//! * [`crate::handler::StyleManager`] - Manages style overrides with trigger-based precedence
//! * [`crate::handler::ElementFinder`] - Finds elements in the UI tree by ID, class, or other selectors
//! * [`crate::handler::ActionContainer`] - Trait for container types that integrate with the action system
//! * [`crate::handler::ActionContext`] - Provides context-dependent operations (repaint, navigation, logging)
//!
//! # Main Types
//!
//! * [`crate::handler::ActionHandler`] - Main coordinator for action processing
//! * [`crate::handler::BTreeMapStyleManager`] - Default style manager implementation
//! * [`crate::handler::ActionTimingManager`] - Manages action throttling and delay-off timing
//!
//! # Integration Example
//!
//! ```rust,ignore
//! use hyperchad_actions::handler::{ActionHandler, BTreeMapStyleManager, utils};
//!
//! // Create style managers for different properties
//! let visibility_mgr = BTreeMapStyleManager::default();
//! let background_mgr = BTreeMapStyleManager::default();
//! let display_mgr = BTreeMapStyleManager::default();
//!
//! // Create action handler with a custom element finder
//! let handler = ActionHandler::new(
//!     my_element_finder,
//!     visibility_mgr,
//!     background_mgr,
//!     display_mgr,
//! );
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, fmt::Debug, time::Instant};

use crate::{
    ActionEffect, ActionTrigger, ActionType, ElementTarget, StyleAction, Target, logic::Value,
};
use hyperchad_color::Color;
use hyperchad_transformer_models::Visibility;

/// Trait for managing style overrides with different triggers
pub trait StyleManager<T> {
    /// Add a style override for an element
    fn add_override(&mut self, element_id: usize, trigger: StyleTrigger, value: T);

    /// Remove overrides for an element with a specific trigger
    fn remove_overrides(&mut self, element_id: usize, trigger: StyleTrigger);

    /// Get the current effective value for an element (last override wins)
    fn get_current_value(&self, element_id: usize) -> Option<&T>;

    /// Check if element has any overrides
    fn has_overrides(&self, element_id: usize) -> bool;

    /// Clear all overrides for an element
    fn clear_element(&mut self, element_id: usize);
}

/// Trait for finding elements in the UI tree
pub trait ElementFinder {
    /// Find element by string ID
    fn find_by_str_id(&self, str_id: &str) -> Option<usize>;

    /// Find element by class name
    fn find_by_class(&self, class: &str) -> Option<usize>;

    /// Find element by class name within a parent element
    fn find_child_by_class(&self, parent_id: usize, class: &str) -> Option<usize>;

    /// Get last child of an element
    fn get_last_child(&self, parent_id: usize) -> Option<usize>;

    /// Get element data attribute value
    fn get_data_attr(&self, element_id: usize, attr: &str) -> Option<String>;

    /// Get element string ID
    fn get_str_id(&self, element_id: usize) -> Option<String>;

    /// Get element dimensions
    fn get_dimensions(&self, element_id: usize) -> Option<(f32, f32)>;

    /// Get element position
    fn get_position(&self, element_id: usize) -> Option<(f32, f32)>;
}

/// Trait for container types that can be used with the action system
///
/// ## Implementation Example
///
/// To integrate with an existing Container type from `hyperchad_transformer`:
///
/// ```ignore
/// use hyperchad_actions::handler::ActionContainer;
/// use hyperchad_transformer::Container;
///
/// impl ActionContainer for Container {
///     fn find_element_by_id(&self, id: usize) -> Option<&Self> {
///         self.find_element_by_id(id)
///     }
///
///     fn find_element_by_str_id(&self, str_id: &str) -> Option<&Self> {
///         self.find_element_by_str_id(str_id)
///     }
///
///     fn find_element_by_class(&self, class: &str) -> Option<&Self> {
///         self.find_element_by_class(class)
///     }
///
///     fn get_id(&self) -> usize {
///         self.id
///     }
///
///     fn get_str_id(&self) -> Option<&str> {
///         self.str_id.as_deref()
///     }
///
///     fn get_children(&self) -> &[Self] {
///         &self.children
///     }
///
///     fn get_data_attrs(&self) -> Option<&std::collections::BTreeMap<String, String>> {
///         Some(&self.data)
///     }
///
///     fn get_calculated_dimensions(&self) -> Option<(f32, f32)> {
///         Some((self.calculated_width?, self.calculated_height?))
///     }
///
///     fn get_calculated_position(&self) -> Option<(f32, f32)> {
///         Some((
///             self.calculated_x.unwrap_or(0.0),
///             self.calculated_y.unwrap_or(0.0),
///         ))
///     }
/// }
/// ```
pub trait ActionContainer {
    /// Find element by ID
    fn find_element_by_id(&self, id: usize) -> Option<&Self>;

    /// Find element by string ID
    fn find_element_by_str_id(&self, str_id: &str) -> Option<&Self>;

    /// Find element by class name
    fn find_element_by_class(&self, class: &str) -> Option<&Self>;

    /// Get element ID
    fn get_id(&self) -> usize;

    /// Get string ID
    fn get_str_id(&self) -> Option<&str>;

    /// Get children
    fn get_children(&self) -> &[Self]
    where
        Self: Sized;

    /// Get data attributes
    fn get_data_attrs(&self) -> Option<&std::collections::BTreeMap<String, String>>;

    /// Get calculated dimensions if available
    fn get_calculated_dimensions(&self) -> Option<(f32, f32)>;

    /// Get calculated position if available
    fn get_calculated_position(&self) -> Option<(f32, f32)>;
}

/// Trait for context-dependent operations
pub trait ActionContext {
    /// Request a UI repaint/redraw
    fn request_repaint(&self);

    /// Get current mouse position (global coordinates)
    fn get_mouse_position(&self) -> Option<(f32, f32)>;

    /// Get mouse position relative to an element
    fn get_mouse_position_relative(&self, element_id: usize) -> Option<(f32, f32)>;

    /// Send navigation request
    ///
    /// # Errors
    ///
    /// * If navigation fails
    fn navigate(&self, url: String) -> Result<(), Box<dyn std::error::Error + Send>>;

    /// Send custom action request
    ///
    /// # Errors
    ///
    /// * If custom action fails
    fn request_custom_action(
        &self,
        action: String,
        value: Option<Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send>>;

    /// Log a message
    fn log(&self, level: LogLevel, message: &str);
}

/// Trait for action event handling
pub trait ActionEventHandler {
    /// Handle an action event
    fn handle_event(&self, event_name: &str, event_value: Option<&str>);
}

/// Style trigger types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleTrigger {
    /// Trigger from UI event (click, hover, etc.)
    UiEvent,
    /// Trigger from custom event
    CustomEvent,
}

/// Style override with trigger information
#[derive(Debug, Clone)]
pub struct StyleOverride<T> {
    /// Trigger type that caused this override
    pub trigger: StyleTrigger,
    /// Override value
    pub value: T,
}

/// Log level enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Error log level
    Error,
    /// Warning log level
    Warn,
    /// Info log level
    Info,
    /// Debug log level
    Debug,
    /// Trace log level
    Trace,
}

/// Default style manager implementation using `BTreeMap`
#[derive(Debug, Default)]
pub struct BTreeMapStyleManager<T> {
    overrides: BTreeMap<usize, Vec<StyleOverride<T>>>,
}

impl<T> StyleManager<T> for BTreeMapStyleManager<T> {
    fn add_override(&mut self, element_id: usize, trigger: StyleTrigger, value: T) {
        let style_override = StyleOverride { trigger, value };
        match self.overrides.entry(element_id) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().push(style_override);
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(vec![style_override]);
            }
        }
    }

    fn remove_overrides(&mut self, element_id: usize, trigger: StyleTrigger) {
        if let Some(overrides) = self.overrides.get_mut(&element_id) {
            overrides.retain(|x| x.trigger != trigger);
            if overrides.is_empty() {
                self.overrides.remove(&element_id);
            }
        }
    }

    fn get_current_value(&self, element_id: usize) -> Option<&T> {
        self.overrides.get(&element_id)?.last().map(|x| &x.value)
    }

    fn has_overrides(&self, element_id: usize) -> bool {
        self.overrides.contains_key(&element_id)
    }

    fn clear_element(&mut self, element_id: usize) {
        self.overrides.remove(&element_id);
    }
}

/// Action delay/throttle manager
#[derive(Debug, Default)]
pub struct ActionTimingManager {
    delay_off: BTreeMap<usize, (Instant, u64)>,
    throttle: BTreeMap<usize, (Instant, u64)>,
}

impl ActionTimingManager {
    /// Check if action should be throttled
    pub fn should_throttle(&mut self, element_id: usize, throttle_ms: u64) -> bool {
        if let Some((instant, throttle)) = self.throttle.get(&element_id) {
            let ms = switchy_time::instant_now()
                .duration_since(*instant)
                .as_millis();
            if ms < u128::from(*throttle) {
                return true;
            }
        }

        self.throttle
            .insert(element_id, (switchy_time::instant_now(), throttle_ms));
        false
    }

    /// Start delay off timer
    pub fn start_delay_off(&mut self, element_id: usize, delay_ms: u64) {
        self.delay_off
            .insert(element_id, (switchy_time::instant_now(), delay_ms));
    }

    /// Check if delay off has expired
    #[must_use]
    pub fn is_delay_off_expired(&self, element_id: usize) -> bool {
        if let Some((instant, delay)) = self.delay_off.get(&element_id) {
            let ms = switchy_time::instant_now()
                .duration_since(*instant)
                .as_millis();
            ms >= u128::from(*delay)
        } else {
            true
        }
    }

    /// Clear throttle for element
    pub fn clear_throttle(&mut self, element_id: usize) {
        self.throttle.remove(&element_id);
    }

    /// Clear delay off for element
    pub fn clear_delay_off(&mut self, element_id: usize) {
        self.delay_off.remove(&element_id);
    }
}

/// Main action handler that coordinates all action processing
pub struct ActionHandler<F, V, B, D>
where
    F: ElementFinder,
    V: StyleManager<Option<Visibility>>,
    B: StyleManager<Option<Color>>,
    D: StyleManager<bool>,
{
    finder: F,
    visibility_manager: V,
    background_manager: B,
    display_manager: D,
    timing_manager: ActionTimingManager,
}

impl<F, V, B, D> ActionHandler<F, V, B, D>
where
    F: ElementFinder,
    V: StyleManager<Option<Visibility>>,
    B: StyleManager<Option<Color>>,
    D: StyleManager<bool>,
{
    /// Create new action handler
    pub fn new(
        finder: F,
        visibility_manager: V,
        background_manager: B,
        display_manager: D,
    ) -> Self {
        Self {
            finder,
            visibility_manager,
            background_manager,
            display_manager,
            timing_manager: ActionTimingManager::default(),
        }
    }

    /// Get element ID from target
    pub fn get_element_id(&self, target: &ElementTarget, self_id: usize) -> Option<usize> {
        match target {
            ElementTarget::StrId(str_id) => {
                let Target::Literal(str_id) = str_id else {
                    return None;
                };
                self.finder.find_by_str_id(str_id)
            }
            ElementTarget::Class(class) => {
                let Target::Literal(class) = class else {
                    return None;
                };
                self.finder.find_by_class(class)
            }
            ElementTarget::ChildClass(class) => {
                let Target::Literal(class) = class else {
                    return None;
                };
                self.finder.find_child_by_class(self_id, class)
            }
            ElementTarget::Id(id) => Some(*id),
            ElementTarget::SelfTarget => Some(self_id),
            ElementTarget::LastChild => self.finder.get_last_child(self_id),
        }
    }

    /// Calculate dynamic value
    pub fn calc_value(
        &self,
        value: &Value,
        self_id: usize,
        context: &impl ActionContext,
        event_value: Option<&str>,
    ) -> Option<Value> {
        use crate::logic::{CalcValue, Value as LogicValue};

        let calc_func = |calc_value: &CalcValue| match calc_value {
            CalcValue::Visibility { target } => {
                let element_id = self.get_element_id(target, self_id)?;
                let visibility = self
                    .visibility_manager
                    .get_current_value(element_id)
                    .copied()
                    .flatten()
                    .unwrap_or_default();
                Some(LogicValue::Visibility(visibility))
            }
            CalcValue::Display { target } => {
                let element_id = self.get_element_id(target, self_id)?;
                let display = self
                    .display_manager
                    .get_current_value(element_id)
                    .copied()
                    .unwrap_or_default();
                Some(LogicValue::Display(display))
            }
            CalcValue::Id { target } => {
                let element_id = self.get_element_id(target, self_id)?;
                self.finder.get_str_id(element_id).map(LogicValue::String)
            }
            CalcValue::DataAttrValue { attr, target } => {
                let element_id = self.get_element_id(target, self_id)?;
                self.finder
                    .get_data_attr(element_id, attr)
                    .map(LogicValue::String)
            }
            CalcValue::Key { key } => Some(LogicValue::String(key.to_string())),
            CalcValue::EventValue => event_value.map(ToString::to_string).map(LogicValue::String),
            CalcValue::WidthPx { target } => {
                let element_id = self.get_element_id(target, self_id)?;
                self.finder
                    .get_dimensions(element_id)
                    .map(|(w, _)| LogicValue::Real(w))
            }
            CalcValue::HeightPx { target } => {
                let element_id = self.get_element_id(target, self_id)?;
                self.finder
                    .get_dimensions(element_id)
                    .map(|(_, h)| LogicValue::Real(h))
            }
            CalcValue::PositionX { target } => {
                let element_id = self.get_element_id(target, self_id)?;
                self.finder
                    .get_position(element_id)
                    .map(|(x, _)| LogicValue::Real(x))
            }
            CalcValue::PositionY { target } => {
                let element_id = self.get_element_id(target, self_id)?;
                self.finder
                    .get_position(element_id)
                    .map(|(_, y)| LogicValue::Real(y))
            }
            CalcValue::MouseX { target } => {
                let pos = context.get_mouse_position()?.0;
                if let Some(target) = target {
                    let element_id = self.get_element_id(target, self_id)?;
                    let element_pos = self.finder.get_position(element_id)?.0;
                    Some(LogicValue::Real(pos - element_pos))
                } else {
                    Some(LogicValue::Real(pos))
                }
            }
            CalcValue::MouseY { target } => {
                let pos = context.get_mouse_position()?.1;
                if let Some(target) = target {
                    let element_id = self.get_element_id(target, self_id)?;
                    let element_pos = self.finder.get_position(element_id)?.1;
                    Some(LogicValue::Real(pos - element_pos))
                } else {
                    Some(LogicValue::Real(pos))
                }
            }
        };

        match value {
            LogicValue::Calc(x) => calc_func(x),
            LogicValue::Arithmetic(x) => x.as_f32(Some(&calc_func)).map(LogicValue::Real),
            LogicValue::Real(..)
            | LogicValue::Visibility(..)
            | LogicValue::Display(..)
            | LogicValue::String(..)
            | LogicValue::Key(..)
            | LogicValue::LayoutDirection(..) => Some(value.clone()),
        }
    }

    /// Handle a style action
    pub fn handle_style_action(
        &mut self,
        action: &StyleAction,
        target: &ElementTarget,
        trigger: StyleTrigger,
        self_id: usize,
    ) -> bool {
        let Some(element_id) = self.get_element_id(target, self_id) else {
            return false;
        };

        match action {
            StyleAction::SetVisibility(visibility) => {
                self.visibility_manager
                    .add_override(element_id, trigger, Some(*visibility));
                true
            }
            StyleAction::SetDisplay(display) => {
                self.display_manager
                    .add_override(element_id, trigger, *display);
                true
            }
            StyleAction::SetFocus(_focus) => {
                // TODO: Implement focus management
                true
            }
            StyleAction::SetBackground(background) => {
                let color = if let Some(background) = background {
                    match Color::try_from_hex(background) {
                        Ok(color) => Some(color),
                        Err(_) => return false,
                    }
                } else {
                    None
                };
                self.background_manager
                    .add_override(element_id, trigger, color);
                true
            }
        }
    }

    /// Unhandle a style action (cleanup)
    pub fn unhandle_style_action(
        &mut self,
        action: &StyleAction,
        target: &ElementTarget,
        trigger: StyleTrigger,
        self_id: usize,
    ) {
        let Some(element_id) = self.get_element_id(target, self_id) else {
            return;
        };

        match action {
            StyleAction::SetVisibility(..) => {
                self.visibility_manager
                    .remove_overrides(element_id, trigger);
            }
            StyleAction::SetDisplay(..) => {
                self.display_manager.remove_overrides(element_id, trigger);
            }
            StyleAction::SetFocus(..) => {
                // TODO: Implement focus management
            }
            StyleAction::SetBackground(..) => {
                self.background_manager
                    .remove_overrides(element_id, trigger);
            }
        }
    }

    /// Handle an action
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub fn handle_action(
        &mut self,
        action: &ActionType,
        effect: Option<&ActionEffect>,
        trigger: StyleTrigger,
        self_id: usize,
        context: &impl ActionContext,
        event_value: Option<&str>,
        value: Option<&Value>,
    ) -> bool {
        // Check throttling
        if let Some(ActionEffect {
            throttle: Some(throttle),
            ..
        }) = effect
            && self.timing_manager.should_throttle(self_id, *throttle)
        {
            context.request_repaint();
            return true;
        }

        match action {
            ActionType::Style { target, action } => {
                // Handle delay off
                if let Some(ActionEffect {
                    delay_off: Some(delay),
                    ..
                }) = effect
                {
                    self.timing_manager.start_delay_off(self_id, *delay);
                }

                self.handle_style_action(action, target, trigger, self_id)
            }
            ActionType::Navigate { url } => {
                if let Err(e) = context.navigate(url.clone()) {
                    context.log(LogLevel::Error, &format!("Failed to navigate: {e:?}"));
                    false
                } else {
                    true
                }
            }
            ActionType::Log { message, level } => {
                let log_level = match level {
                    crate::LogLevel::Error => LogLevel::Error,
                    crate::LogLevel::Warn => LogLevel::Warn,
                    crate::LogLevel::Info => LogLevel::Info,
                    crate::LogLevel::Debug => LogLevel::Debug,
                    crate::LogLevel::Trace => LogLevel::Trace,
                };
                context.log(log_level, message);
                true
            }
            ActionType::Custom { action } => {
                if let Err(e) = context.request_custom_action(action.clone(), value.cloned()) {
                    context.log(
                        LogLevel::Error,
                        &format!("Failed to request custom action: {e:?}"),
                    );
                    false
                } else {
                    true
                }
            }
            ActionType::Logic(eval) => {
                let success = match &eval.condition {
                    crate::logic::Condition::Eq(a, b) => {
                        let a = self.calc_value(a, self_id, context, event_value);
                        let b = self.calc_value(b, self_id, context, event_value);
                        a == b
                    }
                    crate::logic::Condition::Bool(b) => *b,
                };

                let actions = if success {
                    &eval.actions
                } else {
                    &eval.else_actions
                };

                for action in actions {
                    if !self.handle_action(
                        &action.action,
                        Some(action),
                        trigger,
                        self_id,
                        context,
                        event_value,
                        value,
                    ) {
                        return false;
                    }
                }
                true
            }
            ActionType::Multi(actions) => {
                for action in actions {
                    if !self.handle_action(
                        action,
                        effect,
                        trigger,
                        self_id,
                        context,
                        event_value,
                        value,
                    ) {
                        return false;
                    }
                }
                true
            }
            ActionType::MultiEffect(effects) => {
                for effect in effects {
                    if !self.handle_action(
                        &effect.action,
                        Some(effect),
                        trigger,
                        self_id,
                        context,
                        event_value,
                        value,
                    ) {
                        return false;
                    }
                }
                true
            }
            ActionType::Parameterized { action, value } => {
                let calculated_value = self.calc_value(value, self_id, context, event_value);
                self.handle_action(
                    action,
                    effect,
                    trigger,
                    self_id,
                    context,
                    event_value,
                    calculated_value.as_ref(),
                )
            }
            ActionType::Event { .. } // Event actions are handled by the event system
            | ActionType::NoOp | ActionType::Input(..) | ActionType::Let { .. } => {
                // TODO: Implement input and variable handling
                true
            }
        }
    }

    /// Unhandle an action (cleanup)
    pub fn unhandle_action(
        &mut self,
        action: &ActionType,
        trigger: StyleTrigger,
        self_id: usize,
        context: &impl ActionContext,
    ) {
        self.timing_manager.clear_throttle(self_id);

        match action {
            ActionType::Style { target, action } => {
                // Check delay off
                if !self.timing_manager.is_delay_off_expired(self_id) {
                    context.request_repaint();
                    return;
                }

                self.unhandle_style_action(action, target, trigger, self_id);
            }
            ActionType::Multi(actions) => {
                for action in actions {
                    self.unhandle_action(action, trigger, self_id, context);
                }
            }
            ActionType::MultiEffect(effects) => {
                for effect in effects {
                    self.unhandle_action(&effect.action, trigger, self_id, context);
                }
            }
            ActionType::Parameterized { action, .. } => {
                self.unhandle_action(action, trigger, self_id, context);
            }
            // Most actions don't need cleanup
            _ => {}
        }
    }

    /// Get current visibility override for element
    pub fn get_visibility_override(&self, element_id: usize) -> Option<&Option<Visibility>> {
        self.visibility_manager.get_current_value(element_id)
    }

    /// Get current background override for element
    pub fn get_background_override(&self, element_id: usize) -> Option<&Option<Color>> {
        self.background_manager.get_current_value(element_id)
    }

    /// Get current display override for element
    pub fn get_display_override(&self, element_id: usize) -> Option<&bool> {
        self.display_manager.get_current_value(element_id)
    }

    /// Clear all overrides for an element
    pub fn clear_element_overrides(&mut self, element_id: usize) {
        self.visibility_manager.clear_element(element_id);
        self.background_manager.clear_element(element_id);
        self.display_manager.clear_element(element_id);
        self.timing_manager.clear_delay_off(element_id);
        self.timing_manager.clear_throttle(element_id);
    }
}

/// Helper function to determine if action should be triggered based on trigger type
#[must_use]
pub fn should_trigger_action(trigger: &ActionTrigger, event_type: &str) -> bool {
    match trigger {
        ActionTrigger::Click => event_type == "click",
        ActionTrigger::ClickOutside => event_type == "click_outside",
        ActionTrigger::MouseDown => event_type == "mouse_down",
        ActionTrigger::KeyDown => event_type == "key_down",
        ActionTrigger::Hover => event_type == "hover",
        ActionTrigger::Change => event_type == "change",
        ActionTrigger::Resize => event_type == "resize",
        ActionTrigger::Event(name) => event_type == name,
        ActionTrigger::Immediate => event_type == "immediate",
        ActionTrigger::HttpBeforeRequest => event_type == "http_before_request",
        ActionTrigger::HttpAfterRequest => event_type == "http_after_request",
        ActionTrigger::HttpRequestSuccess => event_type == "http_request_success",
        ActionTrigger::HttpRequestError => event_type == "http_request_error",
        ActionTrigger::HttpRequestAbort => event_type == "http_request_abort",
        ActionTrigger::HttpRequestTimeout => event_type == "http_request_timeout",
    }
}

/// Common utilities and helper implementations
pub mod utils {
    use super::{
        ActionContainer, ActionContext, ActionHandler, ActionTrigger, BTreeMap,
        BTreeMapStyleManager, Color, ElementFinder, StyleManager, StyleTrigger, Visibility,
        should_trigger_action,
    };

    /// Default action handler type using BTreeMap-based style managers
    pub type DefaultActionHandler<F> = ActionHandler<
        F,
        BTreeMapStyleManager<Option<Visibility>>,
        BTreeMapStyleManager<Option<Color>>,
        BTreeMapStyleManager<bool>,
    >;

    /// Create a default action handler with BTreeMap-based style managers
    pub fn create_default_handler<F: ElementFinder>(finder: F) -> DefaultActionHandler<F> {
        ActionHandler::new(
            finder,
            BTreeMapStyleManager::default(),
            BTreeMapStyleManager::default(),
            BTreeMapStyleManager::default(),
        )
    }

    /// Generic container-based element finder implementation
    pub struct ContainerElementFinder<'a, C: ActionContainer> {
        container: &'a C,
        positions: &'a BTreeMap<usize, (f32, f32)>,
        dimensions: &'a BTreeMap<usize, (f32, f32)>,
    }

    impl<'a, C: ActionContainer> ContainerElementFinder<'a, C> {
        /// Creates a new container-based element finder
        pub const fn new(
            container: &'a C,
            positions: &'a BTreeMap<usize, (f32, f32)>,
            dimensions: &'a BTreeMap<usize, (f32, f32)>,
        ) -> Self {
            Self {
                container,
                positions,
                dimensions,
            }
        }
    }

    impl<C: ActionContainer> ElementFinder for ContainerElementFinder<'_, C> {
        fn find_by_str_id(&self, str_id: &str) -> Option<usize> {
            self.container
                .find_element_by_str_id(str_id)
                .map(ActionContainer::get_id)
        }

        fn find_by_class(&self, class: &str) -> Option<usize> {
            self.container
                .find_element_by_class(class)
                .map(ActionContainer::get_id)
        }

        fn find_child_by_class(&self, parent_id: usize, class: &str) -> Option<usize> {
            self.container
                .find_element_by_id(parent_id)?
                .find_element_by_class(class)
                .map(ActionContainer::get_id)
        }

        fn get_last_child(&self, parent_id: usize) -> Option<usize> {
            self.container
                .find_element_by_id(parent_id)?
                .get_children()
                .last()
                .map(ActionContainer::get_id)
        }

        fn get_data_attr(&self, element_id: usize, attr: &str) -> Option<String> {
            self.container
                .find_element_by_id(element_id)?
                .get_data_attrs()?
                .get(attr)
                .cloned()
        }

        fn get_str_id(&self, element_id: usize) -> Option<String> {
            self.container
                .find_element_by_id(element_id)?
                .get_str_id()
                .map(ToString::to_string)
        }

        fn get_dimensions(&self, element_id: usize) -> Option<(f32, f32)> {
            self.dimensions.get(&element_id).copied().or_else(|| {
                self.container
                    .find_element_by_id(element_id)?
                    .get_calculated_dimensions()
            })
        }

        fn get_position(&self, element_id: usize) -> Option<(f32, f32)> {
            self.positions.get(&element_id).copied().or_else(|| {
                self.container
                    .find_element_by_id(element_id)?
                    .get_calculated_position()
            })
        }
    }

    /// Event matching helper
    #[must_use]
    pub fn matches_trigger(
        trigger: &ActionTrigger,
        event_type: &str,
        event_name: Option<&str>,
    ) -> bool {
        match trigger {
            ActionTrigger::Event(name) => event_name.is_some_and(|e| e == name),
            _ => should_trigger_action(trigger, event_type),
        }
    }

    /// Batch process actions for an element
    #[allow(clippy::too_many_arguments)]
    pub fn process_element_actions<F, V, B, D, C>(
        handler: &mut ActionHandler<F, V, B, D>,
        actions: &[crate::Action],
        element_id: usize,
        event_type: &str,
        event_name: Option<&str>,
        event_value: Option<&str>,
        context: &C,
        trigger_type: StyleTrigger,
    ) -> bool
    where
        F: ElementFinder,
        V: StyleManager<Option<Visibility>>,
        B: StyleManager<Option<Color>>,
        D: StyleManager<bool>,
        C: ActionContext,
    {
        let mut success = true;

        for action in actions {
            if matches_trigger(&action.trigger, event_type, event_name)
                && !handler.handle_action(
                    &action.effect.action,
                    Some(&action.effect),
                    trigger_type,
                    element_id,
                    context,
                    event_value,
                    None,
                )
            {
                success = false;
                break;
            }
        }

        success
    }
}

/// Example integration showing how to use the shared action handler in a renderer
#[cfg(all(test, feature = "logic"))]
pub mod example_integration {
    use super::*;
    use flume::Sender;
    use std::sync::{Arc, RwLock};

    /// Example context implementation for a renderer
    pub struct ExampleActionContext {
        /// Channel for sending navigation requests
        navigation_sender: Sender<String>,
        /// Channel for sending custom actions
        action_sender: Sender<(String, Option<Value>)>,
        /// Function to request repaint (could be egui context, etc.)
        repaint_fn: Arc<dyn Fn() + Send + Sync>,
        /// Mouse position provider
        mouse_position: Arc<RwLock<Option<(f32, f32)>>>,
    }

    impl ExampleActionContext {
        pub fn new(
            navigation_sender: Sender<String>,
            action_sender: Sender<(String, Option<Value>)>,
            repaint_fn: Arc<dyn Fn() + Send + Sync>,
        ) -> Self {
            Self {
                navigation_sender,
                action_sender,
                repaint_fn,
                mouse_position: Arc::new(RwLock::new(None)),
            }
        }

        /// Update mouse position (called by renderer during mouse events)
        ///
        /// # Panics
        ///
        /// * If the RwLock is poisoned (another thread panicked while holding the lock)
        pub fn update_mouse_position(&self, x: f32, y: f32) {
            *self.mouse_position.write().unwrap() = Some((x, y));
        }
    }

    impl ActionContext for ExampleActionContext {
        fn request_repaint(&self) {
            (self.repaint_fn)();
        }

        fn get_mouse_position(&self) -> Option<(f32, f32)> {
            *self.mouse_position.read().unwrap()
        }

        fn get_mouse_position_relative(&self, _element_id: usize) -> Option<(f32, f32)> {
            // Implementation would need element position information
            self.get_mouse_position()
        }

        fn navigate(&self, url: String) -> Result<(), Box<dyn std::error::Error + Send>> {
            self.navigation_sender
                .send(url)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
        }

        fn request_custom_action(
            &self,
            action: String,
            value: Option<Value>,
        ) -> Result<(), Box<dyn std::error::Error + Send>> {
            self.action_sender
                .send((action, value))
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
        }

        fn log(&self, level: LogLevel, message: &str) {
            match level {
                LogLevel::Error => log::error!("{message}"),
                LogLevel::Warn => log::warn!("{message}"),
                LogLevel::Info => log::info!("{message}"),
                LogLevel::Debug => log::debug!("{message}"),
                LogLevel::Trace => log::trace!("{message}"),
            }
        }
    }

    /// Example of how a renderer would integrate the action handler
    /// This is a simplified example - in practice you'd need to handle lifetimes properly
    pub struct ExampleRenderer<C: ActionContainer> {
        /// Element positions for position queries
        positions: BTreeMap<usize, (f32, f32)>,
        /// Element dimensions for dimension queries
        dimensions: BTreeMap<usize, (f32, f32)>,
        /// Current container being rendered
        container: Option<C>,
    }

    impl<C: ActionContainer> Default for ExampleRenderer<C> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<C: ActionContainer> ExampleRenderer<C> {
        #[must_use]
        pub const fn new() -> Self {
            Self {
                container: None,
                positions: BTreeMap::new(),
                dimensions: BTreeMap::new(),
            }
        }

        /// Set the current container
        pub fn set_container(&mut self, container: C) {
            self.container = Some(container);
        }

        /// Create an action handler for the current container
        pub fn create_action_handler(
            &self,
        ) -> Option<utils::DefaultActionHandler<utils::ContainerElementFinder<'_, C>>> {
            let container = self.container.as_ref()?;
            let finder =
                utils::ContainerElementFinder::new(container, &self.positions, &self.dimensions);
            Some(utils::create_default_handler(finder))
        }

        /// Example: Handle a UI event (click, hover, etc.)
        /// In practice, you'd store the action handler or create it as needed
        pub fn handle_ui_event_example(&self, element_id: usize, event_type: &str) -> bool {
            // This is just an example - in practice you'd handle actions based on your UI framework
            // You could store an action handler, or create one when processing events
            log::debug!("UI event: {event_type} on element {element_id}");
            true
        }

        /// Update element position (called during layout)
        pub fn update_element_position(&mut self, element_id: usize, x: f32, y: f32) {
            self.positions.insert(element_id, (x, y));
        }

        /// Update element dimensions (called during layout)
        pub fn update_element_dimensions(&mut self, element_id: usize, width: f32, height: f32) {
            self.dimensions.insert(element_id, (width, height));
        }

        /// Clear position and dimension state for element
        pub fn clear_element(&mut self, element_id: usize) {
            self.positions.remove(&element_id);
            self.dimensions.remove(&element_id);
        }
    }
}

// Re-export commonly used types
pub use utils::{ContainerElementFinder, DefaultActionHandler};
