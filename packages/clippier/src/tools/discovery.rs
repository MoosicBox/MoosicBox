//! Shared repository discovery and capability-aware automatic tool planning.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use super::{TOOL_CATALOG, ToolCapability, ToolRegistry, automatic_exclusion_patterns};

/// Strength of repository evidence selecting a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SelectionEvidenceKind {
    /// A relevant source file exists, enabling a narrow content default.
    Content,
    /// An ecosystem manifest activates a conventional tool.
    Manifest,
    /// Native configuration explicitly identifies the tool.
    NativeConfig,
    /// Clippier configuration explicitly enables the tool.
    ClippierConfig,
}

/// One repository fact relevant to automatic tool selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionEvidence {
    /// Evidence strength.
    pub kind: SelectionEvidenceKind,
    /// Repository-relative path which supplied the evidence.
    pub path: PathBuf,
}

/// Ignore-aware repository facts shared by check and format planning.
#[derive(Debug, Clone, Default)]
pub struct RepositoryDiscovery {
    root: PathBuf,
    files: BTreeSet<PathBuf>,
    basenames: BTreeMap<String, BTreeSet<PathBuf>>,
    extensions: BTreeMap<String, BTreeSet<PathBuf>>,
}

impl RepositoryDiscovery {
    /// Discovers files below a repository root while respecting VCS ignore files.
    ///
    /// # Errors
    ///
    /// * If the working directory cannot be canonicalized
    pub fn discover(root: &Path) -> Result<Self, std::io::Error> {
        Self::discover_with_excludes(root, &[])
    }

    /// Discovers repository files with additional root-relative exclusions.
    ///
    /// # Errors
    ///
    /// * If the working directory cannot be canonicalized
    pub fn discover_with_excludes(
        root: &Path,
        excludes: &[String],
    ) -> Result<Self, std::io::Error> {
        let root = root.canonicalize()?;
        let matchers = excludes
            .iter()
            .filter_map(|pattern| globset::Glob::new(pattern).ok())
            .map(|glob| glob.compile_matcher())
            .collect::<Vec<_>>();
        let filter_root = root.clone();
        let mut result = Self {
            root: root.clone(),
            ..Self::default()
        };
        let mut builder = WalkBuilder::new(&root);
        builder
            .hidden(false)
            .require_git(false)
            .ignore(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .filter_entry(move |entry| {
                let relative = entry
                    .path()
                    .strip_prefix(&filter_root)
                    .unwrap_or_else(|_| entry.path());
                entry
                    .file_name()
                    .to_str()
                    .is_none_or(|name| !is_universal_excluded_dir(name))
                    && !matchers.iter().any(|matcher| matcher.is_match(relative))
            });

        for entry in builder.build().filter_map(Result::ok) {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(&root) else {
                continue;
            };
            let relative = relative.to_path_buf();
            result.files.insert(relative.clone());
            if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                result
                    .basenames
                    .entry(name.to_string())
                    .or_default()
                    .insert(relative.clone());
            }
            if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
                result
                    .extensions
                    .entry(extension.to_ascii_lowercase())
                    .or_default()
                    .insert(relative);
            }
        }
        Ok(result)
    }

    fn matching_signal(&self, signal: &str) -> Option<PathBuf> {
        if signal.contains('/') {
            self.files.get(Path::new(signal)).cloned()
        } else {
            self.basenames
                .get(signal)
                .and_then(|paths| paths.first().cloned())
        }
    }

    fn manifest_contains_tool_config(
        &self,
        signal: super::EmbeddedConfigSignal,
    ) -> Option<PathBuf> {
        for path in self.basenames.get(signal.manifest)? {
            let contents = std::fs::read_to_string(self.root.join(path)).ok()?;
            let configured = match signal.manifest {
                "pyproject.toml" => toml::from_str::<toml::Value>(&contents)
                    .ok()
                    .and_then(|value| {
                        let container = signal
                            .container
                            .map_or(Some(&value), |container| value.get(container));
                        container.and_then(|value| value.get(signal.key)).cloned()
                    })
                    .is_some(),
                "package.json" => serde_json::from_str::<serde_json::Value>(&contents)
                    .ok()
                    .is_some_and(|value| {
                        let container = signal
                            .container
                            .map_or(Some(&value), |container| value.get(container));
                        container.is_some_and(|value| value.get(signal.key).is_some())
                    }),
                _ => false,
            };
            if configured {
                return Some(path.clone());
            }
        }
        None
    }

    fn embedded_config_for(&self, entry: &super::ToolCatalogEntry) -> Option<PathBuf> {
        super::embedded_config_signals(entry.name)
            .iter()
            .find_map(|signal| self.manifest_contains_tool_config(*signal))
    }

    fn evidence_for(&self, tool_name: &str) -> Option<SelectionEvidence> {
        let entry = TOOL_CATALOG.iter().find(|entry| entry.name == tool_name)?;
        for config in entry.signals.configs {
            if let Some(path) = self.matching_signal(config) {
                return Some(SelectionEvidence {
                    kind: SelectionEvidenceKind::NativeConfig,
                    path,
                });
            }
        }
        if let Some(path) = self.embedded_config_for(entry) {
            return Some(SelectionEvidence {
                kind: SelectionEvidenceKind::NativeConfig,
                path,
            });
        }
        for manifest in entry.signals.manifests {
            if let Some(path) = self.matching_signal(manifest) {
                return Some(SelectionEvidence {
                    kind: SelectionEvidenceKind::Manifest,
                    path,
                });
            }
        }
        for extension in entry.signals.content_extensions {
            if let Some(path) = self
                .extensions
                .get(*extension)
                .and_then(|paths| paths.first().cloned())
            {
                return Some(SelectionEvidence {
                    kind: SelectionEvidenceKind::Content,
                    path,
                });
            }
        }
        None
    }
}

