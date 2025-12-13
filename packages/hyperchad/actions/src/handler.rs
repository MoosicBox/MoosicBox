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
    /// Checks if an action should be throttled based on timing
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

    /// Starts a delay-off timer for an element
    pub fn start_delay_off(&mut self, element_id: usize, delay_ms: u64) {
        self.delay_off
            .insert(element_id, (switchy_time::instant_now(), delay_ms));
    }

    /// Checks if the delay-off timer has expired for an element
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

    /// Clears the throttle timer for an element
    pub fn clear_throttle(&mut self, element_id: usize) {
        self.throttle.remove(&element_id);
    }

    /// Clears the delay-off timer for an element
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
    /// Creates a new action handler
    #[must_use]
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

    /// Gets the element ID from a target selector
    #[must_use]
    pub fn get_element_id(&self, target: &ElementTarget, self_id: usize) -> Option<usize> {
        match target {
            ElementTarget::ById(id) | ElementTarget::Selector(id) => {
                let Target::Literal(id) = id else {
                    return None;
                };
                self.finder.find_by_str_id(id)
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

    /// Calculates a dynamic value by evaluating computed values
    #[must_use]
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

    /// Handles a style action and returns success status
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

    /// Cleans up a style action (reverses its effects)
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

    /// Handles an action and returns success status
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

    /// Cleans up an action (reverses its effects)
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

    /// Gets the current visibility override for an element
    #[must_use]
    pub fn get_visibility_override(&self, element_id: usize) -> Option<&Option<Visibility>> {
        self.visibility_manager.get_current_value(element_id)
    }

    /// Gets the current background override for an element
    #[must_use]
    pub fn get_background_override(&self, element_id: usize) -> Option<&Option<Color>> {
        self.background_manager.get_current_value(element_id)
    }

    /// Gets the current display override for an element
    #[must_use]
    pub fn get_display_override(&self, element_id: usize) -> Option<&bool> {
        self.display_manager.get_current_value(element_id)
    }

    /// Clears all overrides for an element
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

    /// Creates a default action handler with BTreeMap-based style managers
    #[must_use]
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
        /// * If the `RwLock` is poisoned (another thread panicked while holding the lock)
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

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================
    // BTreeMapStyleManager tests
    // ============================================

    #[test_log::test]
    fn test_btree_map_style_manager_add_and_get_override() {
        let mut manager: BTreeMapStyleManager<i32> = BTreeMapStyleManager::default();

        manager.add_override(1, StyleTrigger::UiEvent, 100);

        assert!(manager.has_overrides(1));
        assert_eq!(manager.get_current_value(1), Some(&100));
    }

    #[test_log::test]
    fn test_btree_map_style_manager_multiple_overrides_returns_last() {
        let mut manager: BTreeMapStyleManager<i32> = BTreeMapStyleManager::default();

        manager.add_override(1, StyleTrigger::UiEvent, 100);
        manager.add_override(1, StyleTrigger::CustomEvent, 200);
        manager.add_override(1, StyleTrigger::UiEvent, 300);

        // Last override wins
        assert_eq!(manager.get_current_value(1), Some(&300));
    }

    #[test_log::test]
    fn test_btree_map_style_manager_remove_overrides_by_trigger() {
        let mut manager: BTreeMapStyleManager<i32> = BTreeMapStyleManager::default();

        manager.add_override(1, StyleTrigger::UiEvent, 100);
        manager.add_override(1, StyleTrigger::CustomEvent, 200);
        manager.add_override(1, StyleTrigger::UiEvent, 300);

        // Remove UiEvent overrides (100 and 300)
        manager.remove_overrides(1, StyleTrigger::UiEvent);

        // Should now return the CustomEvent override
        assert_eq!(manager.get_current_value(1), Some(&200));
        assert!(manager.has_overrides(1));
    }

    #[test_log::test]
    fn test_btree_map_style_manager_remove_all_overrides_clears_element() {
        let mut manager: BTreeMapStyleManager<i32> = BTreeMapStyleManager::default();

        manager.add_override(1, StyleTrigger::UiEvent, 100);
        manager.add_override(1, StyleTrigger::UiEvent, 200);

        // Remove all UiEvent overrides
        manager.remove_overrides(1, StyleTrigger::UiEvent);

        // Element should have no overrides
        assert!(!manager.has_overrides(1));
        assert_eq!(manager.get_current_value(1), None);
    }

    #[test_log::test]
    fn test_btree_map_style_manager_remove_nonexistent_trigger_no_effect() {
        let mut manager: BTreeMapStyleManager<i32> = BTreeMapStyleManager::default();

        manager.add_override(1, StyleTrigger::UiEvent, 100);

        // Try to remove CustomEvent overrides (none exist)
        manager.remove_overrides(1, StyleTrigger::CustomEvent);

        // UiEvent override should still be there
        assert_eq!(manager.get_current_value(1), Some(&100));
    }

    #[test_log::test]
    fn test_btree_map_style_manager_clear_element() {
        let mut manager: BTreeMapStyleManager<i32> = BTreeMapStyleManager::default();

        manager.add_override(1, StyleTrigger::UiEvent, 100);
        manager.add_override(1, StyleTrigger::CustomEvent, 200);

        manager.clear_element(1);

        assert!(!manager.has_overrides(1));
        assert_eq!(manager.get_current_value(1), None);
    }

    #[test_log::test]
    fn test_btree_map_style_manager_independent_elements() {
        let mut manager: BTreeMapStyleManager<i32> = BTreeMapStyleManager::default();

        manager.add_override(1, StyleTrigger::UiEvent, 100);
        manager.add_override(2, StyleTrigger::UiEvent, 200);
        manager.add_override(3, StyleTrigger::CustomEvent, 300);

        assert_eq!(manager.get_current_value(1), Some(&100));
        assert_eq!(manager.get_current_value(2), Some(&200));
        assert_eq!(manager.get_current_value(3), Some(&300));
        assert_eq!(manager.get_current_value(4), None);

        // Clear one element shouldn't affect others
        manager.clear_element(2);
        assert_eq!(manager.get_current_value(1), Some(&100));
        assert!(!manager.has_overrides(2));
        assert_eq!(manager.get_current_value(3), Some(&300));
    }

    // ============================================
    // should_trigger_action tests
    // ============================================

    #[test_log::test]
    fn test_should_trigger_action_click() {
        assert!(should_trigger_action(&ActionTrigger::Click, "click"));
        assert!(!should_trigger_action(&ActionTrigger::Click, "hover"));
    }

    #[test_log::test]
    fn test_should_trigger_action_click_outside() {
        assert!(should_trigger_action(
            &ActionTrigger::ClickOutside,
            "click_outside"
        ));
        assert!(!should_trigger_action(
            &ActionTrigger::ClickOutside,
            "click"
        ));
    }

    #[test_log::test]
    fn test_should_trigger_action_hover() {
        assert!(should_trigger_action(&ActionTrigger::Hover, "hover"));
        assert!(!should_trigger_action(&ActionTrigger::Hover, "click"));
    }

    #[test_log::test]
    fn test_should_trigger_action_change() {
        assert!(should_trigger_action(&ActionTrigger::Change, "change"));
        assert!(!should_trigger_action(&ActionTrigger::Change, "hover"));
    }

    #[test_log::test]
    fn test_should_trigger_action_resize() {
        assert!(should_trigger_action(&ActionTrigger::Resize, "resize"));
        assert!(!should_trigger_action(&ActionTrigger::Resize, "change"));
    }

    #[test_log::test]
    fn test_should_trigger_action_immediate() {
        assert!(should_trigger_action(
            &ActionTrigger::Immediate,
            "immediate"
        ));
        assert!(!should_trigger_action(&ActionTrigger::Immediate, "click"));
    }

    #[test_log::test]
    fn test_should_trigger_action_custom_event() {
        assert!(should_trigger_action(
            &ActionTrigger::Event("my-custom-event".to_string()),
            "my-custom-event"
        ));
        assert!(!should_trigger_action(
            &ActionTrigger::Event("my-custom-event".to_string()),
            "other-event"
        ));
    }

    #[test_log::test]
    fn test_should_trigger_action_http_triggers() {
        assert!(should_trigger_action(
            &ActionTrigger::HttpBeforeRequest,
            "http_before_request"
        ));
        assert!(should_trigger_action(
            &ActionTrigger::HttpAfterRequest,
            "http_after_request"
        ));
        assert!(should_trigger_action(
            &ActionTrigger::HttpRequestSuccess,
            "http_request_success"
        ));
        assert!(should_trigger_action(
            &ActionTrigger::HttpRequestError,
            "http_request_error"
        ));
        assert!(should_trigger_action(
            &ActionTrigger::HttpRequestAbort,
            "http_request_abort"
        ));
        assert!(should_trigger_action(
            &ActionTrigger::HttpRequestTimeout,
            "http_request_timeout"
        ));

        // Check that they don't match wrong events
        assert!(!should_trigger_action(
            &ActionTrigger::HttpRequestSuccess,
            "http_request_error"
        ));
    }

    #[test_log::test]
    fn test_should_trigger_action_mouse_down() {
        assert!(should_trigger_action(
            &ActionTrigger::MouseDown,
            "mouse_down"
        ));
        assert!(!should_trigger_action(&ActionTrigger::MouseDown, "click"));
    }

    #[test_log::test]
    fn test_should_trigger_action_key_down() {
        assert!(should_trigger_action(&ActionTrigger::KeyDown, "key_down"));
        assert!(!should_trigger_action(
            &ActionTrigger::KeyDown,
            "mouse_down"
        ));
    }

    // ============================================
    // utils::matches_trigger tests
    // ============================================

    #[test_log::test]
    fn test_matches_trigger_with_custom_event() {
        use utils::matches_trigger;

        // Custom event trigger should match when event name matches
        assert!(matches_trigger(
            &ActionTrigger::Event("my-event".to_string()),
            "event",
            Some("my-event")
        ));

        // Custom event trigger should not match when event name differs
        assert!(!matches_trigger(
            &ActionTrigger::Event("my-event".to_string()),
            "event",
            Some("other-event")
        ));

        // Custom event trigger should not match when no event name provided
        assert!(!matches_trigger(
            &ActionTrigger::Event("my-event".to_string()),
            "event",
            None
        ));
    }

    #[test_log::test]
    fn test_matches_trigger_fallback_to_should_trigger() {
        use utils::matches_trigger;

        // Non-event triggers should fall back to should_trigger_action
        assert!(matches_trigger(&ActionTrigger::Click, "click", None));
        assert!(!matches_trigger(&ActionTrigger::Click, "hover", None));
        assert!(matches_trigger(
            &ActionTrigger::Hover,
            "hover",
            Some("ignored")
        ));
    }

    // ============================================
    // ActionTimingManager tests
    // ============================================

    #[test_log::test]
    fn test_action_timing_manager_is_delay_off_expired_when_no_timer() {
        let manager = ActionTimingManager::default();

        // When no delay_off timer exists, should return true (expired)
        assert!(manager.is_delay_off_expired(1));
        assert!(manager.is_delay_off_expired(999));
    }

    #[test_log::test]
    fn test_action_timing_manager_start_delay_off() {
        let mut manager = ActionTimingManager::default();

        // Start a delay_off timer
        manager.start_delay_off(1, 1000);

        // Timer just started, should not be expired yet
        assert!(!manager.is_delay_off_expired(1));
        // Other elements should still be "expired" (no timer)
        assert!(manager.is_delay_off_expired(2));
    }

    #[test_log::test]
    fn test_action_timing_manager_clear_delay_off() {
        let mut manager = ActionTimingManager::default();

        // Start a delay_off timer
        manager.start_delay_off(1, 10000);
        assert!(!manager.is_delay_off_expired(1));

        // Clear the timer
        manager.clear_delay_off(1);

        // Now should be "expired" (no timer)
        assert!(manager.is_delay_off_expired(1));
    }

    #[test_log::test]
    fn test_action_timing_manager_clear_throttle() {
        let mut manager = ActionTimingManager::default();

        // First call should not throttle
        assert!(!manager.should_throttle(1, 1000));

        // Clear the throttle
        manager.clear_throttle(1);

        // After clearing, should not throttle again (timer was removed)
        assert!(!manager.should_throttle(1, 1000));
    }

    #[test_log::test]
    fn test_action_timing_manager_should_throttle_first_call() {
        let mut manager = ActionTimingManager::default();

        // First call should not be throttled - it sets up the throttle
        let throttled = manager.should_throttle(1, 1000);
        assert!(!throttled);
    }

    #[test_log::test]
    fn test_action_timing_manager_should_throttle_immediate_second_call() {
        let mut manager = ActionTimingManager::default();

        // First call sets up throttle
        assert!(!manager.should_throttle(1, 10000));

        // Immediate second call should be throttled (within throttle period)
        let throttled = manager.should_throttle(1, 10000);
        assert!(throttled);
    }

    #[test_log::test]
    fn test_action_timing_manager_independent_elements() {
        let mut manager = ActionTimingManager::default();

        // Set up throttle for element 1
        assert!(!manager.should_throttle(1, 10000));

        // Element 2 should not be affected
        assert!(!manager.should_throttle(2, 10000));

        // Element 1 should still be throttled
        assert!(manager.should_throttle(1, 10000));
    }

    #[test_log::test]
    fn test_action_timing_manager_delay_off_independent_of_throttle() {
        let mut manager = ActionTimingManager::default();

        // Set up both timers for element 1
        manager.start_delay_off(1, 10000);
        assert!(!manager.should_throttle(1, 10000));

        // Clear only throttle
        manager.clear_throttle(1);

        // Delay off should still be active
        assert!(!manager.is_delay_off_expired(1));
        // Throttle should be cleared
        assert!(!manager.should_throttle(1, 10000));
    }

    // ============================================
    // ActionHandler tests with mock implementations
    // ============================================

    /// Mock element finder for testing `ActionHandler`
    #[derive(Default)]
    struct MockElementFinder {
        str_id_map: BTreeMap<String, usize>,
        class_map: BTreeMap<String, usize>,
        child_class_map: BTreeMap<(usize, String), usize>,
        last_child_map: BTreeMap<usize, usize>,
        data_attrs: BTreeMap<(usize, String), String>,
        str_ids: BTreeMap<usize, String>,
        dimensions: BTreeMap<usize, (f32, f32)>,
        positions: BTreeMap<usize, (f32, f32)>,
    }

    impl ElementFinder for MockElementFinder {
        fn find_by_str_id(&self, str_id: &str) -> Option<usize> {
            self.str_id_map.get(str_id).copied()
        }

        fn find_by_class(&self, class: &str) -> Option<usize> {
            self.class_map.get(class).copied()
        }

        fn find_child_by_class(&self, parent_id: usize, class: &str) -> Option<usize> {
            self.child_class_map
                .get(&(parent_id, class.to_string()))
                .copied()
        }

        fn get_last_child(&self, parent_id: usize) -> Option<usize> {
            self.last_child_map.get(&parent_id).copied()
        }

        fn get_data_attr(&self, element_id: usize, attr: &str) -> Option<String> {
            self.data_attrs
                .get(&(element_id, attr.to_string()))
                .cloned()
        }

        fn get_str_id(&self, element_id: usize) -> Option<String> {
            self.str_ids.get(&element_id).cloned()
        }

        fn get_dimensions(&self, element_id: usize) -> Option<(f32, f32)> {
            self.dimensions.get(&element_id).copied()
        }

        fn get_position(&self, element_id: usize) -> Option<(f32, f32)> {
            self.positions.get(&element_id).copied()
        }
    }

    /// Mock action context for testing `ActionHandler`
    struct MockActionContext {
        repaint_called: std::cell::Cell<bool>,
        mouse_position: Option<(f32, f32)>,
        logs: std::cell::RefCell<Vec<(LogLevel, String)>>,
    }

    impl Default for MockActionContext {
        fn default() -> Self {
            Self {
                repaint_called: std::cell::Cell::new(false),
                mouse_position: None,
                logs: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl ActionContext for MockActionContext {
        fn request_repaint(&self) {
            self.repaint_called.set(true);
        }

        fn get_mouse_position(&self) -> Option<(f32, f32)> {
            self.mouse_position
        }

        fn get_mouse_position_relative(&self, _element_id: usize) -> Option<(f32, f32)> {
            self.mouse_position
        }

        fn navigate(&self, _url: String) -> Result<(), Box<dyn std::error::Error + Send>> {
            Ok(())
        }

        fn request_custom_action(
            &self,
            _action: String,
            _value: Option<Value>,
        ) -> Result<(), Box<dyn std::error::Error + Send>> {
            Ok(())
        }

        fn log(&self, level: LogLevel, message: &str) {
            self.logs.borrow_mut().push((level, message.to_string()));
        }
    }

    type TestHandler = ActionHandler<
        MockElementFinder,
        BTreeMapStyleManager<Option<Visibility>>,
        BTreeMapStyleManager<Option<Color>>,
        BTreeMapStyleManager<bool>,
    >;

    fn create_test_handler() -> TestHandler {
        let mut finder = MockElementFinder::default();
        finder.str_id_map.insert("test-element".to_string(), 10);
        finder.str_id_map.insert("other-element".to_string(), 20);
        finder.class_map.insert("my-class".to_string(), 30);
        finder
            .child_class_map
            .insert((10, "child-class".to_string()), 11);
        finder.last_child_map.insert(10, 12);
        finder.str_ids.insert(10, "test-element".to_string());
        finder
            .data_attrs
            .insert((10, "custom-attr".to_string()), "attr-value".to_string());
        finder.dimensions.insert(10, (100.0, 200.0));
        finder.positions.insert(10, (50.0, 75.0));

        ActionHandler::new(
            finder,
            BTreeMapStyleManager::default(),
            BTreeMapStyleManager::default(),
            BTreeMapStyleManager::default(),
        )
    }

    #[test_log::test]
    fn test_action_handler_get_element_id_by_id() {
        let handler = create_test_handler();
        let target = ElementTarget::ById(Target::from("test-element"));

        let result = handler.get_element_id(&target, 1);
        assert_eq!(result, Some(10));
    }

    #[test_log::test]
    fn test_action_handler_get_element_id_class() {
        let handler = create_test_handler();
        let target = ElementTarget::Class(Target::from("my-class"));

        let result = handler.get_element_id(&target, 1);
        assert_eq!(result, Some(30));
    }

    #[test_log::test]
    fn test_action_handler_get_element_id_child_class() {
        let handler = create_test_handler();
        let target = ElementTarget::ChildClass(Target::from("child-class"));

        // Using parent element 10 which has a child with "child-class" at ID 11
        let result = handler.get_element_id(&target, 10);
        assert_eq!(result, Some(11));
    }

    #[test_log::test]
    fn test_action_handler_get_element_id_numeric_id() {
        let handler = create_test_handler();
        let target = ElementTarget::Id(42);

        let result = handler.get_element_id(&target, 1);
        assert_eq!(result, Some(42));
    }

    #[test_log::test]
    fn test_action_handler_get_element_id_self_target() {
        let handler = create_test_handler();
        let target = ElementTarget::SelfTarget;

        let result = handler.get_element_id(&target, 99);
        assert_eq!(result, Some(99));
    }

    #[test_log::test]
    fn test_action_handler_get_element_id_last_child() {
        let handler = create_test_handler();
        let target = ElementTarget::LastChild;

        // Element 10 has last child at ID 12
        let result = handler.get_element_id(&target, 10);
        assert_eq!(result, Some(12));
    }

    #[test_log::test]
    fn test_action_handler_get_element_id_not_found() {
        let handler = create_test_handler();
        let target = ElementTarget::ById(Target::from("nonexistent"));

        let result = handler.get_element_id(&target, 1);
        assert_eq!(result, None);
    }

    #[test_log::test]
    fn test_action_handler_handle_style_action_visibility() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);
        let action = StyleAction::SetVisibility(Visibility::Hidden);

        let success = handler.handle_style_action(&action, &target, StyleTrigger::UiEvent, 1);

        assert!(success);
        assert_eq!(
            handler.get_visibility_override(10),
            Some(&Some(Visibility::Hidden))
        );
    }

    #[test_log::test]
    fn test_action_handler_handle_style_action_display() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);
        let action = StyleAction::SetDisplay(false);

        let success = handler.handle_style_action(&action, &target, StyleTrigger::UiEvent, 1);

        assert!(success);
        assert_eq!(handler.get_display_override(10), Some(&false));
    }

    #[test_log::test]
    fn test_action_handler_handle_style_action_background_valid_hex() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);
        let action = StyleAction::SetBackground(Some("#FF5500".to_string()));

        let success = handler.handle_style_action(&action, &target, StyleTrigger::UiEvent, 1);

        assert!(success);
        assert!(handler.get_background_override(10).is_some());
    }

    #[test_log::test]
    fn test_action_handler_handle_style_action_background_remove() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);

        // First set a background
        let set_action = StyleAction::SetBackground(Some("#FF5500".to_string()));
        handler.handle_style_action(&set_action, &target, StyleTrigger::UiEvent, 1);

        // Then remove it
        let remove_action = StyleAction::SetBackground(None);
        let success =
            handler.handle_style_action(&remove_action, &target, StyleTrigger::UiEvent, 1);

        assert!(success);
    }

    #[test_log::test]
    fn test_action_handler_handle_style_action_background_invalid_hex() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);
        let action = StyleAction::SetBackground(Some("not-a-color".to_string()));

        let success = handler.handle_style_action(&action, &target, StyleTrigger::UiEvent, 1);

        // Should fail for invalid hex color
        assert!(!success);
    }

    #[test_log::test]
    fn test_action_handler_handle_style_action_target_not_found() {
        let mut handler = create_test_handler();
        let target = ElementTarget::ById(Target::from("nonexistent"));
        let action = StyleAction::SetVisibility(Visibility::Hidden);

        let success = handler.handle_style_action(&action, &target, StyleTrigger::UiEvent, 1);

        assert!(!success);
    }

    #[test_log::test]
    fn test_action_handler_unhandle_style_action_visibility() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);
        let action = StyleAction::SetVisibility(Visibility::Hidden);

        // First apply the style
        handler.handle_style_action(&action, &target, StyleTrigger::UiEvent, 1);
        assert!(handler.get_visibility_override(10).is_some());

        // Then remove it
        handler.unhandle_style_action(&action, &target, StyleTrigger::UiEvent, 1);
        assert!(handler.get_visibility_override(10).is_none());
    }

    #[test_log::test]
    fn test_action_handler_unhandle_style_action_display() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);
        let action = StyleAction::SetDisplay(false);

        // First apply the style
        handler.handle_style_action(&action, &target, StyleTrigger::UiEvent, 1);
        assert!(handler.get_display_override(10).is_some());

        // Then remove it
        handler.unhandle_style_action(&action, &target, StyleTrigger::UiEvent, 1);
        assert!(handler.get_display_override(10).is_none());
    }

    #[test_log::test]
    fn test_action_handler_unhandle_style_action_background() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);
        let action = StyleAction::SetBackground(Some("#FF0000".to_string()));

        // First apply the style
        handler.handle_style_action(&action, &target, StyleTrigger::UiEvent, 1);
        assert!(handler.get_background_override(10).is_some());

        // Then remove it
        handler.unhandle_style_action(&action, &target, StyleTrigger::UiEvent, 1);
        assert!(handler.get_background_override(10).is_none());
    }

    #[test_log::test]
    fn test_action_handler_clear_element_overrides() {
        let mut handler = create_test_handler();
        let target = ElementTarget::Id(10);

        // Apply multiple style overrides
        handler.handle_style_action(
            &StyleAction::SetVisibility(Visibility::Hidden),
            &target,
            StyleTrigger::UiEvent,
            1,
        );
        handler.handle_style_action(
            &StyleAction::SetDisplay(false),
            &target,
            StyleTrigger::UiEvent,
            1,
        );
        handler.handle_style_action(
            &StyleAction::SetBackground(Some("#FF0000".to_string())),
            &target,
            StyleTrigger::UiEvent,
            1,
        );

        // Verify they're applied
        assert!(handler.get_visibility_override(10).is_some());
        assert!(handler.get_display_override(10).is_some());
        assert!(handler.get_background_override(10).is_some());

        // Clear all overrides
        handler.clear_element_overrides(10);

        // Verify all are removed
        assert!(handler.get_visibility_override(10).is_none());
        assert!(handler.get_display_override(10).is_none());
        assert!(handler.get_background_override(10).is_none());
    }

    #[test_log::test]
    fn test_action_handler_handle_action_noop() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();
        let action = ActionType::NoOp;

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
    }

    #[test_log::test]
    fn test_action_handler_handle_action_style() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();
        let action = ActionType::Style {
            target: ElementTarget::Id(10),
            action: StyleAction::SetVisibility(Visibility::Hidden),
        };

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
        assert_eq!(
            handler.get_visibility_override(10),
            Some(&Some(Visibility::Hidden))
        );
    }

    #[test_log::test]
    fn test_action_handler_handle_action_log() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();
        let action = ActionType::Log {
            message: "Test log message".to_string(),
            level: crate::LogLevel::Info,
        };

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
        let logs = context.logs.borrow();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].0, LogLevel::Info);
        assert_eq!(logs[0].1, "Test log message");
    }

    #[test_log::test]
    fn test_action_handler_handle_action_navigate() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();
        let action = ActionType::Navigate {
            url: "/home".to_string(),
        };

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
    }

    #[test_log::test]
    fn test_action_handler_handle_action_custom() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();
        let action = ActionType::Custom {
            action: "my-custom-action".to_string(),
        };

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
    }

    #[test_log::test]
    fn test_action_handler_handle_action_multi() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();
        let action = ActionType::Multi(vec![
            ActionType::Style {
                target: ElementTarget::Id(10),
                action: StyleAction::SetVisibility(Visibility::Hidden),
            },
            ActionType::Style {
                target: ElementTarget::Id(20),
                action: StyleAction::SetDisplay(false),
            },
        ]);

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
        assert_eq!(
            handler.get_visibility_override(10),
            Some(&Some(Visibility::Hidden))
        );
        assert_eq!(handler.get_display_override(20), Some(&false));
    }

    #[test_log::test]
    fn test_action_handler_handle_action_multi_effect() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();
        let action = ActionType::MultiEffect(vec![
            ActionEffect {
                action: ActionType::Style {
                    target: ElementTarget::Id(10),
                    action: StyleAction::SetVisibility(Visibility::Hidden),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            },
            ActionEffect {
                action: ActionType::Style {
                    target: ElementTarget::Id(20),
                    action: StyleAction::SetDisplay(false),
                },
                delay_off: Some(100),
                throttle: None,
                unique: None,
            },
        ]);

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
        assert_eq!(
            handler.get_visibility_override(10),
            Some(&Some(Visibility::Hidden))
        );
        assert_eq!(handler.get_display_override(20), Some(&false));
    }

    #[test_log::test]
    fn test_action_handler_handle_action_with_throttle() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();
        let action = ActionType::Style {
            target: ElementTarget::Id(10),
            action: StyleAction::SetVisibility(Visibility::Hidden),
        };
        let effect = ActionEffect {
            action: action.clone(),
            throttle: Some(10000), // 10 second throttle
            delay_off: None,
            unique: None,
        };

        // First call should succeed
        let success1 = handler.handle_action(
            &action,
            Some(&effect),
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );
        assert!(success1);

        // Second immediate call should be throttled (returns true but requests repaint)
        let success2 = handler.handle_action(
            &action,
            Some(&effect),
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );
        assert!(success2);
        assert!(context.repaint_called.get());
    }

    #[test_log::test]
    fn test_action_handler_unhandle_action_multi() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();

        // First, apply Multi action
        let action = ActionType::Multi(vec![
            ActionType::Style {
                target: ElementTarget::Id(10),
                action: StyleAction::SetVisibility(Visibility::Hidden),
            },
            ActionType::Style {
                target: ElementTarget::Id(20),
                action: StyleAction::SetDisplay(false),
            },
        ]);

        handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        // Verify styles were applied
        assert!(handler.get_visibility_override(10).is_some());
        assert!(handler.get_display_override(20).is_some());

        // Now unhandle the action
        handler.unhandle_action(&action, StyleTrigger::UiEvent, 1, &context);

        // Verify styles were removed
        assert!(handler.get_visibility_override(10).is_none());
        assert!(handler.get_display_override(20).is_none());
    }

    #[test_log::test]
    fn test_action_handler_calc_value_real() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        let value = Value::Real(42.5);
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(42.5)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_visibility() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        let value = Value::Visibility(Visibility::Hidden);
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Visibility(Visibility::Hidden)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_string() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        let value = Value::String("test".to_string());
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::String("test".to_string())));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_width_px() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        // Element 10 has dimensions (100.0, 200.0)
        let value = Value::Calc(crate::logic::CalcValue::WidthPx {
            target: ElementTarget::Id(10),
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(100.0)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_height_px() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        // Element 10 has dimensions (100.0, 200.0)
        let value = Value::Calc(crate::logic::CalcValue::HeightPx {
            target: ElementTarget::Id(10),
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(200.0)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_position_x() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        // Element 10 has position (50.0, 75.0)
        let value = Value::Calc(crate::logic::CalcValue::PositionX {
            target: ElementTarget::Id(10),
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(50.0)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_position_y() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        // Element 10 has position (50.0, 75.0)
        let value = Value::Calc(crate::logic::CalcValue::PositionY {
            target: ElementTarget::Id(10),
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(75.0)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_id() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        // Element 10 has str_id "test-element"
        let value = Value::Calc(crate::logic::CalcValue::Id {
            target: ElementTarget::Id(10),
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::String("test-element".to_string())));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_data_attr_value() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        // Element 10 has data attr "custom-attr" = "attr-value"
        let value = Value::Calc(crate::logic::CalcValue::DataAttrValue {
            attr: "custom-attr".to_string(),
            target: ElementTarget::Id(10),
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::String("attr-value".to_string())));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_event_value() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        let value = Value::Calc(crate::logic::CalcValue::EventValue);
        let result = handler.calc_value(&value, 1, &context, Some("input-value"));

        assert_eq!(result, Some(Value::String("input-value".to_string())));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_event_value_none() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        let value = Value::Calc(crate::logic::CalcValue::EventValue);
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, None);
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_key() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        let value = Value::Calc(crate::logic::CalcValue::Key {
            key: crate::Key::Enter,
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::String("Enter".to_string())));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_mouse_x_global() {
        let handler = create_test_handler();
        let context = MockActionContext {
            mouse_position: Some((150.0, 200.0)),
            ..Default::default()
        };

        let value = Value::Calc(crate::logic::CalcValue::MouseX { target: None });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(150.0)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_mouse_y_global() {
        let handler = create_test_handler();
        let context = MockActionContext {
            mouse_position: Some((150.0, 200.0)),
            ..Default::default()
        };

        let value = Value::Calc(crate::logic::CalcValue::MouseY { target: None });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(200.0)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_mouse_x_relative() {
        let handler = create_test_handler();
        let context = MockActionContext {
            mouse_position: Some((150.0, 200.0)),
            ..Default::default()
        };

        // Element 10 has position (50.0, 75.0), so relative mouse X should be 150 - 50 = 100
        let value = Value::Calc(crate::logic::CalcValue::MouseX {
            target: Some(ElementTarget::Id(10)),
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(100.0)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_calc_mouse_y_relative() {
        let handler = create_test_handler();
        let context = MockActionContext {
            mouse_position: Some((150.0, 200.0)),
            ..Default::default()
        };

        // Element 10 has position (50.0, 75.0), so relative mouse Y should be 200 - 75 = 125
        let value = Value::Calc(crate::logic::CalcValue::MouseY {
            target: Some(ElementTarget::Id(10)),
        });
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(125.0)));
    }

    #[test_log::test]
    fn test_action_handler_calc_value_arithmetic() {
        let handler = create_test_handler();
        let context = MockActionContext::default();

        // Test arithmetic: 10 + 5 = 15
        let arith = crate::logic::Arithmetic::Plus(Value::Real(10.0), Value::Real(5.0));
        let value = Value::Arithmetic(Box::new(arith));
        let result = handler.calc_value(&value, 1, &context, None);

        assert_eq!(result, Some(Value::Real(15.0)));
    }

    #[test_log::test]
    fn test_action_handler_handle_action_logic_condition_true() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();

        // Logic action with Bool(true) condition
        let action = ActionType::Logic(crate::logic::If {
            condition: crate::logic::Condition::Bool(true),
            actions: vec![ActionEffect {
                action: ActionType::Style {
                    target: ElementTarget::Id(10),
                    action: StyleAction::SetVisibility(Visibility::Hidden),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
            else_actions: vec![ActionEffect {
                action: ActionType::Style {
                    target: ElementTarget::Id(20),
                    action: StyleAction::SetDisplay(false),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
        });

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
        // true branch should execute (element 10 hidden)
        assert_eq!(
            handler.get_visibility_override(10),
            Some(&Some(Visibility::Hidden))
        );
        // else branch should NOT execute (element 20 unchanged)
        assert!(handler.get_display_override(20).is_none());
    }

    #[test_log::test]
    fn test_action_handler_handle_action_logic_condition_false() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();

        // Logic action with Bool(false) condition
        let action = ActionType::Logic(crate::logic::If {
            condition: crate::logic::Condition::Bool(false),
            actions: vec![ActionEffect {
                action: ActionType::Style {
                    target: ElementTarget::Id(10),
                    action: StyleAction::SetVisibility(Visibility::Hidden),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
            else_actions: vec![ActionEffect {
                action: ActionType::Style {
                    target: ElementTarget::Id(20),
                    action: StyleAction::SetDisplay(false),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
        });

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
        // true branch should NOT execute (element 10 unchanged)
        assert!(handler.get_visibility_override(10).is_none());
        // else branch should execute (element 20 display false)
        assert_eq!(handler.get_display_override(20), Some(&false));
    }

    #[test_log::test]
    fn test_action_handler_handle_action_logic_condition_eq() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();

        // Logic action with Eq condition comparing equal values
        let action = ActionType::Logic(crate::logic::If {
            condition: crate::logic::Condition::Eq(Value::Real(42.0), Value::Real(42.0)),
            actions: vec![ActionEffect {
                action: ActionType::Style {
                    target: ElementTarget::Id(10),
                    action: StyleAction::SetVisibility(Visibility::Hidden),
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
            else_actions: vec![],
        });

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
        // Values are equal, so true branch should execute
        assert_eq!(
            handler.get_visibility_override(10),
            Some(&Some(Visibility::Hidden))
        );
    }

    #[test_log::test]
    fn test_action_handler_handle_action_parameterized() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();

        // Parameterized action that passes a value
        let action = ActionType::Parameterized {
            action: Box::new(ActionType::Custom {
                action: "test-action".to_string(),
            }),
            value: Value::Real(100.0),
        };

        let success = handler.handle_action(
            &action,
            None,
            StyleTrigger::UiEvent,
            1,
            &context,
            None,
            None,
        );

        assert!(success);
    }

    #[test_log::test]
    fn test_action_handler_log_level_mapping() {
        let mut handler = create_test_handler();
        let context = MockActionContext::default();

        // Test all log levels
        let levels = [
            (crate::LogLevel::Error, LogLevel::Error),
            (crate::LogLevel::Warn, LogLevel::Warn),
            (crate::LogLevel::Info, LogLevel::Info),
            (crate::LogLevel::Debug, LogLevel::Debug),
            (crate::LogLevel::Trace, LogLevel::Trace),
        ];

        for (i, (action_level, expected_level)) in levels.iter().enumerate() {
            let action = ActionType::Log {
                message: format!("Message {i}"),
                level: *action_level,
            };

            handler.handle_action(
                &action,
                None,
                StyleTrigger::UiEvent,
                1,
                &context,
                None,
                None,
            );

            let logs = context.logs.borrow();
            let (logged_level, _) = &logs[i];
            assert_eq!(logged_level, expected_level);
        }
    }
}
