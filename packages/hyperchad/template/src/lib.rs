#![no_std]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

extern crate alloc;

use alloc::string::ToString;
use alloc::{borrow::Cow, boxed::Box, string::String, sync::Arc, vec::Vec};
use core::fmt::{Arguments, Write};

pub use hyperchad_template_macros::container;
pub use hyperchad_transformer::Container;

pub use hyperchad_actions as actions;
pub use hyperchad_color as color;
pub use hyperchad_template_actions_dsl as template_actions_dsl;
pub use hyperchad_transformer as transformer;
pub use hyperchad_transformer_models as transformer_models;

/// Prelude module that re-exports commonly used traits.
///
/// This module is automatically imported when you use `hyperchad_template::container`,
/// so you don't need to manually import these traits.
pub mod prelude {
    pub use crate::{
        self as hyperchad_template, ContainerVecExt, ContainerVecMethods, IntoActionEffect,
        IntoBorder, ToBool, actions as hyperchad_actions, calc, color as hyperchad_color, fx,
        template_actions_dsl as hyperchad_template_actions_dsl,
        transformer as hyperchad_transformer, transformer_models as hyperchad_transformer_models,
    };
}

/// The result type for the container! macro.
///
/// The `container!` macro expands to an expression of this type.
pub type Containers = Vec<Container>;

/// Extension methods for Vec<Container> that are automatically available.
///
/// This trait is automatically implemented and in scope, so you can call
/// these methods on any Vec<Container> without importing anything.
pub trait ContainerVecMethods {
    /// # Errors
    ///
    /// * If the container could not be converted to a string.
    fn display_to_string(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>>;

    /// # Errors
    ///
    /// * If the container could not be converted to a string.
    fn display_to_string_pretty(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>>;

    /// Convert the containers to an HTML string
    fn to_string(&self) -> String;
    /// Convert the containers to an HTML string, consuming self
    fn into_string(self) -> String;
}

impl ContainerVecMethods for Vec<Container> {
    fn display_to_string(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>> {
        self.iter()
            .map(|c| c.display_to_string_default(with_debug_attrs, wrap_raw_in_element))
            .collect::<Result<String, _>>()
    }

    fn display_to_string_pretty(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>> {
        self.iter()
            .map(|c| c.display_to_string_default_pretty(with_debug_attrs, wrap_raw_in_element))
            .collect::<Result<String, _>>()
    }

    fn to_string(&self) -> String {
        self.iter().map(ToString::to_string).collect::<String>()
    }

    fn into_string(self) -> String {
        self.iter().map(ToString::to_string).collect::<String>()
    }
}

/// Convert a Vec<Container> to an HTML string without requiring trait imports.
///
/// This is a convenience function that works with the result of the `container!` macro.
///
/// # Example
/// ```rust
/// use hyperchad_template::{container, to_html};
///
/// let containers = container! {
///     div { "Hello World" }
/// };
/// let html = to_html(&containers);
/// ```
#[must_use]
pub fn to_html(containers: &[Container]) -> String {
    containers
        .iter()
        .map(ToString::to_string)
        .collect::<String>()
}

/// Convert a Vec<Container> to an HTML string, consuming the vector.
///
/// This is a convenience function that works with the result of the `container!` macro.
///
/// # Example
/// ```rust
/// use hyperchad_template::{container, into_html};
///
/// let containers = container! {
///     div { "Hello World" }
/// };
/// let html = into_html(&containers);
/// ```
pub fn into_html(containers: &[Container]) -> String {
    containers
        .iter()
        .map(ToString::to_string)
        .collect::<String>()
}

/// Extension trait to add missing methods to Vec<Container>
pub trait ContainerVecExt {
    /// # Errors
    ///
    /// * If the container could not be converted to a string.
    fn display_to_string(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>>;

    /// # Errors
    ///
    /// * If the container could not be converted to a string.
    fn display_to_string_pretty(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>>;

    fn into_string(self) -> String;
    fn to_string(&self) -> String;
}

impl ContainerVecExt for Vec<Container> {
    fn display_to_string(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>> {
        self.iter()
            .map(|c| c.display_to_string_default(with_debug_attrs, wrap_raw_in_element))
            .collect::<Result<String, _>>()
    }

    fn display_to_string_pretty(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>> {
        self.iter()
            .map(|c| c.display_to_string_default_pretty(with_debug_attrs, wrap_raw_in_element))
            .collect::<Result<String, _>>()
    }

    fn into_string(self) -> String {
        self.iter().map(ToString::to_string).collect::<String>()
    }

    fn to_string(&self) -> String {
        self.iter().map(ToString::to_string).collect::<String>()
    }
}

/// Represents a type that can be rendered as a Container.
///
/// To implement this for your own type, override either the `.render()`
/// or `.render_to()` methods; since each is defined in terms of the
/// other, you only need to implement one of them. See the example below.
///
/// # Minimal implementation
///
/// An implementation of this trait must override at least one of
/// `.render()` or `.render_to()`. Since the default definitions of
/// these methods call each other, not doing this will result in
/// infinite recursion.
///
/// # Example
///
/// ```rust
/// use hyperchad_template::{container, Containers, RenderContainer};
///
/// use core::convert::Infallible;
///
/// /// Provides a shorthand for a styled button.
/// pub struct StyledButton {
///     pub text: String,
///     pub primary: bool,
/// }
///
/// impl RenderContainer for StyledButton {
///     type Error = Infallible;
///
///     fn render_to(&self, containers: &mut Containers) -> Result<(), Self::Error> {
///         *containers = container! {
///             button id=(if self.primary { "primary" } else { "secondary" }) {
///                 (self.text.clone())
///             }
///         };
///         Ok(())
///     }
/// }
/// ```
pub trait RenderContainer {
    type Error;

    /// # Errors
    ///
    /// * If the value could not be rendered.
    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error>;

    /// # Errors
    ///
    /// * If the value could not be converted to a string.
    fn render_to_string(&self) -> Result<String, Self::Error> {
        let mut containers = Vec::new();
        self.render_to(&mut containers)?;
        Ok(containers
            .iter()
            .map(ToString::to_string)
            .collect::<String>())
    }

    /// # Errors
    ///
    /// * If the value could not be converted to a `Vec<Container>`.
    fn render(&self) -> Result<Vec<Container>, Self::Error> {
        let mut containers = Vec::new();
        self.render_to(&mut containers)?;
        Ok(containers)
    }
}

impl RenderContainer for Container {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        containers.push(self.clone());
        Ok(())
    }
}

impl RenderContainer for &str {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        if self.is_empty() {
            return Ok(());
        }
        containers.push(Container {
            element: hyperchad_transformer::Element::Raw {
                value: (*self).to_string(),
            },
            ..Default::default()
        });
        Ok(())
    }
}

impl RenderContainer for String {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        if self.is_empty() {
            return Ok(());
        }
        containers.push(Container {
            element: hyperchad_transformer::Element::Raw {
                value: self.clone(),
            },
            ..Default::default()
        });
        Ok(())
    }
}

impl RenderContainer for Cow<'_, str> {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        if self.is_empty() {
            return Ok(());
        }
        <&str as RenderContainer>::render_to(&self.as_ref(), containers)
    }
}

impl RenderContainer for Arguments<'_> {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        let mut s = String::new();
        s.write_fmt(*self)?;
        containers.push(Container {
            element: hyperchad_transformer::Element::Raw { value: s },
            ..Default::default()
        });
        Ok(())
    }
}

