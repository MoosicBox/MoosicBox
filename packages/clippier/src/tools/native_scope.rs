//! Capability-local native file selectors.
//!
//! These matchers are deliberately separate from Clippier's policy globs. A
//! delegated selector must never remove a file from the execution plan. Paths
//! and flags are interpreted relative to the native invocation's working dir;
//! this module does not rewrite either configuration or invocation arguments.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::overrides::{Override, OverrideBuilder};
use serde_json::Value;

mod jsonc;
mod python;

/// Resolved native scopes for one capability and one invocation directory.
///
/// Callers must resolve a separate set when effective arguments or capability
/// change. In particular Biome `format`, `lint`, and `check` are not equivalent.
/// Share clones across capability-local sets within one command only.
#[derive(Clone, Default)]
pub(super) struct NativeScopeCache(Arc<Mutex<CacheData>>);

#[derive(Default)]
struct CacheData {
    configs: BTreeMap<PathBuf, Value>,
    ignores: BTreeMap<PathBuf, String>,
    markers: BTreeMap<PathBuf, bool>,
    snapshot: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

pub(super) struct NativeScopeSet {
    root: PathBuf,
    scopes: BTreeMap<String, NativeScope>,
    diagnostics: BTreeSet<String>,
    parsed: BTreeSet<PathBuf>,
    cache: NativeScopeCache,
    biome_patterns: BTreeMap<(PathBuf, String), BiomePatterns>,
    dprint_patterns: BTreeMap<PathBuf, (Option<Override>, Gitignore, bool)>,
    rustfmt_checked: BTreeMap<PathBuf, Option<Gitignore>>,
    nested_ruff: BTreeMap<PathBuf, python::PythonScope>,
    directory_configs: BTreeMap<(String, PathBuf), Option<PathBuf>>,
}

enum NativeScope {
    Python(python::PythonScope),
    Prettier {
        ignores: Vec<Gitignore>,
    },
    Taplo {
        include: GlobSet,
        exclude: GlobSet,
    },
    Json {
        tool: String,
        config: Option<PathBuf>,
        args: Vec<String>,
    },
    Rustfmt {
        config: Option<PathBuf>,
        args: Vec<String>,
    },
    Delegated,
}

impl NativeScopeSet {
    /// Resolves the native configuration selected by the effective arguments.
    ///
    /// # Errors
    ///
    /// * Returns filesystem errors while reading native configuration.
    /// * Returns `InvalidData` for malformed known static selectors or config.
    /// * Returns `InvalidInput` for incomplete selector flags or duplicate tools.
    #[cfg(test)]
    pub(crate) fn resolve(root: &Path, tools: &[(&str, &[String])]) -> io::Result<Self> {
        Self::resolve_with_cache(root, tools, NativeScopeCache::default())
    }

