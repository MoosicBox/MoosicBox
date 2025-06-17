#![no_std]

//! A macro for writing Container templates.
//!
//! This documentation only describes the runtime API. For a general
//! guide, check out the [book] instead.
//!
//! [book]: https://hyperchad_template.lambda.xyz/

#![doc(html_root_url = "https://docs.rs/hyperchad_template/0.27.0")]

extern crate alloc;

use alloc::string::ToString;
use alloc::{borrow::Cow, boxed::Box, string::String, sync::Arc, vec::Vec};
use core::fmt::{Arguments, Write};

pub use hyperchad_template_macros::container;
pub use hyperchad_transformer::Container;

pub use hyperchad_actions as actions;
pub use hyperchad_color as color;
pub use hyperchad_transformer as transformer;
pub use hyperchad_transformer_models as transformer_models;

/// Prelude module that re-exports commonly used traits.
///
/// This module is automatically imported when you use `hyperchad_template::container`,
/// so you don't need to manually import these traits.
pub mod prelude {
    pub use crate::{
        ContainerVecExt, ContainerVecMethods, IntoActionEffect, IntoBorder, ToBool, calc,
        color as hyperchad_color, transformer as hyperchad_transformer,
        transformer_models as hyperchad_transformer_models,
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
    fn display_to_string(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>>;

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
        self.iter().map(|c| c.to_string()).collect::<String>()
    }

    fn into_string(self) -> String {
        self.iter().map(|c| c.to_string()).collect::<String>()
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
pub fn to_html(containers: &[Container]) -> String {
    containers.iter().map(|c| c.to_string()).collect::<String>()
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
/// let html = into_html(containers);
/// ```
pub fn into_html(containers: Vec<Container>) -> String {
    containers.iter().map(|c| c.to_string()).collect::<String>()
}

/// Extension trait to add missing methods to Vec<Container>
pub trait ContainerVecExt {
    fn display_to_string(
        &self,
        with_debug_attrs: bool,
        wrap_raw_in_element: bool,
    ) -> Result<String, Box<dyn core::error::Error>>;

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
        self.iter().map(|c| c.to_string()).collect::<String>()
    }

    fn to_string(&self) -> String {
        self.iter().map(|c| c.to_string()).collect::<String>()
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

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error>;

    fn render_to_string(&self) -> Result<String, Self::Error> {
        let mut containers = Vec::new();
        self.render_to(&mut containers)?;
        Ok(containers.iter().map(|c| c.to_string()).collect::<String>())
    }

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
        containers.push(Container {
            element: hyperchad_transformer::Element::Raw {
                value: self.to_string(),
            },
            ..Default::default()
        });
        Ok(())
    }
}

impl RenderContainer for String {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
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
        match self {
            Some(value) => value.render_to(containers),
            None => Ok(()),
        }
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
    pub fn new(containers: Vec<Container>) -> Self {
        Self(containers)
    }

    /// Convert into a String representation (HTML)
    pub fn into_string(self) -> String {
        self.0.iter().map(|c| c.to_string()).collect::<String>()
    }

    pub fn iter(&self) -> core::slice::Iter<'_, Container> {
        self.0.iter()
    }

    /// Get the inner Vec<Container>
    pub fn into_inner(self) -> Vec<Container> {
        self.0
    }

    /// Get a reference to the inner Vec<Container>
    pub fn as_inner(&self) -> &Vec<Container> {
        &self.0
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

/// Trait for converting values to bool (to handle IfExpression<bool, Responsive>)
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

/// Trait for converting various types to ActionEffect.
/// This handles the conversion chain properly without violating the orphan rule.
pub trait IntoActionEffect {
    fn into_action_effect(self) -> actions::ActionEffect;
}

impl IntoActionEffect for actions::Action {
    fn into_action_effect(self) -> actions::ActionEffect {
        self.action
    }
}

impl IntoActionEffect for actions::ActionEffect {
    fn into_action_effect(self) -> actions::ActionEffect {
        self
    }
}

impl IntoActionEffect for actions::ActionType {
    fn into_action_effect(self) -> actions::ActionEffect {
        self.into()
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
            hyperchad_transformer::Number::Integer(self.1 as i64),
        )
    }
}

impl IntoBorder for (hyperchad_color::Color, u16) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            self.0,
            hyperchad_transformer::Number::Integer(self.1 as i64),
        )
    }
}

impl IntoBorder for (hyperchad_color::Color, f32) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (self.0, hyperchad_transformer::Number::Real(self.1))
    }
}

impl IntoBorder for (hyperchad_color::Color, f64) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (self.0, hyperchad_transformer::Number::Real(self.1 as f32))
    }
}

