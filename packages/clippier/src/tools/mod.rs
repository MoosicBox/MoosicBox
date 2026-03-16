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
//! let registry = ToolRegistry::new(config, None)?;
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

use serde::Deserialize;

pub use registry::ToolRegistry;
pub use runner::{AggregatedResults, ToolResult, ToolRunner, print_summary, results_to_json};
pub use types::{
    OverlapWarningCapability, OverlapWarningSuppressRule, Tool, ToolCapability, ToolKind,
    ToolsConfig,
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone)]
struct WorkspaceFile {
    relative_path: String,
    extension: String,
}

#[derive(Debug, Clone)]
struct GlobRule {
    negated: bool,
    matcher: globset::GlobMatcher,
}

#[derive(Debug, Deserialize)]
struct PrettierSupportInfo {
    #[serde(default)]
    languages: Vec<PrettierLanguage>,
}

#[derive(Debug, Deserialize)]
struct PrettierLanguage {
    #[serde(default)]
    extensions: Vec<String>,
}

const KNOWN_TOOL_NAMES: &[&str] = &[
    "rustfmt",
    "clippy",
    "taplo",
    "prettier",
    "biome",
    "eslint",
    "dprint",
    "remark",
    "mdformat",
    "yamlfmt",
    "ruff",
    "black",
    "gofmt",
    "shfmt",
    "shellcheck",
];

fn parse_tool_path_overrides(
    tool_paths: &[String],
) -> Result<std::collections::BTreeMap<String, String>, BoxError> {
    let mut overrides = std::collections::BTreeMap::new();

    for entry in tool_paths {
        let Some((key, value)) = entry.split_once('=') else {
            return Err(format!("Invalid --tool-path '{entry}'. Expected format key=value").into());
        };

        if key.is_empty() || value.is_empty() {
            return Err(format!(
                "Invalid --tool-path '{entry}'. Tool name and value must be non-empty"
            )
            .into());
        }

        if !KNOWN_TOOL_NAMES.iter().any(|name| name == &key) {
            return Err(format!(
                "Unknown tool '{key}' in --tool-path. Supported tools: {}",
                KNOWN_TOOL_NAMES.join(", ")
            )
            .into());
        }

        overrides.insert(key.to_string(), value.to_string());
    }

    Ok(overrides)
}

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

fn normalize_tool_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn normalize_extension(extension: &str) -> String {
    extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

fn normalized_tool_pair(a: &str, b: &str) -> (String, String) {
    let mut pair = [normalize_tool_name(a), normalize_tool_name(b)];
    pair.sort();
    (pair[0].clone(), pair[1].clone())
}

fn should_skip_overlap_scan_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | "dist" | "build" | ".next" | ".direnv"
    )
}

fn collect_workspace_files(base_dir: &std::path::Path) -> Vec<WorkspaceFile> {
    let mut files = Vec::new();
    let mut stack = vec![base_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();

            if file_type.is_dir() {
                if let Some(name) = entry.file_name().to_str()
                    && should_skip_overlap_scan_dir(name)
                {
                    continue;
                }
                stack.push(path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let Some(extension) = path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(normalize_extension)
            else {
                continue;
            };

            let Ok(relative) = path.strip_prefix(base_dir) else {
                continue;
            };

            files.push(WorkspaceFile {
                relative_path: relative.to_string_lossy().replace('\\', "/"),
                extension,
            });
        }
    }

    files
}

fn find_file_in_ancestors(
    base_dir: &std::path::Path,
    names: &[&str],
) -> Option<std::path::PathBuf> {
    let mut current = Some(base_dir);
    while let Some(dir) = current {
        for name in names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        current = dir.parent();
    }
    None
}

fn parse_glob_rules(raw_patterns: &[String]) -> Vec<GlobRule> {
    raw_patterns
        .iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return None;
            }

            let (negated, pattern) = trimmed
                .strip_prefix('!')
                .map_or((false, trimmed), |rest| (true, rest));

            let Ok(glob) = globset::Glob::new(pattern) else {
                return None;
            };

            Some(GlobRule {
                negated,
                matcher: glob.compile_matcher(),
            })
        })
        .collect()
}

fn parse_prettier_ignore_rules(base_dir: &std::path::Path) -> Vec<GlobRule> {
    let Some(path) = find_file_in_ancestors(base_dir, &[".prettierignore"]) else {
        return Vec::new();
    };

    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let patterns = contents
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            Some(trimmed.to_string())
        })
        .collect::<Vec<_>>();
    parse_glob_rules(&patterns)
}