    /// Resolves scopes using a command-local parsed configuration cache.
    ///
    /// # Errors
    ///
    /// * Returns the same errors as `resolve`.
    pub(crate) fn resolve_with_cache(
        root: &Path,
        tools: &[(&str, &[String])],
        cache: NativeScopeCache,
    ) -> io::Result<Self> {
        let root = root.canonicalize()?;
        let mut result = Self {
            root,
            scopes: BTreeMap::new(),
            diagnostics: BTreeSet::new(),
            parsed: BTreeSet::new(),
            cache,
            rustfmt_checked: BTreeMap::new(),
            nested_ruff: BTreeMap::new(),
            directory_configs: BTreeMap::new(),
            biome_patterns: BTreeMap::new(),
            dprint_patterns: BTreeMap::new(),
        };
        for &(tool, args) in tools {
            if result.scopes.contains_key(tool) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "native scope: duplicate {tool}; resolve each capability/argument set separately"
                    ),
                ));
            }
            let scope = result.resolve_tool(tool, args)?;
            result.scopes.insert(tool.to_owned(), scope);
        }
        Ok(result)
    }

    /// Tests a file without changing native invocation identity.
    ///
    /// # Errors
    ///
    /// * Returns filesystem or parse errors for lazily discovered nested config.
    pub(crate) fn allows(&mut self, tool: &str, path: &Path) -> io::Result<bool> {
        let path = absolute(&self.root, path);
        // Do not normalize a symlink into a different configuration scope.
        if !path.starts_with(&self.root) {
            return Ok(true);
        }
        // Temporarily remove the scope so nested config can use the shared cache.
        let Some(scope) = self.scopes.remove(tool) else {
            return Ok(true);
        };
        let allowed = (|| match &scope {
            NativeScope::Python(scope) => self.allows_python(scope, &path),
            NativeScope::Prettier { ignores } => Ok(!ignores.iter().any(|ignore| {
                path.starts_with(ignore.path()) && ignored_with_parents(ignore, &path, false)
            })),
            NativeScope::Taplo { include, exclude } => {
                Ok(include.is_match(&path) && !exclude.is_match(&path))
            }
            NativeScope::Json { tool, config, args } => {
                self.allows_json(tool, config.as_deref(), args, &path)
            }
            NativeScope::Rustfmt { config, args } => {
                let selected = match config {
                    Some(config) => Some(config.clone()),
                    None => self.find_ancestor(
                        path.parent().unwrap_or(&self.root),
                        &[".rustfmt.toml", "rustfmt.toml"],
                    )?,
                };
                if let Some(config) = selected {
                    self.rustfmt_config(&config, args)?;
                    if let Some(Some(ignore)) = self.rustfmt_checked.get(&config)
                        && path.starts_with(ignore.path())
                    {
                        return Ok(!ignore.matched_path_or_any_parents(&path, false).is_ignore());
                    }
                }
                Ok(true)
            }
            NativeScope::Delegated => Ok(true),
        })();
        self.scopes.insert(tool.to_owned(), scope);
        allowed
    }

    /// Whether all descendants are certainly excluded by this native scope.
    ///
    /// Only fixed Prettier ignore files permit pruning. Includes alone do not
    /// prove that a directory cannot contain a matching descendant, and nested
    /// native configuration may change the meaning of JSON-tool selectors.
    #[must_use]
    pub(crate) fn can_prune(&self, tool: &str, directory: &Path) -> bool {
        let path = absolute(&self.root, directory);
        match self.scopes.get(tool) {
            Some(NativeScope::Prettier { ignores }) => ignores.iter().any(|ignore| {
                path.starts_with(ignore.path()) && ignored_with_parents(ignore, &path, true)
            }),
            _ => false,
        }
    }

    /// Stable support/provenance messages, including delegated semantics.
    #[must_use]
    pub(crate) fn diagnostics(&self) -> Vec<String> {
        self.diagnostics.iter().cloned().collect()
    }

    /// Unique native config/ignore files actually parsed (not lookup probes).
    #[must_use]
    pub(crate) fn configs_parsed(&self) -> usize {
        self.cache.0.lock().map_or(self.parsed.len(), |cache| {
            cache
                .configs
                .keys()
                .chain(cache.ignores.keys())
                .chain(self.parsed.iter())
                .collect::<BTreeSet<_>>()
                .len()
        })
    }

    /// Checks the command-local native configuration snapshot before execution.
    ///
    /// # Errors
    ///
    /// * Returns an error if a discovered config changed, disappeared, or an
    ///   absent higher-precedence config appeared since planning.
    pub(crate) fn validate_snapshot(&self) -> io::Result<()> {
        let cache = self
            .cache
            .0
            .lock()
            .map_err(|_| io::Error::other("native config cache poisoned"))?;
        let markers = cache.markers.clone();
        let snapshot = cache.snapshot.clone();
        drop(cache);
        for (path, expected) in &markers {
            if path.try_exists()? != *expected {
                return Err(io::Error::other(format!(
                    "native project marker changed since planning: {}",
                    path.display()
                )));
            }
        }
        for (path, expected) in &snapshot {
            let actual = snapshot_file(path)?;
            if &actual != expected {
                return Err(io::Error::other(format!(
                    "native configuration changed since planning: {}; rebuild the plan",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    fn snapshot(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        let mut cache = self
            .cache
            .0
            .lock()
            .map_err(|_| io::Error::other("native config cache poisoned"))?;
        if let Some(value) = cache.snapshot.get(path) {
            return Ok(value.clone());
        }
        let value = snapshot_file(path)?;
        cache.snapshot.insert(path.to_path_buf(), value.clone());
        drop(cache);
        Ok(value)
    }

    fn find_here(&self, directory: &Path, names: &[&str]) -> io::Result<Option<PathBuf>> {
        for name in names {
            let path = directory.join(name);
            if self.snapshot(&path)?.is_some() {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    fn find_ancestor(&self, directory: &Path, names: &[&str]) -> io::Result<Option<PathBuf>> {
        for ancestor in directory.ancestors() {
            if let Some(path) = self.find_here(ancestor, names)? {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    fn note(&mut self, tool: &str, detail: impl AsRef<str>) {
        self.diagnostics
            .insert(format!("native scope {tool}: {}", detail.as_ref()));
    }

    fn resolve_tool(&mut self, tool: &str, args: &[String]) -> io::Result<NativeScope> {
        match tool {
            "ruff" | "black" => self.python_scope(tool, args),
            "prettier" => self.prettier(args),
            "taplo" => self.taplo(args),
            "rustfmt" => {
                if option_values(args, &["--config"])?.iter().any(|value| {
                    value
                        .split(',')
                        .any(|entry| entry.trim_start().starts_with("ignore="))
                }) {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "inline rustfmt ignore overrides are not supported; use a config file so planning and execution share identity",
                    ));
                }
                let explicit = option_values(args, &["--config-path"])?;
                let config = explicit
                    .last()
                    .map(|value| absolute(&self.root, Path::new(value)));
                let selected = match &config {
                    Some(path) if path.is_dir() => {
                        self.find_here(path, &[".rustfmt.toml", "rustfmt.toml"])?
                    }
                    Some(path) => Some(path.clone()),
                    None => self.find_ancestor(&self.root, &[".rustfmt.toml", "rustfmt.toml"])?,
                };
                if let Some(path) = &selected {
                    self.rustfmt_config(path, args)?;
                }
                self.note(tool, "stable unstable-ignore warns without activation; verified nightly ignore uses rustfmt's own gitignore parent matcher");
                Ok(NativeScope::Rustfmt {
                    args: args.to_vec(),
                    config: if config.is_some() { selected } else { None },
                })
            }
            "biome" | "dprint" => {
                let flags: &[&str] = if tool == "biome" {
                    &["--config-path"]
                } else {
                    &["--config", "-c"]
                };
                let values = option_values(args, flags)?;
                let config = if let Some(value) = values.last() {
                    if value.contains("://") {
                        self.note(
                            tool,
                            "remote configuration delegated; no planning exclusions",
                        );
                        return Ok(NativeScope::Delegated);
                    }
                    let path = absolute(&self.root, Path::new(value));
                    if path.is_dir() {
                        self.find_here(&path, json_names(tool))?.ok_or_else(|| {
                            invalid(&path, "no native configuration in explicit directory")
                        })?
                    } else {
                        path
                    }
                } else if let Some(path) = self.find_ancestor(&self.root, json_names(tool))? {
                    path
                } else {
                    self.note(
                        tool,
                        "no static configuration found; native defaults delegated",
                    );
                    return Ok(NativeScope::Json {
                        tool: tool.to_owned(),
                        config: None,
                        args: args.to_vec(),
                    });
                };
                let value = self.load(&config)?;
                validate_json_selectors(tool, &config, &value)?;
                self.note(tool, format!("static selectors from {} (config-relative); extends, nested configs and unsupported selectors delegated", config.display()));
                // Validate and compile even when no candidate file is selected.
                self.json_filter(tool, &config, &value, args, &self.root.clone())?;
                Ok(NativeScope::Json {
                    tool: tool.to_owned(),
                    config: Some(config),
                    args: args.to_vec(),
                })
            }
            _ => {
                self.note(tool, delegated_selectors(tool));
                Ok(NativeScope::Delegated)
            }
        }
    }

    fn load(&mut self, path: &Path) -> io::Result<Value> {
        self.load_format(path, false)
    }

    fn load_format(&mut self, path: &Path, is_toml: bool) -> io::Result<Value> {
        let mut cache = self
            .cache
            .0
            .lock()
            .map_err(|_| io::Error::other("native config cache poisoned"))?;
        if let Some(value) = cache.configs.get(path) {
            return Ok(value.clone());
        }
        let bytes = if let Some(bytes) = cache.snapshot.get(path) {
            bytes.clone()
        } else {
            let bytes = snapshot_file(path)?;
            cache.snapshot.insert(path.to_path_buf(), bytes.clone());
            bytes
        }
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("native config missing: {}", path.display()),
            )
        })?;
        let text = String::from_utf8(bytes).map_err(|error| invalid(path, error))?;
        let value = if is_toml || path.extension().is_some_and(|ext| ext == "toml") {
            let value: toml::Value = toml::from_str(&text).map_err(|error| invalid(path, error))?;
            serde_json::to_value(value).map_err(|error| invalid(path, error))?
        } else {
            jsonc::parse(&text).map_err(|error| invalid(path, error))?
        };
        if !value.is_object() {
            return Err(invalid(path, "configuration must be an object/table"));
        }
        self.parsed.insert(path.to_path_buf());
        cache.configs.insert(path.to_path_buf(), value.clone());
        drop(cache);
        Ok(value)
    }

    fn prettier(&mut self, args: &[String]) -> io::Result<NativeScope> {
        let paths = option_values(args, &["--ignore-path"])?;
        let paths = if paths.is_empty() {
            vec![".gitignore".to_owned(), ".prettierignore".to_owned()]
        } else {
            paths
        };
        let with_modules = args
            .iter()
            .any(|arg| arg == "--with-node-modules" || arg == "--with-node-modules=true");
        let mut ignores = Vec::new();
        for value in paths {
            let path = absolute(&self.root, Path::new(&value));
            let base = path.parent().unwrap_or(&self.root);
            let mut builder = GitignoreBuilder::new(base);
            let snapshot = self.snapshot(&path)?;
            let text = {
                let mut cache = self
                    .cache
                    .0
                    .lock()
                    .map_err(|_| io::Error::other("native config cache poisoned"))?;
                let cached = cache.ignores.get(&path).cloned();
                let text = cached.map_or_else(
                    || {
                        snapshot
                            .ok_or_else(|| {
                                io::Error::new(io::ErrorKind::NotFound, "ignore file absent")
                            })
                            .and_then(|bytes| {
                                String::from_utf8(bytes).map_err(|error| invalid(&path, error))
                            })
                            .inspect(|text| {
                                cache.ignores.insert(path.clone(), text.clone());
                            })
                    },
                    Ok,
                );
                drop(cache);
                text
            };
            match text {
                Ok(text) => {
                    for line in text.lines() {
                        builder
                            .add_line(Some(path.clone()), line)
                            .map_err(|error| invalid(&path, error))?;
                    }
                    self.parsed.insert(path.clone());
                    self.note(
                        "prettier",
                        format!(
                            "gitignore semantics from {} (ignore-file-relative, independent union)",
                            path.display()
                        ),
                    );
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            // Prettier appends this after each ignore file, so an ignore-file
            // negation cannot opt node_modules back in without the CLI flag.
            if !with_modules {
                builder
                    .add_line(None, "node_modules")
                    .map_err(io::Error::other)?;
            }
            ignores.push(builder.build().map_err(io::Error::other)?);
        }
        let mut vcs = GitignoreBuilder::new(&self.root);
        for name in [".git", ".jj", ".sl", ".svn", ".hg"] {
            vcs.add_line(None, name).map_err(io::Error::other)?;
        }
        ignores.push(vcs.build().map_err(io::Error::other)?);
        self.note("prettier", "supported: effective --ignore-path, parent matching, negation, VCS and node_modules defaults; positional fast-glob selectors and executable configuration delegated");
        Ok(NativeScope::Prettier { ignores })
    }

    fn taplo(&mut self, args: &[String]) -> io::Result<NativeScope> {
        if args.iter().any(|arg| arg == "--no-config") {
            self.note("taplo", "--no-config: native configuration disabled");
            return Ok(NativeScope::Delegated);
        }
        let paths = option_values(args, &["--config", "-c"])?;
        let config = if let Some(path) = paths.last() {
            Some(absolute(&self.root, Path::new(path)))
        } else {
            self.find_ancestor(&self.root, &[".taplo.toml", "taplo.toml"])?
        };
        let Some(config) = config else {
            self.note("taplo", "default **/*.toml; no native configuration");
            return Ok(NativeScope::Delegated);
        };
        let value = self.load_format(&config, true)?;
        let include = strings(&value, "include", &config)?;
        let exclude = strings(&value, "exclude", &config)?.unwrap_or_default();
        // Taplo's GlobRule uses Glob::new after making patterns absolute from
        // the invocation/workspace base, NOT from the config file's directory.
        let include = include.unwrap_or_else(|| vec!["**/*.toml".to_owned()]);
        let include = taplo_globs(&self.root, &include, &config)?;
        let exclude = taplo_globs(&self.root, &exclude, &config)?;
        // [[rule]] include/exclude selects options, not files. Validate these
        // known fields but never turn them into global file exclusions.
        if let Some(rules) = value.get("rule") {
            let rules = rules
                .as_array()
                .ok_or_else(|| invalid(&config, "rule must be an array"))?;
            for rule in rules {
                for key in ["include", "exclude"] {
                    if let Some(patterns) = strings(rule, key, &config)? {
                        taplo_globs(&self.root, &patterns, &config)?;
                    }
                }
            }
        }
        self.note("taplo", format!("supported top-level include/exclude from {} (invocation-relative, missing include defaults to **/*.toml; empty includes none); rule selectors only change options", config.display()));
        if args.iter().any(|arg| arg == "--force") {
            self.note(
                "taplo",
                "--force bypasses config exclusions; selection delegated",
            );
            return Ok(NativeScope::Delegated);
        }
        Ok(NativeScope::Taplo { include, exclude })
    }

    fn rustfmt_config(&mut self, config: &Path, args: &[String]) -> io::Result<()> {
        if self.rustfmt_checked.contains_key(config) {
            return Ok(());
        }
        let value = self.load_format(config, true)?;
        let ignores = strings(&value, "ignore", config)?.unwrap_or_default();
        let mut matcher = None;
        if !ignores.is_empty() {
            let configured = std::env::var_os("RUSTFMT").map(PathBuf::from);
            let executable = if let Some(configured) = configured {
                if configured.is_file() {
                    configured
                } else {
                    which::which(&configured).map_err(|error| {
                        invalid(config, format!("RUSTFMT executable unavailable: {error}"))
                    })?
                }
            } else {
                which::which("rustfmt").map_err(|error| invalid(config, error))?
            };
            let mut command = std::process::Command::new(executable);
            command.current_dir(&self.root);
            if let Some(channel) = args.iter().find(|arg| arg.starts_with('+')) {
                command.arg(channel);
            }
            let output = command.arg("--version").output().map_err(|error| {
                invalid(
                    config,
                    format!("cannot verify rustfmt channel for ignore: {error}"),
                )
            })?;
            let version = String::from_utf8_lossy(&output.stdout);
            if !output.status.success() || !version.starts_with("rustfmt ") {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!(
                        "{}: cannot verify rustfmt toolchain for unstable ignore",
                        config.display()
                    ),
                ));
            }
            if version.contains("nightly") || version.contains("dev") {
                let mut builder = GitignoreBuilder::new(config.parent().unwrap_or(&self.root));
                for pattern in &ignores {
                    builder
                        .add_line(None, pattern)
                        .map_err(|error| invalid(config, error))?;
                }
                matcher = Some(builder.build().map_err(|error| invalid(config, error))?);
                self.note("rustfmt", format!("{}: verified {}: active config-relative ignore supported with rustfmt parent-match semantics", config.display(), version.trim()));
            } else {
                self.note("rustfmt", format!("{}: verified {}: unstable ignore ignored on stable/beta, matching native warning behavior", config.display(), version.trim()));
            }
        }
        self.rustfmt_checked.insert(config.to_path_buf(), matcher);
        self.note(
            "rustfmt",
            format!("{} parsed (ancestor lookup retained)", config.display()),
        );
        Ok(())
    }

    fn allows_json(
        &mut self,
        tool: &str,
        config: Option<&Path>,
        args: &[String],
        path: &Path,
    ) -> io::Result<bool> {
        let explicit_flags: &[&str] = if tool == "biome" {
            &["--config-path"]
        } else {
            &["--config", "-c"]
        };
        if !option_values(args, explicit_flags)?.is_empty() {
            if let Some(config) = config {
                let value = self.load(config)?;
                return self.json_filter(tool, config, &value, args, path);
            }
            return Ok(true);
        }
        // A nested native config can reset, extend or re-enable parent selectors.
        // Do not apply a parent filter once native ownership changes.
        let stop = config
            .and_then(Path::parent)
            .unwrap_or(&self.root)
            .to_path_buf();
        for ancestor in path.parent().unwrap_or(&self.root).ancestors() {
            if ancestor == stop {
                break;
            }
            if !ancestor.starts_with(&stop) {
                break;
            }
            let key = (tool.to_owned(), ancestor.to_path_buf());
            if !self.directory_configs.contains_key(&key) {
                let selected = self.find_here(ancestor, json_names(tool))?;
                self.directory_configs.insert(key.clone(), selected);
            }
            if let Some(nested) = self.directory_configs[&key].clone() {
                let value = self.load(&nested)?;
                validate_json_selectors(tool, &nested, &value)?;
                self.note(
                    tool,
                    format!(
                        "nested configuration {} delegated; parent selectors not applied",
                        nested.display()
                    ),
                );
                return Ok(true);
            }
        }
        let Some(config) = config else {
            return Ok(true);
        };
        let value = self.load(config)?;
        self.json_filter(tool, config, &value, args, path)
    }

    fn json_filter(
        &mut self,
        tool: &str,
        config: &Path,
        value: &Value,
        args: &[String],
        path: &Path,
    ) -> io::Result<bool> {
        if value.get("extends").is_some() {
            self.note(
                tool,
                format!(
                    "{}: extends delegated; no static exclusions",
                    config.display()
                ),
            );
            return Ok(true);
        }
        if tool == "biome" {
            self.biome_filter(config, value, args, path)
        } else {
            self.dprint_filter(config, value, args, path)
        }
    }

    fn biome_filter(
        &mut self,
        config: &Path,
        value: &Value,
        args: &[String],
        path: &Path,
    ) -> io::Result<bool> {
        if value.get("overrides").is_some() || value.get("experimental").is_some() {
            self.note(
                "biome",
                "overrides/experimental configuration delegated; no static exclusions",
            );
            return Ok(true);
        }
        let mode = if args.iter().any(|arg| arg == "format") {
            "formatter"
        } else if args.iter().any(|arg| arg == "lint") {
            "linter"
        } else {
            self.note("biome", "combined check/unknown capability delegated (formatter/linter/assist scopes differ)");
            return Ok(true);
        };
        let base = config.parent().unwrap_or(&self.root);
        let Ok(relative) = path.strip_prefix(base) else {
            return Ok(true);
        };
        let mut groups = Vec::new();
        for key in ["files", mode] {
            if let Some(group) = value.get(key) {
                if let Some(patterns) = strings(group, "includes", config)? {
                    let cache_key = (config.to_path_buf(), key.to_owned());
                    if !self.biome_patterns.contains_key(&cache_key) {
                        self.biome_patterns
                            .insert(cache_key.clone(), BiomePatterns::new(&patterns, config)?);
                    }
                    groups.push(cache_key);
                }
                // Biome 1 uses include/ignore, Biome 2 uses includes. Do not
                // silently reinterpret a mixed/unknown schema as either version.
                if group.get("include").is_some() || group.get("ignore").is_some() {
                    self.note(
                        "biome",
                        "legacy include/ignore selectors delegated (version-dependent semantics)",
                    );
                    return Ok(true);
                }
            }
        }
        if groups.iter().any(|key| self.biome_patterns[key].delegated) {
            self.note(
                "biome",
                "non-portable glob syntax delegated; no static exclusions",
            );
            return Ok(true);
        }
        if args.iter().any(|arg| {
            (arg.starts_with("--files-") && !arg.starts_with("--files-ignore-unknown"))
                || arg.starts_with("--formatter-enabled")
                || arg.starts_with("--linter-enabled")
                || (arg.starts_with("--vcs-")
                    && !arg.starts_with("--vcs-root")
                    && !arg.starts_with("--vcs-enabled")
                    && !arg.starts_with("--vcs-use-ignore-file")
                    && !arg.starts_with("--vcs-client-kind"))
        }) {
            self.note(
                "biome",
                "CLI files/enabled/VCS overrides delegated; no static exclusions",
            );
            return Ok(true);
        }
        if value
            .get(mode)
            .and_then(|group| group.get("enabled"))
            .and_then(Value::as_bool)
            == Some(false)
        {
            return Ok(false);
        }
        self.note("biome", format!("supported {mode} and files.includes ordered static */** patterns; VCS/language support delegated"));
        Ok(groups
            .iter()
            .all(|key| self.biome_patterns[key].allows(relative)))
    }

    fn dprint_filter(
        &mut self,
        config: &Path,
        value: &Value,
        args: &[String],
        path: &Path,
    ) -> io::Result<bool> {
        if args.iter().any(|arg| {
            [
                "--includes-override",
                "--excludes-override",
                "--excludes",
                "--includes",
                "--exclude",
                "--allow-node-modules",
                "--config-discovery",
            ]
            .iter()
            .any(|flag| arg == flag || arg.starts_with(&format!("{flag}=")))
        }) {
            self.note(
                "dprint",
                "CLI traversal/pattern overrides delegated; no static exclusions",
            );
            return Ok(true);
        }
        self.note("dprint", "Clippier boundary: discovery VCS ignores may omit files that native dprint can reinclude with !pattern; native VCS parity is not claimed");
        let base = config.parent().unwrap_or(&self.root);
        if !path.starts_with(base) {
            return Ok(true);
        }
        let includes = strings(value, "includes", config)?;
        let excludes = strings(value, "excludes", config)?.unwrap_or_default();
        if excludes.iter().any(|pattern| pattern.starts_with('!')) {
            self.note("dprint", "negated excludes delegated pending native differential validation of directory re-inclusion; no static exclusions");
            return Ok(true);
        }
        if includes
            .as_ref()
            .is_some_and(|patterns| patterns.iter().any(|pattern| pattern.starts_with('!')))
        {
            self.note(
                "dprint",
                "negated include patterns delegated (independent include/exclude precedence)",
            );
            return Ok(true);
        }
        if !self.dprint_patterns.contains_key(config) {
            let include = includes
                .as_ref()
                .map(|patterns| dprint_includes(base, patterns, config))
                .transpose()?;
            let exclude = gitignore_patterns(base, &excludes, config)?;
            self.dprint_patterns.insert(
                config.to_path_buf(),
                (
                    include,
                    exclude,
                    includes.as_ref().is_some_and(Vec::is_empty),
                ),
            );
        }
        self.note("dprint", "supported config-relative includes (override semantics) and excludes (gitignore semantics, parent matching); plugin associations, nested config discovery and native VCS defaults delegated");
        let (include, exclude, empty) = &self.dprint_patterns[config];
        if *empty || ignored_with_parents(exclude, path, false) {
            return Ok(false);
        }
        Ok(include
            .as_ref()
            .is_none_or(|include| !include.matched(path, false).is_ignore()))
    }
}

// Check every parent even when the file has an explicit whitelist match:
// gitignore negation cannot resurrect a child of an excluded directory.
fn ignored_with_parents(ignore: &Gitignore, path: &Path, is_dir: bool) -> bool {
    if ignore.matched(path, is_dir).is_ignore() {
        return true;
    }
    for parent in path.parent().into_iter().flat_map(Path::ancestors) {
        if parent == ignore.path() || !parent.starts_with(ignore.path()) {
            break;
        }
        if ignore.matched(parent, true).is_ignore() {
            return true;
        }
    }
    false
}

fn invalid(path: &Path, error: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("native scope {}: {error}", path.display()),
    )
}

fn absolute(base: &Path, path: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn snapshot_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn json_names(tool: &str) -> &'static [&'static str] {
    if tool == "biome" {
        &["biome.json", "biome.jsonc"]
    } else {
        &[
            "dprint.json",
            "dprint.jsonc",
            ".dprint.json",
            ".dprint.jsonc",
        ]
    }
}

fn option_values(args: &[String], names: &[&str]) -> io::Result<Vec<String>> {
    let mut values = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if names.contains(&arg.as_str()) {
            let value = iter
                .next()
                .filter(|value| !value.starts_with('-'))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("native scope: {arg} requires a value"),
                    )
                })?;
            values.push(value.clone());
        } else if let Some((name, value)) = arg.split_once('=')
            && names.contains(&name)
        {
            if value.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("native scope: {name} requires a value"),
                ));
            }
            values.push(value.to_owned());
        }
    }
    Ok(values)
}

