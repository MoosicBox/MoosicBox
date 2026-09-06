//! Canonical metadata for Clippier's built-in formatter and linter integrations.

use std::collections::BTreeSet;

use super::{Tool, ToolCapability, ToolKind};

/// Relationship between two tools which may intentionally cover the same files.
#[derive(Debug, Clone, Copy)]
pub struct CompatibleOverlap {
    /// First tool ID.
    pub left: &'static str,
    /// Second tool ID.
    pub right: &'static str,
    /// Capability for which the relationship applies.
    pub capability: ToolCapability,
    /// Extension subset; empty means every shared extension.
    pub extensions: &'static [&'static str],
}

/// Evidence-backed compatible overlaps. Entries must only be added after the
/// tools' transformations are verified as complementary and order-safe.
///
/// The initial support matrix intentionally contains no such relationship:
/// every pair of registered formatters can rewrite shared syntax, so configured
/// overlap continues to warn unless a repository explicitly suppresses it.
pub const COMPATIBLE_OVERLAPS: &[CompatibleOverlap] = &[];

/// Returns extensions whose overlap is cataloged as compatible.
#[must_use]
pub fn compatible_overlap_extensions(
    left: &str,
    right: &str,
    capability: ToolCapability,
) -> BTreeSet<String> {
    COMPATIBLE_OVERLAPS
        .iter()
        .filter(|relationship| {
            relationship.capability == capability
                && ((relationship.left == left && relationship.right == right)
                    || (relationship.left == right && relationship.right == left))
        })
        .flat_map(|relationship| relationship.extensions.iter())
        .map(|extension| (*extension).to_string())
        .collect()
}

/// Location and shape of configuration embedded in a repository manifest.
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedConfigSignal {
    /// Tool ID configured by this signal.
    pub tool: &'static str,
    /// Manifest basename.
    pub manifest: &'static str,
    /// Top-level object or table containing the tool configuration.
    pub container: Option<&'static str>,
    /// Configuration key within the container.
    pub key: &'static str,
}

/// Repository evidence used to decide whether a tool is relevant.
#[derive(Debug, Clone, Copy)]
pub struct ToolSignals {
    /// Manifest files which activate the tool's ecosystem defaults.
    pub manifests: &'static [&'static str],
    /// Native configuration files which explicitly activate the tool.
    pub configs: &'static [&'static str],
    /// File extensions which may activate a content-based default.
    pub content_extensions: &'static [&'static str],
}

/// Explicit native adapter for irreducible tool CLI behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAdapter {
    /// Standard catalog invocation.
    Standard,
    /// Isolated, scoped rustfmt execution.
    Rustfmt,
    /// Workspace-local Clippier Markdown delegation.
    ClippierMarkdown,
    /// Prettier ignore-path behavior.
    Prettier,
    /// Biome configuration and VCS flags.
    Biome,
    /// mdformat extension probing.
    Mdformat,
    /// Strict remark check behavior.
    Remark,
}

/// Returns the file's format key, including conventional extensionless build files.
/// Keys are also used in formatter extension policies so inventory and execution agree.
pub fn file_format(path: &std::path::Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    match name {
        "Dockerfile" | "Containerfile" => Some("dockerfile"),
        "BUILD" | "BUILD.bazel" | "WORKSPACE" | "WORKSPACE.bazel" | "MODULE.bazel" => Some("bzl"),
        "CMakeLists.txt" => Some("cmake"),
        _ if name.starts_with("Dockerfile.")
            || name.starts_with("Containerfile.")
            || name.ends_with(".Dockerfile") =>
        {
            Some("dockerfile")
        }
        _ => path.extension()?.to_str(),
    }
}

/// Canonical metadata for a built-in tool.
#[derive(Debug, Clone, Copy)]
pub struct ToolCatalogEntry {
    /// Stable tool identifier.
    pub name: &'static str,
    /// Human-readable name.
    pub display_name: &'static str,
    /// Executable name.
    pub binary: &'static str,
    /// How the executable is invoked.
    pub kind: ToolKindSpec,
    /// Supported capabilities.
    pub capabilities: &'static [ToolCapability],
    /// Arguments used by `clippier check`.
    pub check_args: &'static [&'static str],
    /// Arguments used by `clippier fmt`.
    pub format_args: &'static [&'static str],
    /// Repository relevance signals.
    pub signals: ToolSignals,
    /// Extensions supported while formatting.
    pub format_extensions: &'static [&'static str],
    /// Extensions supported while linting.
    pub lint_extensions: &'static [&'static str],
    /// Lower values win automatic formatter ownership conflicts.
    pub formatter_priority: u16,
}