fn parse_biome_include_rules(base_dir: &std::path::Path) -> Option<Vec<String>> {
    let path = find_file_in_ancestors(base_dir, &["biome.json", "biome.jsonc"])?;
    let contents = std::fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&contents).ok()?;
    let includes = value
        .get("files")
        .and_then(|files| files.get("includes"))
        .and_then(serde_json::Value::as_array)?;

    Some(
        includes
            .iter()
            .filter_map(|item| item.as_str().map(ToString::to_string))
            .collect(),
    )
}

fn toml_value_contains_mdx(value: &toml::Value) -> bool {
    value.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item.as_str()
                .is_some_and(|entry| normalize_extension(entry) == "mdx")
        })
    })
}

fn mdformat_config_supports_mdx(base_dir: &std::path::Path) -> bool {
    if let Some(path) = find_file_in_ancestors(base_dir, &[".mdformat.toml"])
        && let Ok(contents) = std::fs::read_to_string(path)
        && let Ok(parsed) = toml::from_str::<toml::Value>(&contents)
        && parsed
            .get("extensions")
            .is_some_and(toml_value_contains_mdx)
    {
        return true;
    }

    if let Some(path) = find_file_in_ancestors(base_dir, &["pyproject.toml"])
        && let Ok(contents) = std::fs::read_to_string(path)
        && let Ok(parsed) = toml::from_str::<toml::Value>(&contents)
        && parsed
            .get("tool")
            .and_then(|tool| tool.get("mdformat"))
            .and_then(|mdformat| mdformat.get("extensions"))
            .is_some_and(toml_value_contains_mdx)
    {
        return true;
    }

    false
}

fn is_prettier_ignored(path: &str, rules: &[GlobRule]) -> bool {
    let mut ignored = false;
    for rule in rules {
        if rule.matcher.is_match(path) {
            ignored = !rule.negated;
        }
    }
    ignored
}

fn is_biome_included(path: &str, include_patterns: &[String]) -> bool {
    if include_patterns.is_empty() {
        return true;
    }

    let mut included = false;
    let mut force_ignored = false;
    for pattern in include_patterns {
        if let Some(rest) = pattern.strip_prefix("!!") {
            if let Ok(glob) = globset::Glob::new(rest)
                && glob.compile_matcher().is_match(path)
            {
                force_ignored = true;
            }
            continue;
        }

        if let Some(rest) = pattern.strip_prefix('!') {
            if let Ok(glob) = globset::Glob::new(rest)
                && glob.compile_matcher().is_match(path)
            {
                included = false;
            }
            continue;
        }

        if let Ok(glob) = globset::Glob::new(pattern)
            && glob.compile_matcher().is_match(path)
        {
            included = true;
        }
    }

    included && !force_ignored
}

fn dynamic_extensions_for_tools(
    base_dir: &std::path::Path,
    tools: &[&Tool],
    capabilities: &[ToolCapability],
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let workspace_files = collect_workspace_files(base_dir);
    let prettier_ignore_rules = parse_prettier_ignore_rules(base_dir);
    let biome_includes = parse_biome_include_rules(base_dir).unwrap_or_default();

    let mut dynamic = std::collections::BTreeMap::new();
    for tool in tools {
        let normalized_name = normalize_tool_name(&tool.name);
        let mut extensions = std::collections::BTreeSet::new();

        for capability in capabilities {
            if !tool.capabilities.contains(capability) {
                continue;
            }

            let default_extensions = effective_extensions_for_tool(base_dir, tool, *capability);
            if default_extensions.is_empty() {
                continue;
            }

            for file in &workspace_files {
                if !default_extensions.contains(&file.extension) {
                    continue;
                }

                let tool_allows_file = match normalized_name.as_str() {
                    "prettier" => !is_prettier_ignored(&file.relative_path, &prettier_ignore_rules),
                    "biome" => {
                        if biome_includes.is_empty() {
                            true
                        } else {
                            is_biome_included(&file.relative_path, &biome_includes)
                        }
                    }
                    _ => true,
                };

                if tool_allows_file {
                    extensions.insert(file.extension.clone());
                }
            }
        }

        dynamic.insert(normalized_name, extensions);
    }

    dynamic
}

