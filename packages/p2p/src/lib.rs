//! P2P communication abstraction system
//!
//! This crate provides a generic abstraction layer for peer-to-peer networking,
//! supporting multiple underlying implementations through a trait-based design.
//!
//! # Features
//!
//! * **Generic P2P traits** - [`P2PSystem`], [`P2PConnection`], and [`P2PNodeId`] traits
//!   provide implementation-agnostic interfaces for P2P communication
//! * **Network simulator** - A complete P2P network simulator with realistic network
//!   conditions including latency, packet loss, and network partitions (enabled with
//!   the `simulator` feature, on by default)
//! * **Type-safe error handling** - Comprehensive error types via [`P2PError`]
//!
//! # Getting Started
//!
//! ```rust,no_run
//! # #[cfg(feature = "simulator")]
//! # async fn example() {
//! use switchy_p2p::simulator::SimulatorP2P;
//!
//! // Create a new P2P node with deterministic ID for testing
//! let node = SimulatorP2P::with_seed("alice");
//! let node_id = node.local_node_id().clone();
//!
//! println!("Node ID: {}", node_id.fmt_short());
//! # }
//! ```
//!
//! # Main Entry Points
//!
//! * [`traits`] - Core P2P traits for implementing or using P2P systems
//! * [`types`] - Error types and type aliases
//! * [`simulator`] - Network simulator implementation (requires `simulator` feature)
//!
//! [`P2PSystem`]: traits::P2PSystem
//! [`P2PConnection`]: traits::P2PConnection
//! [`P2PNodeId`]: traits::P2PNodeId
//! [`P2PError`]: types::P2PError

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

// Phase 2.1: First module added
#[cfg(feature = "simulator")]
pub mod simulator;

// Phase 3.1: Core types and traits
pub mod traits;
pub mod types;

// Modules will be added in later phases:
// - Phase 4.1: (extend types with thiserror)
// - Phase 5.1: mod router;
