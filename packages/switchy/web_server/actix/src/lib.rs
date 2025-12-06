//! Actix Web backend for `switchy_web_server`.
//!
//! This crate provides the Actix Web implementation for the `switchy_web_server` framework.
//! It includes HTTP request handling, server building, and optional static file serving.
//!
//! # Features
//!
//! * `cors` - Enable CORS (Cross-Origin Resource Sharing) support
//! * `htmx` - Enable HTMX middleware
//! * `static-files` - Enable static file serving via `actix-files`
//! * `tls` - Enable TLS/SSL support
//!
//! # Example
//!
//! ```rust,ignore
//! use switchy_web_server::{WebServerBuilder, Scope, HttpResponse};
//! use switchy_web_server_actix::WebServerBuilderActixExt;
//!
//! let server = WebServerBuilder::new()
//!     .with_scope(Scope::new("/api").get("/hello", |_| {
//!         Box::pin(async { Ok(HttpResponse::text("Hello!")) })
//!     }))
//!     .build_actix();
//!
//! // server.start().await;
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod error;
mod request;
mod server;

#[cfg(feature = "static-files")]
mod static_files;

pub use error::{IntoActixError, TryIntoWebServerError, into_actix_error, try_from_actix_error};
pub use request::ActixRequest;
pub use server::{ActixWebServer, WebServerBuilderActixExt};

#[cfg(feature = "static-files")]
pub use static_files::StaticFilesExt;

// Re-export actix-web for users who need direct access
pub use actix_web;