impl<T: RenderContainer + ?Sized> RenderContainer for &T {
    type Error = T::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        T::render_to(self, containers)
    }
}

impl<T: RenderContainer + ?Sized> RenderContainer for &mut T {
    type Error = T::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        T::render_to(self, containers)
    }
}

impl<T: RenderContainer + ?Sized> RenderContainer for Box<T> {
    type Error = T::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        T::render_to(self, containers)
    }
}

impl<T: RenderContainer + ?Sized> RenderContainer for Arc<T> {
    type Error = T::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        T::render_to(self, containers)
    }
}

// char and floats are handled by specialized implementations below

macro_rules! impl_render_container_with_itoa {
    ($($ty:ty)*) => {
        $(
            impl RenderContainer for $ty {
                type Error = core::fmt::Error;

                fn render_to(
                    &self,
                    containers: &mut Vec<Container>,
                ) -> Result<(), Self::Error> {
                    let mut buffer = itoa::Buffer::new();
                    let s = buffer.format(*self);
                    <&str as RenderContainer>::render_to(&s, containers)
                }
            }
        )*
    };
}

impl_render_container_with_itoa! {
    i8 i16 i32 i64 i128 isize
    u8 u16 u32 u64 u128 usize
}

impl RenderContainer for f32 {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        let mut buffer = ryu::Buffer::new();
        let s = buffer.format(*self);
        <&str as RenderContainer>::render_to(&s, containers)
    }
}

impl RenderContainer for f64 {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        let mut buffer = ryu::Buffer::new();
        let s = buffer.format(*self);
        <&str as RenderContainer>::render_to(&s, containers)
    }
}

impl RenderContainer for bool {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        let s = if *self { "true" } else { "false" };
        <&str as RenderContainer>::render_to(&s, containers)
    }
}

impl RenderContainer for char {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        let mut buffer = [0; 4];
        let s = self.encode_utf8(&mut buffer);
        let s: &str = s;
        <&str as RenderContainer>::render_to(&s, containers)
    }
}

impl<T: RenderContainer> RenderContainer for Option<T> {
    type Error = T::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        self.as_ref()
            .map_or_else(|| Ok(()), |value| value.render_to(containers))
    }
}

impl RenderContainer for Vec<Container> {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        containers.extend_from_slice(self);
        Ok(())
    }
}

/// A wrapper around Vec<Container> that provides convenient methods without requiring trait imports.
///
/// This is what the `container!` macro returns, providing `to_string()` and other
/// methods without needing to import `ContainerVecExt`.
#[derive(Debug, Clone)]
pub struct ContainerList(pub Vec<Container>);

impl core::fmt::Display for ContainerList {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0
            .iter()
            .try_for_each(|c| f.write_fmt(format_args!("{c}")))
    }
}

impl ContainerList {
    #[must_use]
    pub const fn new(containers: Vec<Container>) -> Self {
        Self(containers)
    }

    /// Convert into a String representation (HTML)
    #[must_use]
    pub fn into_string(self) -> String {
        self.0.iter().map(ToString::to_string).collect::<String>()
    }

    pub fn iter(&self) -> core::slice::Iter<'_, Container> {
        self.0.iter()
    }

    /// Get the inner Vec<Container>
    #[must_use]
    pub fn into_inner(self) -> Vec<Container> {
        self.0
    }

    /// Get a reference to the inner Vec<Container>
    #[must_use]
    pub const fn as_inner(&self) -> &Vec<Container> {
        &self.0
    }
}

