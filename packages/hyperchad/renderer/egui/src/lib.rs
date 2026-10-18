//! Egui-based renderer for `HyperChad` applications.
//!
//! This crate provides desktop rendering capabilities for `HyperChad` using the egui framework.
//! It implements the `hyperchad_renderer::Renderer` trait to render `HyperChad` UI elements
//! as native desktop applications through egui's immediate mode GUI system.
//!
//! # Features
//!
//! One native backend owns document and interaction state on the UI thread;
//! asynchronous producers submit ordered commands without blocking frames.
//!
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

/// The native document renderer and its event-loop runner.
pub use backend::{EguiRenderRunner, EguiRenderer};

/// Font metrics utilities for text measurement.
///
/// This module provides egui-specific implementations of font metrics,
/// enabling text measurement and layout calculations using egui's font system.
pub mod font_metrics;

/// Immutable font registry and shared measurement/painting resolver.
pub mod text_layout;

/// Layout calculation traits and utilities.
///
/// This module provides the `EguiCalc` trait that extends the base layout
/// calculation trait with egui-specific context handling for layout calculations.
pub mod layout;

pub use eframe;

/// Persistent shared-handler action execution.
pub mod action_runtime;
/// UI-owned document frame core and canonical native renderer.
pub mod backend;
/// Retained canvas command application and painting.
pub mod canvas;
/// UI-owned document reconciliation and stable element identities.
pub mod document;
/// Persistent ordered event routing and native input contracts.
pub mod events;
/// Shared calculated geometry projection.
pub mod geometry;
/// Generation-checked image resource state and worker completions.
pub mod resources;
/// Scoped asynchronous route and form completion transport.
pub mod routing;
/// Nonblocking document command transport.
pub mod runtime;
/// Projected native widgets and state adapters.
pub mod widgets;

/// Production frame host and opt-in screenshot inspection.
pub mod feedback;

/// Opt-in native observation and correlated screenshot protocol.
#[cfg(feature = "feedback")]
pub mod interaction;

#[cfg(test)]
mod facade_tests {
    #[test]
    fn canonical_root_types_are_backend_types() {
        use std::any::TypeId;

        assert_eq!(
            TypeId::of::<crate::EguiRenderer<()>>(),
            TypeId::of::<crate::backend::EguiRenderer<()>>()
        );
        assert_eq!(
            TypeId::of::<crate::EguiRenderRunner<()>>(),
            TypeId::of::<crate::backend::EguiRenderRunner<()>>()
        );
    }
}