/// One automatically selected tool and the evidence that selected it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTool {
    /// Tool identifier.
    pub name: String,
    /// Strongest repository evidence for the tool.
    pub evidence: SelectionEvidence,
    /// Formatter extensions owned by this tool in automatic mode or explicitly
    /// assigned to it as part of a configured pipeline.
    pub format_extensions: BTreeSet<String>,
    /// Explicit formatter pipeline order.
    pub format_order: Option<i32>,
}

/// Shared automatic plan consumed by `check` and `fmt`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolPlan {
    /// Selected, installed tools in deterministic order.
    pub tools: Vec<PlannedTool>,
    /// Relevant tools which are not installed.
    pub unavailable: Vec<String>,
}

impl ToolPlan {
    /// Returns selected tool names.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|tool| tool.name.clone()).collect()
    }

    /// Returns true when explicit formatter ordering requires sequential execution.
    #[must_use]
    pub fn has_ordered_formatters(&self) -> bool {
        self.tools.iter().any(|tool| tool.format_order.is_some())
    }
}

/// Builds an installed-only automatic plan from shared repository evidence.
///
/// Format ownership is resolved per extension. Native configuration wins over
/// manifest and content defaults, then the catalog's deterministic priority is
/// used. Linter selection is additive.
///
/// # Errors
///
/// * If repository discovery fails
#[allow(clippy::too_many_lines)]
pub fn plan_tools(
    registry: &ToolRegistry,
    capabilities: &[ToolCapability],
) -> Result<ToolPlan, std::io::Error> {
    let scope_config = registry.config().effective_scope();
    let mut excludes = scope_config.exclude.clone();
    excludes.extend(automatic_exclusion_patterns(
        registry.working_dir(),
        &scope_config,
    ));
    let discovery = RepositoryDiscovery::discover_with_excludes(registry.working_dir(), &excludes)?;
    let mut candidates = Vec::new();
    let mut unavailable = Vec::new();

    for entry in TOOL_CATALOG {
        let policy = registry.config().tool_policy(entry.name);
        let supports_requested_capability = capabilities.iter().any(|capability| {
            entry.capabilities.contains(capability)
                && policy.is_none_or(|policy| policy.enables(*capability))
        });
        if !supports_requested_capability
            || policy.is_some_and(|policy| policy.mode == crate::tools::ToolSelectionMode::Disabled)
        {
            continue;
        }
        let evidence = if policy
            .is_some_and(|policy| policy.mode == crate::tools::ToolSelectionMode::Enabled)
        {
            SelectionEvidence {
                kind: SelectionEvidenceKind::ClippierConfig,
                path: PathBuf::from("clippier.toml"),
            }
        } else if let Some(evidence) = discovery.evidence_for(entry.name) {
            evidence
        } else {
            continue;
        };
        if registry.is_available(entry.name) {
            candidates.push((entry, evidence, policy));
        } else {
            unavailable.push(entry.name.to_string());
        }
    }

    let wants_format = capabilities.contains(&ToolCapability::Format);
    let wants_lint = capabilities.contains(&ToolCapability::Lint);
    let mut owners: BTreeMap<String, (&super::ToolCatalogEntry, SelectionEvidenceKind)> =
        BTreeMap::new();
    if wants_format {
        for (entry, evidence, policy) in &candidates {
            if !entry.capabilities.contains(&ToolCapability::Format)
                || policy.is_some_and(|policy| !policy.enables(ToolCapability::Format))
                || policy.is_some_and(|policy| policy.format_order.is_some())
            {
                continue;
            }
            for extension in configured_format_extensions(entry, *policy) {
                let replace = match owners.get(&extension) {
                    None => true,
                    Some((owner, owner_evidence)) => {
                        if evidence.kind == *owner_evidence
                            && entry.formatter_priority == owner.formatter_priority
                            && entry.name != owner.name
                        {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!(
                                    "automatic formatter ownership is ambiguous for extension '{extension}' between '{}' and '{}'; configure format-extensions and distinct format-order values to declare an explicit pipeline",
                                    owner.name, entry.name
                                ),
                            ));
                        }
                        evidence.kind > *owner_evidence
                            || (evidence.kind == *owner_evidence
                                && entry.formatter_priority < owner.formatter_priority)
                    }
                };
                if replace {
                    owners.insert(extension, (entry, evidence.kind));
                }
            }
        }
    }

    let mut tools = Vec::new();
    for (entry, evidence, policy) in candidates {
        let lint_selected = wants_lint
            && entry.capabilities.contains(&ToolCapability::Lint)
            && policy.is_none_or(|policy| policy.enables(ToolCapability::Lint));
        let explicitly_ordered = wants_format
            && entry.capabilities.contains(&ToolCapability::Format)
            && policy.is_some_and(|policy| {
                policy.enables(ToolCapability::Format) && policy.format_order.is_some()
            });
        let format_extensions = if explicitly_ordered {
            configured_format_extensions(entry, policy)
        } else if wants_format
            && entry.capabilities.contains(&ToolCapability::Format)
            && policy.is_none_or(|policy| policy.enables(ToolCapability::Format))
        {
            owners
                .iter()
                .filter(|(_, (owner, _))| owner.name == entry.name)
                .map(|(extension, _)| extension.clone())
                .collect()
        } else {
            BTreeSet::new()
        };
        if lint_selected || !format_extensions.is_empty() {
            tools.push(PlannedTool {
                name: entry.name.to_string(),
                evidence,
                format_extensions,
                format_order: policy.and_then(|policy| policy.format_order),
            });
        }
    }
    tools.sort_by(|left, right| {
        left.format_order
            .unwrap_or(i32::MAX)
            .cmp(&right.format_order.unwrap_or(i32::MAX))
            .then_with(|| left.name.cmp(&right.name))
    });
    unavailable.sort();
    unavailable.dedup();
    Ok(ToolPlan { tools, unavailable })
}