impl<'a> IntoIterator for &'a ContainerList {
    type Item = &'a Container;
    type IntoIter = core::slice::Iter<'a, Container>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for ContainerList {
    type Item = Container;
    type IntoIter = alloc::vec::IntoIter<Container>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<Vec<Container>> for ContainerList {
    fn from(containers: Vec<Container>) -> Self {
        Self(containers)
    }
}

impl From<ContainerList> for Vec<Container> {
    fn from(list: ContainerList) -> Self {
        list.0
    }
}

impl core::ops::Deref for ContainerList {
    type Target = Vec<Container>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for ContainerList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Re-export for convenience
pub use hyperchad_transformer_models::*;

// Add responsive support infrastructure
#[cfg(feature = "logic")]
pub use hyperchad_actions::logic::{IfExpression, Responsive, if_responsive};

/// Trait for converting values to bool (to handle `IfExpression<bool, Responsive>`)
pub trait ToBool {
    fn to_bool(self) -> bool;
}

// Implement ToBool trait
impl ToBool for bool {
    fn to_bool(self) -> bool {
        self
    }
}

#[cfg(feature = "logic")]
impl ToBool for IfExpression<bool, Responsive> {
    fn to_bool(self) -> bool {
        self.default
            .unwrap_or_else(|| self.value.unwrap_or_default())
    }
}

/// Trait for converting various types to `ActionEffect`.
/// This handles the conversion chain properly without violating the orphan rule.
pub trait IntoActionEffect {
    fn into_action_effect(self) -> actions::ActionEffect;
}

impl IntoActionEffect for actions::Action {
    fn into_action_effect(self) -> actions::ActionEffect {
        self.effect
    }
}

impl IntoActionEffect for actions::ActionEffect {
    fn into_action_effect(self) -> actions::ActionEffect {
        self
    }
}

impl IntoActionEffect for Vec<actions::ActionEffect> {
    fn into_action_effect(self) -> actions::ActionEffect {
        actions::ActionType::MultiEffect(self).into()
    }
}

impl IntoActionEffect for actions::ActionType {
    fn into_action_effect(self) -> actions::ActionEffect {
        self.into()
    }
}

impl IntoActionEffect for Vec<actions::ActionType> {
    fn into_action_effect(self) -> actions::ActionEffect {
        self.into_iter()
            .map(IntoActionEffect::into_action_effect)
            .collect::<Vec<_>>()
            .into()
    }
}

#[cfg(feature = "logic")]
impl IntoActionEffect for actions::logic::If {
    fn into_action_effect(self) -> actions::ActionEffect {
        actions::ActionType::Logic(self).into()
    }
}

/// Trait for converting various types to border tuples (Color, Number).
/// This handles flexible border specification in templates.
pub trait IntoBorder {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number);
}

impl IntoBorder for (hyperchad_color::Color, hyperchad_transformer::Number) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        self
    }
}

impl IntoBorder for (hyperchad_color::Color, i32) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            self.0,
            hyperchad_transformer::Number::Integer(i64::from(self.1)),
        )
    }
}

impl IntoBorder for (hyperchad_color::Color, u16) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            self.0,
            hyperchad_transformer::Number::Integer(i64::from(self.1)),
        )
    }
}

impl IntoBorder for (hyperchad_color::Color, f32) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (self.0, hyperchad_transformer::Number::Real(self.1))
    }
}

impl IntoBorder for (hyperchad_color::Color, f64) {
    #[allow(clippy::cast_possible_truncation)]
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (self.0, hyperchad_transformer::Number::Real(self.1 as f32))
    }
}

impl IntoBorder for (i32, hyperchad_color::Color) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            self.1,
            hyperchad_transformer::Number::Integer(i64::from(self.0)),
        )
    }
}

impl IntoBorder for (u16, hyperchad_color::Color) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            self.1,
            hyperchad_transformer::Number::Integer(i64::from(self.0)),
        )
    }
}

impl IntoBorder for (f32, hyperchad_color::Color) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (self.1, hyperchad_transformer::Number::Real(self.0))
    }
}

impl IntoBorder for (f64, hyperchad_color::Color) {
    #[allow(clippy::cast_possible_truncation)]
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (self.1, hyperchad_transformer::Number::Real(self.0 as f32))
    }
}

impl IntoBorder for (i32, &str) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.1),
            hyperchad_transformer::Number::Integer(i64::from(self.0)),
        )
    }
}

impl IntoBorder for (u16, &str) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.1),
            hyperchad_transformer::Number::Integer(i64::from(self.0)),
        )
    }
}

impl IntoBorder for (f32, &str) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.1),
            hyperchad_transformer::Number::Real(self.0),
        )
    }
}

impl IntoBorder for (f64, &str) {
    #[allow(clippy::cast_possible_truncation)]
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.1),
            hyperchad_transformer::Number::Real(self.0 as f32),
        )
    }
}

impl IntoBorder for (&str, i32) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.0),
            hyperchad_transformer::Number::Integer(i64::from(self.1)),
        )
    }
}

impl IntoBorder for (&str, u16) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.0),
            hyperchad_transformer::Number::Integer(i64::from(self.1)),
        )
    }
}

impl IntoBorder for (&str, f32) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.0),
            hyperchad_transformer::Number::Real(self.1),
        )
    }
}

