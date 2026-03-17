//! Tool registry for managing available tools.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

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
            vec![ToolCapability::Format],
            vec!["format".to_string()],
            vec!["format".to_string(), "--write".to_string()],
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

        // Markdown/MDX - clippier_md
        self.register(Tool::new(
            "clippier_md",
            "Clippier MD",
            "cargo",
            ToolKind::Cargo,
            vec![ToolCapability::Format],
            vec![
                "run".to_string(),
                "-p".to_string(),
                "clippier_md".to_string(),
                "--".to_string(),
                "fmt".to_string(),
                "--check".to_string(),
                ".".to_string(),
            ],
            vec![
                "run".to_string(),
                "-p".to_string(),
                "clippier_md".to_string(),
                "--".to_string(),
                "fmt".to_string(),
                ".".to_string(),
            ],
        ));

        // Markdown/MDX - remark
        self.register(Tool::new(
            "remark",
            "remark",
            "remark",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![".".to_string(), "--ext".to_string(), "md,mdx".to_string()],
            vec![
                ".".to_string(),
                "--output".to_string(),
                "--ext".to_string(),
                "md,mdx".to_string(),
            ],
        ));

        // Markdown - mdformat
        self.register(Tool::new(
            "mdformat",
            "mdformat",
            "mdformat",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec!["--check".to_string(), ".".to_string()],
            vec![".".to_string()],
        ));

        // YAML - yamlfmt
        self.register(Tool::new(
            "yamlfmt",
            "yamlfmt",
            "yamlfmt",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec!["-lint".to_string(), ".".to_string()],
            vec![".".to_string()],
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
                }
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
    /// Effective execution mode for this tool (`cargo`, `binary`, `runner`)
    pub execution_mode: String,
    /// Runner command when execution mode is `runner` (e.g. `bunx`, `uvx`, `nix`)
    pub runner: Option<String>,
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