fn effective_extensions_for_tool(
    base_dir: &std::path::Path,
    tool: &Tool,
    capability: ToolCapability,
) -> std::collections::BTreeSet<String> {
    let normalized = normalize_tool_name(&tool.name);

    if normalized == "prettier"
        && capability == ToolCapability::Format
        && let Some(prettier_extensions) = query_prettier_support_info_extensions(base_dir, tool)
        && !prettier_extensions.is_empty()
    {
        return prettier_extensions;
    }

    if normalized == "mdformat" && capability == ToolCapability::Format {
        let mut extensions = default_extensions_for_tool(&tool.name, capability);
        if mdformat_config_supports_mdx(base_dir) || probe_mdformat_supports_mdx(base_dir, tool) {
            extensions.insert("mdx".to_string());
        }
        return extensions;
    }

    if normalized == "dprint"
        && let Some(dprint_extensions) = parse_dprint_include_extensions(base_dir)
    {
        return dprint_extensions;
    }

    default_extensions_for_tool(&tool.name, capability)
}

fn run_tool_probe_command(
    base_dir: &std::path::Path,
    tool: &Tool,
    args: &[&str],
    stdin: Option<&str>,
) -> Option<std::process::Output> {
    let mut command = match &tool.kind {
        ToolKind::Binary => {
            let executable = tool.detected_path.as_ref().map_or_else(
                || std::ffi::OsString::from(&tool.binary),
                |path| path.as_os_str().to_os_string(),
            );
            std::process::Command::new(executable)
        }
        ToolKind::Runner {
            runner,
            runner_args,
        } => {
            let mut command = std::process::Command::new(runner);
            for arg in runner_args {
                command.arg(arg);
            }
            command.arg(&tool.binary);
            command
        }
        ToolKind::Cargo => return None,
    };

    command.args(args).current_dir(base_dir);
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if stdin.is_some() {
        command.stdin(std::process::Stdio::piped());
    }

    let mut child = command.spawn().ok()?;
    if let Some(input) = stdin
        && let Some(mut child_stdin) = child.stdin.take()
    {
        use std::io::Write as _;
        let _ = child_stdin.write_all(input.as_bytes());
    }

    child.wait_with_output().ok()
}

fn probe_mdformat_supports_mdx(base_dir: &std::path::Path, tool: &Tool) -> bool {
    let Some(output) = run_tool_probe_command(
        base_dir,
        tool,
        &["--check", "--extensions", "mdx", "-"],
        Some("# mdx-probe\n"),
    ) else {
        return false;
    };

    output.status.success()
}

fn parse_prettier_support_info_extensions(
    json: &str,
) -> Option<std::collections::BTreeSet<String>> {
    let parsed = serde_json::from_str::<PrettierSupportInfo>(json).ok()?;
    Some(
        parsed
            .languages
            .into_iter()
            .flat_map(|language| language.extensions.into_iter())
            .map(|extension| normalize_extension(&extension))
            .filter(|extension| !extension.is_empty())
            .collect(),
    )
}

fn query_prettier_support_info_extensions(
    base_dir: &std::path::Path,
    tool: &Tool,
) -> Option<std::collections::BTreeSet<String>> {
    let output = run_tool_probe_command(base_dir, tool, &["--support-info"], None)?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_prettier_support_info_extensions(&stdout)
}

fn parse_dprint_include_extensions(
    base_dir: &std::path::Path,
) -> Option<std::collections::BTreeSet<String>> {
    let config_path = if base_dir.join("dprint.json").exists() {
        base_dir.join("dprint.json")
    } else if base_dir.join("dprint.jsonc").exists() {
        // `serde_json` doesn't parse JSONC; keep fallback behavior for this case.
        return None;
    } else {
        return None;
    };

    let contents = std::fs::read_to_string(config_path).ok()?;
    let parsed = serde_json::from_str::<serde_json::Value>(&contents).ok()?;
    let includes = parsed.get("includes")?.as_array()?;

    let mut extensions = std::collections::BTreeSet::new();
    for value in includes {
        let Some(pattern) = value.as_str() else {
            continue;
        };

        if let Some(extension) = pattern.rsplit_once("*.").map(|(_, ext)| ext)
            && !extension.contains('/')
            && !extension.contains('{')
            && !extension.contains('}')
            && !extension.contains(',')
            && !extension.contains('*')
            && !extension.contains('?')
        {
            let normalized = normalize_extension(extension);
            if !normalized.is_empty() {
                extensions.insert(normalized);
            }
        }
    }

    if extensions.is_empty() {
        None
    } else {
        Some(extensions)
    }
}

