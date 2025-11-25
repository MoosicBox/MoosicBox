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

/// Available npm package manager commands in priority order.
///
/// The package managers are listed in order of preference: pnpm, bun, npm.
/// Actual availability is determined by enabled feature flags.
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
                if e.kind() == std::io::ErrorKind::NotFound {
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
pub(crate) fn fixup_binary_filename(binary: PathBuf) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test_log::test]
    fn test_fixup_binary_filename_non_windows() {
        // On non-Windows systems, should always return the original path
        if !cfg!(windows) {
            let binary = PathBuf::from("/usr/bin/node");
            let result = fixup_binary_filename(binary.clone());
            assert_eq!(result, binary);

            let binary_with_ext = PathBuf::from("/usr/bin/node.exe");
            let result = fixup_binary_filename(binary_with_ext.clone());
            assert_eq!(result, binary_with_ext);
        }
    }

    #[test_log::test]
    fn test_fixup_binary_filename_windows_no_cmd_exists() {
        // On Windows, if no .CMD file exists, should return original path
        if cfg!(windows) {
            let temp_dir = tempfile::tempdir().unwrap();
            let binary = temp_dir.path().join("nonexistent");
            let result = fixup_binary_filename(binary.clone());
            assert_eq!(result, binary);
        }
    }

    #[test_log::test]
    fn test_fixup_binary_filename_windows_cmd_exists() {
        // On Windows, if .CMD file exists, should return the .CMD path
        if cfg!(windows) {
            let temp_dir = tempfile::tempdir().unwrap();
            let binary = temp_dir.path().join("testbin");
            let cmd_file = temp_dir.path().join("testbin.CMD");

            // Create the .CMD file
            fs::write(&cmd_file, "").unwrap();

            let result = fixup_binary_filename(binary);
            assert_eq!(result, cmd_file);
        }
    }

    #[test_log::test]
    fn test_fixup_binary_filename_no_parent() {
        // Path with no parent directory should return original
        let binary = PathBuf::from("node");
        let result = fixup_binary_filename(binary.clone());
        assert_eq!(result, binary);
    }

    #[test_log::test]
    fn test_enabled_npm_commands_contains_valid_managers() {
        // Test that ENABLED_NPM_COMMANDS only contains valid package managers
        let valid_managers = ["pnpm", "bun", "npm"];
        for manager in ENABLED_NPM_COMMANDS.iter() {
            assert!(
                valid_managers.contains(&manager.as_str()),
                "Invalid npm manager: {manager}"
            );
        }
    }

    #[test_log::test]
    fn test_enabled_npm_commands_respects_features() {
        // Verify that enabled commands match the feature flags
        #[cfg(feature = "pnpm")]
        assert!(ENABLED_NPM_COMMANDS.contains(&"pnpm".to_string()));

        #[cfg(feature = "bun")]
        assert!(ENABLED_NPM_COMMANDS.contains(&"bun".to_string()));

        #[cfg(feature = "npm")]
        assert!(ENABLED_NPM_COMMANDS.contains(&"npm".to_string()));

        #[cfg(not(any(feature = "pnpm", feature = "bun", feature = "npm")))]
        assert!(ENABLED_NPM_COMMANDS.is_empty());
    }
}
