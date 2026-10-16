//! Static Python recursive selection intent, applied before explicit-file calls.
//!
//! Ruff's `FilePattern::User` uses `Glob::new` on both an absolute pattern and
//! (for slashless patterns) a basename. Black searches Python regexes against
//! slash-prefixed project-relative paths, with a trailing slash for directories.

use super::{
    Glob, GlobSet, GlobSetBuilder, NativeScope, NativeScopeSet, Path, PathBuf, Value, absolute,
    invalid, io, option_values, strings,
};
use regex::{Regex, RegexBuilder};

pub(super) struct PythonScope {
    tool: String,
    base: PathBuf,
    explicit: bool,
    args: Vec<String>,
    config: Option<PathBuf>,
    selector: Selector,
}

enum Selector {
    Ruff {
        include: GlobSet,
        exclude: GlobSet,
        operation_exclude: GlobSet,
    },
    Black {
        include: Regex,
        excludes: Vec<Regex>,
    },
}

const RUFF_EXCLUDES: &[&str] = &[
    ".bzr",
    ".direnv",
    ".eggs",
    ".git",
    ".git-rewrite",
    ".hg",
    ".ipynb_checkpoints",
    ".mypy_cache",
    ".nox",
    ".pants.d",
    ".pyenv",
    ".pytest_cache",
    ".pytype",
    ".ruff_cache",
    ".svn",
    ".tox",
    ".venv",
    ".vscode",
    "__pypackages__",
    "_build",
    "buck-out",
    "build",
    "dist",
    "node_modules",
    "site-packages",
    "venv",
];
const BLACK_EXCLUDE: &str = r"/(\.direnv|\.eggs|\.git|\.hg|\.ipynb_checkpoints|\.mypy_cache|\.nox|\.pytest_cache|\.ruff_cache|\.tox|\.svn|\.venv|\.vscode|__pypackages__|_build|buck-out|build|dist|venv)/";

impl NativeScopeSet {
    pub(super) fn python_scope(&mut self, tool: &str, args: &[String]) -> io::Result<NativeScope> {
        let flags: &[&str] = &["--config"];
        let explicit = option_values(args, flags)?;
        if explicit.len() > 1 || explicit.iter().any(|value| value.contains('=')) {
            return Err(unsupported(
                tool,
                "multiple/inline configs require native configuration evaluation",
            ));
        }
        let isolated = args.iter().any(|arg| arg == "--isolated");
        let config = if let Some(value) = explicit.last() {
            Some(absolute(&self.root, Path::new(value)))
        } else if isolated {
            None
        } else {
            self.python_find(&self.root.clone(), tool)?
        };
        let mut settings = match &config {
            Some(path) => self
                .python_settings(path, tool)?
                .unwrap_or_else(|| serde_json::json!({})),
            None => serde_json::json!({}),
        };
        if tool == "black" {
            normalize_black_keys(&mut settings, config.as_deref().unwrap_or(&self.root))?;
        }
        if settings.get("extend").is_some() {
            return Err(unsupported(
                tool,
                "configuration inheritance/extend is not supported; no files were excluded",
            ));
        }
        // Explicit Ruff config patterns are cwd-relative; discovered Ruff
        // patterns are config-directory-relative. Black regexes are rooted at
        // its discovered project, not an externally supplied --config file.
        let base = if tool == "ruff" && explicit.is_empty() {
            config
                .as_deref()
                .and_then(Path::parent)
                .unwrap_or(&self.root)
                .to_path_buf()
        } else if tool == "black" {
            self.black_root()?
        } else {
            self.root.clone()
        };
        let provenance = config.as_ref().map_or_else(
            || "native defaults".to_owned(),
            |path| path.display().to_string(),
        );
        let selector = if tool == "ruff" {
            ruff_selector(&base, &settings, args, config.as_deref().unwrap_or(&base))?
        } else {
            black_selector(&settings, args, config.as_deref().unwrap_or(&base))?
        };
        self.note(tool, format!("supported static recursive include/exclude selection from {provenance}; root {}. Clippier preserves recursive intent before explicit-file invocation: native normal excludes/includes can be bypassed by explicit filenames; Black force-exclude always applies and Ruff requires force-exclude for native explicit-file exclusion", base.display()));
        self.note(tool, "Clippier discovery VCS policy remains an outer boundary; native user/global configuration and VCS re-inclusion are not claimed as parity");
        Ok(NativeScope::Python(PythonScope {
            tool: tool.to_owned(),
            base,
            explicit: !explicit.is_empty() || isolated,
            args: args.to_vec(),
            config,
            selector,
        }))
    }