impl IntoBorder for (&str, f64) {
    #[allow(clippy::cast_possible_truncation)]
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.0),
            hyperchad_transformer::Number::Real(self.1 as f32),
        )
    }
}

impl IntoBorder for (i32, String) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.1),
            hyperchad_transformer::Number::Integer(i64::from(self.0)),
        )
    }
}

impl IntoBorder for (u16, String) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.1),
            hyperchad_transformer::Number::Integer(i64::from(self.0)),
        )
    }
}

impl IntoBorder for (f32, String) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.1),
            hyperchad_transformer::Number::Real(self.0),
        )
    }
}

impl IntoBorder for (f64, String) {
    #[allow(clippy::cast_possible_truncation)]
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.1),
            hyperchad_transformer::Number::Real(self.0 as f32),
        )
    }
}

impl IntoBorder for (String, i32) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.0),
            hyperchad_transformer::Number::Integer(i64::from(self.1)),
        )
    }
}

impl IntoBorder for (String, u16) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.0),
            hyperchad_transformer::Number::Integer(i64::from(self.1)),
        )
    }
}

impl IntoBorder for (String, f32) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.0),
            hyperchad_transformer::Number::Real(self.1),
        )
    }
}

impl IntoBorder for (String, f64) {
    #[allow(clippy::cast_possible_truncation)]
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.0),
            hyperchad_transformer::Number::Real(self.1 as f32),
        )
    }
}

/// Helper module for `calc()` expressions and mathematical operations on Number types
///
/// # Examples
///
/// The calc module enables mathematical expressions in attributes:
///
/// ```rust
/// use hyperchad_template::container;
/// use hyperchad_transformer::Number;
///
/// // Basic calc expressions
/// let containers = container! {
///     div width=calc(100% - 20) {
///         "Content with calculated width"
///     }
/// };
///
/// // With variables
/// let margin = 10;
/// let containers = container! {
///     div width=calc(100% - (margin * 2)) {
///         "Content with variable-based calculation"
///     }
/// };
///
/// // With helper functions
/// let height_value = 50;
/// let containers = container! {
///     div height=calc(vh(height_value) - 20) {
///         "Content with viewport units"
///     }
/// };
///
/// // Complex nested calculations
/// let base_width = Number::IntegerPercent(80);
/// let containers = container! {
///     div width=calc(base_width - (percent(10) - (100% / 5))) {
///         "Complex calculation"
///     }
/// };
/// ```
pub mod calc {
    use hyperchad_transformer::Number;

    /// Convert any type that can be converted to Number into a Number
    pub fn to_number<T: Into<Number>>(value: T) -> Number {
        value.into()
    }

