#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # Generic Pipelines AST
//!
//! Abstract syntax tree types for representing workflow definitions in a unified format.
//! This crate provides the core data structures for workflows, jobs, steps, and expressions
//! that can be parsed from various CI/CD formats and executed locally or translated to
//! different backend formats.

/// Re-exported [`serde_yaml`] crate for convenience when working with YAML workflow files.
///
/// This allows users to serialize and deserialize workflow definitions without adding
/// `serde_yaml` as a separate dependency.
pub use serde_yaml;

mod expression;
mod step;
mod workflow;

pub use expression::*;
pub use step::*;
pub use workflow::*;