fn configured_format_extensions(
    entry: &super::ToolCatalogEntry,
    policy: Option<&crate::tools::ToolPolicy>,
) -> BTreeSet<String> {
    policy
        .filter(|policy| !policy.format_extensions.is_empty())
        .map_or_else(
            || {
                entry
                    .format_extensions
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect()
            },
            |policy| policy.format_extensions.clone(),
        )
}

fn is_universal_excluded_dir(name: &str) -> bool {
    matches!(name, ".git" | ".hg" | ".svn")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_configuration_is_native_evidence() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname = \"test\"\n[tool.ruff]\nline-length = 100\n",
        )
        .unwrap();
        let discovery = RepositoryDiscovery::discover(root.path()).unwrap();
        let evidence = discovery.evidence_for("ruff").unwrap();

        assert_eq!(evidence.kind, SelectionEvidenceKind::NativeConfig);
        assert_eq!(evidence.path, PathBuf::from("pyproject.toml"));
    }

    #[test]
    fn nested_embedded_configuration_preserves_source_location() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("packages/web")).unwrap();
        std::fs::write(
            root.path().join("packages/web/package.json"),
            r#"{"prettier":{"semi":false}}"#,
        )
        .unwrap();
        let discovery = RepositoryDiscovery::discover(root.path()).unwrap();
        let evidence = discovery.evidence_for("prettier").unwrap();

        assert_eq!(evidence.kind, SelectionEvidenceKind::NativeConfig);
        assert_eq!(evidence.path, PathBuf::from("packages/web/package.json"));
    }

    #[test]
    #[cfg(unix)]
    fn discovery_skips_symlinked_files_outside_repository() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("rustfmt.toml"), "").unwrap();
        symlink(
            outside.path().join("rustfmt.toml"),
            root.path().join("rustfmt.toml"),
        )
        .unwrap();

        let discovery = RepositoryDiscovery::discover(root.path()).unwrap();
        assert!(discovery.matching_signal("rustfmt.toml").is_none());
    }

    #[test]
    #[cfg(unix)]
    fn discovery_tolerates_unreadable_directories() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let unreadable = root.path().join("private");
        std::fs::create_dir(&unreadable).unwrap();
        std::fs::write(unreadable.join("rustfmt.toml"), "").unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();

        let discovery = RepositoryDiscovery::discover(root.path()).unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(discovery.root, root.path().canonicalize().unwrap());
    }

    #[test]
    fn ignored_native_configuration_is_not_discovered() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".gitignore"), "generated/\n").unwrap();
        std::fs::create_dir(root.path().join("generated")).unwrap();
        std::fs::write(root.path().join("generated/rustfmt.toml"), "").unwrap();

        let discovery = RepositoryDiscovery::discover(root.path()).unwrap();
        assert!(discovery.matching_signal("rustfmt.toml").is_none());
    }

    #[test]
    fn automatic_planning_reports_relevant_missing_tools_without_fallback() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".yamlfmt"), "{}").unwrap();
        std::fs::write(root.path().join("file.yaml"), "test: true\n").unwrap();
        let mut config = crate::tools::ToolsConfig::default();
        config.tools.insert(
            "yamlfmt".to_string(),
            crate::tools::ToolPolicy {
                executable: Some(
                    root.path()
                        .join("missing-yamlfmt")
                        .to_string_lossy()
                        .to_string(),
                ),
                ..Default::default()
            },
        );
        let registry = ToolRegistry::new(config, Some(root.path())).unwrap();
        let plan = plan_tools(&registry, &[ToolCapability::Format]).unwrap();

        assert!(plan.unavailable.contains(&"yamlfmt".to_string()));
        assert!(!plan.names().contains(&"yamlfmt".to_string()));
    }

    #[test]
    fn required_relevant_missing_tool_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".yamlfmt"), "{}").unwrap();
        let missing = root.path().join("missing-yamlfmt");
        let config = crate::tools::ToolsConfig::default()
            .with_required("yamlfmt")
            .with_path("yamlfmt", missing.to_string_lossy());

        let error = ToolRegistry::new(config, Some(root.path())).unwrap_err();
        assert!(matches!(
            error,
            crate::tools::registry::ToolError::RequiredToolNotFound(name) if name == "yamlfmt"
        ));
    }

    #[test]
    fn automatic_formatter_ownership_uses_catalog_priority_for_equal_evidence() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file.tf"), "variable \"test\" {}\n").unwrap();
        let binary = root.path().join("formatter");
        std::fs::write(&binary, "").unwrap();
        let mut config = crate::tools::ToolsConfig::default();
        for name in ["terraform", "tofu"] {
            config.tools.insert(
                name.to_string(),
                crate::tools::ToolPolicy {
                    mode: crate::tools::ToolSelectionMode::Enabled,
                    executable: Some(binary.to_string_lossy().to_string()),
                    format_extensions: BTreeSet::from(["tf".to_string()]),
                    ..Default::default()
                },
            );
        }

        // The production catalog priorities intentionally disambiguate this pair.
        let registry = ToolRegistry::new(config, Some(root.path())).unwrap();
        let plan = plan_tools(&registry, &[ToolCapability::Format]).unwrap();
        assert_eq!(plan.names(), vec!["terraform"]);
    }

    #[test]
    fn automatic_formatter_ownership_is_deterministic_by_evidence_then_priority() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("package.json"), "{}").unwrap();
        std::fs::write(root.path().join("biome.json"), "{}").unwrap();
        std::fs::write(root.path().join(".prettierrc"), "{}").unwrap();
        std::fs::write(root.path().join("file.js"), "const value=1;\n").unwrap();
        let binary = root.path().join("formatter");
        std::fs::write(&binary, "").unwrap();
        let mut config = crate::tools::ToolsConfig::default();
        for name in ["biome", "prettier"] {
            config
                .executables
                .insert(name.to_string(), binary.to_string_lossy().to_string());
        }
        let registry = ToolRegistry::new(config, Some(root.path())).unwrap();
        let plan = plan_tools(&registry, &[ToolCapability::Format]).unwrap();

        assert_eq!(plan.names(), vec!["biome", "prettier"]);
        let biome = plan.tools.iter().find(|tool| tool.name == "biome").unwrap();
        let prettier = plan
            .tools
            .iter()
            .find(|tool| tool.name == "prettier")
            .unwrap();
        assert!(biome.format_extensions.contains("js"));
        assert!(!prettier.format_extensions.contains("js"));
        assert!(prettier.format_extensions.contains("md"));
    }

    #[test]
    fn explicit_formatter_orders_preserve_overlapping_pipeline_ownership() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file.md"), "# test\n").unwrap();
        let binary = root.path().join("formatter");
        std::fs::write(&binary, "").unwrap();
        let mut config = crate::tools::ToolsConfig::default();
        for (name, order) in [("prettier", 20), ("dprint", 10)] {
            config.tools.insert(
                name.to_string(),
                crate::tools::ToolPolicy {
                    mode: crate::tools::ToolSelectionMode::Enabled,
                    executable: Some(binary.to_string_lossy().to_string()),
                    format_extensions: BTreeSet::from(["md".to_string()]),
                    format_order: Some(order),
                    ..Default::default()
                },
            );
        }
        let registry = ToolRegistry::new(config, Some(root.path())).unwrap();
        let plan = plan_tools(&registry, &[ToolCapability::Format]).unwrap();

        assert_eq!(plan.names(), vec!["dprint", "prettier"]);
        assert!(plan.has_ordered_formatters());
        assert!(
            plan.tools
                .iter()
                .all(|tool| tool.format_extensions.contains("md"))
        );
    }

    #[test]
    fn explicit_profile_reinclusion_is_shared_by_discovery() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::create_dir(root.path().join("target")).unwrap();
        std::fs::write(root.path().join("target/rustfmt.toml"), "").unwrap();
        let binary = root.path().join("formatter");
        std::fs::write(&binary, "").unwrap();
        let mut config = crate::tools::ToolsConfig::default();
        config.tools.insert(
            "rustfmt".to_string(),
            crate::tools::ToolPolicy {
                include: vec!["target/**".to_string()],
                executable: Some(binary.to_string_lossy().to_string()),
                ..Default::default()
            },
        );
        let registry = ToolRegistry::new(config, Some(root.path())).unwrap();
        let plan = plan_tools(&registry, &[ToolCapability::Format]).unwrap();

        let rustfmt = plan
            .tools
            .iter()
            .find(|tool| tool.name == "rustfmt")
            .unwrap();
        assert_eq!(rustfmt.evidence.kind, SelectionEvidenceKind::NativeConfig);
        assert_eq!(rustfmt.evidence.path, PathBuf::from("target/rustfmt.toml"));
    }

    #[test]
    fn planning_and_runner_share_automatic_exclusion_boundaries() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(root.path().join("included.rs"), "fn included() {}\n").unwrap();
        std::fs::create_dir(root.path().join("target")).unwrap();
        std::fs::write(root.path().join("target/excluded.rs"), "fn excluded() {}\n").unwrap();
        let registry =
            ToolRegistry::new(crate::tools::ToolsConfig::default(), Some(root.path())).unwrap();
        let plan = plan_tools(&registry, &[ToolCapability::Format]).unwrap();
        let runner = crate::tools::ToolRunner::new(&registry).with_tool_plan(&plan);
        let rustfmt = registry.get("rustfmt").unwrap();
        let files = runner.scoped_files_for(rustfmt).unwrap();

        assert!(files.contains(&"included.rs".to_string()));
        assert!(!files.contains(&"target/excluded.rs".to_string()));
    }

    #[test]
    fn discovery_prunes_dependency_trees() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::create_dir(root.path().join("target")).unwrap();
        std::fs::write(root.path().join("target/rustfmt.toml"), "").unwrap();
        let exclusions =
            automatic_exclusion_patterns(root.path(), &crate::tools::ScopeConfig::default());
        let discovery =
            RepositoryDiscovery::discover_with_excludes(root.path(), &exclusions).unwrap();
        assert!(discovery.matching_signal("Cargo.toml").is_some());
        assert!(discovery.matching_signal("rustfmt.toml").is_none());
    }
}
