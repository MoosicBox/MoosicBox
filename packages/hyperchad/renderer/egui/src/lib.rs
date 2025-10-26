//! Egui-based renderer for `HyperChad` applications.
//!
//! This crate provides desktop rendering capabilities for `HyperChad` using the egui framework.
//! It implements the `hyperchad_renderer::Renderer` trait to render `HyperChad` UI elements
//! as native desktop applications through egui's immediate mode GUI system.
//!
//! # Features
//!
//! * **v1** - Version 1 renderer implementation (enabled by default)
//! * **v2** - Version 2 renderer implementation (enabled by default)
//! * **wgpu** - Use wgpu backend for rendering (enabled by default)
//! * **glow** - Use OpenGL backend for rendering
//! * **wayland** - Enable Wayland support on Linux
//! * **x11** - Enable X11 support on Linux
//!
//! # Example
//!
//! ```rust,no_run
//! use hyperchad_renderer_egui::{EguiRenderer, layout::EguiCalc};
//! use hyperchad_router::Router;
//! use hyperchad_router::ClientInfo;
//! use std::sync::Arc;
//!
//! # fn example<C: EguiCalc + Clone + Send + Sync + 'static>(
//! #     router: Router,
//! #     calculator: C,
//! # ) {
//! let (tx, _rx) = flume::unbounded();
//! let (resize_tx, _resize_rx) = flume::unbounded();
//! let client_info = Arc::new(ClientInfo::default());
//!
//! let renderer = EguiRenderer::new(
//!     router,
//!     tx,
//!     resize_tx,
//!     client_info,
//!     calculator,
//! );
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Version 1 of the egui renderer implementation.
///
/// This module provides the first generation egui-based renderer for `HyperChad`.
/// It includes the core renderer types and implements the `Renderer` trait for
/// rendering `HyperChad` UI elements using egui.
#[cfg(feature = "v1")]
pub mod v1;

#[cfg(all(feature = "v1", not(feature = "v2")))]
pub use v1::*;

/// Version 2 of the egui renderer implementation.
///
/// This module provides the second generation egui-based renderer for `HyperChad`.
/// It includes improved rendering capabilities and updated implementations of the
/// `Renderer` trait for rendering `HyperChad` UI elements using egui.
#[cfg(any(feature = "v2", not(feature = "v1")))]
pub mod v2;

#[cfg(any(feature = "v2", not(feature = "v1")))]
pub use v2::*;

/// Font metrics utilities for text measurement.
///
/// This module provides egui-specific implementations of font metrics,
/// enabling text measurement and layout calculations using egui's font system.
pub mod font_metrics;

/// Layout calculation traits and utilities.
///
/// This module provides the `EguiCalc` trait that extends the base layout
/// calculation trait with egui-specific context handling for layout calculations.
pub mod layout;

pub use eframe;
