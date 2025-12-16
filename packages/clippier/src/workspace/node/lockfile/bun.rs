//! bun bun.lock parser.

use super::{NodeLockEntry, NodeLockfile};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Parses a bun bun.lock file.
///
/// bun.lock is a JSONC (JSON with comments) file similar to package-lock.json v3.
///
/// # Errors
///
/// Returns an error if the JSON is invalid.
pub fn parse_bun_lockfile(content: &str) -> Result<NodeLockfile, BoxError> {
    // Strip JSONC comments
    let clean_content = strip_jsonc_comments(content);
    let json: serde_json::Value = serde_json::from_str(&clean_content)?;

    let mut entries = Vec::new();

    // bun.lock structure is similar to package-lock.json
    // It has a "packages" object where keys are package paths
    if let Some(packages) = json.get("packages").and_then(serde_json::Value::as_object) {
        for (path, info) in packages {
            // Skip the root package (empty path or just project name)
            if path.is_empty() || !path.contains('@') {
                continue;
            }

            // Parse package name and version from the path/info
            if let Some((name, version)) = parse_bun_package_entry(path, info) {
                let dependencies = extract_bun_dependencies(info);

                entries.push(NodeLockEntry {
                    name,
                    version,
                    dependencies,
                });
            }
        }
    }

    Ok(NodeLockfile { entries })
}

/// Strips JSONC comments from content.
fn strip_jsonc_comments(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    while let Some(c) = chars.next() {
        if escape_next {
            result.push(c);
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                result.push(c);
                escape_next = true;
            }
            '"' => {
                result.push(c);
                in_string = !in_string;
            }
            '/' if !in_string => {
                if let Some(&next) = chars.peek() {
                    match next {
                        '/' => {
                            // Line comment - skip until newline
                            chars.next(); // consume second /
                            for c in chars.by_ref() {
                                if c == '\n' {
                                    result.push('\n');
                                    break;
                                }
                            }
                        }
                        '*' => {
                            // Block comment - skip until */
                            chars.next(); // consume *
                            while let Some(c) = chars.next() {
                                if c == '*' && chars.peek() == Some(&'/') {
                                    chars.next(); // consume /
                                    result.push(' '); // Replace comment with space
                                    break;
                                }
                            }
                        }
                        _ => result.push(c),
                    }
                } else {
                    result.push(c);
                }
            }
            _ => result.push(c),
        }
    }

    result
}

/// Parses a bun package entry to extract name and version.
fn parse_bun_package_entry(path: &str, info: &serde_json::Value) -> Option<(String, String)> {
    // Try to get version from info first
    if let Some(version) = info.get("version").and_then(serde_json::Value::as_str) {
        // Path might be the package name or node_modules path
        let name = extract_package_name(path);
        return Some((name, version.to_string()));
    }

    // Fallback: parse from path like "lodash@4.17.21"
    if let Some(at_pos) = path.rfind('@')
        // Make sure @ is not at the start (scoped package)
        && at_pos > 0
    {
        let name = &path[..at_pos];
        let version = &path[at_pos + 1..];
        return Some((name.to_string(), version.to_string()));
    }

    None
}

/// Extracts the package name from a path.
fn extract_package_name(path: &str) -> String {
    // Handle node_modules paths
    if let Some(stripped) = path.strip_prefix("node_modules/") {
        // Handle nested node_modules
        if let Some(last_nm) = stripped.rfind("node_modules/") {
            return stripped[last_nm + 13..].to_string();
        }
        return stripped.to_string();
    }

    // Handle name@version format
    if let Some(at_pos) = path.rfind('@')
        && at_pos > 0
        && !path.starts_with('@')
    {
        return path[..at_pos].to_string();
    }

    // Scoped package: @scope/name@version
    if path.starts_with('@')
        && let Some(slash) = path.find('/')
        && let Some(version_at) = path[slash..].find('@')
    {
        return path[..slash + version_at].to_string();
    }

    path.to_string()
}