fn strings(value: &Value, key: &str, path: &Path) -> io::Result<Option<Vec<String>>> {
    value
        .get(key)
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| invalid(path, format!("{key} must be an array of strings")))?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invalid(path, format!("{key} must contain only strings")))
                })
                .collect()
        })
        .transpose()
}

fn taplo_globs(base: &Path, patterns: &[String], config: &Path) -> io::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let pattern = absolute(base, Path::new(pattern));
        builder.add(Glob::new(&pattern.to_string_lossy()).map_err(|error| invalid(config, error))?);
    }
    builder.build().map_err(|error| invalid(config, error))
}

fn validate_json_selectors(tool: &str, path: &Path, value: &Value) -> io::Result<()> {
    if tool == "dprint" {
        strings(value, "includes", path)?;
        strings(value, "excludes", path)?;
    } else {
        for name in ["files", "formatter", "linter", "assist"] {
            if let Some(group) = value.get(name) {
                if !group.is_object() {
                    return Err(invalid(path, format!("{name} must be an object")));
                }
                for key in ["includes", "include", "ignore"] {
                    strings(group, key, path)?;
                }
                if group
                    .get("enabled")
                    .is_some_and(|value| !value.is_boolean())
                {
                    return Err(invalid(path, format!("{name}.enabled must be a boolean")));
                }
            }
        }
    }
    Ok(())
}

