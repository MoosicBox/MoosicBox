//! Shared runner scope configuration and path filtering.

use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use super::BoxError;

/// Repository content boundaries applied to all file-oriented tools.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ScopeConfig {
    /// Glob patterns excluded from every file-oriented tool invocation.
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug)]
pub struct ScopeMatcher {
    root: PathBuf,
    excludes: GlobSet,
    excluded_dirs: GlobSet,
}

impl ScopeMatcher {
    pub(crate) fn new(root: &Path, patterns: &[String]) -> Result<Self, BoxError> {
        let root = absolute_path(root, root);
        let mut excludes = GlobSetBuilder::new();
        let mut excluded_dirs = GlobSetBuilder::new();

        for pattern in patterns {
            let resolved = resolve_pattern(pattern, &root);
            excludes.add(Glob::new(&resolved)?);
            if let Some(directory) = resolved.strip_suffix("/**") {
                excluded_dirs.add(Glob::new(directory)?);
            }
        }

        Ok(Self {
            root,
            excludes: excludes.build()?,
            excluded_dirs: excluded_dirs.build()?,
        })
    }

    pub(crate) fn is_excluded(&self, path: &Path) -> bool {
        self.excludes.is_match(self.absolute(path))
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