    fn python_settings(&mut self, path: &Path, tool: &str) -> io::Result<Option<Value>> {
        let value = self.load_format(path, true)?;
        let settings = if tool == "black"
            || path
                .file_name()
                .is_some_and(|name| name == "pyproject.toml")
        {
            value.get("tool").and_then(|tools| tools.get(tool)).cloned()
        } else {
            Some(value)
        };
        if settings
            .as_ref()
            .is_some_and(|settings| !settings.is_object())
        {
            return Err(invalid(path, format!("tool.{tool} must be a table")));
        }
        Ok(settings)
    }

    fn python_find(&mut self, start: &Path, tool: &str) -> io::Result<Option<PathBuf>> {
        for directory in start.ancestors() {
            if tool == "ruff" {
                for name in [".ruff.toml", "ruff.toml"] {
                    let path = directory.join(name);
                    if self.snapshot(&path)?.is_some() {
                        return Ok(Some(path));
                    }
                }
            }
            let pyproject = directory.join("pyproject.toml");
            if self.snapshot(&pyproject)?.is_some()
                && self.python_settings(&pyproject, tool)?.is_some()
            {
                return Ok(Some(pyproject));
            }
            // Black project discovery stops at a repository boundary. Ruff
            // keeps walking for configuration; neither adopts unrelated tables.
            if tool == "black" && self.python_project_marker(directory)? {
                break;
            }
        }
        Ok(None)
    }

