#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # Generic Pipelines (gpipe)
//!
//! Universal CI/CD workflow orchestration tool. Currently provides type definitions and AST.
//! Planned: execution and translation across multiple backends.
//!
//! ## Features
//!
//! ### Implemented
//! * Workflow AST - Complete abstract syntax tree types for workflow definitions
//! * Type Safety - Fully typed Rust data structures with serde support
//!
//! ### Planned
//! * Parse and execute generic workflow formats
//! * Translate workflows between different CI/CD platforms
//! * Local execution without containerization
//! * Backend-agnostic workflow definitions

/// Abstract syntax tree types for workflow definitions.
///
/// Re-exported from the `gpipe_ast` crate. Provides core data structures for
/// workflows, jobs, steps, and expressions that can be parsed from various CI/CD
/// formats and executed locally or translated to different backend formats.
///
/// Enable the `ast` feature to access this module.
#[cfg(feature = "ast")]
pub use gpipe_ast as ast;
