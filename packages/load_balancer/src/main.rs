//! Binary entry point for the `MoosicBox` load balancer.
//!
//! This executable starts the Pingora-based HTTP/HTTPS load balancer server.
//! See the [`moosicbox_load_balancer`] crate documentation for configuration details.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod server;

fn main() {
    server::serve();
}