fn is_literal_pattern(pattern: &str) -> bool {
    !pattern.contains(['*', '?', '[', ']', '{', '}', '\\', '!'])
}

fn normalize_dprint(pattern: &str) -> String {
    pattern.strip_prefix("!./").map_or_else(
        || {
            pattern
                .strip_prefix("./")
                .map_or_else(|| pattern.to_owned(), |rest| format!("/{rest}"))
        },
        |rest| format!("!/{rest}"),
    )
}

fn gitignore_patterns(base: &Path, patterns: &[String], config: &Path) -> io::Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(base);
    let literal_fast_path = !patterns.iter().any(|pattern| pattern.starts_with('!'));
    for pattern in patterns {
        let pattern = if literal_fast_path && is_literal_pattern(pattern) {
            format!(
                "/{}",
                pattern.trim_start_matches("./").trim_start_matches('/')
            )
        } else {
            normalize_dprint(pattern)
        };
        builder
            .add_line(Some(config.to_path_buf()), &pattern)
            .map_err(|error| invalid(config, error))?;
    }
    builder.build().map_err(|error| invalid(config, error))
}

fn dprint_includes(base: &Path, patterns: &[String], config: &Path) -> io::Result<Override> {
    let mut builder = OverrideBuilder::new(base);
    for pattern in patterns {
        let pattern = if is_literal_pattern(pattern) {
            format!(
                "/{}",
                pattern.trim_start_matches("./").trim_start_matches('/')
            )
        } else {
            normalize_dprint(pattern)
        };
        builder
            .add(&pattern)
            .map_err(|error| invalid(config, error))?;
    }
    builder.build().map_err(|error| invalid(config, error))
}