fn default_extensions_for_tool(
    tool_name: &str,
    capability: ToolCapability,
) -> std::collections::BTreeSet<String> {
    let normalized = normalize_tool_name(tool_name);
    let entries: &[&str] = match capability {
        ToolCapability::Format => match normalized.as_str() {
            "rustfmt" => &["rs"],
            "taplo" => &["toml"],
            "prettier" => &[
                "js", "jsx", "ts", "tsx", "json", "md", "mdx", "yaml", "yml", "html", "css",
                "scss", "less",
            ],
            "biome" => &[
                "js", "jsx", "ts", "tsx", "json", "jsonc", "css", "graphql", "html",
            ],
            "dprint" => &["ts", "tsx", "js", "jsx", "json", "md", "toml"],
            "remark" => &["md", "mdx"],
            "mdformat" => &["md"],
            "yamlfmt" => &["yaml", "yml"],
            "ruff" | "black" => &["py", "pyi", "ipynb"],
            "gofmt" => &["go"],
            "shfmt" => &["sh", "bash"],
            _ => &[],
        },
        ToolCapability::Lint => match normalized.as_str() {
            "clippy" => &["rs"],
            "taplo" => &["toml"],
            "biome" => &[
                "js", "jsx", "ts", "tsx", "json", "jsonc", "css", "graphql", "html",
            ],
            "eslint" => &["js", "jsx", "ts", "tsx"],
            "dprint" => &["ts", "tsx", "js", "jsx", "json", "md", "toml"],
            "ruff" => &["py", "pyi", "ipynb"],
            "shellcheck" => &["sh", "bash"],
            _ => &[],
        },
    };

    entries.iter().map(|value| (*value).to_string()).collect()
}

fn apply_overlap_suppressions(
    overlap_extensions: &mut std::collections::BTreeSet<String>,
    capability: ToolCapability,
    tool_a: &str,
    tool_b: &str,
    suppressions: &[OverlapWarningSuppressRule],
) {
    let normalized_pair = normalized_tool_pair(tool_a, tool_b);

    for suppression in suppressions {
        if !suppression.capability.matches(capability) {
            continue;
        }

        if suppression.tools.len() != 2 {
            log::warn!(
                "ignoring overlap-warning-suppress rule for {:?}: expected exactly 2 tools",
                suppression.tools
            );
            continue;
        }

        let suppression_pair = normalized_tool_pair(&suppression.tools[0], &suppression.tools[1]);
        if suppression_pair != normalized_pair {
            continue;
        }

        if suppression.extensions.is_empty() {
            overlap_extensions.clear();
            return;
        }

        let normalized_extensions = suppression
            .extensions
            .iter()
            .map(|value| normalize_extension(value))
            .collect::<std::collections::BTreeSet<_>>();
        overlap_extensions.retain(|extension| !normalized_extensions.contains(extension));

        if overlap_extensions.is_empty() {
            return;
        }
    }
}

fn overlap_warnings_for_tools(
    tools: &[&Tool],
    capabilities: &[ToolCapability],
    suppressions: &[OverlapWarningSuppressRule],
    dynamic_extensions: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) -> Vec<String> {
    let mut warnings = Vec::new();

    for (index, left) in tools.iter().enumerate() {
        for right in tools.iter().skip(index + 1) {
            for capability in capabilities {
                if !left.capabilities.contains(capability)
                    || !right.capabilities.contains(capability)
                {
                    continue;
                }

                let left_extensions = dynamic_extensions
                    .get(&normalize_tool_name(&left.name))
                    .cloned()
                    .unwrap_or_else(|| default_extensions_for_tool(&left.name, *capability));
                let right_extensions = dynamic_extensions
                    .get(&normalize_tool_name(&right.name))
                    .cloned()
                    .unwrap_or_else(|| default_extensions_for_tool(&right.name, *capability));
                if left_extensions.is_empty() || right_extensions.is_empty() {
                    continue;
                }

                let mut overlap_extensions = left_extensions
                    .intersection(&right_extensions)
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>();
                if overlap_extensions.is_empty() {
                    continue;
                }

                apply_overlap_suppressions(
                    &mut overlap_extensions,
                    *capability,
                    &left.name,
                    &right.name,
                    suppressions,
                );
                if overlap_extensions.is_empty() {
                    continue;
                }

                let capability_label = match capability {
                    ToolCapability::Format => "format",
                    ToolCapability::Lint => "lint",
                };
                warnings.push(format!(
                    "WARNING: potential {capability_label} overlap between '{}' and '{}' on extensions: {}",
                    left.name,
                    right.name,
                    overlap_extensions.into_iter().collect::<Vec<_>>().join(", ")
                ));
                warnings.push(format!(
                    "HINT: suppress intentionally shared coverage via [[tools.overlap-warning-suppress]] with capability='{capability_label}', tools=['{}','{}'], and extensions=[...].",
                    left.name, right.name
                ));
            }
        }
    }

    warnings
}

