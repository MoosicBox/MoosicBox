//! Type definitions for the tools module.

use std::collections::BTreeSet;
use std::path::PathBuf;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// File-selection mode used by the format command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum FormatScope {
    /// Format staged, unstaged, renamed, and untracked Git files.
    #[default]
    Changed,
    /// Format files changed from a Git merge base plus local changes.
    Branch,
    /// Preserve each formatter's native recursive/all-files behavior.
    All,
}

/// Configuration specific to formatter file selection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FormatConfig {
    /// File-selection mode for formatter invocations.
    #[serde(default)]
    pub scope: FormatScope,
    /// Base revision used by branch scope.
    pub git_base: Option<String>,
}

/// Capabilities that a tool can have
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolCapability {
    /// Tool can format files
    Format,
    /// Tool can lint/check files
    Lint,
}

/// Capability scope used for overlap warning suppression rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlapWarningCapability {
    /// Formatter overlap warnings
    Format,
    /// Linter/check overlap warnings
    Lint,
}

impl OverlapWarningCapability {
    /// Returns true when this suppression capability matches a tool capability.
    #[must_use]
    pub const fn matches(self, capability: ToolCapability) -> bool {
        matches!(
            (self, capability),
            (Self::Format, ToolCapability::Format) | (Self::Lint, ToolCapability::Lint)
        )
    }
}

/// Rule for suppressing overlap warnings between tools.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct OverlapWarningSuppressRule {
    /// Capability scope for this suppression
    pub capability: OverlapWarningCapability,

    /// Pair of tools to suppress warnings for (order-insensitive)
    pub tools: Vec<String>,

    /// Optional extension subset (case-insensitive, without leading dot)
    #[serde(default)]
    pub extensions: Vec<String>,
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
        /// Additional arguments passed before tool binary (e.g., `dlx`)
        runner_args: Vec<String>,
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

    /// Native extensions requested by the tool's resolved configuration.
    pub native_requested_extensions: BTreeSet<String>,

    /// Requested native extensions supported by the resolved runtime.
    pub native_supported_extensions: BTreeSet<String>,

    /// Effective native format extensions resolved for this tool.
    pub native_format_extensions: Option<BTreeSet<String>>,

    /// Native include patterns resolved from the tool's own configuration.
    pub native_includes: Vec<String>,

    /// Resolved native ignore file consumed by this tool.
    pub native_ignore_path: Option<PathBuf>,

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
            native_requested_extensions: BTreeSet::new(),
            native_supported_extensions: BTreeSet::new(),
            native_format_extensions: None,
            native_includes: Vec::new(),
            native_ignore_path: None,
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

/// Resolved formatter selection for a command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatSelection {
    /// Run formatters with their native recursive/all-files behavior.
    All,
    /// Run formatters only for these paths, relative to the working directory.
    Files(BTreeSet<PathBuf>),
    /// No repository could be discovered from the working directory.
    NoRepository,
}

/// Automatic selection policy for one registered tool.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolSelectionMode {
    /// Select the tool from repository evidence.
    #[default]
    Auto,
    /// Select the tool whenever it is available.
    Enabled,
    /// Never select the tool automatically.
    Disabled,
}

/// Per-tool selection, coverage, and execution policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ToolPolicy {
    /// Automatic selection policy.
    #[serde(default)]
    pub mode: ToolSelectionMode,
    /// Capabilities enabled for this tool. Empty means all catalog capabilities.
    #[serde(default)]
    pub capabilities: BTreeSet<ToolCapability>,
    /// Additional file globs included for this tool.
    #[serde(default)]
    pub include: Vec<String>,
    /// File globs excluded for this tool.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Explicit executable path for this tool.
    pub executable: Option<String>,
    /// Explicit formatter ownership extensions.
    #[serde(default)]
    pub format_extensions: BTreeSet<String>,
    /// Formatter pipeline order. Tools with the same value may overlap and warn.
    pub format_order: Option<i32>,
}

impl ToolPolicy {
    /// Returns whether this policy enables a capability.
    #[must_use]
    pub fn enables(&self, capability: ToolCapability) -> bool {
        self.capabilities.is_empty() || self.capabilities.contains(&capability)
    }
}

/// Configuration for tool detection and execution
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolsConfig {
    /// Tools that MUST be installed (error if missing)
    #[serde(default)]
    pub required: Vec<String>,

    /// Tools to skip even if detected
    #[serde(default)]
    pub skip: Vec<String>,

    /// Explicit executable paths for tools that cannot be auto-detected.
    #[serde(default)]
    pub executables: std::collections::BTreeMap<String, String>,

    /// Allow executing missing tools through package manager runners
    #[serde(default)]
    pub runner_fallback: bool,

    /// Make biome read `.editorconfig` when formatting/linting
    #[serde(default = "default_true")]
    pub biome_use_editorconfig: bool,

    /// Make biome use VCS ignore semantics for file traversal
    #[serde(default = "default_true")]
    pub biome_use_vcs_ignore: bool,

    /// Suppress overlap warnings for specific tool pairs/capabilities/extensions
    #[serde(default)]
    pub overlap_warning_suppress: Vec<OverlapWarningSuppressRule>,

    /// Allow Nix-based ephemeral tool fallback when running on Nix systems
    #[serde(default)]
    pub nix_fallback: bool,

    /// Optional per-tool Nix package overrides (e.g. `nixpkgs#yamlfmt`)
    #[serde(default)]
    pub nix_packages: std::collections::BTreeMap<String, String>,

    /// Typed policy keyed by registered tool ID.
    #[serde(default)]
    pub tools: std::collections::BTreeMap<String, ToolPolicy>,

    /// Repository content boundaries shared by file-oriented tools.
    #[serde(default)]
    pub scope: super::ScopeConfig,

    /// Formatter-specific file-selection policy.
    #[serde(default)]
    pub format: FormatConfig,

    /// Directory containing the configuration that defined runner scope.
    #[serde(skip)]
    pub scope_base: Option<PathBuf>,
}