    /// Convert a value to a percentage Number variant
    pub fn to_percent_number<T>(value: T) -> Number
    where
        T: Into<Number>,
    {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerPercent(n),
            Number::Real(n) => Number::RealPercent(n),
            // If it's already a percent type, keep it
            Number::IntegerPercent(_) | Number::RealPercent(_) => num,
            // For other number types, convert to Real percentage
            _ => {
                // Use the existing calc method to get the raw value
                let raw_value = num.calc(0.0, 100.0, 100.0);
                Number::RealPercent(raw_value)
            }
        }
    }

    /// Convert a value to a viewport width Number variant
    pub fn to_vw_number<T>(value: T) -> Number
    where
        T: Into<Number>,
    {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerVw(n),
            Number::Real(n) => Number::RealVw(n),
            // If it's already a vw type, keep it
            Number::IntegerVw(_) | Number::RealVw(_) => num,
            // For other number types, convert to Real vw
            _ => {
                let raw_value = num.calc(0.0, 100.0, 100.0);
                Number::RealVw(raw_value)
            }
        }
    }

    /// Convert a value to a viewport height Number variant
    pub fn to_vh_number<T>(value: T) -> Number
    where
        T: Into<Number>,
    {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerVh(n),
            Number::Real(n) => Number::RealVh(n),
            // If it's already a vh type, keep it
            Number::IntegerVh(_) | Number::RealVh(_) => num,
            // For other number types, convert to Real vh
            _ => {
                let raw_value = num.calc(0.0, 100.0, 100.0);
                Number::RealVh(raw_value)
            }
        }
    }

    /// Convert a value to a dynamic viewport width Number variant
    pub fn to_dvw_number<T>(value: T) -> Number
    where
        T: Into<Number>,
    {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerDvw(n),
            Number::Real(n) => Number::RealDvw(n),
            // If it's already a dvw type, keep it
            Number::IntegerDvw(_) | Number::RealDvw(_) => num,
            // For other number types, convert to Real dvw
            _ => {
                let raw_value = num.calc(0.0, 100.0, 100.0);
                Number::RealDvw(raw_value)
            }
        }
    }

    /// Convert a value to a dynamic viewport height Number variant
    pub fn to_dvh_number<T>(value: T) -> Number
    where
        T: Into<Number>,
    {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerDvh(n),
            Number::Real(n) => Number::RealDvh(n),
            // If it's already a dvh type, keep it
            Number::IntegerDvh(_) | Number::RealDvh(_) => num,
            // For other number types, convert to Real dvh
            _ => {
                let raw_value = num.calc(0.0, 100.0, 100.0);
                Number::RealDvh(raw_value)
            }
        }
    }

    /// Add two Number values, handling different Number types appropriately
    #[must_use]
    pub fn add_numbers(left: &Number, right: &Number) -> Number {
        // For now, use a simple approach that converts both to common units if possible
        // or falls back to real numbers for calculations
        #[allow(clippy::cast_precision_loss)]
        match (left, right) {
            // Same unit types - direct addition
            (Number::Integer(a), Number::Integer(b)) => Number::Integer(a + b),
            (Number::Real(a), Number::Real(b)) => Number::Real(a + b),
            (Number::IntegerPercent(a), Number::IntegerPercent(b)) => Number::IntegerPercent(a + b),
            (Number::RealPercent(a), Number::RealPercent(b)) => Number::RealPercent(a + b),
            (Number::IntegerVw(a), Number::IntegerVw(b)) => Number::IntegerVw(a + b),
            (Number::RealVw(a), Number::RealVw(b)) => Number::RealVw(a + b),
            (Number::IntegerVh(a), Number::IntegerVh(b)) => Number::IntegerVh(a + b),
            (Number::RealVh(a), Number::RealVh(b)) => Number::RealVh(a + b),
            (Number::IntegerDvw(a), Number::IntegerDvw(b)) => Number::IntegerDvw(a + b),
            (Number::RealDvw(a), Number::RealDvw(b)) => Number::RealDvw(a + b),
            (Number::IntegerDvh(a), Number::IntegerDvh(b)) => Number::IntegerDvh(a + b),
            (Number::RealDvh(a), Number::RealDvh(b)) => Number::RealDvh(a + b),

            // Mixed integer/real of same unit
            (Number::Integer(a), Number::Real(b)) => Number::Real(*a as f32 + b),
            (Number::Real(a), Number::Integer(b)) => Number::Real(a + *b as f32),
            (Number::IntegerPercent(a), Number::RealPercent(b)) => {
                Number::RealPercent(*a as f32 + b)
            }
            (Number::RealPercent(a), Number::IntegerPercent(b)) => {
                Number::RealPercent(a + *b as f32)
            }
            (Number::IntegerVw(a), Number::RealVw(b)) => Number::RealVw(*a as f32 + b),
            (Number::RealVw(a), Number::IntegerVw(b)) => Number::RealVw(a + *b as f32),
            (Number::IntegerVh(a), Number::RealVh(b)) => Number::RealVh(*a as f32 + b),
            (Number::RealVh(a), Number::IntegerVh(b)) => Number::RealVh(a + *b as f32),
            (Number::IntegerDvw(a), Number::RealDvw(b)) => Number::RealDvw(*a as f32 + b),
            (Number::RealDvw(a), Number::IntegerDvw(b)) => Number::RealDvw(a + *b as f32),
            (Number::IntegerDvh(a), Number::RealDvh(b)) => Number::RealDvh(*a as f32 + b),
            (Number::RealDvh(a), Number::IntegerDvh(b)) => Number::RealDvh(a + *b as f32),

            // Different units - for now, use calc() to convert to pixels and add
            // This is a simplified approach; a more sophisticated implementation might
            // preserve the calc() expression for runtime evaluation
            _ => {
                // Use dummy viewport dimensions for conversion
                let left_pixels = left.calc(100.0, 1920.0, 1080.0);
                let right_pixels = right.calc(100.0, 1920.0, 1080.0);
                Number::Real(left_pixels + right_pixels)
            }
        }
    }

    /// Subtract two Number values
    #[must_use]
    pub fn subtract_numbers(left: &Number, right: &Number) -> Number {
        #[allow(clippy::cast_precision_loss)]
        match (left, right) {
            // Same unit types - direct subtraction
            (Number::Integer(a), Number::Integer(b)) => Number::Integer(a - b),
            (Number::Real(a), Number::Real(b)) => Number::Real(a - b),
            (Number::IntegerPercent(a), Number::IntegerPercent(b)) => Number::IntegerPercent(a - b),
            (Number::RealPercent(a), Number::RealPercent(b)) => Number::RealPercent(a - b),
            (Number::IntegerVw(a), Number::IntegerVw(b)) => Number::IntegerVw(a - b),
            (Number::RealVw(a), Number::RealVw(b)) => Number::RealVw(a - b),
            (Number::IntegerVh(a), Number::IntegerVh(b)) => Number::IntegerVh(a - b),
            (Number::RealVh(a), Number::RealVh(b)) => Number::RealVh(a - b),
            (Number::IntegerDvw(a), Number::IntegerDvw(b)) => Number::IntegerDvw(a - b),
            (Number::RealDvw(a), Number::RealDvw(b)) => Number::RealDvw(a - b),
            (Number::IntegerDvh(a), Number::IntegerDvh(b)) => Number::IntegerDvh(a - b),
            (Number::RealDvh(a), Number::RealDvh(b)) => Number::RealDvh(a - b),

            // Mixed integer/real of same unit
            (Number::Integer(a), Number::Real(b)) => Number::Real(*a as f32 - b),
            (Number::Real(a), Number::Integer(b)) => Number::Real(a - *b as f32),
            (Number::IntegerPercent(a), Number::RealPercent(b)) => {
                Number::RealPercent(*a as f32 - b)
            }
            (Number::RealPercent(a), Number::IntegerPercent(b)) => {
                Number::RealPercent(a - *b as f32)
            }
            (Number::IntegerVw(a), Number::RealVw(b)) => Number::RealVw(*a as f32 - b),
            (Number::RealVw(a), Number::IntegerVw(b)) => Number::RealVw(a - *b as f32),
            (Number::IntegerVh(a), Number::RealVh(b)) => Number::RealVh(*a as f32 - b),
            (Number::RealVh(a), Number::IntegerVh(b)) => Number::RealVh(a - *b as f32),
            (Number::IntegerDvw(a), Number::RealDvw(b)) => Number::RealDvw(*a as f32 - b),
            (Number::RealDvw(a), Number::IntegerDvw(b)) => Number::RealDvw(a - *b as f32),
            (Number::IntegerDvh(a), Number::RealDvh(b)) => Number::RealDvh(*a as f32 - b),
            (Number::RealDvh(a), Number::IntegerDvh(b)) => Number::RealDvh(a - *b as f32),

            // Different units - convert to pixels and subtract
            _ => {
                let left_pixels = left.calc(100.0, 1920.0, 1080.0);
                let right_pixels = right.calc(100.0, 1920.0, 1080.0);
                Number::Real(left_pixels - right_pixels)
            }
        }
    }

    /// Multiply two Number values
    #[must_use]
    pub fn multiply_numbers(left: &Number, right: &Number) -> Number {
        #[allow(clippy::cast_precision_loss)]
        match (left, right) {
            // When multiplying, typically one operand should be unitless
            // For now, we'll handle some common cases and fall back to pixel conversion
            (Number::Integer(a), Number::Integer(b)) => Number::Integer(a * b),
            (Number::Real(a), Number::Real(b)) => Number::Real(a * b),
            (Number::Integer(a), Number::Real(b)) => Number::Real(*a as f32 * b),
            (Number::Real(a), Number::Integer(b)) => Number::Real(a * *b as f32),

            // For units, if one is unitless, preserve the unit of the other
            (Number::IntegerPercent(a), Number::Integer(b))
            | (Number::Integer(a), Number::IntegerPercent(b)) => Number::IntegerPercent(a * b),
            (Number::RealPercent(a), Number::Real(b))
            | (Number::Real(a), Number::RealPercent(b)) => Number::RealPercent(a * b),
            // For more complex cases, convert to pixels and multiply
            _ => {
                let left_pixels = left.calc(100.0, 1920.0, 1080.0);
                let right_pixels = right.calc(100.0, 1920.0, 1080.0);
                Number::Real(left_pixels * right_pixels)
            }
        }
    }

    /// Divide two Number values
    #[must_use]
    pub fn divide_numbers(left: &Number, right: &Number) -> Number {
        #[allow(clippy::cast_precision_loss)]
        match (left, right) {
            // Basic division
            (Number::Integer(a), Number::Integer(b)) => {
                if *b != 0 {
                    Number::Real(*a as f32 / *b as f32)
                } else {
                    Number::Real(0.0) // Avoid division by zero
                }
            }
            (Number::Real(a), Number::Real(b)) => {
                if *b == 0.0 {
                    Number::Real(0.0)
                } else {
                    Number::Real(a / b)
                }
            }
            (Number::Integer(a), Number::Real(b)) => {
                if *b == 0.0 {
                    Number::Real(0.0)
                } else {
                    Number::Real(*a as f32 / b)
                }
            }
            (Number::Real(a), Number::Integer(b)) => {
                if *b != 0 {
                    Number::Real(a / *b as f32)
                } else {
                    Number::Real(0.0)
                }
            }

            // For units, if the divisor is unitless, preserve the unit
            (Number::IntegerPercent(a), Number::Integer(b)) => {
                if *b != 0 {
                    Number::RealPercent(*a as f32 / *b as f32)
                } else {
                    Number::RealPercent(0.0)
                }
            }
            (Number::RealPercent(a), Number::Real(b)) => {
                if *b == 0.0 {
                    Number::RealPercent(0.0)
                } else {
                    Number::RealPercent(a / b)
                }
            }

            // For more complex cases, convert to pixels and divide
            _ => {
                let left_pixels = left.calc(100.0, 1920.0, 1080.0);
                let right_pixels = right.calc(100.0, 1920.0, 1080.0);
                if right_pixels == 0.0 {
                    Number::Real(0.0)
                } else {
                    Number::Real(left_pixels / right_pixels)
                }
            }
        }
    }
}

