//! Tool registry for managing available tools.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::tools::EffectiveScope;
use crate::tools::catalog::TOOL_CATALOG;
use crate::tools::types::{Tool, ToolCapability, ToolKind, ToolsConfig};

#[derive(Debug, Clone)]
enum ToolResolution {
    Binary(PathBuf),
    Runner {
        runner: String,
        runner_args: Vec<String>,
        tool_binary: String,
    },
}

const BIOME_EDITORCONFIG_FLAG: &str = "--use-editorconfig=true";
const BIOME_VCS_ENABLED_TRUE_FLAG: &str = "--vcs-enabled=true";
const BIOME_VCS_ENABLED_FALSE_FLAG: &str = "--vcs-enabled=false";
const BIOME_VCS_IGNORE_TRUE_FLAG: &str = "--vcs-use-ignore-file=true";
const BIOME_FILES_IGNORE_UNKNOWN_TRUE_FLAG: &str = "--files-ignore-unknown=true";

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

    fn node_project_boundary(base_dir: &Path) -> PathBuf {
        let mut nearest_package = None;
        let mut current = Some(base_dir);
        while let Some(dir) = current {
            if dir.join(".git").exists() {
                return dir.to_path_buf();
            }
            let package_json = dir.join("package.json");
            if package_json.exists() {
                nearest_package.get_or_insert_with(|| dir.to_path_buf());
                if std::fs::read_to_string(&package_json)
                    .ok()
                    .and_then(|contents| serde_json::from_str::<serde_json::Value>(&contents).ok())
                    .is_some_and(|value| value.get("workspaces").is_some())
                {
                    return dir.to_path_buf();
                }
            }
            current = dir.parent();
        }
        nearest_package.unwrap_or_else(|| base_dir.to_path_buf())
    }

    fn resolve_node_bin_in_ancestors(base_dir: &Path, bin_name: &str) -> Option<PathBuf> {
        let boundary = Self::node_project_boundary(base_dir);
        let mut current = Some(base_dir);
        while let Some(dir) = current {
            let candidate = dir.join("node_modules").join(".bin").join(bin_name);
            if candidate.exists() {
                return Some(candidate);
            }
            if dir == boundary {
                break;
            }
            current = dir.parent();
        }
        None
    }

    fn resolve_configured_executable(base_dir: &Path, configured: &str) -> Option<PathBuf> {
        let path = PathBuf::from(configured);
        if path.is_absolute() && path.exists() {
            return Some(path);
        }
        let project_relative = base_dir.join(&path);
        if project_relative.exists() {
            return Some(project_relative);
        }
        which::which(configured).ok()
    }

    fn is_nix_system() -> bool {
        if std::env::var_os("IN_NIX_SHELL").is_some() {
            return true;
        }

        if Path::new("/etc/NIXOS").exists() {
            return true;
        }

        if let Some(path) = std::env::var_os("PATH") {
            let path_value = path.to_string_lossy();
            if path_value.contains("/nix/store")
                || path_value.contains("/etc/profiles/per-user")
                || path_value.contains("/nix/var/nix/profiles")
            {
                return true;
            }
        }

        false
    }

    fn nix_fallback_enabled(config: &ToolsConfig) -> bool {
        config.nix_fallback && Self::is_nix_system() && which::which("nix").is_ok()
    }

    fn nix_package_for_tool(config: &ToolsConfig, tool_name: &str) -> Option<String> {
        if let Some(value) = config.nix_packages.get(tool_name) {
            return Some(value.clone());
        }

        match tool_name {
            "mdformat" => Some("nixpkgs#mdformat".to_string()),
            "yamlfmt" => Some("nixpkgs#yamlfmt".to_string()),
            _ => None,
        }
    }

    fn nix_package_for_mdformat_extension(config: &ToolsConfig, extension: &str) -> Option<String> {
        if let Some(value) = config.nix_packages.get(&format!("mdformat-{extension}")) {
            return Some(value.clone());
        }

        match extension {
            "gfm" => Some("nixpkgs#python3Packages.mdformat-gfm".to_string()),
            _ => None,
        }
    }

    fn find_file_in_ancestors(base_dir: &Path, names: &[&str]) -> Option<PathBuf> {
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

    fn parse_mdformat_requested_extensions(base_dir: &Path) -> BTreeSet<String> {
        fn parse_extensions(value: &toml::Value) -> BTreeSet<String> {
            value
                .as_array()
                .into_iter()
                .flat_map(|values| values.iter())
                .filter_map(toml::Value::as_str)
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| value == "gfm" || value == "mdx" || value == "frontmatter")
                .collect()
        }

        let mut requested = BTreeSet::new();

        if let Some(path) = Self::find_file_in_ancestors(base_dir, &[".mdformat.toml"])
            && let Ok(contents) = std::fs::read_to_string(path)
            && let Ok(parsed) = toml::from_str::<toml::Value>(&contents)
            && let Some(extensions) = parsed.get("extensions")
        {
            requested.extend(parse_extensions(extensions));
        }

        if let Some(path) = Self::find_file_in_ancestors(base_dir, &["pyproject.toml"])
            && let Ok(contents) = std::fs::read_to_string(path)
            && let Ok(parsed) = toml::from_str::<toml::Value>(&contents)
            && let Some(extensions) = parsed
                .get("tool")
                .and_then(|tool| tool.get("mdformat"))
                .and_then(|mdformat| mdformat.get("extensions"))
        {
            requested.extend(parse_extensions(extensions));
        }

        requested
    }

    fn mdformat_resolution_supports_extension(
        resolution: &ToolResolution,
        extension: &str,
        base_dir: &Path,
    ) -> bool {
        let (program, mut args) = match resolution {
            ToolResolution::Binary(path) => (path.display().to_string(), Vec::new()),
            ToolResolution::Runner {
                runner,
                runner_args,
                tool_binary,
            } => {
                let mut values = runner_args.clone();
                values.push(tool_binary.clone());
                (runner.clone(), values)
            }
        };

        args.extend([
            "--check".to_string(),
            "--extensions".to_string(),
            extension.to_string(),
            "-".to_string(),
        ]);

        let mut command = std::process::Command::new(program);
        command
            .args(args)
            .current_dir(base_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        let Ok(mut child) = command.spawn() else {
            return false;
        };

        if let Some(mut child_stdin) = child.stdin.take() {
            use std::io::Write as _;
            let _ = child_stdin.write_all(b"# mdformat extension probe\n");
        }

        child.wait().is_ok_and(|status| status.success())
    }

    fn mdformat_supported_extensions_for_resolution(
        resolution: &ToolResolution,
        requested_extensions: &BTreeSet<String>,
        base_dir: &Path,
    ) -> BTreeSet<String> {
        requested_extensions
            .iter()
            .filter(|extension| {
                Self::mdformat_resolution_supports_extension(resolution, extension, base_dir)
            })
            .cloned()
            .collect()
    }

    fn mdformat_extension_subsets_desc(
        requested_extensions: &BTreeSet<String>,
    ) -> Vec<Vec<String>> {
        let values = requested_extensions.iter().cloned().collect::<Vec<_>>();
        let mut subsets = Vec::new();

        let total = 1_usize << values.len();
        for mask in 1..total {
            let mut subset = Vec::new();
            for (index, value) in values.iter().enumerate() {
                if mask & (1 << index) != 0 {
                    subset.push(value.clone());
                }
            }
            subsets.push(subset);
        }

        subsets.sort_by_key(|subset| std::cmp::Reverse(subset.len()));
        subsets
    }

    fn mdformat_runner_candidates(
        config: &ToolsConfig,
        requested_extensions: &BTreeSet<String>,
    ) -> Vec<ToolResolution> {
        let mut candidates = Vec::new();
        let extension_subsets = Self::mdformat_extension_subsets_desc(requested_extensions);

        if which::which("uvx").is_ok() {
            if extension_subsets.is_empty() {
                candidates.push(ToolResolution::Runner {
                    runner: "uvx".to_string(),
                    runner_args: Vec::new(),
                    tool_binary: "mdformat".to_string(),
                });
            }

            for subset in &extension_subsets {
                let mut runner_args = Vec::new();
                for extension in subset {
                    let package = match extension.as_str() {
                        "gfm" => "mdformat-gfm",
                        "mdx" => "mdformat-mdx",
                        "frontmatter" => "mdformat-frontmatter",
                        _ => continue,
                    };
                    runner_args.push("--with".to_string());
                    runner_args.push(package.to_string());
                }

                candidates.push(ToolResolution::Runner {
                    runner: "uvx".to_string(),
                    runner_args,
                    tool_binary: "mdformat".to_string(),
                });
            }
        }

        if Self::nix_fallback_enabled(config) && !requested_extensions.is_empty() {
            for subset in &extension_subsets {
                let mut runner_args = vec![
                    "shell".to_string(),
                    "nixpkgs#uv".to_string(),
                    "--command".to_string(),
                    "uvx".to_string(),
                ];
                for extension in subset {
                    let package = match extension.as_str() {
                        "gfm" => "mdformat-gfm",
                        "mdx" => "mdformat-mdx",
                        "frontmatter" => "mdformat-frontmatter",
                        _ => continue,
                    };
                    runner_args.push("--with".to_string());
                    runner_args.push(package.to_string());
                }

                candidates.push(ToolResolution::Runner {
                    runner: "nix".to_string(),
                    runner_args,
                    tool_binary: "mdformat".to_string(),
                });
            }
        }

        if requested_extensions.is_empty() && which::which("pipx").is_ok() {
            candidates.push(ToolResolution::Runner {
                runner: "pipx".to_string(),
                runner_args: vec!["run".to_string()],
                tool_binary: "mdformat".to_string(),
            });
        }

        if Self::nix_fallback_enabled(config)
            && let Some(mdformat_package) = Self::nix_package_for_tool(config, "mdformat")
        {
            let mut packages = vec![mdformat_package];

            for extension in requested_extensions {
                if let Some(package) = Self::nix_package_for_mdformat_extension(config, extension) {
                    packages.push(package);
                }
            }

            candidates.push(Self::nix_runner_resolution(&packages, "mdformat"));
        }

        candidates
    }

    fn node_runner_resolution(tool_binary: &str) -> Option<ToolResolution> {
        if which::which("bunx").is_ok() {
            return Some(ToolResolution::Runner {
                runner: "bunx".to_string(),
                runner_args: vec![],
                tool_binary: tool_binary.to_string(),
            });
        }

        if which::which("pnpm").is_ok() {
            return Some(ToolResolution::Runner {
                runner: "pnpm".to_string(),
                runner_args: vec!["dlx".to_string()],
                tool_binary: tool_binary.to_string(),
            });
        }

        if which::which("npx").is_ok() {
            return Some(ToolResolution::Runner {
                runner: "npx".to_string(),
                runner_args: vec!["--yes".to_string()],
                tool_binary: tool_binary.to_string(),
            });
        }

        None
    }

    fn remark_runner_resolution() -> Option<ToolResolution> {
        if which::which("npx").is_ok() {
            return Some(ToolResolution::Runner {
                runner: "npx".to_string(),
                runner_args: vec![
                    "--yes".to_string(),
                    "--package".to_string(),
                    "remark-cli".to_string(),
                    "--package".to_string(),
                    "remark-frontmatter".to_string(),
                    "--package".to_string(),
                    "remark-gfm".to_string(),
                    "--package".to_string(),
                    "remark-mdx".to_string(),
                ],
                tool_binary: "remark".to_string(),
            });
        }

        None
    }

    fn nix_runner_resolution(packages: &[String], binary: &str) -> ToolResolution {
        let mut runner_args = vec!["shell".to_string()];
        runner_args.extend(packages.iter().cloned());
        runner_args.push("--command".to_string());

        ToolResolution::Runner {
            runner: "nix".to_string(),
            runner_args,
            tool_binary: binary.to_string(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_preferred_tool(
        name: &str,
        tool: &Tool,
        base_dir: &Path,
        runner_fallback: bool,
        config: &ToolsConfig,
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

                Self::node_runner_resolution("prettier")
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

                Self::node_runner_resolution("@biomejs/biome")
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

                Self::node_runner_resolution("eslint")
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

                Self::node_runner_resolution("dprint")
            }
            "remark" => {
                if let Some(path) = Self::resolve_node_bin_in_ancestors(base_dir, &tool.binary) {
                    return Some(ToolResolution::Binary(path));
                }

                if let Ok(path) = which::which(&tool.binary) {
                    return Some(ToolResolution::Binary(path));
                }

                if !runner_fallback {
                    return None;
                }

                Self::remark_runner_resolution()
            }
            "mdformat" => {
                let requested_extensions = Self::parse_mdformat_requested_extensions(base_dir);

                if !requested_extensions.is_empty() && runner_fallback {
                    let mut candidates =
                        Self::mdformat_runner_candidates(config, &requested_extensions);
                    if let Ok(path) = which::which("mdformat") {
                        candidates.push(ToolResolution::Binary(path));
                    }

                    let mut best: Option<(usize, ToolResolution)> = None;
                    for candidate in candidates {
                        let supported = Self::mdformat_supported_extensions_for_resolution(
                            &candidate,
                            &requested_extensions,
                            base_dir,
                        )
                        .len();

                        if best.as_ref().is_none_or(|(count, _)| supported > *count) {
                            best = Some((supported, candidate));
                        }
                    }

                    return best.map(|(_, candidate)| candidate);
                }

                if let Ok(path) = which::which("mdformat") {
                    return Some(ToolResolution::Binary(path));
                }

                if !runner_fallback {
                    return None;
                }

                Self::mdformat_runner_candidates(config, &requested_extensions)
                    .into_iter()
                    .next()
            }
            "yamlfmt" => {
                if let Ok(path) = which::which("yamlfmt") {
                    return Some(ToolResolution::Binary(path));
                }

                if !runner_fallback {
                    return None;
                }

                if Self::nix_fallback_enabled(config)
                    && let Some(package) = Self::nix_package_for_tool(config, "yamlfmt")
                {
                    return Some(Self::nix_runner_resolution(&[package], "yamlfmt"));
                }

                None
            }
            _ => which::which(&tool.binary).ok().map(ToolResolution::Binary),
        }
    }

    fn to_absolute_path(path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
        }
    }

    fn insert_biome_flag(args: &mut Vec<String>, flag: &str) {
        if args.iter().any(|arg| arg == flag) {
            return;
        }

        let insert_index = args
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
            .unwrap_or(args.len());
        args.insert(insert_index, flag.to_string());
    }

    fn insert_biome_vcs_root(args: &mut Vec<String>, vcs_root: &Path) {
        let already_set = args
            .iter()
            .any(|arg| arg == "--vcs-root" || arg.starts_with("--vcs-root="));
        if already_set {
            return;
        }

        args.push("--vcs-root".to_string());
        args.push(vcs_root.display().to_string());
    }

    fn find_biome_config_path(base_dir: &Path) -> Option<PathBuf> {
        let mut current = Some(base_dir);
        while let Some(dir) = current {
            let jsonc = dir.join("biome.jsonc");
            if jsonc.exists() {
                return Some(jsonc);
            }
            let json = dir.join("biome.json");
            if json.exists() {
                return Some(json);
            }
            current = dir.parent();
        }
        None
    }

    fn insert_biome_config_path(args: &mut Vec<String>, config_path: &Path) {
        let already_set = args
            .iter()
            .any(|arg| arg == "--config-path" || arg.starts_with("--config-path="));
        if already_set {
            return;
        }

        args.push("--config-path".to_string());
        args.push(config_path.display().to_string());
    }

    fn maybe_apply_biome_settings(tool: &mut Tool, config: &ToolsConfig, working_dir: &Path) {
        if tool.name != "biome" {
            return;
        }

        if config.biome_use_editorconfig {
            Self::insert_biome_flag(&mut tool.check_args, BIOME_EDITORCONFIG_FLAG);
            Self::insert_biome_flag(&mut tool.format_args, BIOME_EDITORCONFIG_FLAG);
        }

        Self::insert_biome_flag(&mut tool.check_args, BIOME_FILES_IGNORE_UNKNOWN_TRUE_FLAG);
        Self::insert_biome_flag(&mut tool.format_args, BIOME_FILES_IGNORE_UNKNOWN_TRUE_FLAG);

        if config.biome_use_vcs_ignore {
            Self::insert_biome_flag(&mut tool.check_args, BIOME_VCS_ENABLED_TRUE_FLAG);
            Self::insert_biome_flag(&mut tool.check_args, BIOME_VCS_IGNORE_TRUE_FLAG);
            Self::insert_biome_flag(&mut tool.format_args, BIOME_VCS_ENABLED_TRUE_FLAG);
            Self::insert_biome_flag(&mut tool.format_args, BIOME_VCS_IGNORE_TRUE_FLAG);

            let absolute_root = Self::to_absolute_path(working_dir);
            Self::insert_biome_vcs_root(&mut tool.check_args, &absolute_root);
            Self::insert_biome_vcs_root(&mut tool.format_args, &absolute_root);
        } else {
            Self::insert_biome_flag(&mut tool.check_args, BIOME_VCS_ENABLED_FALSE_FLAG);
            Self::insert_biome_flag(&mut tool.format_args, BIOME_VCS_ENABLED_FALSE_FLAG);
        }

        if let Some(config_path) = Self::find_biome_config_path(working_dir) {
            let absolute_path = Self::to_absolute_path(&config_path);
            Self::insert_biome_config_path(&mut tool.check_args, &absolute_path);
            Self::insert_biome_config_path(&mut tool.format_args, &absolute_path);
        }
    }

    /// Registers a tool definition
    pub fn register(&mut self, tool: Tool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Registers all built-in tool definitions.
    fn register_builtin_tools(&mut self) {
        for entry in TOOL_CATALOG {
            self.register(entry.tool());
        }
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
                if let Some(path_buf) = Self::resolve_configured_executable(&self.working_dir, path)
                {
                    log::debug!(
                        "Tool '{name}' found at configured path: {}",
                        path_buf.display()
                    );
                    let mut available_tool = tool.clone();
                    available_tool.detected_path = Some(path_buf);
                    Self::maybe_apply_biome_settings(
                        &mut available_tool,
                        &self.config,
                        &self.working_dir,
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
                &self.config,
            ) {
                let available_tool = match resolution {
                    ToolResolution::Binary(path) => {
                        log::debug!("Tool '{name}' detected at: {}", path.display());
                        let mut detected_tool = tool.clone().with_detected_path(path);
                        Self::maybe_apply_biome_settings(
                            &mut detected_tool,
                            &self.config,
                            &self.working_dir,
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
                        Self::maybe_apply_biome_settings(
                            &mut available_tool,
                            &self.config,
                            &self.working_dir,
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

    /// Returns the runner configuration.
    #[must_use]
    pub const fn config(&self) -> &ToolsConfig {
        &self.config
    }

    /// Returns the directory used for local tool discovery and execution.
    #[must_use]
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// Returns the effective execution mode for one tool.
    #[must_use]
    pub fn execution_mode(&self, name: &str) -> Option<String> {
        self.available.get(name).map(|tool| match &tool.kind {
            ToolKind::Cargo => "cargo".to_string(),
            ToolKind::Binary => "binary".to_string(),
            ToolKind::Runner { runner, .. } => format!("runner:{runner}"),
        })
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

    /// Lists tools enriched with one automatic selection plan.
    #[must_use]
    pub fn list_tools_with_plan(&self, plan: &crate::tools::ToolPlan) -> Vec<ToolInfo> {
        let selected = plan
            .tools
            .iter()
            .map(|tool| (tool.name.as_str(), tool))
            .collect::<BTreeMap<_, _>>();
        let unavailable = plan.unavailable.iter().collect::<BTreeSet<_>>();
        self.list_tools()
            .into_iter()
            .map(|mut info| {
                if let Some(planned) = selected.get(info.name.as_str()) {
                    info.relevant = true;
                    info.selected = true;
                    info.evidence = Some(format!(
                        "{:?}:{}",
                        planned.evidence.kind,
                        planned.evidence.path.display()
                    ));
                    info.format_extensions = planned.format_extensions.iter().cloned().collect();
                    info.format_order = planned.format_order;
                } else if unavailable.contains(&info.name) {
                    info.relevant = true;
                }
                info
            })
            .collect()
    }

    /// Lists all known tools with their availability status
    #[must_use]
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools
            .values()
            .map(|tool| {
                let effective_tool = self.available.get(&tool.name).unwrap_or(tool);
                let (execution_mode, runner) = execution_metadata(effective_tool);
                ToolInfo {
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
                    execution_mode,
                    runner,
                    relevant: false,
                    selected: false,
                    configured: self.config.tools.contains_key(&tool.name),
                    evidence: None,
                    format_extensions: Vec::new(),
                    format_order: None,
                    automatic_exclusions: EffectiveScope::resolve(
                        self.config
                            .scope_base
                            .as_deref()
                            .unwrap_or(&self.working_dir),
                        self.config.effective_scope(),
                    )
                    .exclusions,
                }
            })
            .collect()
    }
}

/// Information about a tool for display purposes
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
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
    /// Effective execution mode for this tool (`cargo`, `binary`, `runner`)
    pub execution_mode: String,
    /// Runner command when execution mode is `runner` (e.g. `bunx`, `uvx`, `nix`)
    pub runner: Option<String>,
    /// Whether repository evidence makes the tool relevant.
    pub relevant: bool,
    /// Whether the automatic plan selected the tool.
    pub selected: bool,
    /// Whether typed repository configuration exists for the tool.
    pub configured: bool,
    /// Human-readable strongest selection evidence.
    pub evidence: Option<String>,
    /// Formatter extensions owned by this tool.
    pub format_extensions: Vec<String>,
    /// Configured formatter pipeline order.
    pub format_order: Option<i32>,
    /// Effective automatic exclusion patterns used by planning and execution.
    pub automatic_exclusions: Vec<String>,
}

impl ToolInfo {
    /// Formats selection, evidence, ownership, and execution metadata for raw
    /// tool-list output.
    #[must_use]
    pub fn raw_summary(&self) -> String {
        let status = if self.skipped {
            "SKIPPED"
        } else if self.available {
            "AVAILABLE"
        } else if self.required {
            "REQUIRED (missing)"
        } else {
            "not found"
        };
        let mode = self.runner.as_ref().map_or_else(
            || self.execution_mode.clone(),
            |runner| format!("{} ({runner})", self.execution_mode),
        );
        let selection = if self.selected {
            "SELECTED"
        } else if self.relevant {
            "RELEVANT"
        } else if self.configured {
            "CONFIGURED"
        } else {
            "not relevant"
        };
        let mut details = Vec::new();
        if let Some(evidence) = &self.evidence {
            details.push(format!("evidence={evidence}"));
        }
        if !self.format_extensions.is_empty() {
            details.push(format!("owns={}", self.format_extensions.join(",")));
        }
        if let Some(order) = self.format_order {
            details.push(format!("format-order={order}"));
        }
        if !self.automatic_exclusions.is_empty() {
            details.push(format!(
                "automatic-exclusions={}",
                self.automatic_exclusions.join(",")
            ));
        }
        let details = if details.is_empty() {
            String::new()
        } else {
            format!(" {}", details.join(" "))
        };
        format!(
            "{}: {} [{}] {selection}{details}",
            self.display_name, status, mode
        )
    }
}

fn execution_metadata(tool: &Tool) -> (String, Option<String>) {
    match &tool.kind {
        ToolKind::Cargo => ("cargo".to_string(), None),
        ToolKind::Binary => ("binary".to_string(), None),
        ToolKind::Runner { runner, .. } => ("runner".to_string(), Some(runner.clone())),
    }
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
    fn configured_executable_resolves_relative_to_working_directory_before_path() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("bin/prettier");
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        std::fs::write(&local, "").unwrap();

        let resolved = ToolRegistry::resolve_configured_executable(root.path(), "bin/prettier");
        assert_eq!(resolved.as_deref(), Some(local.as_path()));
    }

    #[test]
    fn raw_summary_includes_plan_metadata() {
        let info = ToolInfo {
            name: "prettier".to_string(),
            display_name: "Prettier".to_string(),
            available: true,
            required: false,
            skipped: false,
            capabilities: vec![ToolCapability::Format],
            path: Some(PathBuf::from("/bin/prettier")),
            execution_mode: "binary".to_string(),
            runner: None,
            relevant: true,
            selected: true,
            configured: true,
            evidence: Some("NativeConfig:.prettierrc".to_string()),
            format_extensions: vec!["js".to_string(), "md".to_string()],
            format_order: Some(20),
            automatic_exclusions: vec!["node_modules/**".to_string()],
        };

        let summary = info.raw_summary();
        assert!(summary.contains("SELECTED"));
        assert!(summary.contains("evidence=NativeConfig:.prettierrc"));
        assert!(summary.contains("owns=js,md"));
        assert!(summary.contains("format-order=20"));
        assert!(summary.contains("automatic-exclusions=node_modules/**"));
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

        let detected = ToolRegistry::resolve_preferred_tool(
            "prettier",
            &tool,
            &dir,
            true,
            &ToolsConfig::default(),
        )
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
        std::fs::write(
            dir.join("package.json"),
            r#"{"private":true,"workspaces":["packages/*"]}"#,
        )
        .unwrap();
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

        let detected = ToolRegistry::resolve_preferred_tool(
            "prettier",
            &tool,
            &nested,
            true,
            &ToolsConfig::default(),
        )
        .expect("expected prettier variant to resolve");

        let path = match detected {
            ToolResolution::Binary(path) => path,
            ToolResolution::Runner { .. } => panic!("expected binary resolution"),
        };

        assert!(path.ends_with("node_modules/.bin/prettier"));
        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn node_bin_lookup_stops_at_nearest_unrelated_package_boundary() {
        let outer = tempfile::tempdir().unwrap();
        let outer_bin = outer.path().join("node_modules/.bin");
        let project = outer.path().join("project");
        let nested = project.join("src");
        std::fs::create_dir_all(&outer_bin).unwrap();
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(outer_bin.join("prettier"), "").unwrap();
        std::fs::write(project.join("package.json"), "{}").unwrap();

        assert!(ToolRegistry::resolve_node_bin_in_ancestors(&nested, "prettier").is_none());
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

        let detected = ToolRegistry::resolve_preferred_tool(
            "prettier",
            &tool,
            &dir,
            true,
            &ToolsConfig::default(),
        );

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
    fn default_resolution_never_uses_download_capable_runners() {
        let dir = temp_dir("clippier-installed-only-default");
        let config = ToolsConfig::default();
        assert!(!config.runner_fallback);
        assert!(!config.nix_fallback);

        for entry in TOOL_CATALOG {
            let resolution = ToolRegistry::resolve_preferred_tool(
                entry.name,
                &entry.tool(),
                &dir,
                config.runner_fallback,
                &config,
            );
            assert!(
                !matches!(resolution, Some(ToolResolution::Runner { .. })),
                "{} unexpectedly resolved through a runner",
                entry.name
            );
        }

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn automatic_registry_paths_never_resolve_through_acquisition_runners() {
        let root = tempfile::tempdir().unwrap();
        for (manifest, tool_name) in [
            (".prettierrc", "prettier"),
            (".mdformat.toml", "mdformat"),
            (".yamlfmt", "yamlfmt"),
        ] {
            std::fs::write(root.path().join(manifest), "").unwrap();
            let mut config = ToolsConfig::default();
            config.tools.insert(
                tool_name.to_string(),
                crate::tools::ToolPolicy {
                    executable: Some(
                        root.path()
                            .join(format!("missing-{tool_name}"))
                            .to_string_lossy()
                            .to_string(),
                    ),
                    ..Default::default()
                },
            );
            let registry = ToolRegistry::new(config, Some(root.path())).unwrap();
            assert!(
                registry
                    .get(tool_name)
                    .is_none_or(|tool| !matches!(tool.kind, ToolKind::Runner { .. }))
            );
        }
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

        let detected = ToolRegistry::resolve_preferred_tool(
            "prettier",
            &tool,
            &dir,
            false,
            &ToolsConfig::default(),
        );

        if which::which("prettier").is_ok() {
            assert!(matches!(detected, Some(ToolResolution::Binary(_))));
        } else {
            assert!(detected.is_none());
        }

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn biome_settings_include_editorconfig_and_vcs_flags_by_default() {
        let dir = temp_dir("clippier-biome-settings-default");
        let mut tool = Tool::new(
            "biome",
            "Biome",
            "biome",
            ToolKind::Binary,
            vec![ToolCapability::Format, ToolCapability::Lint],
            vec!["check".to_string(), ".".to_string()],
            vec!["format".to_string(), "--write".to_string(), ".".to_string()],
        );

        let config = ToolsConfig::default();
        ToolRegistry::maybe_apply_biome_settings(&mut tool, &config, &dir);

        assert!(
            tool.check_args
                .iter()
                .any(|arg| arg == BIOME_EDITORCONFIG_FLAG)
        );
        assert!(
            tool.check_args
                .iter()
                .any(|arg| arg == BIOME_VCS_ENABLED_TRUE_FLAG)
        );
        assert!(
            tool.check_args
                .iter()
                .any(|arg| arg == BIOME_VCS_IGNORE_TRUE_FLAG)
        );
        assert!(tool.check_args.iter().any(|arg| arg == "--vcs-root"));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn biome_settings_disable_vcs_when_configured_off() {
        let dir = temp_dir("clippier-biome-settings-no-vcs");
        let mut tool = Tool::new(
            "biome",
            "Biome",
            "biome",
            ToolKind::Binary,
            vec![ToolCapability::Format, ToolCapability::Lint],
            vec!["check".to_string(), ".".to_string()],
            vec!["format".to_string(), "--write".to_string(), ".".to_string()],
        );

        let config = ToolsConfig {
            biome_use_vcs_ignore: false,
            ..ToolsConfig::default()
        };
        ToolRegistry::maybe_apply_biome_settings(&mut tool, &config, &dir);

        assert!(
            tool.check_args
                .iter()
                .any(|arg| arg == BIOME_VCS_ENABLED_FALSE_FLAG)
        );
        assert!(
            !tool
                .check_args
                .iter()
                .any(|arg| arg == BIOME_VCS_IGNORE_TRUE_FLAG)
        );

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn nix_package_defaults_include_mdformat_and_yamlfmt() {
        let config = ToolsConfig::default();

        assert_eq!(
            ToolRegistry::nix_package_for_tool(&config, "mdformat"),
            Some("nixpkgs#mdformat".to_string())
        );
        assert_eq!(
            ToolRegistry::nix_package_for_tool(&config, "yamlfmt"),
            Some("nixpkgs#yamlfmt".to_string())
        );
    }

    #[test]
    fn nix_package_overrides_are_applied() {
        let mut config = ToolsConfig::default();
        config
            .nix_packages
            .insert("yamlfmt".to_string(), "flake#custom-yamlfmt".to_string());

        assert_eq!(
            ToolRegistry::nix_package_for_tool(&config, "yamlfmt"),
            Some("flake#custom-yamlfmt".to_string())
        );
    }

    #[test]
    fn list_tools_with_plan_reports_configured_and_selected_states() {
        let mut tools = BTreeMap::new();
        let available = BTreeMap::new();
        let tool = TOOL_CATALOG
            .iter()
            .find(|entry| entry.name == "prettier")
            .unwrap()
            .tool();
        tools.insert(tool.name.clone(), tool);
        let mut config = ToolsConfig::default();
        config
            .tools
            .insert("prettier".to_string(), crate::tools::ToolPolicy::default());
        let registry = ToolRegistry {
            tools,
            available,
            config,
            working_dir: std::env::temp_dir(),
        };
        let plan = crate::tools::ToolPlan {
            tools: vec![crate::tools::PlannedTool {
                name: "prettier".to_string(),
                evidence: crate::tools::SelectionEvidence {
                    kind: crate::tools::SelectionEvidenceKind::NativeConfig,
                    path: PathBuf::from(".prettierrc"),
                },
                format_extensions: BTreeSet::from(["md".to_string()]),
                format_order: Some(10),
            }],
            unavailable: Vec::new(),
        };

        let info = registry.list_tools_with_plan(&plan);
        assert!(info[0].configured);
        assert!(info[0].relevant);
        assert!(info[0].selected);
        assert_eq!(info[0].format_extensions, vec!["md"]);
        assert_eq!(info[0].format_order, Some(10));
        assert!(info[0].evidence.as_deref().unwrap().contains(".prettierrc"));
    }

    #[test]
    fn list_tools_includes_runner_execution_metadata() {
        let mut tools = BTreeMap::new();
        let mut available = BTreeMap::new();

        let base = Tool::new(
            "dprint",
            "dprint",
            "dprint",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec!["check".to_string()],
            vec!["fmt".to_string()],
        );
        tools.insert("dprint".to_string(), base.clone());

        let mut resolved = base;
        resolved.kind = ToolKind::Runner {
            runner: "nix".to_string(),
            runner_args: vec!["shell".to_string()],
        };
        available.insert("dprint".to_string(), resolved);

        let registry = ToolRegistry {
            tools,
            available,
            config: ToolsConfig::default(),
            working_dir: std::env::temp_dir(),
        };

        let info = registry.list_tools();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].execution_mode, "runner");
        assert_eq!(info[0].runner, Some("nix".to_string()));
    }
}
