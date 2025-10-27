//! JavaScript and TypeScript bundling using esbuild.
//!
//! This module provides functionality to bundle JavaScript and TypeScript files
//! using esbuild as the underlying bundler. It handles npm dependency installation
//! and executes esbuild with minification and bundling enabled.

use std::path::Path;

use crate::{
    MANIFEST_DIR,
    node::{run_command, run_npm_command},
};

/// Bundles a JavaScript or TypeScript file using esbuild.
///
/// This function installs npm dependencies if needed, then runs esbuild
/// with minification and bundling enabled.
///
/// # Panics
///
/// Panics if the npm install or esbuild command fails to execute.
pub fn bundle(target: &Path, out: &Path) {
    run_npm_command(&["install"], &MANIFEST_DIR);
    run_command(
        std::iter::once(
            MANIFEST_DIR
                .join("node_modules")
                .join(".bin")
                .join("esbuild")
                .to_str()
                .unwrap()
                .to_string(),
        ),
        &[
            target.to_str().unwrap(),
            "--minify",
            "--bundle",
            &format!("--outfile={}", out.display()),
        ],
        &MANIFEST_DIR,
    );
}
