//! Actix-web middleware components for `MoosicBox` applications.
//!
//! This crate provides reusable middleware and configuration utilities for building
//! `MoosicBox` web services with Actix-web:
//!
//! * [`api_logger`] - Request/response logging middleware
//! * [`service_info`] - Service configuration accessible via request extraction
//! * [`tunnel_info`] - Tunnel configuration accessible via request extraction (requires `tunnel` feature)
//!
//! # Example
//!
//! ```rust
//! use actix_web::{App, HttpServer};
//! use moosicbox_middleware::api_logger::ApiLogger;
//! use moosicbox_middleware::service_info::ServiceInfo;
//!
//! # async fn example() -> std::io::Result<()> {
//! // Initialize service configuration
//! moosicbox_middleware::service_info::init(ServiceInfo { port: 8080 })
//!     .expect("Failed to initialize service info");
//!
//! // Create server with middleware
//! HttpServer::new(|| {
//!     App::new()
//!         .wrap(ApiLogger::new())
//!         // ... add your routes
//! })
//! .bind(("127.0.0.1", 8080))?
//! .run()
//! # ;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod api_logger;
pub mod service_info;

#[cfg(feature = "tunnel")]
pub mod tunnel_info;
