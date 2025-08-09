use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLockPackage {
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub dependencies: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLock {
    pub version: u32,
    pub package: Vec<CargoLockPackage>,
}

// Basic parsing function available without git-diff
#[must_use]
pub fn parse_dependency_name(dep_spec: &str) -> String {
    // Parse dependency name (remove version constraints, features, etc.)
    dep_spec
        .split_whitespace()
        .next()
        .unwrap_or(dep_spec)
        .to_string()
}

// Basic parsing function for cargo lock changes
#[must_use]
pub fn parse_cargo_lock_changes(changes: &[(char, String)]) -> Vec<String> {
    let mut changed_packages = std::collections::BTreeSet::new();
    let mut current_package = None;
    let mut has_version_change = false;

    for (op, line) in changes {
        let line = line.trim();

        if line.starts_with("name = \"") {
            // Extract package name
            if let Some(name_start) = line.find('"') {
                if let Some(name_end) = line.rfind('"') {
                    if name_end > name_start {
                        current_package = Some(line[name_start + 1..name_end].to_string());
                        has_version_change = false;
                    }
                }
            }
        } else if line.starts_with("version = \"") && (*op == '-' || *op == '+') {
            has_version_change = true;
        } else if line.is_empty() || line.starts_with("[[package]]") {
            // End of package block
            if let (Some(package), true) = (&current_package, has_version_change) {
                changed_packages.insert(package.clone());
            }
            current_package = None;
            has_version_change = false;
        }
    }

    // Handle case where file ends without empty line
    if let (Some(package), true) = (current_package, has_version_change) {
        changed_packages.insert(package);
    }

    let mut result: Vec<String> = changed_packages.into_iter().collect();
    result.sort();
    result
}

// Basic TOML parsing function
/// # Errors
/// Returns an error if the TOML content cannot be parsed
pub fn parse_cargo_lock(content: &str) -> Result<CargoLock, Box<dyn std::error::Error>> {
    let toml_value: toml::Value = toml::from_str(content)?;

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let version = toml_value
        .get("version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(1) as u32;

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
                        });

                    Some(CargoLockPackage {
                        name,
                        version,
                        source,
                        dependencies,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(CargoLock {
        version,
        package: packages,
    })
}
