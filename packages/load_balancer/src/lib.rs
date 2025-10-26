//! HTTP/HTTPS load balancer built on the Pingora framework.
//!
//! This crate provides a reverse proxy load balancer that routes incoming HTTP/HTTPS requests
//! to upstream servers based on hostname matching. It uses round-robin selection for distributing
//! requests across multiple upstream servers and includes health checking capabilities.
//!
//! # Features
//!
//! * Hostname-based routing with wildcard fallback support
//! * Round-robin load balancing across upstream servers
//! * TCP health checks for upstream availability
//! * TLS/HTTPS support with configurable certificates
//! * ACME challenge request handling for Let's Encrypt
//!
//! # Environment Configuration
//!
//! The load balancer is configured via environment variables:
//!
//! * `CLUSTERS` - Semicolon-separated list of hostname:upstream mappings
//! * `PORT` - HTTP port (default: 6188)
//! * `SSL_PORT` - HTTPS port (default: 6189)
//! * `SSL_CRT_PATH` - TLS certificate path (default: `/etc/pingora/ssl/tls.crt`)
//! * `SSL_KEY_PATH` - TLS private key path (default: `/etc/pingora/ssl/tls.key`)
//!
//! # Example
//!
//! ```rust,no_run
//! use std::collections::BTreeMap;
//! use std::sync::Arc;
//! use moosicbox_load_balancer::Router;
//! use pingora_load_balancing::{LoadBalancer, selection::RoundRobin};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create load balancers for upstream servers
//! let upstreams = ["192.168.1.10:8080", "192.168.1.11:8080"];
//! let lb = Arc::new(LoadBalancer::try_from_iter(&upstreams)?);
//!
//! // Create a router mapping hostnames to load balancers
//! let mut map = BTreeMap::new();
//! map.insert("example.com".to_string(), lb.clone());
//! map.insert("*".to_string(), lb); // Wildcard fallback
//!
//! let router = Router::new(map);
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod load_balancer;
pub use load_balancer::*;
