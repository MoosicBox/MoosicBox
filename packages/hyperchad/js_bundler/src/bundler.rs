use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::LazyLock;

pub static MANIFEST_DIR_STR: &str = env!("CARGO_MANIFEST_DIR");
pub static MANIFEST_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| PathBuf::from_str(MANIFEST_DIR_STR).unwrap());

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
