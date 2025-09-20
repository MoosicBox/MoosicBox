#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # Generic Pipelines AST
//!
//! Abstract syntax tree types for representing workflow definitions in a unified format.
//! This crate provides the core data structures for workflows, jobs, steps, and expressions
//! that can be parsed from various CI/CD formats and executed locally or translated to
//! different backend formats.

// Re-export for convenience
pub use serde_yaml;

mod expression;
mod step;
mod workflow;

pub use expression::*;
pub use step::*;
pub use workflow::*;