/// Helper module for function-style unit syntax like vw(50), vh(100), dvw(90), dvh(60)
pub mod unit_functions {
    use hyperchad_transformer::Number;

    // Helper function to convert Number to f32 for fallback cases
    fn number_to_f32(num: &Number) -> f32 {
        // Use calc with dummy values to get the numeric value
        // For non-percentage/viewport units, this will return the raw value
        num.calc(0.0, 100.0, 100.0)
    }

    // Viewport units
    pub fn vw<T: Into<Number>>(value: T) -> Number {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerVw(n),
            Number::Real(n) => Number::RealVw(n),
            // For other number types, convert to real vw
            _ => Number::RealVw(number_to_f32(&num)),
        }
    }

    pub fn vh<T: Into<Number>>(value: T) -> Number {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerVh(n),
            Number::Real(n) => Number::RealVh(n),
            _ => Number::RealVh(number_to_f32(&num)),
        }
    }

    pub fn dvw<T: Into<Number>>(value: T) -> Number {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerDvw(n),
            Number::Real(n) => Number::RealDvw(n),
            _ => Number::RealDvw(number_to_f32(&num)),
        }
    }

    pub fn dvh<T: Into<Number>>(value: T) -> Number {
        let num = value.into();
        match num {
            Number::Integer(n) => Number::IntegerDvh(n),
            Number::Real(n) => Number::RealDvh(n),
            _ => Number::RealDvh(number_to_f32(&num)),
        }
    }
}