const fn default_true() -> bool {
    true
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            required: Vec::new(),
            skip: Vec::new(),
            executables: std::collections::BTreeMap::new(),
            runner_fallback: false,
            biome_use_editorconfig: true,
            biome_use_vcs_ignore: true,
            overlap_warning_suppress: Vec::new(),
            nix_fallback: false,
            nix_packages: std::collections::BTreeMap::new(),
            tools: std::collections::BTreeMap::new(),
            scope: super::ScopeConfig::default(),
            format: FormatConfig::default(),
            scope_base: None,
        }
    }
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
        self.executables.insert(tool.into(), path.into());
        self
    }

    /// Whether an automatic exclusion profile should be disabled when its
    /// excluded path is explicitly included by a per-tool policy.
    #[must_use]
    pub fn explicitly_includes_exclusion_profile(&self, profile_id: &str) -> bool {
        let Some(profile) = super::scope::AUTOMATIC_EXCLUSION_PROFILES
            .iter()
            .find(|profile| profile.id == profile_id)
        else {
            return false;
        };
        self.tools.values().any(|policy| {
            policy.include.iter().any(|pattern| {
                let normalized = pattern.trim_start_matches('/');
                profile.exclusions.iter().any(|excluded| {
                    normalized == *excluded
                        || normalized
                            .strip_suffix(excluded)
                            .is_some_and(|prefix| prefix.ends_with('/'))
                })
            })
        })
    }

    /// Returns the effective global scope after applying explicit automatic
    /// profile re-inclusion policy.
    #[must_use]
    pub fn effective_scope(&self) -> super::ScopeConfig {
        let mut scope = self.scope.clone();
        for profile in super::scope::AUTOMATIC_EXCLUSION_PROFILES {
            if self.explicitly_includes_exclusion_profile(profile.id) {
                scope.disable_profiles.insert(profile.id.to_string());
            }
        }
        scope
    }

    /// Returns true if a tool is in the skip list
    #[must_use]
    pub fn should_skip(&self, tool_name: &str) -> bool {
        self.skip.iter().any(|s| s == tool_name)
            || self
                .tools
                .get(tool_name)
                .is_some_and(|policy| policy.mode == ToolSelectionMode::Disabled)
    }

    /// Returns true if a tool is required
    #[must_use]
    pub fn is_required(&self, tool_name: &str) -> bool {
        self.required.iter().any(|s| s == tool_name)
    }

    /// Gets the explicit path for a tool, if any
    #[must_use]
    pub fn get_path(&self, tool_name: &str) -> Option<&str> {
        self.tools
            .get(tool_name)
            .and_then(|policy| policy.executable.as_deref())
            .or_else(|| self.executables.get(tool_name).map(String::as_str))
    }

    /// Gets typed policy for one tool.
    #[must_use]
    pub fn tool_policy(&self, tool_name: &str) -> Option<&ToolPolicy> {
        self.tools.get(tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declared_exclusion_can_be_explicitly_reincluded() {
        for profile in super::super::scope::AUTOMATIC_EXCLUSION_PROFILES {
            for exclusion in profile.exclusions {
                let mut config = ToolsConfig::default();
                config.tools.insert(
                    "test".to_owned(),
                    ToolPolicy {
                        include: vec![format!("nested/{exclusion}")],
                        ..ToolPolicy::default()
                    },
                );
                assert!(
                    config
                        .effective_scope()
                        .disable_profiles
                        .contains(profile.id),
                    "{}: {exclusion}",
                    profile.id
                );
            }
        }
    }

    #[test]
    fn explicit_include_disables_only_its_automatic_profile() {
        let mut config = ToolsConfig::default();
        config.tools.insert(
            "rustfmt".to_string(),
            ToolPolicy {
                include: vec!["target/generated/**".to_string()],
                ..Default::default()
            },
        );
        assert!(!config.explicitly_includes_exclusion_profile("rust"));

        config.tools.get_mut("rustfmt").unwrap().include = vec!["target/**".to_string()];
        assert!(config.explicitly_includes_exclusion_profile("rust"));
        assert!(!config.explicitly_includes_exclusion_profile("node"));
        let effective = config.effective_scope();
        assert!(effective.disable_profiles.contains("rust"));
        assert!(!effective.disable_profiles.contains("node"));
    }
}
