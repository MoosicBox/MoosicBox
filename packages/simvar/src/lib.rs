//! Deterministic simulation test harness for concurrent systems.
//!
//! This crate is a facade that re-exports the core simulation testing framework from
//! [`simvar_harness`] and optional utilities from [`simvar_utils`]. It provides a unified
//! interface for writing deterministic tests of distributed systems and concurrent applications.
//!
//! # Features
//!
//! * **Deterministic execution** - Same seed produces identical simulation results
//! * **Host and client actors** - Model persistent services (hosts) and ephemeral clients
//! * **Simulation lifecycle hooks** - Customize behavior at key points via [`SimBootstrap`]
//! * **Built-in TUI** - Optional terminal UI for monitoring simulation progress (with `tui` feature)
//! * **Parallel execution** - Run multiple simulation runs concurrently
//! * **Cancellation support** - Graceful shutdown with Ctrl-C handling
//!
//! # Example
//!
//! ```rust,no_run
//! use simvar::{run_simulation, SimBootstrap, Sim, SimConfig};
//!
//! struct MyBootstrap;
//!
//! impl SimBootstrap for MyBootstrap {
//!     fn build_sim(&self, config: SimConfig) -> SimConfig {
//!         config
//!     }
//!
//!     fn on_start(&self, sim: &mut impl Sim) {
//!         // Spawn a host actor
//!         sim.host("server", || async {
//!             // Server logic here
//!             Ok(())
//!         });
//!
//!         // Spawn a client actor
//!         sim.client("client", async {
//!             // Client logic here
//!             Ok(())
//!         });
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let results = run_simulation(MyBootstrap)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Feature Flags
//!
//! * `all` (default) - Enable all features
//! * `async` - Async runtime support
//! * `database` - Database simulation
//! * `explore` - Model checking (the `explore` module): exhaustive and random-walk exploration
//! * `fs` - Filesystem simulation
//! * `http` - HTTP client/server simulation
//! * `mdns` - mDNS simulation
//! * `random` - Random number generation simulation
//! * `tcp` - TCP connection simulation
//! * `telemetry` - Telemetry support
//! * `time` - Time simulation
//! * `tui` - Terminal UI for simulation visualization
//! * `upnp` - `UPnP` simulation
//! * `utils` - Simulation utilities module
//! * `web-server` - Web server simulation
//! * `pretty_env_logger` - Pretty logging output
//!
//! # Environment Variables
//!
//! * `SIMULATOR_RUNS` - Number of simulation runs to execute (default: 1), or `forever` to
//!   run until stopped (only failed runs' results are then kept)
//! * `SIMULATOR_MAX_PARALLEL` - Maximum parallel runs (default: number of CPUs)
//! * `SIMULATOR_ON_FAILURE` - `stop` (the default) ends the simulation at the first failed run;
//!   `continue` records and reports it and keeps starting runs
//! * `NO_TUI` - Disable terminal UI when set
//!
//! See [`simvar_harness`] for the rest, and [`control`] for steering a running simulation:
//! pausing, resizing and stopping it, and replaying or shrinking earlier runs in-process.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Simulation utilities module.
///
/// Provides utility functions for managing worker threads and cancellation tokens
/// in simulation environments. Includes thread-local and global cancellation support
/// for gracefully terminating simulations and async operations.
///
/// Requires the `utils` feature flag.
#[cfg(feature = "utils")]
pub use simvar_utils as utils;

/// Model checking: exhaustive (breadth- and depth-first) and seeded random-walk exploration
/// of a [`explore::Model`]'s state space, checking its safety, reachability and bounded
/// liveness properties.
///
/// Requires the `explore` feature flag.
#[cfg(feature = "explore")]
pub use simvar_explore as explore;

/// Core simulation harness APIs.
///
/// Re-exports all public types, traits, and functions from [`simvar_harness`],
/// including [`run_simulation`], [`Sim`], [`SimBootstrap`], and [`SimConfig`].
pub use simvar_harness::*;
