//! API types for scan path configuration.
//!
//! This module provides serializable types used for configuring file system
//! paths to be scanned.

#![allow(clippy::module_name_repetitions)]

use serde::{Deserialize, Serialize};

/// Represents a file system path to be scanned.
///
/// This type is used to specify directories or files that should be included
/// in scanning operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiScanPath {
    /// The file system path to scan.
    pub path: String,
}
