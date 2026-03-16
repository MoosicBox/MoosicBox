//! Tool registry for managing available tools.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::tools::types::{Tool, ToolCapability, ToolKind, ToolsConfig};

enum ToolResolution {
    Binary(PathBuf),
    Runner {
        runner: String,
        runner_args: Vec<String>,
        tool_binary: String,
    },
}

const BIOME_EDITORCONFIG_FLAG: &str = "--use-editorconfig=true";

/// Error type for tool-related operations
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// A required tool was not found
    #[error("Required tool '{0}' not found. Please install it and ensure it's in your PATH.")]
    RequiredToolNotFound(String),

    /// Failed to detect a tool
    #[error("Failed to detect tool '{0}': {1}")]
    DetectionFailed(String, String),

    /// No tools available
    #[error("No tools available for the requested operation")]
    NoToolsAvailable,
}

/// Registry of available tools
#[derive(Debug)]
pub struct ToolRegistry {
    /// All known tool definitions
    tools: BTreeMap<String, Tool>,

    /// Tools that have been detected as available
    available: BTreeMap<String, Tool>,

    /// Configuration for tool selection
    config: ToolsConfig,

    /// Working directory used for local tool discovery
    working_dir: PathBuf,
}

impl ToolRegistry {
    /// Creates a new tool registry with the given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if a required tool is not found.
    pub fn new(config: ToolsConfig, working_dir: Option<&Path>) -> Result<Self, ToolError> {
        let resolved_working_dir = match working_dir {
            Some(path) => path.to_path_buf(),
            None => Self::current_working_dir()?,
        };

        let mut registry = Self {
            tools: BTreeMap::new(),
            available: BTreeMap::new(),
            config,
            working_dir: resolved_working_dir,
        };

        // Register all built-in tools
        registry.register_builtin_tools();

        // Detect available tools
        registry.detect_tools()?;

        Ok(registry)
    }

    fn current_working_dir() -> Result<PathBuf, ToolError> {
        std::env::current_dir()
            .map_err(|e| ToolError::DetectionFailed("cwd".to_string(), e.to_string()))
    }