/// FX DSL function for template actions
///
/// This function serves as the entry point for the fx DSL syntax in templates.
/// It supports the curly brace syntax for defining actions:
///
/// # Examples
///
/// **Single action:**
/// ```rust
/// use hyperchad_template::container;
///
/// let containers = container! {
///     button fx-click=fx { hide("search") } {
///         "Close Search"
///     }
/// };
/// ```
///
/// **Multiple actions:**
/// ```rust
/// use hyperchad_template::container;
///
/// let containers = container! {
///     button fx-click=fx {
///         hide("search");
///         show("search-button");
///     } {
///         "Toggle Search"
///     }
/// };
/// ```
///
/// **Conditional actions:**
/// ```rust
/// use hyperchad_template::container;
///
/// let containers = container! {
///     button fx-click=fx {
///         if get_visibility("modal") == visible() {
///             hide("modal")
///         } else {
///             show("modal")
///         }
///     } {
///         "Toggle Modal"
///     }
/// };
/// ```
///
/// The function itself is just a marker - the actual processing is done by the
/// template macro system during compilation. The macro detects `fx` calls and
/// processes them through the `actions_dsl!` macro.
pub fn fx<T>(content: T) -> actions::ActionEffect
where
    T: IntoActionEffect,
{
    // This function is mainly for compilation - the template macro system
    // will intercept fx calls before they reach this runtime implementation.
    // For fallback purposes, try to convert the input to an ActionEffect.
    content.into_action_effect()
}

/// Helper module for color functions
///
/// # Examples
///
/// The `color_functions` module enables CSS-style color functions in attributes:
///
/// ```rust
/// use hyperchad_template::container;
///
/// // RGB color functions
/// let containers = container! {
///     div color=rgb(255, 0, 0) background=rgb(100, 200, 150) {
///         "Red text on green background"
///     }
/// };
///
/// // RGBA color functions with alpha
/// let containers = container! {
///     div color=rgba(255, 0, 0, 0.8) background=rgba(100, 200, 150, 128) {
///         "Semi-transparent colors"
///     }
/// };
///
/// // With variables
/// let red = 255;
/// let green = 100;
/// let blue = 50;
/// let alpha = 0.75;
/// let containers = container! {
///     div color=rgb(red, green, blue) background=rgba(red, green, blue, alpha) {
///         "Variable-based colors"
///     }
/// };
/// ```
pub mod color_functions {
    use alloc::string::String;
    use hyperchad_color::Color;

    /// Create an RGB color from red, green, and blue values (0-255)
    ///
    /// This function creates RGB colors with 3 arguments. For RGBA colors with alpha,
    /// use the template syntax `rgb(r, g, b, a)` which will automatically route to
    /// the appropriate function, or use `rgb_alpha()` directly.
    ///
    /// # Examples
    /// ```rust
    /// use hyperchad_template::color_functions::rgb;
    /// use hyperchad_color::Color;
    ///
    /// // RGB format (3 arguments)
    /// let red_color = rgb(255, 0, 0);
    /// let green_color = rgb(0, 255, 0);
    /// let blue_color = rgb(0, 0, 255);
    /// ```
    pub fn rgb<R, G, B>(red: R, green: G, blue: B) -> Color
    where
        R: ToRgbValue,
        G: ToRgbValue,
        B: ToRgbValue,
    {
        Color {
            r: red.to_rgb_value(),
            g: green.to_rgb_value(),
            b: blue.to_rgb_value(),
            a: None,
        }
    }

    /// Create an RGBA color using the rgb function with 4 arguments
    ///
    /// This is an overloaded version of `rgb()` that accepts an alpha parameter.
    /// Alpha can be specified as:
    /// - A float between 0.0 and 1.0 (e.g., 0.5 for 50% opacity)
    /// - An integer between 0 and 255 (e.g., 128 for 50% opacity)
    /// - A percentage string like "50%" (e.g., "75%" for 75% opacity)
    ///
    /// # Examples
    /// ```rust
    /// use hyperchad_template::color_functions::rgb_alpha;
    /// use hyperchad_color::Color;
    ///
    /// // Float alpha (0.0 - 1.0)
    /// let semi_transparent_red = rgb_alpha(255, 0, 0, 0.5);
    ///
    /// // Integer alpha (0 - 255)
    /// let semi_transparent_green = rgb_alpha(0, 255, 0, 128);
    ///
    /// // Percentage alpha (0% - 100%)
    /// let semi_transparent_blue = rgb_alpha(0, 0, 255, "75%");
    /// ```
    pub fn rgb_alpha<R, G, B, A>(red: R, green: G, blue: B, alpha: A) -> Color
    where
        R: ToRgbValue,
        G: ToRgbValue,
        B: ToRgbValue,
        A: Into<AlphaValue>,
    {
        let alpha_val = alpha.into().to_u8();
        Color {
            r: red.to_rgb_value(),
            g: green.to_rgb_value(),
            b: blue.to_rgb_value(),
            a: Some(alpha_val),
        }
    }