impl IntoBorder for (i32, hyperchad_color::Color) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            self.1,
            hyperchad_transformer::Number::Integer(self.0 as i64),
        )
    }
}

impl IntoBorder for (u16, hyperchad_color::Color) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            self.1,
            hyperchad_transformer::Number::Integer(self.0 as i64),
        )
    }
}

impl IntoBorder for (f32, hyperchad_color::Color) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (self.1, hyperchad_transformer::Number::Real(self.0))
    }
}

impl IntoBorder for (f64, hyperchad_color::Color) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (self.1, hyperchad_transformer::Number::Real(self.0 as f32))
    }
}

impl IntoBorder for (i32, &str) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.1),
            hyperchad_transformer::Number::Integer(self.0 as i64),
        )
    }
}

impl IntoBorder for (u16, &str) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.1),
            hyperchad_transformer::Number::Integer(self.0 as i64),
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
            hyperchad_transformer::Number::Integer(self.1 as i64),
        )
    }
}

impl IntoBorder for (&str, u16) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(self.0),
            hyperchad_transformer::Number::Integer(self.1 as i64),
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
            hyperchad_transformer::Number::Integer(self.0 as i64),
        )
    }
}

impl IntoBorder for (u16, String) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.1),
            hyperchad_transformer::Number::Integer(self.0 as i64),
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
            hyperchad_transformer::Number::Integer(self.1 as i64),
        )
    }
}

impl IntoBorder for (String, u16) {
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.0),
            hyperchad_transformer::Number::Integer(self.1 as i64),
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
    fn into_border(self) -> (hyperchad_color::Color, hyperchad_transformer::Number) {
        (
            hyperchad_color::Color::from_hex(&self.0),
            hyperchad_transformer::Number::Real(self.1 as f32),
        )
    }
}

/// Helper module for calc() expressions and mathematical operations on Number types
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
    pub fn add_numbers(left: Number, right: Number) -> Number {
        // For now, use a simple approach that converts both to common units if possible
        // or falls back to real numbers for calculations
        match (&left, &right) {
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
    pub fn subtract_numbers(left: Number, right: Number) -> Number {
        match (&left, &right) {
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
    pub fn multiply_numbers(left: Number, right: Number) -> Number {
        match (&left, &right) {
            // When multiplying, typically one operand should be unitless
            // For now, we'll handle some common cases and fall back to pixel conversion
            (Number::Integer(a), Number::Integer(b)) => Number::Integer(a * b),
            (Number::Real(a), Number::Real(b)) => Number::Real(a * b),
            (Number::Integer(a), Number::Real(b)) => Number::Real(*a as f32 * b),
            (Number::Real(a), Number::Integer(b)) => Number::Real(a * *b as f32),

            // For units, if one is unitless, preserve the unit of the other
            (Number::IntegerPercent(a), Number::Integer(b)) => Number::IntegerPercent(a * b),
            (Number::Integer(a), Number::IntegerPercent(b)) => Number::IntegerPercent(a * b),
            (Number::RealPercent(a), Number::Real(b)) => Number::RealPercent(a * b),
            (Number::Real(a), Number::RealPercent(b)) => Number::RealPercent(a * b),

            // For more complex cases, convert to pixels and multiply
            _ => {
                let left_pixels = left.calc(100.0, 1920.0, 1080.0);
                let right_pixels = right.calc(100.0, 1920.0, 1080.0);
                Number::Real(left_pixels * right_pixels)
            }
        }
    }

    /// Divide two Number values
    pub fn divide_numbers(left: Number, right: Number) -> Number {
        match (&left, &right) {
            // Basic division
            (Number::Integer(a), Number::Integer(b)) => {
                if *b != 0 {
                    Number::Real(*a as f32 / *b as f32)
                } else {
                    Number::Real(0.0) // Avoid division by zero
                }
            }
            (Number::Real(a), Number::Real(b)) => {
                if *b != 0.0 {
                    Number::Real(a / b)
                } else {
                    Number::Real(0.0)
                }
            }
            (Number::Integer(a), Number::Real(b)) => {
                if *b != 0.0 {
                    Number::Real(*a as f32 / b)
                } else {
                    Number::Real(0.0)
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
                if *b != 0.0 {
                    Number::RealPercent(a / b)
                } else {
                    Number::RealPercent(0.0)
                }
            }

            // For more complex cases, convert to pixels and divide
            _ => {
                let left_pixels = left.calc(100.0, 1920.0, 1080.0);
                let right_pixels = right.calc(100.0, 1920.0, 1080.0);
                if right_pixels != 0.0 {
                    Number::Real(left_pixels / right_pixels)
                } else {
                    Number::Real(0.0)
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
