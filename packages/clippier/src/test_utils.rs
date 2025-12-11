use std::path::Path;

/// Test resources and utilities for creating test workspaces.
///
/// This module provides helper functions and types for loading test workspaces,
/// creating temporary workspace structures, and loading Cargo.lock files for testing.
pub mod test_resources {
    use super::Path;
    use serde::{Deserialize, Serialize};
    use switchy_fs::TempDir;

    /// Representation of a Cargo.lock file for testing.
    ///
    /// A simplified version of Cargo.lock used in test resources, serializable
    /// to/from JSON for easy test fixture creation.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CargoLock {
        /// Cargo.lock format version (typically 3 or 4)
        pub version: u32,
        /// List of locked packages with their versions and dependencies
        pub package: Vec<CargoLockPackage>,
    }

    /// A single package entry in a test Cargo.lock file.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CargoLockPackage {
        /// Package name
        pub name: String,
        /// Package version (semver string)
        pub version: String,
        /// Package source (registry URL, git URL, or path)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub source: Option<String>,
        /// List of dependencies (name and version)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub dependencies: Option<Vec<String>>,
    }

    impl From<CargoLock> for crate::CargoLock {
        fn from(cargo_lock: CargoLock) -> Self {
            Self {
                version: cargo_lock.version,
                package: cargo_lock.package.into_iter().map(Into::into).collect(),
            }
        }
    }

    impl From<CargoLockPackage> for crate::CargoLockPackage {
        fn from(package: CargoLockPackage) -> Self {
            Self {
                name: package.name,
                version: package.version,
                source: package.source,
                dependencies: package.dependencies,
            }
        }
    }

    // Conversions for git_diff types
    #[cfg(feature = "git-diff")]
    impl From<crate::CargoLock> for crate::git_diff::CargoLock {
        fn from(cargo_lock: crate::CargoLock) -> Self {
            Self {
                version: cargo_lock.version,
                package: cargo_lock.package.into_iter().map(Into::into).collect(),
            }
        }
    }

    #[cfg(feature = "git-diff")]
    impl From<crate::CargoLockPackage> for crate::git_diff::CargoLockPackage {
        fn from(package: crate::CargoLockPackage) -> Self {
            Self {
                name: package.name,
                version: package.version,
                source: package.source,
                dependencies: package.dependencies,
            }
        }
    }

    /// Load a Cargo.lock file for `git_diff` functions
    #[cfg(feature = "git-diff")]
    #[must_use]
    pub fn load_cargo_lock_for_git_diff(
        workspace_name: &str,
        cargo_lock_name: &str,
    ) -> crate::git_diff::CargoLock {
        load_cargo_lock(workspace_name, cargo_lock_name).into()
    }

    /// Load a test workspace from the test-resources directory
    ///
    /// # Errors
    ///
    /// * If the workspace directory cannot be found
    /// * If the workspace Cargo.toml file cannot be read
    /// * If the workspace Cargo.toml file cannot be parsed
    ///
    /// # Panics
    ///
    /// * If fails to create a temporary directory
    /// * If fails to copy workspace files to the temporary directory
    /// * If fails to parse the workspace Cargo.toml file
    #[must_use]
    pub fn load_test_workspace(workspace_name: &str) -> (TempDir, Vec<String>) {
        let test_resources_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-resources")
            .join("workspaces")
            .join(workspace_name);

        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Copy workspace files to temp directory
        copy_dir_recursive(&test_resources_path, temp_dir.path())
            .expect("Failed to copy test workspace");

        // Get workspace members from the workspace Cargo.toml
        let workspace_cargo_toml = temp_dir.path().join("Cargo.toml");
        let workspace_content = switchy_fs::sync::read_to_string(&workspace_cargo_toml)
            .expect("Failed to read workspace Cargo.toml");

        let workspace_toml: toml::Value =
            toml::from_str(&workspace_content).expect("Failed to parse workspace Cargo.toml");

        let workspace_members = workspace_toml
            .get("workspace")
            .and_then(|w| w.get("members"))
            .and_then(|m| m.as_array())
            .and_then(|arr| {
                arr.iter()
                    .map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect::<Option<Vec<_>>>()
            })
            .unwrap_or_default();

        (temp_dir, workspace_members)
    }

    /// Load a Cargo.lock file from the test resources
    ///
    /// # Panics
    ///
    /// * If fails to read the Cargo.lock file
    #[must_use]
    pub fn load_cargo_lock(workspace_name: &str, cargo_lock_name: &str) -> crate::CargoLock {
        let cargo_locks_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-resources")
            .join("workspaces")
            .join(workspace_name)
            .join("cargo-locks");
        let cargo_lock_path = cargo_locks_dir.join(format!("{cargo_lock_name}.json"));

        // Seed from real filesystem when in simulator mode
        // seed_from_real_fs expects a directory, so seed the cargo-locks directory
        switchy_fs::seed_from_real_fs_same_path(&cargo_locks_dir)
            .expect("Failed to seed cargo locks directory");

        let cargo_lock_content =
            switchy_fs::sync::read_to_string(&cargo_lock_path).unwrap_or_else(|e| {
                panic!(
                    "Failed to read cargo lock file {}: {}",
                    cargo_lock_path.display(),
                    e
                )
            });

        let cargo_lock: CargoLock = serde_json::from_str(&cargo_lock_content).unwrap_or_else(|e| {
            panic!(
                "Failed to parse cargo lock file {}: {}",
                cargo_lock_path.display(),
                e
            )
        });

        cargo_lock.into()
    }

    /// Create a simple workspace structure for testing
    ///
    /// # Panics
    ///
    /// * If fails to create a temporary directory
    /// * If fails to write the workspace Cargo.toml file
    /// * If fails to write the package Cargo.toml files
    #[must_use]
    pub fn create_simple_workspace(
        workspace_members: &[&str],
        workspace_dependencies: &[&str],
        package_configs: &[(&str, &[&str])], // (package_name, dependencies)
    ) -> (TempDir, Vec<String>) {
        use std::fmt::Write;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create workspace root Cargo.toml
        let mut workspace_toml = String::new();
        workspace_toml.push_str("[workspace]\nmembers = [\n");
        for member in workspace_members {
            writeln!(workspace_toml, "    \"packages/{member}\",").unwrap();
        }
        workspace_toml.push_str("]\n\n[workspace.dependencies]\n");
        for dep in workspace_dependencies {
            writeln!(workspace_toml, "{dep} = \"1.0\"").unwrap();
        }

        switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml)
            .expect("Failed to write workspace Cargo.toml");

        // Create package directories and Cargo.toml files
        for (package_name, dependencies) in package_configs {
            let package_path = temp_dir.path().join("packages").join(package_name);
            switchy_fs::sync::create_dir_all(package_path.join("src"))
                .expect("Failed to create package directory");

            let mut package_toml = format!(
                "[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n"
            );

            for dep in *dependencies {
                if workspace_members.contains(dep) {
                    writeln!(package_toml, "{dep} = {{ path = \"../{dep}\" }}").unwrap();
                } else {
                    writeln!(package_toml, "{dep} = {{ workspace = true }}").unwrap();
                }
            }

            switchy_fs::sync::write(package_path.join("Cargo.toml"), package_toml)
                .expect("Failed to write package Cargo.toml");
        }

        let workspace_members = workspace_members
            .iter()
            .map(|m| format!("packages/{m}"))
            .collect();
        (temp_dir, workspace_members)
    }

    /// Copies a directory recursively from the real filesystem to the `switchy_fs` filesystem.
    ///
    /// This function reads from the real filesystem (for test resources) and writes
    /// to the `switchy_fs` filesystem (which may be simulated or real depending on features).
    fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        if !switchy_fs::exists(dst) {
            switchy_fs::sync::create_dir_all(dst)?;
        }

        // Read from real filesystem since test-resources are on disk
        let mut entries: Vec<_> = std::fs::read_dir(src)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);

        for entry in entries {
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                // Read from real filesystem, write to switchy_fs
                let content = std::fs::read(&src_path)?;
                switchy_fs::sync::write(&dst_path, content)?;
            }
        }

        Ok(())
    }
}