/// Extracts dependencies from a bun package entry.
fn extract_bun_dependencies(info: &serde_json::Value) -> Vec<String> {
    let mut deps = Vec::new();

    for section in ["dependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(section_deps) = info.get(section).and_then(serde_json::Value::as_object) {
            deps.extend(section_deps.keys().cloned());
        }
    }

    deps
}

/// Parses bun lockfile diff to extract changed package names.
#[must_use]
pub fn parse_bun_lock_changes(changes: &[(char, String)]) -> Vec<String> {
    let mut changed_packages = std::collections::BTreeSet::new();
    let mut current_path: Option<String> = None;
    let mut has_version_change = false;
    let mut is_new_entry = false;
    let mut in_packages_section = false;

    for (op, line) in changes {
        let line = line.trim();

        // Detect packages section
        if line.contains("\"packages\"") {
            in_packages_section = true;
            continue;
        }

        if !in_packages_section {
            continue;
        }

        // Detect package entry (key in packages object)
        // Various patterns: "package@version": {, "node_modules/package": {
        if (line.contains('@') || line.contains("node_modules/"))
            && line.ends_with(": {")
            && line.starts_with('"')
        {
            // Extract the path/key
            if let Some(end) = line.find("\": {") {
                let path = &line[1..end];
                let name = extract_package_name(path);

                // Save previous entry if it changed
                if let Some(prev) = &current_path
                    && (has_version_change || is_new_entry)
                {
                    changed_packages.insert(prev.clone());
                }

                current_path = Some(name);
                has_version_change = false;
                is_new_entry = *op == '+';
            }
        } else if line.starts_with("\"version\"") && (*op == '+' || *op == '-') {
            has_version_change = true;
        } else if line == "}," || line == "}" {
            // End of entry might be indicated by closing brace
            // (but this is imprecise for nested objects)
        }
    }

    // Handle last entry
    if let Some(name) = current_path
        && (has_version_change || is_new_entry)
    {
        changed_packages.insert(name);
    }

    changed_packages.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_jsonc_comments() {
        let input = r#"
        {
            // This is a line comment
            "name": "test",
            /* This is a
               block comment */
            "version": "1.0.0"
        }
        "#;

        let result = strip_jsonc_comments(input);
        assert!(!result.contains("//"));
        assert!(!result.contains("/*"));
        assert!(result.contains("\"name\""));
        assert!(result.contains("\"version\""));
    }

    #[test]
    fn test_extract_package_name() {
        assert_eq!(extract_package_name("lodash"), "lodash");
        assert_eq!(extract_package_name("lodash@4.17.21"), "lodash");
        assert_eq!(extract_package_name("node_modules/lodash"), "lodash");
        assert_eq!(
            extract_package_name("node_modules/@babel/core"),
            "@babel/core"
        );
    }

    #[test]
    fn test_parse_bun_lockfile() {
        let content = r#"
        {
            "lockfileVersion": 0,
            "packages": {
                "lodash@4.17.21": {
                    "version": "4.17.21",
                    "resolved": "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"
                }
            }
        }
        "#;

        let lockfile = parse_bun_lockfile(content).unwrap();

        assert_eq!(lockfile.entries.len(), 1);
        let lodash = lockfile.find_by_name("lodash").unwrap();
        assert_eq!(lodash.version, "4.17.21");
    }

    #[test]
    fn test_parse_bun_lock_changes() {
        let changes = vec![
            (' ', "  \"packages\": {".to_string()),
            (' ', "    \"lodash@4.17.20\": {".to_string()),
            ('-', "      \"version\": \"4.17.20\"".to_string()),
            ('+', "    \"lodash@4.17.21\": {".to_string()),
            ('+', "      \"version\": \"4.17.21\"".to_string()),
        ];

        let result = parse_bun_lock_changes(&changes);
        assert!(result.contains(&"lodash".to_string()));
    }
}
