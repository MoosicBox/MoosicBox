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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_dir_str_is_not_empty() {
        // MANIFEST_DIR_STR should contain a valid path
        assert!(!MANIFEST_DIR_STR.is_empty());
        assert!(MANIFEST_DIR_STR.contains("hyperchad"));
        assert!(MANIFEST_DIR_STR.contains("js_bundler"));
    }

    #[test]
    fn test_manifest_dir_is_valid_path() {
        // MANIFEST_DIR should be a valid, existing directory
        assert!(MANIFEST_DIR.exists());
        assert!(MANIFEST_DIR.is_dir());
        assert!(MANIFEST_DIR.ends_with("js_bundler"));
    }

    #[test]
    fn test_manifest_dir_contains_cargo_toml() {
        // The manifest directory should contain Cargo.toml
        let cargo_toml = MANIFEST_DIR.join("Cargo.toml");
        assert!(cargo_toml.exists());
        assert!(cargo_toml.is_file());
    }
}
