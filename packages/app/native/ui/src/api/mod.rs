//! Music API integration utilities.
//!
//! This module provides functions for interacting with music streaming services
//! and managing API authentication and scanning.

pub mod tidal;

/// Re-exports Tidal API integration functions.
pub use tidal::*;
