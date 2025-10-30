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
