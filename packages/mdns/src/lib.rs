//! `MoosicBox` mDNS service scanner for discovering `MoosicBox` servers on the network.
//!
//! This crate provides functionality to scan the local network for `MoosicBox` servers
//! using mDNS service discovery.
//!
//! # Features
//!
//! * `simulator` - Provides a simulated mDNS daemon for testing purposes
//!
//! # Examples
//!
//! Scanning for `MoosicBox` servers:
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use moosicbox_mdns::scanner::{MoosicBox, Context, service};
//! use moosicbox_async_service::Service as _;
//!
//! // Create a channel to receive discovered servers
//! let (tx, rx) = kanal::unbounded_async();
//!
//! // Create and start the scanner service
//! let scanner = service::Service::new(Context::new(tx));
//! let _handle = scanner.start();
//!
//! // Process discovered servers as they arrive
//! while let Ok(server) = rx.recv().await {
//!     println!("Found server: {} at {}", server.name, server.host);
//! }
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// mDNS service scanner for discovering MoosicBox servers on the network.
pub mod scanner;
