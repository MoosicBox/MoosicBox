//! `MoosicBox` `UPnP` player implementation for controlling media playback on UPnP/DLNA devices.
//!
//! This crate provides the `MoosicBox` integration layer on top of `switchy_upnp`, including:
//!
//! - **Player**: `UPnP` player implementation that integrates with `MoosicBox` playback system
//! - **Listener**: Event listener service for monitoring `UPnP` device state changes
//!
//! # Features
//!
//! * `api` - Actix-web API support
//! * `listener` - Event listener service for `UPnP` device monitoring
//! * `openapi` - OpenAPI/utoipa schema support
//! * `player` - `UPnP` player implementation
//! * `simulator` - Simulated `UPnP` devices for testing

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "listener")]
pub mod listener;
#[cfg(feature = "player")]
pub mod player;
