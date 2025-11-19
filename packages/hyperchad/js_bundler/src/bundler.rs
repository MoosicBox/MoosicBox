//! Core bundling functionality with automatic bundler selection.
//!
//! This module provides the main bundling interface that automatically delegates
//! to either the SWC or esbuild bundler based on enabled features. SWC is preferred
//! when both features are enabled.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::LazyLock;

/// The manifest directory path as a string slice.
pub static MANIFEST_DIR_STR: &str = env!("CARGO_MANIFEST_DIR");

/// The manifest directory as a `PathBuf`.
///
/// # Panics
///
/// Panics if the manifest directory path cannot be converted to a `PathBuf`.
pub static MANIFEST_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| PathBuf::from_str(MANIFEST_DIR_STR).unwrap());

/// Bundles a JavaScript or TypeScript file using the configured bundler.
///
/// This function delegates to either the SWC or esbuild bundler based on which
/// feature is enabled. SWC is preferred if both features are enabled.
///
/// # Errors
///
/// This function does not return errors directly, but the underlying bundlers may panic
/// on failure conditions. See the individual bundler implementations for details.
///
/// # Panics
///
/// Panics if neither the `swc` nor `esbuild` feature is enabled.
pub fn bundle(target: &Path, out: &Path) {
    if cfg!(feature = "swc") {
        #[cfg(feature = "swc")]
        return crate::swc::bundle(target, out, true);
    } else if cfg!(feature = "esbuild") {
        #[cfg(feature = "esbuild")]
        return crate::esbuild::bundle(target, out);
    }

    unreachable!();
}
