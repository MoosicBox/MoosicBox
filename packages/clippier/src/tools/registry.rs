//! Tool registry for managing available tools.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::tools::types::{Tool, ToolCapability, ToolKind, ToolsConfig};

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
}

impl ToolRegistry {
    /// Creates a new tool registry with the given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if a required tool is not found.
    pub fn new(config: ToolsConfig) -> Result<Self, ToolError> {
        let mut registry = Self {
            tools: BTreeMap::new(),
            available: BTreeMap::new(),
            config,
        };

        // Register all built-in tools
        registry.register_builtin_tools();

        // Detect available tools
        registry.detect_tools()?;

        Ok(registry)
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
            vec!["--check".to_string(), ".".to_string()],
            vec!["--write".to_string(), ".".to_string()],
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
                    self.available.insert(name.clone(), available_tool);
                    continue;
                }
                log::warn!(
                    "Tool '{name}' configured path '{path}' not found, trying auto-detection"
                );
            }

            // Auto-detect using which
            if let Ok(path) = which::which(&tool.binary) {
                log::debug!("Tool '{name}' detected at: {}", path.display());
                let available_tool = tool.clone().with_detected_path(path);
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
