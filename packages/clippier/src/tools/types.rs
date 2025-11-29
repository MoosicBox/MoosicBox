//! Type definitions for the tools module.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Capabilities that a tool can have
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCapability {
    /// Tool can format files
    Format,
    /// Tool can lint/check files
    Lint,
}

/// The kind of tool (how it's invoked)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolKind {
    /// Invoked via cargo (e.g., `cargo fmt`, `cargo clippy`)
    Cargo,
    /// Invoked directly as a binary
    Binary,
    /// Invoked via a runner like npx, pnpm exec, bunx
    Runner {
        /// The runner command (e.g., "npx", "pnpm exec")
        runner: String,
    },
}

/// Definition of an external tool
#[derive(Debug, Clone)]
pub struct Tool {
    /// Unique identifier for the tool
    pub name: String,

    /// Human-readable display name
    pub display_name: String,

    /// The binary name to check for (e.g., "rustfmt", "prettier")
    pub binary: String,

    /// How the tool is invoked
    pub kind: ToolKind,

    /// What capabilities this tool has
    pub capabilities: Vec<ToolCapability>,

    /// Command to run for checking/linting (without the binary name)
    /// e.g., for `cargo fmt --check`, this would be `["fmt", "--check"]`
    pub check_args: Vec<String>,

    /// Command to run for formatting (without the binary name)
    /// e.g., for `cargo fmt`, this would be `["fmt"]`
    pub format_args: Vec<String>,

    /// Optional: The path to the detected binary
    pub detected_path: Option<PathBuf>,
}

impl Tool {
    /// Creates a new tool definition
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        display_name: impl Into<String>,
        binary: impl Into<String>,
        kind: ToolKind,
        capabilities: Vec<ToolCapability>,
        check_args: Vec<String>,
        format_args: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            display_name: display_name.into(),
            binary: binary.into(),
            kind,
            capabilities,
            check_args,
            format_args,
            detected_path: None,
        }
    }

    /// Returns true if this tool can format files
    #[must_use]
    pub fn can_format(&self) -> bool {
        self.capabilities.contains(&ToolCapability::Format)
    }

    /// Returns true if this tool can lint files
    #[must_use]
    pub fn can_lint(&self) -> bool {
        self.capabilities.contains(&ToolCapability::Lint)
    }

    /// Sets the detected path for this tool
    #[must_use]
    pub fn with_detected_path(mut self, path: PathBuf) -> Self {
        self.detected_path = Some(path);
        self
    }
}

/// Configuration for tool detection and execution
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ToolsConfig {
    /// Tools that MUST be installed (error if missing)
    #[serde(default)]
    pub required: Vec<String>,

    /// Tools to skip even if detected
    #[serde(default)]
    pub skip: Vec<String>,

    /// Explicit paths for tools that can't be auto-detected
    #[serde(default)]
    pub paths: std::collections::BTreeMap<String, String>,
}

impl ToolsConfig {
    /// Creates a new empty configuration
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a required tool
    #[must_use]
    pub fn with_required(mut self, tool: impl Into<String>) -> Self {
        self.required.push(tool.into());
        self
    }

    /// Adds a tool to skip
    #[must_use]
    pub fn with_skip(mut self, tool: impl Into<String>) -> Self {
        self.skip.push(tool.into());
        self
    }

    /// Adds an explicit path for a tool
    #[must_use]
    pub fn with_path(mut self, tool: impl Into<String>, path: impl Into<String>) -> Self {
        self.paths.insert(tool.into(), path.into());
        self
    }

    /// Returns true if a tool is in the skip list
    #[must_use]
    pub fn should_skip(&self, tool_name: &str) -> bool {
        self.skip.iter().any(|s| s == tool_name)
    }

    /// Returns true if a tool is required
    #[must_use]
    pub fn is_required(&self, tool_name: &str) -> bool {
        self.required.iter().any(|s| s == tool_name)
    }

    /// Gets the explicit path for a tool, if any
    #[must_use]
    pub fn get_path(&self, tool_name: &str) -> Option<&str> {
        self.paths.get(tool_name).map(String::as_str)
    }
}
