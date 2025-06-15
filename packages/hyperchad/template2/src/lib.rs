#![no_std]

//! A macro for writing Container templates.
//!
//! This documentation only describes the runtime API. For a general
//! guide, check out the [book] instead.
//!
//! [book]: https://hyperchad_template2.lambda.xyz/

#![doc(html_root_url = "https://docs.rs/hyperchad_template2/0.27.0")]

extern crate alloc;

use alloc::string::ToString;
use alloc::{borrow::Cow, boxed::Box, string::String, sync::Arc, vec::Vec};
use core::fmt::{Arguments, Write};

pub use hyperchad_template2_macros::container;
pub use hyperchad_transformer::Container;

pub use hyperchad_actions as actions;
pub use hyperchad_color as color;
pub use hyperchad_transformer as transformer;
pub use hyperchad_transformer_models as transformer_models;

/// The result type for the container! macro.
///
/// The `container!` macro expands to an expression of this type.
pub type Containers = Vec<Container>;

/// Extension trait to add missing methods to Vec<Container>
pub trait ContainerVecExt {
    fn into_string(self) -> String;
    fn to_string(&self) -> String;
}

impl ContainerVecExt for Vec<Container> {
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
/// use hyperchad_template2::{container, Containers, RenderContainer};
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
///             Button id=(if self.primary { "primary" } else { "secondary" }) {
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

// New wrapper types to avoid orphan rule violations
#[derive(Debug, Clone)]
pub struct ContainerList(pub Vec<Container>);

impl ContainerList {
    pub fn new(containers: Vec<Container>) -> Self {
        Self(containers)
    }

    pub fn iter(&self) -> core::slice::Iter<'_, Container> {
        self.0.iter()
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