/// Declarative executable kind used by catalog entries.
#[derive(Debug, Clone, Copy)]
pub enum ToolKindSpec {
    /// Invoke through Cargo.
    Cargo,
    /// Invoke a standalone binary.
    Binary,
}

impl ToolCatalogEntry {
    /// Converts catalog metadata into the existing executable tool model.
    #[must_use]
    pub fn tool(self) -> Tool {
        Tool::new(
            self.name,
            self.display_name,
            self.binary,
            match self.kind {
                ToolKindSpec::Cargo => ToolKind::Cargo,
                ToolKindSpec::Binary => ToolKind::Binary,
            },
            self.capabilities.to_vec(),
            self.check_args.iter().map(ToString::to_string).collect(),
            self.format_args.iter().map(ToString::to_string).collect(),
        )
    }

    /// Returns the explicit native adapter for irreducible CLI behavior.
    #[must_use]
    pub fn adapter(self) -> ToolAdapter {
        match self.name {
            "rustfmt" => ToolAdapter::Rustfmt,
            "clippier_md" => ToolAdapter::ClippierMarkdown,
            "prettier" => ToolAdapter::Prettier,
            "biome" => ToolAdapter::Biome,
            "mdformat" => ToolAdapter::Mdformat,
            "remark" => ToolAdapter::Remark,
            _ => ToolAdapter::Standard,
        }
    }

    /// Returns the package-runner package for Node ecosystem tools.
    #[must_use]
    pub fn node_runner_package(self) -> Option<&'static str> {
        match self.name {
            "cspell" => Some("cspell"),
            "oxfmt" => Some("oxfmt"),
            "oxlint" => Some("oxlint"),
            "pyright" => Some("pyright"),
            "prettier" => Some("prettier"),
            "biome" => Some("@biomejs/biome"),
            "eslint" => Some("eslint"),
            "dprint" => Some("dprint"),
            "remark" => Some("remark-cli"),
            "stylelint" => Some("stylelint"),
            "markdownlint" => Some("markdownlint-cli"),
            _ => None,
        }
    }

    /// Returns whether project-local Node executable lookup applies.
    #[must_use]
    pub fn uses_local_node_bin(self) -> bool {
        self.node_runner_package().is_some()
    }

    /// Returns whether generic execution should pass resolved scoped files.
    #[must_use]
    pub fn uses_scoped_file_arguments(self) -> bool {
        (self.capabilities.contains(&ToolCapability::Format) && self.name != "rustfmt")
            || self.check_args.contains(&".")
    }

    /// Returns the extensions for one capability.
    #[must_use]
    pub fn extensions(self, capability: ToolCapability) -> BTreeSet<String> {
        let values = match capability {
            ToolCapability::Format => self.format_extensions,
            ToolCapability::Lint => self.lint_extensions,
        };
        values.iter().map(|value| (*value).to_string()).collect()
    }
}

const FORMAT: &[ToolCapability] = &[ToolCapability::Format];
const LINT: &[ToolCapability] = &[ToolCapability::Lint];
const BOTH: &[ToolCapability] = &[ToolCapability::Format, ToolCapability::Lint];
const NONE: &[&str] = &[];
const EMBEDDED_CONFIG_SIGNALS: &[EmbeddedConfigSignal] = &[
    EmbeddedConfigSignal {
        tool: "black",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "black",
    },
    EmbeddedConfigSignal {
        tool: "codespell",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "codespell",
    },
    EmbeddedConfigSignal {
        tool: "isort",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "isort",
    },
    EmbeddedConfigSignal {
        tool: "mdformat",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "mdformat",
    },
    EmbeddedConfigSignal {
        tool: "mypy",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "mypy",
    },
    EmbeddedConfigSignal {
        tool: "prettier",
        manifest: "package.json",
        container: None,
        key: "prettier",
    },
    EmbeddedConfigSignal {
        tool: "pylint",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "pylint",
    },
    EmbeddedConfigSignal {
        tool: "pyright",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "pyright",
    },
    EmbeddedConfigSignal {
        tool: "ruff",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "ruff",
    },
    EmbeddedConfigSignal {
        tool: "sqlfluff",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "sqlfluff",
    },
    EmbeddedConfigSignal {
        tool: "stylelint",
        manifest: "package.json",
        container: None,
        key: "stylelint",
    },
    EmbeddedConfigSignal {
        tool: "ty",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "ty",
    },
    EmbeddedConfigSignal {
        tool: "yapf",
        manifest: "pyproject.toml",
        container: Some("tool"),
        key: "yapf",
    },
];

