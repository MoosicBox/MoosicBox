//! Menu functionality for managing library content.
//!
//! This crate provides functionality for managing music library content, including
//! albums, artists, and tracks. It includes both programmatic APIs for library
//! operations and HTTP endpoint handlers for web service integration.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "api")]
pub mod api;

pub mod library;

/// Re-exported menu model types.
///
/// This module provides access to all menu-related data models including album
/// versions and API response types.
pub use moosicbox_menu_models as models;
