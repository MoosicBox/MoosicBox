#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

// Common types and functions
pub mod common;
pub use common::*;

#[cfg(feature = "git-diff")]
pub mod git_diff;

// Only re-export the git-diff specific functions, not the common types
#[cfg(feature = "git-diff")]
pub use git_diff::{
    build_external_dependency_map, extract_changed_dependencies_from_git,
    find_packages_affected_by_external_deps, find_transitively_affected_external_deps,
};

// Make test_utils available in test mode or when explicitly enabled
// Also available when building for tests (covers integration tests)
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
