//! Tool detection and execution infrastructure for linting and formatting.
//!
//! This module provides a framework for detecting, configuring, and running
//! external linting and formatting tools. It acts as an orchestrator that:
//!
//! - Detects installed tools using the `which` crate for cross-platform support
//! - Invokes tools with their native CLI interfaces
//! - Aggregates results and exit codes
//! - Reports results in a unified way
//!
//! # Design Philosophy
//!
//! Clippier acts as an **orchestrator, not a controller**. It:
//! - Delegates all actual linting/formatting to the native tools
//! - Uses tools' own configuration files (`.prettierrc`, `rustfmt.toml`, etc.)
//! - Does not try to abstract away tool-specific arguments
//! - Only provides minimal configuration for tool selection (required/skip)
//!
//! # Example
//!
//! ```rust,ignore
//! use clippier::tools::{ToolRegistry, ToolsConfig};
//!
//! let config = ToolsConfig::default();
//! let registry = ToolRegistry::new(config);
//!
//! // Run all available formatters
//! let results = registry.run_formatters(&["src/"])?;
//!
//! // Run all available linters
//! let results = registry.run_linters(&["src/"])?;
//! ```

mod registry;
mod runner;
#[cfg(feature = "tools-tui")]
mod tui;
mod types;

pub use registry::ToolRegistry;
pub use runner::{AggregatedResults, ToolResult, ToolRunner, print_summary, results_to_json};
pub use types::{Tool, ToolCapability, ToolKind, ToolsConfig};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn merge_unique_strings(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.iter().any(|existing| existing == value) {
            target.push(value.clone());
        }
    }
}

/// Merges auto-detected and required tool names with de-duplication.
#[must_use]
pub fn merge_tool_names(auto_detected: &[String], required: &[String]) -> Vec<String> {
    let mut merged = auto_detected.to_vec();
    merge_unique_strings(&mut merged, required);
    merged
}

/// Loads tool defaults from `clippier.toml` in the working directory.
///
/// Returns an empty config when the file does not exist.
///
/// # Errors
///
/// Returns an error when the config file cannot be read or parsed.
pub fn load_tools_config(working_dir: Option<&std::path::Path>) -> Result<ToolsConfig, BoxError> {
    let base_dir = match working_dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()?,
    };

    let config_path = base_dir.join("clippier.toml");
    if !config_path.exists() {
        return Ok(ToolsConfig::default());
    }

    let source = std::fs::read_to_string(config_path)?;
    let conf: crate::ClippierConf = toml::from_str(&source)?;
    Ok(conf.tools.unwrap_or_default())
}

/// Builds final tool config from file defaults plus CLI overrides.
///
/// CLI `required` and `skip` values are merged additively with de-duplication.
///
/// # Errors
///
/// Returns an error when loading config from disk fails.
pub fn build_tools_config(
    working_dir: Option<&std::path::Path>,
    required: Option<&[String]>,
    skip: Option<&[String]>,
    explicit_tools: Option<&[String]>,
) -> Result<ToolsConfig, BoxError> {
    let mut config = load_tools_config(working_dir)?;

    if let Some(required_tools) = required {
        merge_unique_strings(&mut config.required, required_tools);
    }

    if let Some(skip_tools) = skip {
        merge_unique_strings(&mut config.skip, skip_tools);
    }

    if let Some(explicit) = explicit_tools {
        config
            .skip
            .retain(|tool| !explicit.iter().any(|requested| requested == tool));
    }

    Ok(config)
}

fn has_file(base: &std::path::Path, relative_path: &str) -> bool {
    base.join(relative_path).exists()
}

/// Auto-detects tool names for the `check` command from project manifests.
///
/// # Errors
///
/// Returns an error if the current directory cannot be read.
pub fn auto_detect_check_tools(
    working_dir: Option<&std::path::Path>,
) -> Result<Vec<String>, BoxError> {
    let base_dir = match working_dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()?,
    };

    let has_cargo = has_file(&base_dir, "Cargo.toml");
    let has_package_json = has_file(&base_dir, "package.json");
    let has_pyproject = has_file(&base_dir, "pyproject.toml");
    let has_requirements = has_file(&base_dir, "requirements.txt");
    let has_setup_py = has_file(&base_dir, "setup.py");
    let has_go_mod = has_file(&base_dir, "go.mod");
    let has_taplo_config = has_file(&base_dir, "taplo.toml");
    let has_shellcheck_config = has_file(&base_dir, ".shellcheckrc");

    let mut tools = Vec::new();

    if has_cargo {
        tools.push("clippy".to_string());
        tools.push("rustfmt".to_string());
        tools.push("taplo".to_string());
    }

    if has_package_json {
        tools.push("eslint".to_string());
        tools.push("prettier".to_string());
        tools.push("biome".to_string());
    }

    if has_pyproject || has_requirements || has_setup_py {
        tools.push("ruff".to_string());
        tools.push("black".to_string());
    }

    if has_go_mod {
        tools.push("gofmt".to_string());
    }

    if has_taplo_config {
        tools.push("taplo".to_string());
    }

    if has_shellcheck_config {
        tools.push("shellcheck".to_string());
    }

    let mut deduped = Vec::new();
    merge_unique_strings(&mut deduped, &tools);

    Ok(deduped)
}

