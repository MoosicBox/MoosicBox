//! Cargo workspace support.
//!
//! This module provides the `Workspace` trait implementation for Cargo (Rust)
//! workspaces, enabling:
//!
//! - Workspace member discovery from `Cargo.toml`
//! - Package dependency analysis
//! - `Cargo.lock` parsing and diff analysis
//!
//! # Example
//!
//! ```rust,ignore
//! use clippier::workspace::cargo::CargoWorkspace;
//! use clippier::workspace::Workspace;
//!
//! async fn analyze() -> Result<(), Box<dyn std::error::Error>> {
//!     let workspace = CargoWorkspace::new(Path::new(".")).await?;
//!     
//!     for pkg in workspace.packages().await? {
//!         println!("Package: {} v{:?}", pkg.name(), pkg.version());
//!     }
//!     
//!     Ok(())
//! }
//! ```

mod context;
mod lockfile;
mod package;

pub use context::CargoWorkspace;
pub use lockfile::{CargoLockDiffParser, CargoLockEntry, CargoLockfile};
pub use package::{CargoPackage, parse_dependencies, read_package_name, read_package_version};