    fn python_project_marker(&self, directory: &Path) -> io::Result<bool> {
        for name in [".git", ".hg"] {
            let mut cache = self
                .cache
                .0
                .lock()
                .map_err(|_| io::Error::other("native config cache poisoned"))?;
            let path = directory.join(name);
            let exists = if let Some(exists) = cache.markers.get(&path) {
                *exists
            } else {
                let exists = path.try_exists()?;
                cache.markers.insert(path, exists);
                exists
            };
            drop(cache);
            if exists {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn black_root(&mut self) -> io::Result<PathBuf> {
        self.black_root_from(&self.root.clone())
    }

    fn black_root_from(&mut self, root: &Path) -> io::Result<PathBuf> {
        let mut last = root.to_path_buf();
        for directory in root.ancestors() {
            last = directory.to_path_buf();
            let config = directory.join("pyproject.toml");
            if self.snapshot(&config)?.is_some()
                && self.python_settings(&config, "black")?.is_some()
            {
                return Ok(last);
            }
            if self.python_project_marker(directory)? {
                return Ok(last);
            }
        }
        Ok(last)
    }

    /// Verifies that a final explicit-file invocation retains planned native identity.
    /// Call once per actual batch, before `validate_snapshot` and execution.
    ///
    /// # Errors
    ///
    /// * Returns `Unsupported` if Black would discover a different root/config.
    /// * Returns configuration lookup and parsing errors.
    pub(crate) fn validate_files(&mut self, tool: &str, files: &[PathBuf]) -> io::Result<()> {
        let Some(scope) = self.scopes.remove(tool) else {
            return Ok(());
        };
        let result = (|| {
            let NativeScope::Python(scope) = &scope else {
                return Ok(());
            };
            if scope.tool != "black" || files.is_empty() {
                return Ok(());
            }
            let mut parents = files.iter().map(|file| {
                let path = absolute(&self.root, file);
                path.parent().unwrap_or(&self.root).to_path_buf()
            });
            let mut common = parents.next().expect("nonempty batch");
            for parent in parents {
                while !parent.starts_with(&common) {
                    if !common.pop() {
                        return Err(unsupported("black", "batch paths have no common root"));
                    }
                }
            }
            let execution_root = self.black_root_from(&common)?;
            let execution_config = if scope.explicit {
                scope.config.clone()
            } else {
                self.python_find(&common, "black")?
            };
            if execution_root != scope.base || execution_config != scope.config {
                return Err(unsupported(
                    "black",
                    &format!(
                        "selected filename batch changes project root/config: planned {} {:?}, native batch {} {:?}; pin --config and select files sharing the planned project root, or rebuild a scope for this batch",
                        scope.base.display(),
                        scope.config,
                        execution_root.display(),
                        execution_config
                    ),
                ));
            }
            Ok(())
        })();
        self.scopes.insert(tool.to_owned(), scope);
        result
    }

    fn nested_ruff_config(
        &mut self,
        scope: &PythonScope,
        path: &Path,
    ) -> io::Result<Option<PathBuf>> {
        for directory in path.parent().unwrap_or(&scope.base).ancestors() {
            if directory == scope.base || !directory.starts_with(&scope.base) {
                break;
            }
            let key = ("ruff".to_owned(), directory.to_path_buf());
            if !self.directory_configs.contains_key(&key) {
                let mut found = None;
                for name in [".ruff.toml", "ruff.toml", "pyproject.toml"] {
                    let candidate = directory.join(name);
                    if self.snapshot(&candidate)?.is_some()
                        && self.python_settings(&candidate, "ruff")?.is_some()
                    {
                        found = Some(candidate);
                        break;
                    }
                }
                self.directory_configs.insert(key.clone(), found);
            }
            if let Some(config) = &self.directory_configs[&key] {
                return Ok(Some(config.clone()));
            }
        }
        Ok(None)
    }

    // Keep the two native dialects adjacent so their parent matching stays auditable.
    #[allow(clippy::too_many_lines)]
    pub(super) fn allows_python(&mut self, scope: &PythonScope, path: &Path) -> io::Result<bool> {
        if !path.starts_with(&scope.base) {
            return Ok(true);
        }
        if !scope.explicit
            && scope.tool == "ruff"
            && let Some(config) = self.nested_ruff_config(scope, path)?
        {
            if !self.nested_ruff.contains_key(&config) {
                let settings = self
                    .python_settings(&config, "ruff")?
                    .expect("selected Ruff settings");
                if settings.get("extend").is_some() {
                    return Err(unsupported(
                        "ruff",
                        &format!(
                            "{}: nested configuration inheritance/extend is not supported",
                            config.display()
                        ),
                    ));
                }
                let base = config.parent().expect("config parent").to_path_buf();
                let selector = ruff_selector(&base, &settings, &scope.args, &config)?;
                self.note("ruff", format!("supported nearest per-file nested static selectors from {} (config-relative, independent defaults and operation exclusions)", config.display()));
                self.nested_ruff.insert(
                    config.clone(),
                    PythonScope {
                        tool: "ruff".to_owned(),
                        base,
                        explicit: true,
                        args: scope.args.clone(),
                        config: Some(config.clone()),
                        selector,
                    },
                );
            }
            let nested = self
                .nested_ruff
                .remove(&config)
                .expect("cached nested scope");
            let result = self.allows_python(&nested, path);
            self.nested_ruff.insert(config, nested);
            return result;
        }
        let relative = path.strip_prefix(&scope.base).expect("prefix checked");
        match &scope.selector {
            Selector::Ruff {
                include,
                exclude,
                operation_exclude,
            } => {
                let matches = |set: &GlobSet, path: &Path| {
                    set.is_match(path)
                        || path
                            .file_name()
                            .is_some_and(|name| set.is_match(Path::new(name)))
                };
                if !matches(include, path) {
                    self.note(
                        &scope.tool,
                        format!(
                            "{} excluded: no include/extend-include rule matched",
                            relative.display()
                        ),
                    );
                    return Ok(false);
                }
                if matches(operation_exclude, path) {
                    self.note(
                        &scope.tool,
                        format!(
                            "{} excluded: operation-specific exclude matched",
                            relative.display()
                        ),
                    );
                    return Ok(false);
                }
                for ancestor in path.ancestors() {
                    if ancestor == scope.base {
                        break;
                    }
                    if matches(exclude, ancestor) {
                        self.note(
                            &scope.tool,
                            format!(
                                "{} excluded: exclude/extend-exclude matched {}",
                                relative.display(),
                                ancestor.display()
                            ),
                        );
                        return Ok(false);
                    }
                }
            }
            Selector::Black { include, excludes } => {
                let text = format!("/{}", relative.to_string_lossy().replace('\\', "/"));
                if !include.is_match(&text) {
                    self.note(
                        &scope.tool,
                        format!(
                            "{} excluded: include regex {} did not match",
                            relative.display(),
                            include.as_str()
                        ),
                    );
                    return Ok(false);
                }
                if let Some(regex) = excludes.iter().find(|regex| nonempty_match(regex, &text)) {
                    self.note(
                        &scope.tool,
                        format!(
                            "{} excluded: regex {} matched file",
                            relative.display(),
                            regex.as_str()
                        ),
                    );
                    return Ok(false);
                }
                for parent in relative.parent().into_iter().flat_map(Path::ancestors) {
                    if parent.as_os_str().is_empty() {
                        break;
                    }
                    let text = format!("/{}/", parent.to_string_lossy().replace('\\', "/"));
                    if let Some(regex) = excludes.iter().find(|regex| nonempty_match(regex, &text))
                    {
                        self.note(
                            &scope.tool,
                            format!(
                                "{} excluded: regex {} matched parent {text}",
                                relative.display(),
                                regex.as_str()
                            ),
                        );
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
}

fn normalize_black_keys(value: &mut Value, config: &Path) -> io::Result<()> {
    let Some(settings) = value.as_object_mut() else {
        return Err(invalid(config, "Black settings must be a table"));
    };
    for key in settings.keys() {
        let normalized = key.replace('-', "_");
        if normalized != *key && settings.contains_key(&normalized) {
            return Err(invalid(
                config,
                format!("ambiguous Black aliases {key} and {normalized}; specify only one"),
            ));
        }
    }
    for (key, value) in std::mem::take(settings) {
        let normalized = key.replace('_', "-");
        if settings.insert(normalized.clone(), value).is_some() {
            return Err(invalid(
                config,
                format!("ambiguous Black aliases {key} and {normalized}"),
            ));
        }
    }
    Ok(())
}

fn unsupported(tool: &str, message: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("native scope {tool}: {message}"),
    )
}

fn array_setting(
    value: &Value,
    key: &str,
    default: &[&str],
    path: &Path,
) -> io::Result<Vec<String>> {
    Ok(strings(value, key, path)?
        .unwrap_or_else(|| default.iter().map(|value| (*value).to_owned()).collect()))
}

fn ruff_selector(
    base: &Path,
    value: &Value,
    args: &[String],
    config: &Path,
) -> io::Result<Selector> {
    let mut include = array_setting(
        value,
        "include",
        &["*.py", "*.pyi", "*.ipynb", "**/pyproject.toml"],
        config,
    )?;
    let mut exclude = array_setting(value, "exclude", RUFF_EXCLUDES, config)?;
    for (flag, patterns) in [("--include", &mut include), ("--exclude", &mut exclude)] {
        let values = option_values(args, &[flag])?;
        if !values.is_empty() {
            *patterns = values
                .iter()
                .flat_map(|value| value.split(',').map(str::to_owned))
                .collect();
        }
    }
    for (key, flag, patterns) in [
        ("extend-include", "--extend-include", &mut include),
        ("extend-exclude", "--extend-exclude", &mut exclude),
    ] {
        let values = option_values(args, &[flag])?;
        if values.is_empty() {
            patterns.extend(array_setting(value, key, &[], config)?);
        } else {
            patterns.extend(
                values
                    .iter()
                    .flat_map(|value| value.split(',').map(str::to_owned)),
            );
        }
    }
    let operation = if args.iter().any(|arg| arg == "format") {
        "format"
    } else if args.iter().any(|arg| arg == "check") {
        "lint"
    } else {
        return Err(unsupported(
            "ruff",
            "operation must be check or format to resolve operation-specific exclusions",
        ));
    };
    let operation_exclude = if let Some(group) = value.get(operation) {
        if !group.is_object() {
            return Err(invalid(config, format!("{operation} must be a table")));
        }
        array_setting(group, "exclude", &[], config)?
    } else {
        Vec::new()
    };
    if value
        .get("force-exclude")
        .is_some_and(|value| !value.is_boolean())
        || value
            .get("respect-gitignore")
            .is_some_and(|value| !value.is_boolean())
    {
        return Err(invalid(
            config,
            "force-exclude and respect-gitignore must be booleans",
        ));
    }
    Ok(Selector::Ruff {
        include: ruff_globs(base, &include, config)?,
        exclude: ruff_globs(base, &exclude, config)?,
        operation_exclude: ruff_globs(base, &operation_exclude, config)?,
    })
}

fn ruff_globs(base: &Path, patterns: &[String], config: &Path) -> io::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        // Ruff does not implement gitignore negation: ! is an ordinary glob
        // character. Compile exactly the same globset dialect as FilePattern.
        builder.add(
            Glob::new(&absolute(base, Path::new(pattern)).to_string_lossy())
                .map_err(|error| invalid(config, error))?,
        );
        if !pattern.contains(std::path::MAIN_SEPARATOR) {
            builder.add(Glob::new(pattern).map_err(|error| invalid(config, error))?);
        }
    }
    builder.build().map_err(|error| invalid(config, error))
}

fn regex_setting(
    value: &Value,
    key: &str,
    default: Option<&str>,
    args: &[String],
    config: &Path,
) -> io::Result<Option<Regex>> {
    let flag = format!("--{key}");
    let cli = option_values(args, &[flag.as_str()])?;
    let pattern = if let Some(pattern) = cli.last() {
        Some(pattern.as_str())
    } else if let Some(pattern) = value.get(key) {
        Some(
            pattern
                .as_str()
                .ok_or_else(|| invalid(config, format!("{key} must be a regex string")))?,
        )
    } else {
        default
    };
    pattern
        .map(|pattern| {
            // Black enables re.VERBOSE only for multiline strings. Rust regex
            // deliberately rejects Python-only lookaround/backreferences, rather
            // than approximating an exclusion that could remove the wrong files.
            RegexBuilder::new(pattern)
                .ignore_whitespace(pattern.contains('\n'))
                .build()
                .map_err(|error| {
                    unsupported(
                        "black",
                        &format!(
                            "{}: unsupported or invalid {key} regex: {error}",
                            config.display()
                        ),
                    )
                })
        })
        .transpose()
}

fn black_selector(value: &Value, args: &[String], config: &Path) -> io::Result<Selector> {
    let include = regex_setting(value, "include", Some(r"(\.pyi?|\.ipynb)$"), args, config)?
        .expect("default include");
    let mut excludes = Vec::new();
    for (key, default) in [
        ("exclude", Some(BLACK_EXCLUDE)),
        ("extend-exclude", None),
        ("force-exclude", None),
    ] {
        if let Some(regex) = regex_setting(value, key, default, args, config)? {
            excludes.push(regex);
        }
    }
    Ok(Selector::Black { include, excludes })
}

fn nonempty_match(regex: &Regex, text: &str) -> bool {
    regex.find(text).is_some_and(|matched| !matched.is_empty())
}