    /// Create an RGBA color from red, green, blue, and alpha values
    ///
    /// This is the legacy `rgba()` function name for compatibility.
    /// Alpha can be specified as:
    /// - A float between 0.0 and 1.0 (e.g., 0.5 for 50% opacity)
    /// - An integer between 0 and 255 (e.g., 128 for 50% opacity)
    /// - A percentage string like "50%" (e.g., "75%" for 75% opacity)
    ///
    /// # Examples
    /// ```rust
    /// use hyperchad_template::color_functions::rgba;
    /// use hyperchad_color::Color;
    ///
    /// // Float alpha (0.0 - 1.0)
    /// let semi_transparent_red = rgba(255, 0, 0, 0.5);
    ///
    /// // Integer alpha (0 - 255)
    /// let semi_transparent_green = rgba(0, 255, 0, 128);
    ///
    /// // Percentage alpha (0% - 100%)
    /// let semi_transparent_blue = rgba(0, 0, 255, "50%");
    /// ```
    pub fn rgba<R, G, B, A>(red: R, green: G, blue: B, alpha: A) -> Color
    where
        R: ToRgbValue,
        G: ToRgbValue,
        B: ToRgbValue,
        A: Into<AlphaValue>,
    {
        // Call the rgb_alpha function
        rgb_alpha(red, green, blue, alpha)
    }

    /// Helper type for alpha values that can be converted from various numeric types
    pub enum AlphaValue {
        Float(f32),      // 0.0 - 1.0
        Integer(u8),     // 0 - 255
        Percentage(f32), // 0% - 100%
    }

    impl AlphaValue {
        #[must_use]
        pub fn to_u8(self) -> u8 {
            match self {
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                Self::Float(f) => {
                    // Clamp to 0.0-1.0 range and convert to 0-255
                    let clamped = f.clamp(0.0, 1.0);
                    (clamped * 255.0).round() as u8
                }
                Self::Integer(i) => i,
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                Self::Percentage(p) => {
                    // Clamp to 0.0-100.0 range and convert to 0-255
                    let clamped = p.clamp(0.0, 100.0);
                    (clamped / 100.0 * 255.0).round() as u8
                }
            }
        }
    }

    impl From<f32> for AlphaValue {
        fn from(f: f32) -> Self {
            Self::Float(f)
        }
    }

    impl From<f64> for AlphaValue {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn from(f: f64) -> Self {
            Self::Float(f as f32)
        }
    }

    impl From<u8> for AlphaValue {
        fn from(i: u8) -> Self {
            Self::Integer(i)
        }
    }

    impl From<i32> for AlphaValue {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn from(i: i32) -> Self {
            // Clamp to 0-255 range
            let clamped = i.clamp(0, 255);
            Self::Integer(clamped as u8)
        }
    }

    impl From<i64> for AlphaValue {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn from(i: i64) -> Self {
            // Clamp to 0-255 range
            let clamped = i.clamp(0, 255);
            Self::Integer(clamped as u8)
        }
    }

    impl From<u16> for AlphaValue {
        fn from(i: u16) -> Self {
            // Clamp to 0-255 range
            let clamped = i.min(255);
            Self::Integer(clamped as u8)
        }
    }

    impl From<u32> for AlphaValue {
        fn from(i: u32) -> Self {
            // Clamp to 0-255 range
            let clamped = i.min(255);
            Self::Integer(clamped as u8)
        }
    }

    impl From<u64> for AlphaValue {
        fn from(i: u64) -> Self {
            // Clamp to 0-255 range
            let clamped = i.min(255);
            Self::Integer(clamped as u8)
        }
    }

    impl From<&str> for AlphaValue {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn from(s: &str) -> Self {
            let trimmed = s.trim();
            trimmed.strip_suffix('%').map_or_else(
                || {
                    trimmed.parse::<f32>().map_or(Self::Integer(0), |f| {
                        if f <= 1.0 {
                            Self::Float(f)
                        } else {
                            Self::Integer(f.clamp(0.0, 255.0) as u8)
                        }
                    })
                },
                |percent_str| {
                    percent_str
                        .parse::<f32>()
                        .map_or(Self::Percentage(0.0), Self::Percentage)
                },
            )
        }
    }

    impl From<String> for AlphaValue {
        fn from(s: String) -> Self {
            Self::from(s.as_str())
        }
    }

    // Helper trait for converting RGB values from various numeric types to u8
    pub trait ToRgbValue {
        fn to_rgb_value(self) -> u8;
    }

    impl ToRgbValue for u8 {
        fn to_rgb_value(self) -> u8 {
            self
        }
    }

    impl ToRgbValue for i32 {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn to_rgb_value(self) -> u8 {
            // Clamp to 0-255 range
            self.clamp(0, 255) as u8
        }
    }

    impl ToRgbValue for i64 {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn to_rgb_value(self) -> u8 {
            // Clamp to 0-255 range
            self.clamp(0, 255) as u8
        }
    }

    impl ToRgbValue for u16 {
        fn to_rgb_value(self) -> u8 {
            // Clamp to 0-255 range
            self.min(255) as u8
        }
    }

    impl ToRgbValue for u32 {
        fn to_rgb_value(self) -> u8 {
            // Clamp to 0-255 range
            self.min(255) as u8
        }
    }

    impl ToRgbValue for u64 {
        fn to_rgb_value(self) -> u8 {
            // Clamp to 0-255 range
            self.min(255) as u8
        }
    }

    impl ToRgbValue for f32 {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn to_rgb_value(self) -> u8 {
            // Clamp to 0-255 range
            self.clamp(0.0, 255.0).round() as u8
        }
    }

    impl ToRgbValue for f64 {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn to_rgb_value(self) -> u8 {
            // Clamp to 0-255 range
            self.clamp(0.0, 255.0).round() as u8
        }
    }
}