    fn resolve_node_bin_in_ancestors(base_dir: &Path, bin_name: &str) -> Option<PathBuf> {
        let mut current = Some(base_dir);
        while let Some(dir) = current {
            let candidate = dir.join("node_modules").join(".bin").join(bin_name);
            if candidate.exists() {
                return Some(candidate);
            }
            current = dir.parent();
        }
        None
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_preferred_tool(
        name: &str,
        tool: &Tool,
        base_dir: &Path,
        runner_fallback: bool,
    ) -> Option<ToolResolution> {
        match name {
            "prettier" => {
                if let Some(path) = Self::resolve_node_bin_in_ancestors(base_dir, "prettier") {
                    return Some(ToolResolution::Binary(path));
                }

                if let Ok(path) = which::which("prettier") {
                    return Some(ToolResolution::Binary(path));
                }

                if !runner_fallback {
                    return None;
                }

                if which::which("bunx").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "bunx".to_string(),
                        runner_args: vec![],
                        tool_binary: "prettier".to_string(),
                    });
                }

                if which::which("pnpm").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "pnpm".to_string(),
                        runner_args: vec!["dlx".to_string()],
                        tool_binary: "prettier".to_string(),
                    });
                }

                if which::which("npx").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "npx".to_string(),
                        runner_args: vec!["--yes".to_string()],
                        tool_binary: "prettier".to_string(),
                    });
                }

                None
            }
            "biome" => {
                if let Some(path) = Self::resolve_node_bin_in_ancestors(base_dir, &tool.binary) {
                    return Some(ToolResolution::Binary(path));
                }

                if let Ok(path) = which::which(&tool.binary) {
                    return Some(ToolResolution::Binary(path));
                }

                if !runner_fallback {
                    return None;
                }

                if which::which("bunx").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "bunx".to_string(),
                        runner_args: vec![],
                        tool_binary: "@biomejs/biome".to_string(),
                    });
                }

                if which::which("pnpm").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "pnpm".to_string(),
                        runner_args: vec!["dlx".to_string()],
                        tool_binary: "@biomejs/biome".to_string(),
                    });
                }

                if which::which("npx").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "npx".to_string(),
                        runner_args: vec!["--yes".to_string()],
                        tool_binary: "@biomejs/biome".to_string(),
                    });
                }

                None
            }
            "eslint" => {
                if let Some(path) = Self::resolve_node_bin_in_ancestors(base_dir, &tool.binary) {
                    return Some(ToolResolution::Binary(path));
                }

                if let Ok(path) = which::which(&tool.binary) {
                    return Some(ToolResolution::Binary(path));
                }

                if !runner_fallback {
                    return None;
                }

                if which::which("bunx").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "bunx".to_string(),
                        runner_args: vec![],
                        tool_binary: "eslint".to_string(),
                    });
                }

                if which::which("pnpm").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "pnpm".to_string(),
                        runner_args: vec!["dlx".to_string()],
                        tool_binary: "eslint".to_string(),
                    });
                }

                if which::which("npx").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "npx".to_string(),
                        runner_args: vec!["--yes".to_string()],
                        tool_binary: "eslint".to_string(),
                    });
                }

                None
            }
            "dprint" => {
                if let Some(path) = Self::resolve_node_bin_in_ancestors(base_dir, &tool.binary) {
                    return Some(ToolResolution::Binary(path));
                }

                if let Ok(path) = which::which(&tool.binary) {
                    return Some(ToolResolution::Binary(path));
                }

                if !runner_fallback {
                    return None;
                }

                if which::which("bunx").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "bunx".to_string(),
                        runner_args: vec![],
                        tool_binary: "dprint".to_string(),
                    });
                }

                if which::which("pnpm").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "pnpm".to_string(),
                        runner_args: vec!["dlx".to_string()],
                        tool_binary: "dprint".to_string(),
                    });
                }

                if which::which("npx").is_ok() {
                    return Some(ToolResolution::Runner {
                        runner: "npx".to_string(),
                        runner_args: vec!["--yes".to_string()],
                        tool_binary: "dprint".to_string(),
                    });
                }

                None
            }
            _ => which::which(&tool.binary).ok().map(ToolResolution::Binary),
        }
    }

    fn maybe_apply_biome_editorconfig(tool: &mut Tool, enabled: bool) {
        if tool.name != "biome" || !enabled {
            return;
        }

        if !tool
            .check_args
            .iter()
            .any(|arg| arg == BIOME_EDITORCONFIG_FLAG)
        {
            let insert_index = tool
                .check_args
                .iter()
                .enumerate()
                .skip(1)
                .find_map(|(index, arg)| {
                    if arg.starts_with('-') {
                        None
                    } else {
                        Some(index)
                    }
                })
                .unwrap_or(tool.check_args.len());
            tool.check_args
                .insert(insert_index, BIOME_EDITORCONFIG_FLAG.to_string());
        }

        if !tool
            .format_args
            .iter()
            .any(|arg| arg == BIOME_EDITORCONFIG_FLAG)
        {
            let insert_index = tool
                .format_args
                .iter()
                .enumerate()
                .skip(1)
                .find_map(|(index, arg)| {
                    if arg.starts_with('-') {
                        None
                    } else {
                        Some(index)
                    }
                })
                .unwrap_or(tool.format_args.len());
            tool.format_args
                .insert(insert_index, BIOME_EDITORCONFIG_FLAG.to_string());
        }
    }

    /// Registers a tool definition
    pub fn register(&mut self, tool: Tool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Registers all built-in tool definitions
    #[allow(clippy::too_many_lines)]
    fn register_builtin_tools(&mut self) {
        // Rust tools
        self.register(Tool::new(
            "rustfmt",
            "Rust Formatter",
            "cargo",
            ToolKind::Cargo,
            vec![ToolCapability::Format],
            vec!["fmt".to_string(), "--check".to_string()],
            vec!["fmt".to_string()],
        ));

        self.register(Tool::new(
            "clippy",
            "Rust Linter",
            "cargo",
            ToolKind::Cargo,
            vec![ToolCapability::Lint],
            vec![
                "clippy".to_string(),
                "--all-targets".to_string(),
                "--".to_string(),
                "-D".to_string(),
                "warnings".to_string(),
            ],
            vec![], // Clippy doesn't have a "fix" mode in the same way
        ));

        // TOML
        self.register(Tool::new(
            "taplo",
            "TOML Formatter",
            "taplo",
            ToolKind::Binary,
            vec![ToolCapability::Format, ToolCapability::Lint],
            vec!["fmt".to_string(), "--check".to_string()],
            vec!["fmt".to_string()],
        ));

        // JavaScript/TypeScript - Prettier
        self.register(Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![
                "--check".to_string(),
                "--ignore-unknown".to_string(),
                ".".to_string(),
            ],
            vec![
                "--write".to_string(),
                "--ignore-unknown".to_string(),
                ".".to_string(),
            ],
        ));

        // JavaScript/TypeScript - Biome
        self.register(Tool::new(
            "biome",
            "Biome",
            "biome",
            ToolKind::Binary,
            vec![ToolCapability::Format, ToolCapability::Lint],
            vec!["check".to_string(), ".".to_string()],
            vec!["format".to_string(), "--write".to_string(), ".".to_string()],
        ));

        // JavaScript/TypeScript - ESLint
        self.register(Tool::new(
            "eslint",
            "ESLint",
            "eslint",
            ToolKind::Binary,
            vec![ToolCapability::Lint],
            vec![".".to_string()],
            vec!["--fix".to_string(), ".".to_string()],
        ));

        // Dprint
        self.register(Tool::new(
            "dprint",
            "Dprint",
            "dprint",
            ToolKind::Binary,
            vec![ToolCapability::Format, ToolCapability::Lint],
            vec!["check".to_string()],
            vec!["fmt".to_string()],
        ));

        // Python - Ruff
        self.register(Tool::new(
            "ruff",
            "Ruff",
            "ruff",
            ToolKind::Binary,
            vec![ToolCapability::Format, ToolCapability::Lint],
            vec!["check".to_string(), ".".to_string()],
            vec!["format".to_string(), ".".to_string()],
        ));

        // Python - Black
        self.register(Tool::new(
            "black",
            "Black",
            "black",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec!["--check".to_string(), ".".to_string()],
            vec![".".to_string()],
        ));

        // Go
        self.register(Tool::new(
            "gofmt",
            "Go Formatter",
            "gofmt",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec!["-l".to_string(), ".".to_string()],
            vec!["-w".to_string(), ".".to_string()],
        ));

        // Shell
        self.register(Tool::new(
            "shfmt",
            "Shell Formatter",
            "shfmt",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec!["-d".to_string(), ".".to_string()],
            vec!["-w".to_string(), ".".to_string()],
        ));

        self.register(Tool::new(
            "shellcheck",
            "ShellCheck",
            "shellcheck",
            ToolKind::Binary,
            vec![ToolCapability::Lint],
            vec![], // ShellCheck needs specific files, handled differently
            vec![],
        ));
    }

    /// Detects which tools are available on the system
    fn detect_tools(&mut self) -> Result<(), ToolError> {
        for (name, tool) in &self.tools {
            // Skip if configured to skip
            if self.config.should_skip(name) {
                log::debug!("Skipping tool '{name}' (configured to skip)");
                continue;
            }

            // Check if there's an explicit path configured
            if let Some(path) = self.config.get_path(name) {
                let path_buf = PathBuf::from(path);
                if path_buf.exists() || which::which(path).is_ok() {
                    log::debug!("Tool '{name}' found at configured path: {path}");
                    let mut available_tool = tool.clone();
                    available_tool.detected_path = Some(path_buf);
                    Self::maybe_apply_biome_editorconfig(
                        &mut available_tool,
                        self.config.biome_use_editorconfig,
                    );
                    self.available.insert(name.clone(), available_tool);
                    continue;
                }
                log::warn!(
                    "Tool '{name}' configured path '{path}' not found, trying auto-detection"
                );
            }

            // Auto-detect from local node bins and/or PATH
            if let Some(resolution) = Self::resolve_preferred_tool(
                name,
                tool,
                &self.working_dir,
                self.config.runner_fallback,
            ) {
                let available_tool = match resolution {
                    ToolResolution::Binary(path) => {
                        log::debug!("Tool '{name}' detected at: {}", path.display());
                        let mut detected_tool = tool.clone().with_detected_path(path);
                        Self::maybe_apply_biome_editorconfig(
                            &mut detected_tool,
                            self.config.biome_use_editorconfig,
                        );
                        detected_tool
                    }
                    ToolResolution::Runner {
                        runner,
                        runner_args,
                        tool_binary,
                    } => {
                        log::debug!("Tool '{name}' will run via runner: {runner} {runner_args:?}");
                        let mut available_tool = tool.clone();
                        available_tool.kind = ToolKind::Runner {
                            runner,
                            runner_args,
                        };
                        available_tool.binary = tool_binary;
                        Self::maybe_apply_biome_editorconfig(
                            &mut available_tool,
                            self.config.biome_use_editorconfig,
                        );
                        available_tool
                    }
                };
                self.available.insert(name.clone(), available_tool);
            } else {
                log::debug!("Tool '{name}' not found");

                // Check if this tool is required
                if self.config.is_required(name) {
                    return Err(ToolError::RequiredToolNotFound(name.clone()));
                }
            }
        }

        Ok(())
    }

    /// Returns all available tools
    #[must_use]
    pub fn available_tools(&self) -> Vec<&Tool> {
        self.available.values().collect()
    }

    /// Returns available tools that can format
    #[must_use]
    pub fn formatters(&self) -> Vec<&Tool> {
        self.available.values().filter(|t| t.can_format()).collect()
    }

    /// Returns available tools that can lint
    #[must_use]
    pub fn linters(&self) -> Vec<&Tool> {
        self.available.values().filter(|t| t.can_lint()).collect()
    }

    /// Gets a specific tool by name if available
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.available.get(name)
    }

    /// Returns true if a tool is available
    #[must_use]
    pub fn is_available(&self, name: &str) -> bool {
        self.available.contains_key(name)
    }

    /// Returns the number of available tools
    #[must_use]
    pub fn available_count(&self) -> usize {
        self.available.len()
    }

    /// Lists all known tools with their availability status
    #[must_use]
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools
            .values()
            .map(|tool| ToolInfo {
                name: tool.name.clone(),
                display_name: tool.display_name.clone(),
                available: self.available.contains_key(&tool.name),
                required: self.config.is_required(&tool.name),
                skipped: self.config.should_skip(&tool.name),
                capabilities: tool.capabilities.clone(),
                path: self
                    .available
                    .get(&tool.name)
                    .and_then(|t| t.detected_path.clone()),
            })
            .collect()
    }
}