/// Returns native configurations embedded in repository manifests for a tool.
#[must_use]
pub fn embedded_config_signals(name: &str) -> &'static [EmbeddedConfigSignal] {
    let start = EMBEDDED_CONFIG_SIGNALS.partition_point(|signal| signal.tool < name);
    let end = EMBEDDED_CONFIG_SIGNALS.partition_point(|signal| signal.tool <= name);
    &EMBEDDED_CONFIG_SIGNALS[start..end]
}

macro_rules! entry {
    ($name:literal, $display:literal, $binary:literal, $kind:ident, $caps:expr,
     $check:expr, $format:expr, $manifests:expr, $configs:expr, $content:expr,
     $fmt_ext:expr, $lint_ext:expr, $priority:literal) => {
        ToolCatalogEntry {
            name: $name,
            display_name: $display,
            binary: $binary,
            kind: ToolKindSpec::$kind,
            capabilities: $caps,
            check_args: $check,
            format_args: $format,
            signals: ToolSignals {
                manifests: $manifests,
                configs: $configs,
                content_extensions: $content,
            },
            format_extensions: $fmt_ext,
            lint_extensions: $lint_ext,
            formatter_priority: $priority,
        }
    };
}

/// All built-in integrations. This is the canonical source for registration,
/// discovery signals, capabilities, extensions, and ownership priority.
pub const TOOL_CATALOG: &[ToolCatalogEntry] = &[
    entry!(
        "rustfmt",
        "Rust Formatter",
        "cargo",
        Cargo,
        FORMAT,
        &["fmt", "--check"],
        &["fmt"],
        &["Cargo.toml"],
        &["rustfmt.toml", ".rustfmt.toml"],
        NONE,
        &["rs"],
        NONE,
        10
    ),
    entry!(
        "clippy",
        "Rust Linter",
        "cargo",
        Cargo,
        LINT,
        &["clippy", "--all-targets", "--", "-D", "warnings"],
        NONE,
        &["Cargo.toml"],
        NONE,
        NONE,
        NONE,
        &["rs"],
        0
    ),
    entry!(
        "taplo",
        "TOML Formatter",
        "taplo",
        Binary,
        BOTH,
        &["fmt", "--check"],
        &["fmt"],
        &["Cargo.toml"],
        &["taplo.toml", ".taplo.toml"],
        NONE,
        &["toml"],
        &["toml"],
        20
    ),
    entry!(
        "prettier",
        "Prettier",
        "prettier",
        Binary,
        FORMAT,
        &["--check", "--ignore-unknown", "."],
        &["--write", "--ignore-unknown", "."],
        NONE,
        &[
            ".prettierrc",
            ".prettierrc.json",
            ".prettierrc.json5",
            ".prettierrc.yaml",
            ".prettierrc.yml",
            ".prettierrc.toml",
            "prettier.config.js",
            "prettier.config.cjs",
            "prettier.config.mjs",
            "prettier.config.ts"
        ],
        NONE,
        &[
            "js", "jsx", "ts", "tsx", "json", "md", "mdx", "yaml", "yml", "html", "css", "scss",
            "less"
        ],
        NONE,
        30
    ),
    entry!(
        "biome",
        "Biome",
        "biome",
        Binary,
        FORMAT,
        &["format"],
        &["format", "--write"],
        &["package.json"],
        &["biome.json", "biome.jsonc"],
        NONE,
        &[
            "js", "jsx", "ts", "tsx", "json", "jsonc", "css", "graphql", "html"
        ],
        NONE,
        20
    ),
    entry!(
        "eslint",
        "ESLint",
        "eslint",
        Binary,
        LINT,
        &["."],
        &["--fix", "."],
        NONE,
        &[
            "eslint.config.js",
            "eslint.config.mjs",
            "eslint.config.cjs",
            "eslint.config.ts",
            ".eslintrc",
            ".eslintrc.json",
            ".eslintrc.yml",
            ".eslintrc.yaml",
            ".eslintrc.js",
            ".eslintrc.cjs"
        ],
        NONE,
        NONE,
        &["js", "jsx", "ts", "tsx"],
        0
    ),
    entry!(
        "dprint",
        "Dprint",
        "dprint",
        Binary,
        BOTH,
        &["check"],
        &["fmt"],
        NONE,
        &["dprint.json", "dprint.jsonc"],
        NONE,
        &[
            "ts", "tsx", "js", "jsx", "json", "md", "toml", "yaml", "yml"
        ],
        &[
            "ts", "tsx", "js", "jsx", "json", "md", "toml", "yaml", "yml"
        ],
        15
    ),
    entry!(
        "clippier_md",
        "Clippier MD",
        "cargo",
        Cargo,
        FORMAT,
        &["run", "-p", "clippier_md", "--", "fmt", "--check", "."],
        &["run", "-p", "clippier_md", "--", "fmt", "."],
        NONE,
        &["packages/clippier/md/Cargo.toml"],
        NONE,
        &["md", "mdx"],
        NONE,
        5
    ),
    entry!(
        "remark",
        "remark",
        "remark",
        Binary,
        FORMAT,
        &[".", "--ext", "md,mdx"],
        &[".", "--output", "--ext", "md,mdx"],
        NONE,
        &[
            ".remarkrc",
            ".remarkrc.json",
            ".remarkrc.js",
            ".remarkrc.cjs",
            "remark.config.js",
            "remark.config.cjs"
        ],
        NONE,
        &["md", "mdx"],
        NONE,
        20
    ),
    entry!(
        "mdformat",
        "mdformat",
        "mdformat",
        Binary,
        FORMAT,
        &["--check", "."],
        &["."],
        NONE,
        &[".mdformat.toml"],
        &["md"],
        &["md"],
        NONE,
        40
    ),
    entry!(
        "yamlfmt",
        "yamlfmt",
        "yamlfmt",
        Binary,
        FORMAT,
        &["-lint", "."],
        &["."],
        NONE,
        &[".yamlfmt", "yamlfmt.yml", "yamlfmt.yaml"],
        NONE,
        &["yaml", "yml"],
        NONE,
        20
    ),
    entry!(
        "ruff",
        "Ruff",
        "ruff",
        Binary,
        BOTH,
        &["check", "."],
        &["format", "."],
        &["pyproject.toml", "requirements.txt", "setup.py"],
        &["ruff.toml", ".ruff.toml"],
        NONE,
        &["py", "pyi", "ipynb"],
        &["py", "pyi", "ipynb"],
        10
    ),
    entry!(
        "black",
        "Black",
        "black",
        Binary,
        FORMAT,
        &["--check", "."],
        &["."],
        NONE,
        &[".black"],
        NONE,
        &["py", "pyi", "ipynb"],
        NONE,
        20
    ),
    entry!(
        "gofmt",
        "Go Formatter",
        "gofmt",
        Binary,
        FORMAT,
        &["-l", "."],
        &["-w", "."],
        &["go.mod"],
        NONE,
        NONE,
        &["go"],
        NONE,
        10
    ),
    entry!(
        "shfmt",
        "Shell Formatter",
        "shfmt",
        Binary,
        FORMAT,
        &["-d", "."],
        &["-w", "."],
        NONE,
        &[".shfmt.conf"],
        NONE,
        &["sh", "bash"],
        NONE,
        10
    ),
    entry!(
        "shellcheck",
        "ShellCheck",
        "shellcheck",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".shellcheckrc"],
        NONE,
        NONE,
        &["sh", "bash"],
        0
    ),
    entry!(
        "clang-format",
        "ClangFormat",
        "clang-format",
        Binary,
        FORMAT,
        &["--dry-run", "--Werror"],
        &["-i"],
        NONE,
        &[".clang-format", "_clang-format"],
        NONE,
        &["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx", "m", "mm"],
        NONE,
        10
    ),
    entry!(
        "clang-tidy",
        "Clang-Tidy",
        "clang-tidy",
        Binary,
        LINT,
        &["."],
        NONE,
        &["compile_commands.json"],
        &[".clang-tidy"],
        NONE,
        NONE,
        &["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx"],
        0
    ),
    entry!(
        "stylua",
        "StyLua",
        "stylua",
        Binary,
        FORMAT,
        &["--check", "."],
        &["."],
        NONE,
        &["stylua.toml", ".stylua.toml"],
        NONE,
        &["lua", "luau"],
        NONE,
        10
    ),
    entry!(
        "luacheck",
        "Luacheck",
        "luacheck",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".luacheckrc"],
        NONE,
        NONE,
        &["lua"],
        0
    ),
    entry!(
        "deno",
        "Deno",
        "deno",
        Binary,
        BOTH,
        &["lint", "."],
        &["fmt", "."],
        NONE,
        &["deno.json", "deno.jsonc"],
        NONE,
        &["js", "jsx", "ts", "tsx", "json", "jsonc", "md", "markdown"],
        &["js", "jsx", "ts", "tsx"],
        40
    ),
    entry!(
        "nixfmt",
        "Nixfmt",
        "nixfmt",
        Binary,
        FORMAT,
        &["--check", "."],
        &["."],
        &["flake.nix"],
        NONE,
        &["nix"],
        &["nix"],
        NONE,
        5
    ),
    entry!(
        "deadnix",
        "Deadnix",
        "deadnix",
        Binary,
        LINT,
        &["--fail", "."],
        NONE,
        &["flake.nix"],
        NONE,
        NONE,
        NONE,
        &["nix"],
        0
    ),
    entry!(
        "yamllint",
        "YAML Linter",
        "yamllint",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".yamllint", ".yamllint.yaml", ".yamllint.yml"],
        NONE,
        NONE,
        &["yaml", "yml"],
        0
    ),
    entry!(
        "stylelint",
        "Stylelint",
        "stylelint",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[
            ".stylelintrc",
            ".stylelintrc.json",
            ".stylelintrc.yaml",
            ".stylelintrc.yml",
            ".stylelintrc.js",
            ".stylelintrc.cjs",
            "stylelint.config.js",
            "stylelint.config.mjs",
            "stylelint.config.cjs"
        ],
        NONE,
        NONE,
        &["css", "scss", "less"],
        0
    ),
    entry!(
        "markdownlint",
        "Markdownlint",
        "markdownlint",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[
            ".markdownlint.json",
            ".markdownlint.jsonc",
            ".markdownlint.yaml",
            ".markdownlint.yml",
            ".markdownlint.cjs"
        ],
        NONE,
        NONE,
        &["md", "markdown"],
        0
    ),
    entry!(
        "mypy",
        "Mypy",
        "mypy",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &["mypy.ini", ".mypy.ini"],
        NONE,
        NONE,
        &["py", "pyi"],
        0
    ),
    entry!(
        "pylint",
        "Pylint",
        "pylint",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".pylintrc", "pylintrc"],
        NONE,
        NONE,
        &["py"],
        0
    ),
    entry!(
        "alejandra",
        "Alejandra",
        "alejandra",
        Binary,
        FORMAT,
        &["--check", "."],
        &["."],
        &["flake.nix"],
        NONE,
        NONE,
        &["nix"],
        NONE,
        10
    ),
    entry!(
        "statix",
        "Statix",
        "statix",
        Binary,
        LINT,
        &["check", "."],
        NONE,
        &["flake.nix"],
        &["statix.toml"],
        NONE,
        NONE,
        &["nix"],
        0
    ),
    entry!(
        "oxfmt",
        "Oxfmt",
        "oxfmt",
        Binary,
        FORMAT,
        &["--check", "."],
        &["--write", "."],
        NONE,
        &[
            ".oxfmtrc.json",
            ".oxfmtrc.jsonc",
            "oxfmt.config.ts",
            "oxfmt.config.mts"
        ],
        NONE,
        &[
            "js",
            "jsx",
            "mjs",
            "cjs",
            "ts",
            "tsx",
            "mts",
            "cts",
            "json",
            "jsonc",
            "json5",
            "yaml",
            "yml",
            "toml",
            "html",
            "htm",
            "xhtml",
            "vue",
            "css",
            "scss",
            "less",
            "pcss",
            "postcss",
            "md",
            "markdown",
            "mdx",
            "graphql",
            "gql",
            "graphqls",
            "hbs",
            "handlebars"
        ],
        NONE,
        25
    ),
    entry!(
        "oxlint",
        "Oxlint",
        "oxlint",
        Binary,
        LINT,
        &["--deny-warnings", "."],
        NONE,
        NONE,
        &[
            ".oxlintrc.json",
            ".oxlintrc.jsonc",
            "oxlint.config.ts",
            "oxlint.config.mts"
        ],
        NONE,
        NONE,
        &[
            "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "vue", "svelte", "astro"
        ],
        100
    ),
    entry!(
        "isort",
        "isort",
        "isort",
        Binary,
        FORMAT,
        &["--check-only", "."],
        &["."],
        NONE,
        &[".isort.cfg"],
        NONE,
        &["py", "pyi"],
        NONE,
        40
    ),
    entry!(
        "pyright",
        "Pyright",
        "pyright",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &["pyrightconfig.json"],
        NONE,
        NONE,
        &["py", "pyi"],
        100
    ),
    entry!(
        "bandit",
        "Bandit",
        "bandit",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        NONE,
        NONE,
        NONE,
        &["py"],
        100
    ),
    entry!(
        "rubocop",
        "RuboCop",
        "rubocop",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".rubocop.yml"],
        NONE,
        NONE,
        &["rb", "rake", "gemspec"],
        100
    ),
    entry!(
        "swiftformat",
        "SwiftFormat",
        "swiftformat",
        Binary,
        FORMAT,
        &["--lint", "."],
        &["."],
        NONE,
        &[".swiftformat"],
        NONE,
        &["swift"],
        NONE,
        10
    ),
    entry!(
        "ktlint",
        "ktlint",
        "ktlint",
        Binary,
        FORMAT,
        &["."],
        &["--format", "."],
        NONE,
        NONE,
        NONE,
        &["kt", "kts"],
        NONE,
        10
    ),
    entry!(
        "sqlfluff",
        "SQLFluff",
        "sqlfluff",
        Binary,
        LINT,
        &["lint", "."],
        NONE,
        NONE,
        &[".sqlfluff"],
        NONE,
        NONE,
        &["sql"],
        100
    ),
    entry!(
        "dart",
        "Dart Formatter",
        "dart",
        Binary,
        FORMAT,
        &["format", "--output=none", "--set-exit-if-changed", "."],
        &["format", "."],
        &["pubspec.yaml"],
        NONE,
        NONE,
        &["dart"],
        NONE,
        10
    ),
    entry!(
        "flake8",
        "Flake8",
        "flake8",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".flake8"],
        NONE,
        NONE,
        &["py", "pyi"],
        100
    ),
    entry!(
        "ty",
        "ty",
        "ty",
        Binary,
        LINT,
        &["check", "."],
        NONE,
        NONE,
        &["ty.toml"],
        NONE,
        NONE,
        &["py", "pyi"],
        100
    ),
    entry!(
        "yapf",
        "YAPF",
        "yapf",
        Binary,
        FORMAT,
        &["--diff", "."],
        &["--in-place", "."],
        NONE,
        &[".style.yapf"],
        NONE,
        &["py"],
        NONE,
        50
    ),
    entry!(
        "autopep8",
        "autopep8",
        "autopep8",
        Binary,
        FORMAT,
        &["--diff", "--exit-code", "."],
        &["--in-place", "."],
        NONE,
        NONE,
        NONE,
        &["py"],
        NONE,
        60
    ),
    entry!(
        "phpstan",
        "PHPStan",
        "phpstan",
        Binary,
        LINT,
        &["analyse", "."],
        NONE,
        NONE,
        &["phpstan.neon", "phpstan.neon.dist"],
        NONE,
        NONE,
        &["php"],
        100
    ),
    entry!(
        "phpcs",
        "PHP_CodeSniffer",
        "phpcs",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[
            ".phpcs.xml",
            "phpcs.xml",
            ".phpcs.xml.dist",
            "phpcs.xml.dist"
        ],
        NONE,
        NONE,
        &["php"],
        100
    ),
    entry!(
        "google-java-format",
        "google-java-format",
        "google-java-format",
        Binary,
        FORMAT,
        &["--dry-run", "--set-exit-if-changed", "."],
        &["--replace", "."],
        NONE,
        NONE,
        NONE,
        &["java"],
        NONE,
        10
    ),
    entry!(
        "vale",
        "Vale",
        "vale",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".vale.ini"],
        NONE,
        NONE,
        &["md", "markdown", "rst", "adoc", "txt"],
        100
    ),
    entry!(
        "codespell",
        "codespell",
        "codespell",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".codespellrc"],
        NONE,
        NONE,
        &[
            "md", "markdown", "rst", "txt", "py", "rs", "js", "ts", "go", "c", "cpp", "h", "java",
            "rb", "sh"
        ],
        100
    ),
    entry!(
        "cspell",
        "CSpell",
        "cspell",
        Binary,
        LINT,
        &["--no-progress", "."],
        NONE,
        NONE,
        &[
            "cspell.json",
            "cspell.jsonc",
            "cspell.yaml",
            "cspell.yml",
            "cspell.config.js",
            "cspell.config.cjs",
            "cspell.config.mjs",
            ".cspell.json",
            ".cspell.jsonc"
        ],
        NONE,
        NONE,
        &[
            "md", "markdown", "mdx", "rst", "txt", "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts",
            "cts", "json", "yaml", "yml", "html", "css", "py", "rs", "go", "java", "rb", "php",
            "swift", "kt"
        ],
        100
    ),
    entry!(
        "hadolint",
        "Hadolint",
        "hadolint",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".hadolint.yaml", ".hadolint.yml"],
        NONE,
        NONE,
        &["dockerfile"],
        100
    ),
    entry!(
        "buildifier",
        "Buildifier",
        "buildifier",
        Binary,
        FORMAT,
        &["-mode=check", "."],
        &["-mode=fix", "."],
        NONE,
        NONE,
        NONE,
        &["bzl", "bazel"],
        NONE,
        10
    ),
    entry!(
        "cmake-format",
        "CMake Formatter",
        "cmake-format",
        Binary,
        FORMAT,
        &["--check", "."],
        &["--in-place", "."],
        NONE,
        &[
            ".cmake-format.json",
            ".cmake-format.py",
            ".cmake-format.yaml",
            ".cmake-format.yml"
        ],
        NONE,
        &["cmake"],
        NONE,
        10
    ),
    entry!(
        "zig",
        "Zig Formatter",
        "zig",
        Binary,
        FORMAT,
        &["fmt", "--check", "."],
        &["fmt", "."],
        &["build.zig"],
        NONE,
        NONE,
        &["zig", "zon"],
        NONE,
        10
    ),
    entry!(
        "ormolu",
        "Ormolu",
        "ormolu",
        Binary,
        FORMAT,
        &["--mode", "check", "."],
        &["--mode", "inplace", "."],
        NONE,
        &[".ormolu"],
        NONE,
        &["hs"],
        NONE,
        10
    ),
    entry!(
        "fourmolu",
        "Fourmolu",
        "fourmolu",
        Binary,
        FORMAT,
        &["--mode", "check", "."],
        &["--mode", "inplace", "."],
        NONE,
        &["fourmolu.yaml"],
        NONE,
        &["hs"],
        NONE,
        20
    ),
    entry!(
        "hlint",
        "HLint",
        "hlint",
        Binary,
        LINT,
        &["."],
        NONE,
        NONE,
        &[".hlint.yaml"],
        NONE,
        NONE,
        &["hs", "lhs"],
        100
    ),
    entry!(
        "scalafmt",
        "Scalafmt",
        "scalafmt",
        Binary,
        FORMAT,
        &["--test", "--non-interactive", "."],
        &["--non-interactive", "."],
        NONE,
        &[".scalafmt.conf"],
        NONE,
        &["scala", "sbt", "sc"],
        NONE,
        10
    ),
    entry!(
        "terraform",
        "Terraform",
        "terraform",
        Binary,
        BOTH,
        &["validate"],
        &["fmt", "-recursive"],
        &[".terraform.lock.hcl"],
        NONE,
        NONE,
        &["tf", "tfvars"],
        &["tf", "tfvars"],
        10
    ),
    entry!(
        "tofu",
        "OpenTofu",
        "tofu",
        Binary,
        BOTH,
        &["validate"],
        &["fmt", "-recursive"],
        &[".terraform.lock.hcl"],
        NONE,
        NONE,
        &["tf", "tfvars"],
        &["tf", "tfvars"],
        20
    ),
];

