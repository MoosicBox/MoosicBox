//! Shared runner scope configuration and path filtering.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use super::BoxError;

/// Repository content boundaries applied to all file-oriented tools.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ScopeConfig {
    /// Glob patterns excluded from every file-oriented tool invocation.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Enable ecosystem-derived dependency, cache, and build exclusions.
    #[serde(default = "default_true")]
    pub automatic_excludes: bool,
    /// Automatic exclusion profiles disabled by ecosystem ID.
    #[serde(default)]
    pub disable_profiles: BTreeSet<String>,
}

const fn default_true() -> bool {
    true
}

impl Default for ScopeConfig {
    fn default() -> Self {
        Self {
            exclude: Vec::new(),
            automatic_excludes: true,
            disable_profiles: BTreeSet::new(),
        }
    }
}

/// One effective repository scope shared by discovery, planning, reporting, and
/// file-oriented execution.
#[derive(Debug, Clone)]
pub struct EffectiveScope {
    /// Resolved runner scope configuration.
    pub config: ScopeConfig,
    /// Runner-wide exclusions, including active automatic profiles.
    pub exclusions: Vec<String>,
}

impl EffectiveScope {
    /// Resolves configured and automatic exclusions for a repository root.
    #[must_use]
    pub fn resolve(root: &Path, config: ScopeConfig) -> Self {
        let mut exclusions = config.exclude.clone();
        exclusions.extend(automatic_exclusion_patterns(root, &config));
        exclusions.sort();
        exclusions.dedup();
        Self { config, exclusions }
    }

    /// Builds the global scope matcher.
    pub(crate) fn matcher(&self, root: &Path) -> Result<ScopeMatcher, BoxError> {
        ScopeMatcher::new(root, &self.exclusions)
    }
}

#[derive(Debug)]
pub struct ScopeMatcher {
    root: PathBuf,
    includes: Option<GlobSet>,
    excludes: GlobSet,
    excluded_dirs: GlobSet,
}

impl ScopeMatcher {
    pub(crate) fn new(root: &Path, patterns: &[String]) -> Result<Self, BoxError> {
        Self::with_patterns(root, &[], patterns)
    }

    pub(crate) fn with_patterns(
        root: &Path,
        include_patterns: &[String],
        exclude_patterns: &[String],
    ) -> Result<Self, BoxError> {
        let root = absolute_path(root, root);
        let mut includes = GlobSetBuilder::new();
        for pattern in include_patterns {
            includes.add(Glob::new(&resolve_pattern(pattern, &root))?);
        }
        let includes = if include_patterns.is_empty() {
            None
        } else {
            Some(includes.build()?)
        };
        let mut excludes = GlobSetBuilder::new();
        let mut excluded_dirs = GlobSetBuilder::new();

        for pattern in exclude_patterns {
            let resolved = resolve_pattern(pattern, &root);
            excludes.add(Glob::new(&resolved)?);
            if let Some(directory) = resolved.strip_suffix("/**") {
                excluded_dirs.add(Glob::new(directory)?);
            }
        }

        Ok(Self {
            root,
            includes,
            excludes: excludes.build()?,
            excluded_dirs: excluded_dirs.build()?,
        })
    }

    pub(crate) fn is_excluded(&self, path: &Path) -> bool {
        let absolute = self.absolute(path);
        self.includes
            .as_ref()
            .is_some_and(|includes| !includes.is_match(&absolute))
            || self.excludes.is_match(absolute)
    }

    pub(crate) fn collect_files(
        &self,
        roots: &[PathBuf],
        extensions: &std::collections::BTreeSet<String>,
    ) -> Vec<PathBuf> {
        let mut files = std::collections::BTreeSet::new();
        for root in roots {
            let mut builder = WalkBuilder::new(root);
            builder.hidden(false);
            builder.require_git(false);
            builder.parents(true);
            builder.git_ignore(true);
            builder.git_global(true);
            builder.git_exclude(true);
            builder.ignore(true);
            let excluded_dirs = self.excluded_dirs.clone();
            let excludes = self.excludes.clone();
            let root = self.root.clone();
            builder.filter_entry(move |entry| {
                let absolute = absolute_path(entry.path(), &root);
                !excluded_dirs.is_match(&absolute) && !excludes.is_match(&absolute)
            });
            for result in builder.build() {
                let Ok(entry) = result else {
                    continue;
                };
                let path = entry.path();
                if self.is_excluded(path) || !entry.file_type().is_some_and(|kind| kind.is_file()) {
                    continue;
                }
                let included = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|extension| extensions.contains(&extension.to_ascii_lowercase()));
                if included {
                    files.insert(path.to_path_buf());
                }
            }
        }
        files.into_iter().collect()
    }

    fn absolute(&self, path: &Path) -> PathBuf {
        absolute_path(path, &self.root)
    }
}