/// Information about a tool for display purposes
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// Tool identifier
    pub name: String,
    /// Human-readable name
    pub display_name: String,
    /// Whether the tool is available
    pub available: bool,
    /// Whether the tool is required
    pub required: bool,
    /// Whether the tool is skipped
    pub skipped: bool,
    /// Tool capabilities
    pub capabilities: Vec<ToolCapability>,
    /// Path to the tool binary if detected
    pub path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
        std::fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }

    #[test]
    fn resolve_preferred_prettier_path_uses_local_prettier_bin() {
        let dir = temp_dir("clippier-prettier-priority");
        let bin_dir = dir.join("node_modules").join(".bin");
        std::fs::create_dir_all(&bin_dir).expect("failed to create node bin dir");
        std::fs::write(bin_dir.join("prettier"), "").expect("failed to write prettier file");

        let tool = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );

        let detected = ToolRegistry::resolve_preferred_tool("prettier", &tool, &dir, true)
            .expect("expected prettier variant to resolve");

        let path = match detected {
            ToolResolution::Binary(path) => path,
            ToolResolution::Runner { .. } => panic!("expected binary resolution"),
        };

        assert!(path.ends_with("node_modules/.bin/prettier"));
        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn resolve_preferred_prettier_path_uses_ancestor_node_bin() {
        let dir = temp_dir("clippier-prettier-ancestor");
        let root_bin = dir.join("node_modules").join(".bin");
        let nested = dir.join("packages").join("service");
        std::fs::create_dir_all(&root_bin).expect("failed to create root node bin dir");
        std::fs::create_dir_all(&nested).expect("failed to create nested dir");
        std::fs::write(root_bin.join("prettier"), "").expect("failed to write prettier file");

        let tool = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );

        let detected = ToolRegistry::resolve_preferred_tool("prettier", &tool, &nested, true)
            .expect("expected prettier variant to resolve");

        let path = match detected {
            ToolResolution::Binary(path) => path,
            ToolResolution::Runner { .. } => panic!("expected binary resolution"),
        };

        assert!(path.ends_with("node_modules/.bin/prettier"));
        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn resolve_preferred_prettier_uses_runner_when_enabled_and_binary_missing() {
        let dir = temp_dir("clippier-prettier-runner");

        let tool = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );

        let detected = ToolRegistry::resolve_preferred_tool("prettier", &tool, &dir, true);

        if which::which("prettier").is_ok() {
            if let Some(ToolResolution::Binary(_)) = detected {
                std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
                return;
            }
            panic!("expected direct prettier binary resolution when prettier is installed");
        }

        let detected = detected.expect("expected fallback resolution");

        match detected {
            ToolResolution::Binary(_) => panic!("expected runner fallback resolution"),
            ToolResolution::Runner {
                runner,
                runner_args,
                tool_binary,
            } => {
                if which::which("bunx").is_ok() {
                    assert_eq!(runner, "bunx");
                    assert!(runner_args.is_empty());
                    assert_eq!(tool_binary, "prettier");
                } else if which::which("pnpm").is_ok() {
                    assert_eq!(runner, "pnpm");
                    assert_eq!(runner_args, vec!["dlx"]);
                    assert_eq!(tool_binary, "prettier");
                } else if which::which("npx").is_ok() {
                    assert_eq!(runner, "npx");
                    assert_eq!(runner_args, vec!["--yes"]);
                    assert_eq!(tool_binary, "prettier");
                } else {
                    panic!("expected at least one runner in test environment");
                }
            }
        }

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn resolve_preferred_prettier_returns_none_when_runner_fallback_disabled() {
        let dir = temp_dir("clippier-prettier-runner-disabled");

        let tool = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );

        let detected = ToolRegistry::resolve_preferred_tool("prettier", &tool, &dir, false);

        if which::which("prettier").is_ok() {
            assert!(matches!(detected, Some(ToolResolution::Binary(_))));
        } else {
            assert!(detected.is_none());
        }

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }
}
