//! Cargo.lock parsing and diff analysis.

use serde::{Deserialize, Serialize};

use crate::workspace::traits::{Lockfile, LockfileDiffParser, LockfileEntry};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A single package entry in Cargo.lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLockEntry {
    /// Package name
    pub name: String,

    /// Package version
    pub version: String,

    /// Package source (registry, git, etc.)
    #[serde(default)]
    pub source: Option<String>,

    /// Package dependencies
    #[serde(default)]
    pub dependencies: Vec<String>,
}

impl LockfileEntry for CargoLockEntry {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

/// Parsed Cargo.lock file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLockfile {
    /// Lockfile format version
    #[serde(default)]
    pub version: u32,

    /// All packages in the lockfile
    #[serde(default, rename = "package")]
    pub packages: Vec<CargoLockEntry>,
}

impl CargoLockfile {
    /// Parses Cargo.lock content into a structured representation.
    ///
    /// # Errors
    ///
    /// Returns an error if the content is not valid TOML or doesn't match
    /// the expected Cargo.lock structure.
    pub fn parse(content: &str) -> Result<Self, BoxError> {
        // Try to parse as TOML
        let toml_value: toml::Value = toml::from_str(content)?;

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let version = toml_value
            .get("version")
            .and_then(toml::Value::as_integer)
            .map_or(3, |v| v as u32);

        let packages = toml_value
            .get("package")
            .and_then(|p| p.as_array())
            .map(|packages| {
                packages
                    .iter()
                    .filter_map(|pkg| {
                        let name = pkg.get("name")?.as_str()?.to_string();
                        let version = pkg.get("version")?.as_str()?.to_string();
                        let source = pkg
                            .get("source")
                            .and_then(|s| s.as_str())
                            .map(ToString::to_string);
                        let dependencies = pkg
                            .get("dependencies")
                            .and_then(|deps| deps.as_array())
                            .map(|deps| {
                                deps.iter()
                                    .filter_map(|d| d.as_str().map(ToString::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();

                        Some(CargoLockEntry {
                            name,
                            version,
                            source,
                            dependencies,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self { version, packages })
    }

    /// Finds an entry by package name.
    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<&CargoLockEntry> {
        self.packages.iter().find(|e| e.name == name)
    }
}

impl Lockfile for CargoLockfile {
    fn entries(&self) -> Vec<Box<dyn LockfileEntry>> {
        self.packages
            .iter()
            .cloned()
            .map(|e| Box::new(e) as Box<dyn LockfileEntry>)
            .collect()
    }

    fn find(&self, name: &str) -> Option<Box<dyn LockfileEntry>> {
        self.find_by_name(name)
            .cloned()
            .map(|e| Box::new(e) as Box<dyn LockfileEntry>)
    }
}

/// Parser for Cargo.lock diff output.
#[derive(Debug, Clone, Default)]
pub struct CargoLockDiffParser;

impl LockfileDiffParser for CargoLockDiffParser {
    /// Parses Cargo.lock diff to extract changed package names.
    ///
    /// This detects:
    /// - Version changes (package has both `-version` and `+version` lines)
    /// - New packages (package name line is added with `+`)
    /// - Removed packages (package name line is removed with `-`)
    ///
    /// Checksum-only changes are ignored as they don't indicate meaningful
    /// dependency changes.
    fn parse_changes(&self, changes: &[(char, String)]) -> Vec<String> {
        let mut changed_packages = std::collections::BTreeSet::new();
        let mut current_package: Option<String> = None;
        let mut has_version_change = false;
        let mut is_new_package = false;

        for (op, line) in changes {
            let line = line.trim();

            if line.starts_with("name = \"") {
                // Extract package name
                if let Some(name_start) = line.find('"')
                    && let Some(name_end) = line.rfind('"')
                    && name_end > name_start
                {
                    current_package = Some(line[name_start + 1..name_end].to_string());
                    has_version_change = false;
                    is_new_package = *op == '+';
                }
            } else if line.starts_with("version = \"") && (*op == '-' || *op == '+') {
                has_version_change = true;
            } else if line.starts_with("[[package]]") || line.is_empty() {
                // End of package section
                if let Some(package) = &current_package
                    && (has_version_change || is_new_package)
                {
                    changed_packages.insert(package.clone());
                }
                current_package = None;
                has_version_change = false;
                is_new_package = false;
            }
            // Ignore checksum-only changes
        }

        // Handle the last package in the diff
        if let Some(package) = current_package
            && (has_version_change || is_new_package)
        {
            changed_packages.insert(package);
        }

        let mut result: Vec<String> = changed_packages.into_iter().collect();
        result.sort();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cargo_lock() {
        let content = r#"
            # This file is automatically @generated by Cargo.
            # It is not intended for manual editing.
            version = 3

            [[package]]
            name = "serde"
            version = "1.0.193"
            source = "registry+https://github.com/rust-lang/crates.io-index"
            dependencies = [
                "serde_derive",
            ]

            [[package]]
            name = "serde_derive"
            version = "1.0.193"
            source = "registry+https://github.com/rust-lang/crates.io-index"
        "#;

        let lockfile = CargoLockfile::parse(content).unwrap();

        assert_eq!(lockfile.version, 3);
        assert_eq!(lockfile.packages.len(), 2);

        let serde = lockfile.find_by_name("serde").unwrap();
        assert_eq!(serde.version, "1.0.193");
        assert_eq!(serde.dependencies, vec!["serde_derive"]);
    }

    #[test]
    fn test_parse_cargo_lock_diff_version_change() {
        let parser = CargoLockDiffParser;

        let changes = vec![
            (' ', "[[package]]".to_string()),
            (' ', "name = \"serde\"".to_string()),
            ('-', "version = \"1.0.192\"".to_string()),
            ('+', "version = \"1.0.193\"".to_string()),
            (' ', "source = \"registry+...\"".to_string()),
            (' ', String::new()),
        ];

        let result = parser.parse_changes(&changes);
        assert_eq!(result, vec!["serde"]);
    }

    #[test]
    fn test_parse_cargo_lock_diff_new_package() {
        let parser = CargoLockDiffParser;

        let changes = vec![
            ('+', "[[package]]".to_string()),
            ('+', "name = \"new-crate\"".to_string()),
            ('+', "version = \"1.0.0\"".to_string()),
            ('+', "source = \"registry+...\"".to_string()),
            (' ', String::new()),
        ];

        let result = parser.parse_changes(&changes);
        assert_eq!(result, vec!["new-crate"]);
    }

    #[test]
    fn test_parse_cargo_lock_diff_checksum_only() {
        let parser = CargoLockDiffParser;

        // Checksum-only changes should be ignored
        let changes = vec![
            (' ', "[[package]]".to_string()),
            (' ', "name = \"serde\"".to_string()),
            (' ', "version = \"1.0.193\"".to_string()),
            ('-', "checksum = \"abc123\"".to_string()),
            ('+', "checksum = \"def456\"".to_string()),
            (' ', String::new()),
        ];

        let result = parser.parse_changes(&changes);
        assert!(result.is_empty());
    }

    #[test]
    fn test_reverse_dependency_map() {
        let lockfile = CargoLockfile {
            version: 3,
            packages: vec![
                CargoLockEntry {
                    name: "app".to_string(),
                    version: "1.0.0".to_string(),
                    source: None,
                    dependencies: vec!["serde 1.0.0".to_string(), "tokio 1.0.0".to_string()],
                },
                CargoLockEntry {
                    name: "serde".to_string(),
                    version: "1.0.0".to_string(),
                    source: Some("registry+...".to_string()),
                    dependencies: vec!["serde_derive 1.0.0".to_string()],
                },
                CargoLockEntry {
                    name: "serde_derive".to_string(),
                    version: "1.0.0".to_string(),
                    source: Some("registry+...".to_string()),
                    dependencies: vec![],
                },
                CargoLockEntry {
                    name: "tokio".to_string(),
                    version: "1.0.0".to_string(),
                    source: Some("registry+...".to_string()),
                    dependencies: vec![],
                },
            ],
        };

        let reverse_map = lockfile.reverse_dependency_map();

        // serde is depended on by app
        assert!(
            reverse_map
                .get("serde")
                .unwrap()
                .contains(&"app".to_string())
        );

        // serde_derive is depended on by serde
        assert!(
            reverse_map
                .get("serde_derive")
                .unwrap()
                .contains(&"serde".to_string())
        );

        // tokio is depended on by app
        assert!(
            reverse_map
                .get("tokio")
                .unwrap()
                .contains(&"app".to_string())
        );
    }
}
