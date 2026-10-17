//! Terminal-native `HyperChad` rendering using bmux components and runtime.
//!
//! The renderer preserves `HyperChad` view and action semantics while adapting
//! layout and controls to a terminal cell grid. Terminal ownership belongs to
//! the standalone runner; embedded consumers retain ownership of their host.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod actions;
pub mod admission;
pub mod controls;
pub mod graphics;
pub mod layout;
pub mod presentation;
pub mod program;
pub mod renderer;
pub mod runtime;
pub mod state;

pub use renderer::BmuxRenderer;
