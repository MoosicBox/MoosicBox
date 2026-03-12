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
mod types;

pub use registry::ToolRegistry;
pub use runner::{AggregatedResults, ToolResult, ToolRunner, print_summary, results_to_json};
pub use types::{Tool, ToolCapability, ToolKind, ToolsConfig};

fn merge_unique_strings(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.iter().any(|existing| existing == value) {
            target.push(value.clone());
        }
    }
}

/// Loads tool defaults from `clippier.toml` in the working directory.
///
/// Returns an empty config when the file does not exist.
///
/// # Errors
///
/// Returns an error when the config file cannot be read or parsed.
pub fn load_tools_config(
    working_dir: Option<&std::path::Path>,
) -> Result<ToolsConfig, Box<dyn std::error::Error + Send + Sync>> {
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
) -> Result<ToolsConfig, Box<dyn std::error::Error + Send + Sync>> {
    let mut config = load_tools_config(working_dir)?;

    if let Some(required_tools) = required {
        merge_unique_strings(&mut config.required, required_tools);
    }

    if let Some(skip_tools) = skip {
        merge_unique_strings(&mut config.skip, skip_tools);
    }

    Ok(config)
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

        let merged = build_tools_config(Some(&dir), Some(&required), Some(&skip))
            .expect("failed to build merged tools config");

        assert_eq!(merged.required, vec!["rustfmt", "taplo"]);
        assert_eq!(merged.skip, vec!["gofmt", "shellcheck"]);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }
}
