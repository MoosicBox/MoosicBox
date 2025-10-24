//! Core abstractions for web server implementations.
//!
//! This crate provides the [`WebServer`] trait, which defines a common interface
//! for web server lifecycle management across different implementations.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{future::Future, pin::Pin};

/// A trait for managing the lifecycle of a web server.
///
/// Implementors of this trait provide asynchronous methods for starting and
/// stopping web server instances.
pub trait WebServer {
    /// Starts the web server.
    ///
    /// This method initiates the web server, binding to configured addresses
    /// and beginning to accept incoming connections.
    fn start(&self) -> Pin<Box<dyn Future<Output = ()>>>;

    /// Stops the web server.
    ///
    /// This method gracefully shuts down the web server, closing active
    /// connections and releasing resources.
    fn stop(&self) -> Pin<Box<dyn Future<Output = ()>>>;
}