/// Biome 2's portable static subset: ordered, whole-path matches, with `*`
/// bounded by separators and last-match negation. Braces, escapes, character
/// classes, forced-negation and absolute globs are explicitly delegated, not
/// approximated by a different glob dialect.
struct BiomePatterns {
    patterns: Vec<(bool, globset::GlobMatcher)>,
    delegated: bool,
}

impl BiomePatterns {
    fn new(patterns: &[String], config: &Path) -> io::Result<Self> {
        let mut compiled = Vec::new();
        let mut delegated = false;
        for pattern in patterns {
            let (positive, pattern) = pattern
                .strip_prefix('!')
                .map_or((true, pattern.as_str()), |rest| (false, rest));
            if pattern.is_empty() {
                return Err(invalid(config, "empty Biome glob"));
            }
            if pattern.contains('?') {
                delegated = true;
                continue;
            }
            let segments = pattern.split('/').collect::<Vec<_>>();
            if segments
                .iter()
                .any(|segment| segment.contains("**") && *segment != "**")
                || segments.windows(2).any(|pair| pair == ["**", "**"])
            {
                return Err(invalid(
                    config,
                    "Biome globstar must occupy a segment and cannot be consecutive",
                ));
            }
            if pattern.contains(['[', ']', '{', '}', '\\'])
                || pattern.starts_with(['!', '/'])
                || pattern.contains("***")
                || pattern.ends_with('/')
                || pattern.split('/').any(|part| part == ".." || part == ".")
            {
                delegated = true;
                continue;
            }
            let glob = globset::GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|error| invalid(config, error))?;
            compiled.push((positive, glob.compile_matcher()));
        }
        Ok(Self {
            patterns: compiled,
            delegated,
        })
    }

    fn allows(&self, path: &Path) -> bool {
        self.patterns
            .iter()
            .rev()
            .find_map(|(positive, pattern)| pattern.is_match(path).then_some(*positive))
            .unwrap_or(false)
    }
}