/// Returns safe automatic exclusion patterns activated by repository manifests.
#[must_use]
pub fn automatic_exclusion_patterns(root: &Path, config: &ScopeConfig) -> Vec<String> {
    if !config.automatic_excludes {
        return Vec::new();
    }
    let profiles: &[(&str, &[&str], &[&str])] = &[
        ("rust", &["Cargo.toml"], &["target/**"]),
        (
            "node",
            &["package.json"],
            &[
                "node_modules/**",
                ".pnpm-store/**",
                ".yarn/cache/**",
                ".next/**",
                ".nuxt/**",
            ],
        ),
        (
            "python",
            &["pyproject.toml", "requirements.txt", "setup.py"],
            &[
                ".venv/**",
                "venv/**",
                "__pycache__/**",
                ".pytest_cache/**",
                ".mypy_cache/**",
                ".ruff_cache/**",
            ],
        ),
        ("go", &["go.mod"], &["vendor/**"]),
        ("terraform", &[".terraform.lock.hcl"], &[".terraform/**"]),
    ];
    let mut patterns = BTreeSet::from([
        ".git/**".to_string(),
        ".hg/**".to_string(),
        ".svn/**".to_string(),
    ]);

    let mut builder = WalkBuilder::new(root);
    builder.hidden(false);
    builder.require_git(false);
    builder.parents(true);
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);
    builder.ignore(true);
    builder.filter_entry(|entry| {
        entry.depth() == 0
            || !entry.file_type().is_some_and(|kind| kind.is_dir())
            || !is_proven_generated_directory(entry.file_name().to_str().unwrap_or_default())
    });

    for result in builder.build() {
        let Ok(entry) = result else {
            continue;
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Some(file_name) = entry.file_name().to_str() else {
            continue;
        };
        let package_root = entry.path().parent().unwrap_or(root);
        let Ok(relative_root) = package_root.strip_prefix(root) else {
            continue;
        };
        for (id, manifests, exclusions) in profiles {
            if config.disable_profiles.contains(*id) || !manifests.contains(&file_name) {
                continue;
            }
            for exclusion in *exclusions {
                patterns.insert(if relative_root.as_os_str().is_empty() {
                    (*exclusion).to_string()
                } else {
                    format!(
                        "{}/{}",
                        relative_root.to_string_lossy().replace('\\', "/"),
                        exclusion
                    )
                });
            }
        }
    }
    patterns.into_iter().collect()
}

fn is_proven_generated_directory(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | "target"
            | "node_modules"
            | ".pnpm-store"
            | ".yarn"
            | ".next"
            | ".nuxt"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
            | ".terraform"
    )
}

fn absolute_path(path: &Path, root: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path.strip_prefix(Path::new(".")).unwrap_or(path))
    };
    normalize_path(&path)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn resolve_pattern(pattern: &str, root: &Path) -> String {
    normalize_path(&root.join(pattern.trim_start_matches('/')))
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_profiles_require_ecosystem_evidence_and_can_be_disabled() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let config = ScopeConfig::default();
        assert_eq!(
            automatic_exclusion_patterns(root.path(), &config),
            [".git/**", ".hg/**", ".svn/**", "target/**"]
        );

        let config = ScopeConfig {
            disable_profiles: BTreeSet::from(["rust".to_string()]),
            ..Default::default()
        };
        let patterns = automatic_exclusion_patterns(root.path(), &config);
        assert_eq!(patterns, [".git/**", ".hg/**", ".svn/**"]);
    }

    #[test]
    fn automatic_profiles_are_activated_at_nested_package_roots() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("packages/web")).unwrap();
        std::fs::write(root.path().join("packages/web/package.json"), "{}").unwrap();

        let patterns = automatic_exclusion_patterns(root.path(), &ScopeConfig::default());
        assert!(patterns.contains(&"packages/web/node_modules/**".to_string()));
        assert!(patterns.contains(&"packages/web/.next/**".to_string()));
        assert!(!patterns.contains(&"node_modules/**".to_string()));
    }

    #[test]
    fn generic_source_directories_are_not_excluded_without_a_manifest_role() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("vendor")).unwrap();
        std::fs::write(root.path().join("vendor/source.go"), "package vendor\n").unwrap();

        let patterns = automatic_exclusion_patterns(root.path(), &ScopeConfig::default());
        assert!(!patterns.iter().any(|pattern| pattern.starts_with("vendor")));

        std::fs::write(root.path().join("go.mod"), "module example.test\n").unwrap();
        let patterns = automatic_exclusion_patterns(root.path(), &ScopeConfig::default());
        assert!(patterns.contains(&"vendor/**".to_string()));
    }

    #[test]
    fn include_and_exclude_patterns_compose() {
        let matcher = ScopeMatcher::with_patterns(
            Path::new("/workspace"),
            &["src/**".to_string()],
            &["src/generated/**".to_string()],
        )
        .expect("failed to build scope matcher");

        assert!(!matcher.is_excluded(Path::new("/workspace/src/main.rs")));
        assert!(matcher.is_excluded(Path::new("/workspace/tests/test.rs")));
        assert!(matcher.is_excluded(Path::new("/workspace/src/generated/output.rs")));
    }

    #[test]
    fn anchored_patterns_are_relative_to_the_runner_config_directory() {
        let matcher = ScopeMatcher::new(
            Path::new("/workspace"),
            &["/vendor/checkout/**".to_string()],
        )
        .expect("failed to build scope matcher");

        assert!(matcher.is_excluded(Path::new("/workspace/vendor/checkout/file.js")));
        assert!(!matcher.is_excluded(Path::new("/other/vendor/checkout/file.js")));
    }
}
