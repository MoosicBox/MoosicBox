//! Tool detection and execution infrastructure for linting and formatting.
//!
//! This module provides a framework for detecting, configuring, and running
//! external linting and formatting tools. It acts as an orchestrator that:
//!
//! - Detects installed tools using the `which` crate for cross-platform support
//! - Invokes tools with their native CLI interfaces
//! - Aggregates results and exit codes
//! - Reports results in a unified way
//!
//! # Design Philosophy
//!
//! Clippier acts as an **orchestrator, not a controller**. It:
//! - Delegates all actual linting/formatting to the native tools
//! - Uses tools' own configuration files (`.prettierrc`, `rustfmt.toml`, etc.)
//! - Does not try to abstract away tool-specific arguments
//! - Only provides minimal configuration for tool selection (required/skip)
//!
//! # Example
//!
//! ```rust,ignore
//! use clippier::tools::{ToolRegistry, ToolsConfig};
//!
//! let config = ToolsConfig::default();
//! let registry = ToolRegistry::new(config);
//!
//! // Run all available formatters
//! let results = registry.run_formatters(&["src/"])?;
//!
//! // Run all available linters
//! let results = registry.run_linters(&["src/"])?;
//! ```

mod registry;
mod runner;
mod types;

pub use registry::ToolRegistry;
pub use runner::{AggregatedResults, ToolResult, ToolRunner, print_summary, results_to_json};
pub use types::{Tool, ToolCapability, ToolKind, ToolsConfig};
