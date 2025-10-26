//! Search functionality and data models for global music searches.
//!
//! This module provides types and utilities for performing global searches across
//! artists, albums, and tracks. The search functionality is available behind the
//! `search` feature flag, with API-specific search models available behind the
//! `api-search` feature.

#[cfg(feature = "api-search")]
pub mod api;