/// Catalog audit: integrations not listed as supported are deliberately delegated
/// to their native process. This is not a claim that they have no selectors.
fn delegated_selectors(tool: &str) -> &'static str {
    match tool {
        "eslint" | "stylelint" | "remark" | "standard" => {
            "delegated: executable/extended configuration and version-specific ignores"
        }
        "ruff" => {
            "delegated: recursive include/exclude and extend-exclude are bypassed for explicit files unless force-exclude is enabled; no recursive/explicit parity claimed"
        }
        "black" => {
            "delegated: normal exclude/extend-exclude bypass explicit filenames; force-exclude remains active (regex semantics, not globs)"
        }
        "isort" | "mypy" | "pylint" | "flake8" | "ty" | "pyright" | "bandit" | "yapf"
        | "autopep8" => {
            "delegated: Python config include/exclude, regex/force-exclude and explicit-file semantics"
        }
        "clippy" | "rustfmt" => "delegated: Cargo package/target/workspace selection",
        "deno" => "delegated: config include/exclude and workspace member semantics",
        "stylua" | "selene" | "luacheck" => {
            "delegated: native ignore/config selectors and explicit-file semantics"
        }
        "clang-format" | "clang-tidy" | "cppcheck" => {
            "delegated: native ignore, header/regex and compile-database selectors"
        }
        "markdownlint" | "rumdl" | "yamlfmt" | "yamllint" | "codespell" | "cspell" | "vale"
        | "oxfmt" | "oxlint" => {
            "delegated: native file/ignore selectors and extended configuration"
        }
        "rubocop" | "standardrb" | "swiftformat" | "ktlint" | "sqlfluff" | "phpstan" | "phpcs"
        | "scalafmt" | "csharpier" | "dotnet-format" | "mix-format" | "buf" | "buf-lint" => {
            "delegated: native include/exclude, project and inherited configuration selectors"
        }
        _ => "delegated: native selectors/defaults (no planning-level exclusion support claimed)",
    }
}

#[cfg(test)]
mod tests;