/// Auto-detects tool names for the `fmt` command from project manifests.
///
/// # Errors
///
/// Returns an error if the current directory cannot be read.
pub fn auto_detect_fmt_tools(
    working_dir: Option<&std::path::Path>,
) -> Result<Vec<String>, BoxError> {
    let base_dir = match working_dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()?,
    };

    let has_cargo = has_file(&base_dir, "Cargo.toml");
    let has_package_json = has_file(&base_dir, "package.json");
    let has_pyproject = has_file(&base_dir, "pyproject.toml");
    let has_requirements = has_file(&base_dir, "requirements.txt");
    let has_setup_py = has_file(&base_dir, "setup.py");
    let has_go_mod = has_file(&base_dir, "go.mod");
    let has_taplo_config = has_file(&base_dir, "taplo.toml");
    let has_shfmt_config = has_file(&base_dir, ".shfmt.conf");

    let mut tools = Vec::new();

    if has_cargo {
        tools.push("rustfmt".to_string());
        tools.push("taplo".to_string());
    }

    if has_package_json {
        tools.push("prettier".to_string());
        tools.push("biome".to_string());
    }

    if has_pyproject || has_requirements || has_setup_py {
        tools.push("ruff".to_string());
        tools.push("black".to_string());
    }

    if has_go_mod {
        tools.push("gofmt".to_string());
    }

    if has_taplo_config {
        tools.push("taplo".to_string());
    }

    if has_shfmt_config {
        tools.push("shfmt".to_string());
    }

    let mut deduped = Vec::new();
    merge_unique_strings(&mut deduped, &tools);

    Ok(deduped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
        std::fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }

    #[test]
    fn load_tools_config_reads_tools_section() {
        let dir = temp_dir("clippier-tools-config");
        let config_path = dir.join("clippier.toml");
        std::fs::write(
            &config_path,
            "[tools]\nrequired = [\"rustfmt\"]\nskip = [\"gofmt\"]\n",
        )
        .expect("failed to write clippier.toml");

        let loaded = load_tools_config(Some(&dir)).expect("failed to load tools config");
        assert_eq!(loaded.required, vec!["rustfmt"]);
        assert_eq!(loaded.skip, vec!["gofmt"]);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn build_tools_config_merges_cli_with_file() {
        let dir = temp_dir("clippier-tools-merge");
        let config_path = dir.join("clippier.toml");
        std::fs::write(
            &config_path,
            "[tools]\nrequired = [\"rustfmt\"]\nskip = [\"gofmt\"]\n",
        )
        .expect("failed to write clippier.toml");

        let required = vec!["taplo".to_string(), "rustfmt".to_string()];
        let skip = vec!["shellcheck".to_string(), "gofmt".to_string()];

        let merged = build_tools_config(Some(&dir), Some(&required), Some(&skip), None)
            .expect("failed to build merged tools config");

        assert_eq!(merged.required, vec!["rustfmt", "taplo"]);
        assert_eq!(merged.skip, vec!["gofmt", "shellcheck"]);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn build_tools_config_explicit_tools_override_skip() {
        let dir = temp_dir("clippier-tools-skip-override");
        let config_path = dir.join("clippier.toml");
        std::fs::write(&config_path, "[tools]\nskip = [\"gofmt\", \"taplo\"]\n")
            .expect("failed to write clippier.toml");

        let explicit = vec!["gofmt".to_string()];
        let merged = build_tools_config(Some(&dir), None, None, Some(&explicit))
            .expect("failed to build tools config");

        assert_eq!(merged.skip, vec!["taplo"]);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_tools_uses_manifest_files() {
        let dir = temp_dir("clippier-auto-detect");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n",
        )
        .expect("failed to write Cargo.toml");
        std::fs::write(dir.join("package.json"), "{}\n").expect("failed to write package.json");
        std::fs::write(dir.join("requirements.txt"), "black\n")
            .expect("failed to write requirements.txt");
        std::fs::write(dir.join("go.mod"), "module example.com/x\n")
            .expect("failed to write go.mod");

        let check_tools =
            auto_detect_check_tools(Some(&dir)).expect("failed to detect check tools");
        assert!(check_tools.contains(&"clippy".to_string()));
        assert!(check_tools.contains(&"rustfmt".to_string()));
        assert!(check_tools.contains(&"prettier".to_string()));
        assert!(check_tools.contains(&"biome".to_string()));
        assert!(check_tools.contains(&"eslint".to_string()));
        assert!(check_tools.contains(&"ruff".to_string()));
        assert!(check_tools.contains(&"black".to_string()));
        assert!(check_tools.contains(&"gofmt".to_string()));

        let fmt_tools = auto_detect_fmt_tools(Some(&dir)).expect("failed to detect fmt tools");
        assert!(fmt_tools.contains(&"rustfmt".to_string()));
        assert!(fmt_tools.contains(&"prettier".to_string()));
        assert!(fmt_tools.contains(&"biome".to_string()));
        assert!(fmt_tools.contains(&"ruff".to_string()));
        assert!(fmt_tools.contains(&"black".to_string()));
        assert!(fmt_tools.contains(&"gofmt".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn merge_tool_names_includes_required_without_duplicates() {
        let auto_detected = vec!["rustfmt".to_string(), "taplo".to_string()];
        let required = vec!["taplo".to_string(), "shfmt".to_string()];

        let merged = merge_tool_names(&auto_detected, &required);

        assert_eq!(merged, vec!["rustfmt", "taplo", "shfmt"]);
    }

    #[test]
    fn build_tools_config_keeps_required_and_skip_overlap() {
        let dir = temp_dir("clippier-tools-required-skip-overlap");
        let config_path = dir.join("clippier.toml");
        std::fs::write(
            &config_path,
            "[tools]\nrequired = [\"rustfmt\"]\nskip = [\"rustfmt\"]\n",
        )
        .expect("failed to write clippier.toml");

        let merged = build_tools_config(Some(&dir), None, None, None)
            .expect("failed to build merged tools config");

        assert_eq!(merged.required, vec!["rustfmt"]);
        assert_eq!(merged.skip, vec!["rustfmt"]);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }
}
