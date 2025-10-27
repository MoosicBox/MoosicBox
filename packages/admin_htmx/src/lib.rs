//! HTMX-based admin interface for `MoosicBox`.
//!
//! This crate provides a web-based admin UI built with HTMX for managing `MoosicBox` server
//! configuration, including profiles, music service integrations (Tidal, Qobuz), and library
//! scanning.
//!
//! The admin interface uses server-side rendering with the Maud templating library and
//! implements HTMX patterns for dynamic updates without full page reloads.
//!
//! # Features
//!
//! * `api` - Actix-web endpoints for the admin interface (enabled by default)
//! * `scan` - Local music library scanning functionality (enabled by default)
//! * `tidal` - Tidal music service integration (enabled by default)
//! * `qobuz` - Qobuz music service integration (enabled by default)
//!
//! # Main Entry Point
//!
//! The main entry point for integrating this crate into an Actix-web application is
//! [`api::bind_services`], which registers all admin endpoints on a scope.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "api")]
pub mod api;
