#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
// Allow for utoipa::OpenApi derive macro generated code
#![allow(clippy::needless_for_each)]

//! HTTP API layer for music service provider management.
//!
//! This crate provides REST API endpoints and data models for managing music service
//! providers (such as Qobuz, Tidal, etc.) in `MoosicBox`. It handles authentication,
//! library scanning, and search operations across multiple music APIs.
//!
//! # Features
//!
//! * List and query enabled music APIs for a profile
//! * Authenticate with music services (username/password or OAuth polling)
//! * Enable and trigger library scanning
//! * Search across multiple music APIs
//!
//! # Modules
//!
//! * [`api`] - HTTP endpoint handlers for Actix-Web (requires `api` feature)
//! * [`models`] - Data models for API request/response types

#[cfg(feature = "api")]
pub mod api;

pub mod models;