/// Looks up one catalog entry.
#[must_use]
pub fn tool_catalog_entry(name: &str) -> Option<&'static ToolCatalogEntry> {
    TOOL_CATALOG.iter().find(|entry| entry.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oxc_tools_have_independent_coverage_and_node_resolution() {
        let formatter = tool_catalog_entry("oxfmt").unwrap();
        let linter = tool_catalog_entry("oxlint").unwrap();
        for extension in [
            "json5", "yaml", "toml", "html", "vue", "css", "scss", "md", "mdx", "graphql", "hbs",
        ] {
            assert!(formatter.format_extensions.contains(&extension));
        }
        for extension in ["vue", "svelte", "astro", "mts", "cts"] {
            assert!(linter.lint_extensions.contains(&extension));
        }
        assert!(!linter.lint_extensions.contains(&"json"));
        assert_eq!(formatter.node_runner_package(), Some("oxfmt"));
        assert_eq!(linter.node_runner_package(), Some("oxlint"));
        assert!(formatter.uses_scoped_file_arguments());
        assert!(linter.uses_scoped_file_arguments());
    }

    #[test]
    fn initial_matrix_has_no_unverified_compatible_formatter_pairs() {
        assert!(COMPATIBLE_OVERLAPS.is_empty());
        assert!(
            compatible_overlap_extensions("biome", "prettier", ToolCapability::Format).is_empty()
        );
        assert!(
            compatible_overlap_extensions("dprint", "prettier", ToolCapability::Format).is_empty()
        );
    }

    #[test]
    fn catalog_ids_are_unique_and_capabilities_have_arguments() {
        let mut names = BTreeSet::new();
        for entry in TOOL_CATALOG {
            assert!(names.insert(entry.name), "duplicate tool {}", entry.name);
            assert!(entry.binary.is_ascii());
            assert!(!entry.binary.is_empty());
            assert!(!entry.capabilities.is_empty());
            assert!(
                entry
                    .signals
                    .manifests
                    .iter()
                    .all(|signal| !signal.is_empty())
            );
            assert!(
                entry
                    .signals
                    .configs
                    .iter()
                    .all(|signal| !signal.is_empty())
            );
            assert!(embedded_config_signals(entry.name).iter().all(|signal| {
                signal.tool == entry.name
                    && !signal.manifest.is_empty()
                    && !signal.key.is_empty()
                    && signal
                        .container
                        .is_none_or(|container| !container.is_empty())
            }));
            assert!(
                entry
                    .signals
                    .content_extensions
                    .iter()
                    .all(|extension| !extension.is_empty())
            );
            if entry.uses_scoped_file_arguments() {
                assert!(
                    entry.capabilities.contains(&ToolCapability::Format)
                        || entry.check_args.contains(&"."),
                    "{} has invalid scoped file argument policy",
                    entry.name
                );
            }
            if entry.capabilities.contains(&ToolCapability::Format) {
                assert!(
                    !entry.format_args.is_empty(),
                    "{} cannot format",
                    entry.name
                );
                assert!(!entry.format_extensions.is_empty());
            }
            if entry.capabilities.contains(&ToolCapability::Lint) {
                assert!(!entry.lint_extensions.is_empty());
            }
            assert!(
                entry
                    .format_extensions
                    .iter()
                    .chain(entry.lint_extensions)
                    .all(|extension| !extension.is_empty() && !extension.starts_with('.')),
                "{} has an invalid extension",
                entry.name
            );
        }

        for relationship in COMPATIBLE_OVERLAPS {
            assert_ne!(relationship.left, relationship.right);
            let left = tool_catalog_entry(relationship.left)
                .unwrap_or_else(|| panic!("unknown overlap tool {}", relationship.left));
            let right = tool_catalog_entry(relationship.right)
                .unwrap_or_else(|| panic!("unknown overlap tool {}", relationship.right));
            assert!(left.capabilities.contains(&relationship.capability));
            assert!(right.capabilities.contains(&relationship.capability));
            let shared = left
                .extensions(relationship.capability)
                .intersection(&right.extensions(relationship.capability))
                .cloned()
                .collect::<BTreeSet<_>>();
            assert!(!shared.is_empty());
            assert!(
                relationship.extensions.is_empty()
                    || relationship
                        .extensions
                        .iter()
                        .all(|extension| shared.contains(*extension))
            );
        }
    }
}