/// Computes overlap warnings for selected and available tools.
#[must_use]
pub fn overlap_warnings_for_selected_tools(
    registry: &ToolRegistry,
    tool_names: &[String],
    capabilities: &[ToolCapability],
    suppressions: &[OverlapWarningSuppressRule],
    working_dir: Option<&std::path::Path>,
) -> Vec<String> {
    let mut selected = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for name in tool_names {
        let key = normalize_tool_name(name);
        if !seen.insert(key) {
            continue;
        }
        if let Some(tool) = registry.get(name) {
            selected.push(tool);
        }
    }

    let base_dir = working_dir.map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| std::path::Path::new(".").to_path_buf()),
        std::path::Path::to_path_buf,
    );
    let dynamic_extensions = dynamic_extensions_for_tools(&base_dir, &selected, capabilities);

    overlap_warnings_for_tools(&selected, capabilities, suppressions, &dynamic_extensions)
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
#[allow(clippy::too_many_arguments)]
pub fn build_tools_config(
    working_dir: Option<&std::path::Path>,
    required: Option<&[String]>,
    skip: Option<&[String]>,
    explicit_tools: Option<&[String]>,
    no_runner_fallback: bool,
    tool_paths: &[String],
    biome_use_editorconfig_override: Option<bool>,
    biome_use_vcs_ignore_override: Option<bool>,
) -> Result<ToolsConfig, BoxError> {
    let mut config = load_tools_config(working_dir)?;

    let cli_path_overrides = parse_tool_path_overrides(tool_paths)?;

    for (name, path) in cli_path_overrides {
        config.paths.insert(name, path);
    }

    if no_runner_fallback {
        config.runner_fallback = false;
    }

    if let Some(value) = biome_use_editorconfig_override {
        config.biome_use_editorconfig = value;
    }

    if let Some(value) = biome_use_vcs_ignore_override {
        config.biome_use_vcs_ignore = value;
    }

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

fn has_file_in_ancestors(base: &std::path::Path, relative_path: &str) -> bool {
    let mut current = Some(base);
    while let Some(dir) = current {
        if has_file(dir, relative_path) {
            return true;
        }
        current = dir.parent();
    }
    false
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
    let has_dprint_config =
        has_file(&base_dir, "dprint.json") || has_file(&base_dir, "dprint.jsonc");
    let has_remark_config = [
        ".remarkrc",
        ".remarkrc.json",
        ".remarkrc.yml",
        ".remarkrc.yaml",
        ".remarkrc.js",
        ".remarkrc.cjs",
        ".remarkrc.mjs",
        ".remarkignore",
    ]
    .iter()
    .any(|path| has_file_in_ancestors(&base_dir, path));
    let has_shellcheck_config = has_file(&base_dir, ".shellcheckrc");

    let mut tools = Vec::new();

    if has_cargo {
        tools.push("clippy".to_string());
        tools.push("rustfmt".to_string());
        tools.push("taplo".to_string());
    }

    if has_package_json {
        tools.push("eslint".to_string());
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

    if has_dprint_config {
        tools.push("dprint".to_string());
    }

    if has_remark_config {
        tools.push("remark".to_string());
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
    let has_package_json_ancestor = has_file_in_ancestors(&base_dir, "package.json");
    let has_pyproject = has_file(&base_dir, "pyproject.toml");
    let has_requirements = has_file(&base_dir, "requirements.txt");
    let has_setup_py = has_file(&base_dir, "setup.py");
    let has_go_mod = has_file(&base_dir, "go.mod");
    let has_taplo_config = has_file(&base_dir, "taplo.toml");
    let has_dprint_config =
        has_file(&base_dir, "dprint.json") || has_file(&base_dir, "dprint.jsonc");
    let has_remark_config = [
        ".remarkrc",
        ".remarkrc.json",
        ".remarkrc.yml",
        ".remarkrc.yaml",
        ".remarkrc.js",
        ".remarkrc.cjs",
        ".remarkrc.mjs",
        ".remarkignore",
    ]
    .iter()
    .any(|path| has_file_in_ancestors(&base_dir, path));
    let has_shfmt_config = has_file(&base_dir, ".shfmt.conf");

    let mut tools = Vec::new();

    if has_cargo {
        tools.push("rustfmt".to_string());
        tools.push("taplo".to_string());
    }

    if has_package_json || has_package_json_ancestor {
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

    if has_dprint_config {
        tools.push("dprint".to_string());
    }

    if has_remark_config {
        tools.push("remark".to_string());
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

        let merged = build_tools_config(
            Some(&dir),
            Some(&required),
            Some(&skip),
            None,
            false,
            &[],
            None,
            None,
        )
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
        let merged = build_tools_config(
            Some(&dir),
            None,
            None,
            Some(&explicit),
            false,
            &[],
            None,
            None,
        )
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
        std::fs::write(dir.join("dprint.json"), "{}\n").expect("failed to write dprint.json");

        let check_tools =
            auto_detect_check_tools(Some(&dir)).expect("failed to detect check tools");
        assert!(check_tools.contains(&"clippy".to_string()));
        assert!(check_tools.contains(&"rustfmt".to_string()));
        assert!(!check_tools.contains(&"prettier".to_string()));
        assert!(check_tools.contains(&"biome".to_string()));
        assert!(check_tools.contains(&"eslint".to_string()));
        assert!(check_tools.contains(&"ruff".to_string()));
        assert!(check_tools.contains(&"black".to_string()));
        assert!(check_tools.contains(&"gofmt".to_string()));
        assert!(check_tools.contains(&"dprint".to_string()));

        let fmt_tools = auto_detect_fmt_tools(Some(&dir)).expect("failed to detect fmt tools");
        assert!(fmt_tools.contains(&"rustfmt".to_string()));
        assert!(!fmt_tools.contains(&"prettier".to_string()));
        assert!(fmt_tools.contains(&"biome".to_string()));
        assert!(fmt_tools.contains(&"ruff".to_string()));
        assert!(fmt_tools.contains(&"black".to_string()));
        assert!(fmt_tools.contains(&"gofmt".to_string()));
        assert!(fmt_tools.contains(&"dprint".to_string()));
        assert!(!fmt_tools.contains(&"mdformat".to_string()));
        assert!(!fmt_tools.contains(&"yamlfmt".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_fmt_tools_does_not_default_mdformat_for_generic_markdown_files() {
        let dir = temp_dir("clippier-auto-detect-markdown-files");
        std::fs::write(dir.join("README.md"), "# test\n").expect("failed to write README.md");

        let fmt_tools = auto_detect_fmt_tools(Some(&dir)).expect("failed to detect fmt tools");

        assert!(!fmt_tools.contains(&"mdformat".to_string()));
        assert!(!fmt_tools.contains(&"prettier".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_tools_do_not_default_yamlfmt_when_yaml_files_exist() {
        let dir = temp_dir("clippier-auto-detect-yamlfmt");
        std::fs::write(dir.join("config.yaml"), "a: 1\n").expect("failed to write yaml file");

        let check_tools =
            auto_detect_check_tools(Some(&dir)).expect("failed to detect check tools");
        let fmt_tools = auto_detect_fmt_tools(Some(&dir)).expect("failed to detect fmt tools");

        assert!(!check_tools.contains(&"yamlfmt".to_string()));
        assert!(!fmt_tools.contains(&"yamlfmt".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_tools_do_not_default_mdformat_for_mdx_even_when_configured() {
        let dir = temp_dir("clippier-auto-detect-mdx-mdformat");
        std::fs::write(dir.join("doc.mdx"), "# mdx\n").expect("failed to write mdx file");
        std::fs::write(
            dir.join(".mdformat.toml"),
            "extensions = [\"gfm\", \"mdx\"]\n",
        )
        .expect("failed to write .mdformat.toml");

        let fmt_tools = auto_detect_fmt_tools(Some(&dir)).expect("failed to detect fmt tools");

        assert!(!fmt_tools.contains(&"mdformat".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_tools_do_not_default_mdformat_for_mdx_without_plugin_config() {
        let dir = temp_dir("clippier-auto-detect-mdx-no-mdformat");
        std::fs::write(dir.join("doc.mdx"), "# mdx\n").expect("failed to write mdx file");

        let fmt_tools = auto_detect_fmt_tools(Some(&dir)).expect("failed to detect fmt tools");
        assert!(!fmt_tools.contains(&"mdformat".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_tools_include_remark_when_remark_config_exists() {
        let dir = temp_dir("clippier-auto-detect-remark");
        std::fs::write(dir.join(".remarkrc.yml"), "plugins: []\n")
            .expect("failed to write .remarkrc.yml");

        let check_tools =
            auto_detect_check_tools(Some(&dir)).expect("failed to detect check tools");
        let fmt_tools = auto_detect_fmt_tools(Some(&dir)).expect("failed to detect fmt tools");

        assert!(check_tools.contains(&"remark".to_string()));
        assert!(fmt_tools.contains(&"remark".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_tools_include_remark_from_ancestor_config() {
        let dir = temp_dir("clippier-auto-detect-remark-ancestor");
        std::fs::write(dir.join(".remarkrc.yml"), "plugins: []\n")
            .expect("failed to write .remarkrc.yml");
        let nested = dir.join("docs").join("nested");
        std::fs::create_dir_all(&nested).expect("failed to create nested dir");

        let check_tools =
            auto_detect_check_tools(Some(&nested)).expect("failed to detect check tools");
        let fmt_tools = auto_detect_fmt_tools(Some(&nested)).expect("failed to detect fmt tools");

        assert!(check_tools.contains(&"remark".to_string()));
        assert!(fmt_tools.contains(&"remark".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_fmt_tools_includes_biome_from_ancestor_package_json() {
        let dir = temp_dir("clippier-auto-detect-biome-ancestor");
        std::fs::write(dir.join("package.json"), "{}\n").expect("failed to write package.json");
        let nested = dir.join("packages").join("service");
        std::fs::create_dir_all(&nested).expect("failed to create nested dir");
        std::fs::write(
            nested.join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n",
        )
        .expect("failed to write Cargo.toml");

        let fmt_tools = auto_detect_fmt_tools(Some(&nested)).expect("failed to detect fmt tools");

        assert!(!fmt_tools.contains(&"prettier".to_string()));
        assert!(fmt_tools.contains(&"biome".to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn auto_detect_fmt_tools_ignores_generated_dirs_for_markdown_scan() {
        let dir = temp_dir("clippier-auto-detect-markdown-generated");
        let target_dir = dir.join("target");
        std::fs::create_dir_all(&target_dir).expect("failed to create target dir");
        std::fs::write(target_dir.join("README.md"), "# generated\n")
            .expect("failed to write generated markdown");

        let fmt_tools = auto_detect_fmt_tools(Some(&dir)).expect("failed to detect fmt tools");

        assert!(!fmt_tools.contains(&"mdformat".to_string()));

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

        let merged = build_tools_config(Some(&dir), None, None, None, false, &[], None, None)
            .expect("failed to build merged tools config");

        assert_eq!(merged.required, vec!["rustfmt"]);
        assert_eq!(merged.skip, vec!["rustfmt"]);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn build_tools_config_disables_runner_fallback_with_cli_override() {
        let dir = temp_dir("clippier-tools-runner-fallback-override");
        let merged = build_tools_config(Some(&dir), None, None, None, true, &[], None, None)
            .expect("failed to build merged tools config");

        assert!(!merged.runner_fallback);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn build_tools_config_tool_path_cli_overrides_config_and_validates_tool_names() {
        let dir = temp_dir("clippier-tools-path-override");
        let config_path = dir.join("clippier.toml");
        std::fs::write(
            &config_path,
            "[tools.paths]\nprettier=\"/from/config/prettier\"\n",
        )
        .expect("failed to write clippier.toml");

        let merged = build_tools_config(
            Some(&dir),
            None,
            None,
            None,
            false,
            &["prettier=/from/cli/prettier".to_string()],
            None,
            None,
        )
        .expect("failed to build merged tools config");

        assert_eq!(
            merged.paths.get("prettier"),
            Some(&"/from/cli/prettier".to_string())
        );

        let err = build_tools_config(
            Some(&dir),
            None,
            None,
            None,
            false,
            &["unknown=/tmp/tool".to_string()],
            None,
            None,
        )
        .expect_err("expected unknown tool path override to fail");
        assert!(err.to_string().contains("Unknown tool 'unknown'"));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn build_tools_config_cli_override_sets_biome_editorconfig_behavior() {
        let dir = temp_dir("clippier-tools-biome-editorconfig-override");
        let merged = build_tools_config(
            Some(&dir),
            None,
            None,
            None,
            false,
            &[],
            Some(false),
            Some(false),
        )
        .expect("failed to build merged tools config");

        assert!(!merged.biome_use_editorconfig);
        assert!(!merged.biome_use_vcs_ignore);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn overlap_warnings_include_biome_and_prettier_by_default() {
        let biome = Tool::new(
            "biome",
            "Biome",
            "biome",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );
        let prettier = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );

        let warnings = overlap_warnings_for_tools(
            &[&biome, &prettier],
            &[ToolCapability::Format],
            &[],
            &std::collections::BTreeMap::new(),
        );

        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("biome"));
        assert!(warnings[0].contains("prettier"));
        assert!(warnings[0].contains("js"));
        assert!(warnings[1].contains("HINT"));
    }

    #[test]
    fn overlap_warnings_respect_pair_extension_suppressions_case_insensitive() {
        let biome = Tool::new(
            "biome",
            "Biome",
            "biome",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );
        let prettier = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );
        let suppressions = vec![OverlapWarningSuppressRule {
            capability: OverlapWarningCapability::Format,
            tools: vec!["Prettier".to_string(), "BIOME".to_string()],
            extensions: vec!["JS".to_string(), ".Ts".to_string()],
        }];

        let warnings = overlap_warnings_for_tools(
            &[&biome, &prettier],
            &[ToolCapability::Format],
            &suppressions,
            &std::collections::BTreeMap::new(),
        );

        assert_eq!(warnings.len(), 2);
        assert!(!warnings[0].contains(", js,"));
        assert!(!warnings[0].contains(", ts,"));
        assert!(warnings[0].contains("json"));
        assert!(warnings[1].contains("HINT"));
    }

    #[test]
    fn overlap_warnings_can_be_fully_suppressed_for_pair() {
        let biome = Tool::new(
            "biome",
            "Biome",
            "biome",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );
        let prettier = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );
        let suppressions = vec![OverlapWarningSuppressRule {
            capability: OverlapWarningCapability::Format,
            tools: vec!["biome".to_string(), "prettier".to_string()],
            extensions: Vec::new(),
        }];

        let warnings = overlap_warnings_for_tools(
            &[&biome, &prettier],
            &[ToolCapability::Format],
            &suppressions,
            &std::collections::BTreeMap::new(),
        );

        assert!(warnings.is_empty());
    }

    #[test]
    fn build_tools_config_parses_overlap_warning_suppress_rules() {
        let dir = temp_dir("clippier-tools-overlap-suppress");
        let config_path = dir.join("clippier.toml");
        std::fs::write(
            &config_path,
            "[tools]\n[[tools.overlap-warning-suppress]]\ncapability = \"format\"\ntools = [\"biome\", \"prettier\"]\nextensions = [\"md\", \"mdx\"]\n",
        )
        .expect("failed to write clippier.toml");

        let merged = build_tools_config(Some(&dir), None, None, None, false, &[], None, None)
            .expect("failed to build merged tools config");

        assert_eq!(merged.overlap_warning_suppress.len(), 1);
        assert_eq!(
            merged.overlap_warning_suppress[0].tools,
            vec!["biome".to_string(), "prettier".to_string()]
        );
        assert_eq!(
            merged.overlap_warning_suppress[0].extensions,
            vec!["md".to_string(), "mdx".to_string()]
        );

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn parse_prettier_support_info_extensions_extracts_normalized_extensions() {
        let json = r#"{
            "languages": [
                {"extensions": [".js", ".TS", ".md"]},
                {"extensions": [".json"]}
            ]
        }"#;

        let extensions = parse_prettier_support_info_extensions(json)
            .expect("expected parsed support info extensions");

        assert!(extensions.contains("js"));
        assert!(extensions.contains("ts"));
        assert!(extensions.contains("md"));
        assert!(extensions.contains("json"));
    }
}
