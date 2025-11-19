//! Node.js package manager utilities.
//!
//! This module provides utilities for running Node.js package manager commands
//! (pnpm, bun, or npm) with automatic detection and fallback support based on
//! enabled features.

use std::{
    path::{Path, PathBuf},
    process::Command,
    str::FromStr as _,
    sync::LazyLock,
};

use switchy_env::var;

/// Available npm package manager commands.
static NPM_COMMANDS: [&str; 3] = ["pnpm", "bun", "npm"];

/// List of enabled npm package managers based on feature flags.
///
/// This is computed at runtime based on which package manager features are enabled.
static ENABLED_NPM_COMMANDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    NPM_COMMANDS
        .iter()
        .filter(|x| match **x {
            #[cfg(feature = "pnpm")]
            "pnpm" => true,
            #[cfg(feature = "bun")]
            "bun" => true,
            #[cfg(feature = "npm")]
            "npm" => true,
            _ => false,
        })
        .map(ToString::to_string)
        .collect::<Vec<_>>()
});

/// Runs an npm command using the first available package manager.
///
/// Tries package managers in this order based on enabled features: pnpm, bun, npm.
///
/// # Panics
///
/// Panics if no enabled package manager binary is found or if the command fails.
pub fn run_npm_command(arguments: &[&str], dir: &Path) {
    run_command(ENABLED_NPM_COMMANDS.clone().into_iter(), arguments, dir);
}

/// Runs a command using the first available binary from the provided list.
///
/// Tries each binary in sequence until one successfully executes. Handles special
/// path resolution for pnpm via the `PNPM_HOME` environment variable and adjusts
/// binary names for Windows compatibility.
///
/// # Panics
///
/// * Panics if no binary in the list is found or executes successfully.
/// * Panics if the command executes but returns a non-zero exit status (except 127 which indicates binary not found).
pub(crate) fn run_command(binaries: impl Iterator<Item = String>, arguments: &[&str], dir: &Path) {
    for ref binary in binaries
        .map(|x| PathBuf::from_str(&x).unwrap())
        .map(|x| {
            if x.file_name().is_some_and(|x| x == "pnpm")
                && let Ok(pnpm_home) = var("PNPM_HOME")
            {
                return PathBuf::from_str(&pnpm_home).unwrap().join(x);
            }

            x
        })
        .map(fixup_binary_filename)
        .map(|x| x.to_str().unwrap().to_string())
    {
        let mut command = Command::new(binary);
        let mut command = command.current_dir(dir);

        for arg in arguments {
            command = command.arg(arg);
        }

        println!("Running {binary} {}", arguments.join(" "));

        match command.spawn() {
            Ok(mut child) => {
                let status = child
                    .wait()
                    .unwrap_or_else(|e| panic!("Failed to execute {binary} script: {e:?}"));

                if !status.success() {
                    if status.code() == Some(127) {
                        println!("Binary {binary} not found (status code 127)");
                        continue;
                    }

                    panic!("{binary} script failed: status_code={:?}", status.code());
                }

                return;
            }
            Err(e) => {
                if let std::io::ErrorKind::NotFound = e.kind() {
                    println!("Binary {binary} not found");
                    continue;
                }
                panic!("Failed to execute {binary} script: {e:?}");
            }
        }
    }

    panic!("Failed to execute script for any of the binaries");
}

/// Adjusts binary filename for Windows compatibility.
///
/// On Windows, checks if a `.CMD` version of the binary exists in the same directory
/// and returns that path if found. Otherwise returns the original path.
#[must_use]
fn fixup_binary_filename(binary: PathBuf) -> PathBuf {
    if cfg!(windows) {
        let parent = binary.parent();

        if let Some(parent) = parent {
            let cmd = parent.join(format!(
                "{}.CMD",
                binary.file_name().unwrap().to_str().unwrap()
            ));

            if cmd.is_file() {
                return cmd;
            }
        }
    }

    binary
}
