#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

//! # Generic Pipelines (gpipe)
//!
//! Universal CI/CD workflow tool that can execute and translate between different workflow formats.
//! This crate provides a unified interface for working with generic workflow definitions.
//!
//! ## Features
//! * Parse and execute generic workflow formats
//! * Translate workflows between different CI/CD platforms
//! * Local execution without containerization
//! * Backend-agnostic workflow definitions

// Re-export core types from ast
#[cfg(feature = "ast")]
pub use gpipe_ast as ast;
