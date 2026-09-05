//! Shared repository discovery and capability-aware automatic tool planning.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::WalkBuilder;

use super::scope::automatic_exclusion_patterns_from_files;
use super::{TOOL_CATALOG, ToolCapability, ToolRegistry};

/// Shared command-local parsed native configuration cache.
#[derive(Debug, Default)]
pub struct NativeConfigCache {
    pub(crate) toml: BTreeMap<PathBuf, Option<toml::Value>>,
    pub(crate) json: BTreeMap<PathBuf, Option<serde_json::Value>>,
}

pub type NativeConfigCacheHandle = Arc<Mutex<NativeConfigCache>>;

/// Deterministic command-scoped inventory diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryDiagnostics {
    /// Number of recursive repository walkers constructed.
    pub recursive_walks: usize,
    /// Directories yielded by the authoritative walker.
    pub directories_visited: usize,
    /// Files indexed by the authoritative walker.
    pub files_indexed: usize,
    /// Native configuration files parsed.
    pub native_configs_parsed: usize,
    /// Installed executable resolutions attempted.
    pub executables_resolved: usize,
    /// Native capability probes executed.
    pub probes_executed: usize,
    /// Planned file assignments across tools and capabilities.
    pub files_assigned: usize,
}

/// Shared mutable diagnostics handle retained by one command plan.
pub type InventoryDiagnosticsHandle = Arc<Mutex<InventoryDiagnostics>>;

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
    parsed_toml: BTreeMap<PathBuf, Option<toml::Value>>,
    parsed_json: BTreeMap<PathBuf, Option<serde_json::Value>>,
    native_config_cache: NativeConfigCacheHandle,
    automatic_exclusions: Vec<String>,
    diagnostics: InventoryDiagnosticsHandle,
}

impl RepositoryDiscovery {
    /// Discovers files below a repository root while respecting VCS ignore files.
    ///
    /// # Errors
    ///
    /// * If the working directory cannot be canonicalized
    pub fn discover(root: &Path) -> Result<Self, std::io::Error> {
        Self::discover_with_scope_excludes(root, root, &[], None)
    }

    /// Discovers repository files with one authoritative ignore-aware walk.
    /// Configured and universal exclusions prune during traversal; ecosystem
    /// exclusions are derived from manifests indexed by that same walk.
    ///
    /// # Errors
    ///
    /// * If the working directory cannot be canonicalized
    pub fn inventory(
        root: &Path,
        config: &crate::tools::ScopeConfig,
    ) -> Result<Self, std::io::Error> {
        Self::inventory_with_scope_base(root, root, config)
    }

    /// Discovers repository files while resolving configured scope patterns
    /// relative to the directory which owns the runner configuration.
    ///
    /// # Errors
    ///
    /// * If the working directory cannot be canonicalized
    /// * If a configured scope pattern is invalid
    pub fn inventory_with_scope_base(
        root: &Path,
        scope_base: &Path,
        config: &crate::tools::ScopeConfig,
    ) -> Result<Self, std::io::Error> {
        let mut result =
            Self::discover_with_scope_excludes(root, scope_base, &config.exclude, Some(config))?;
        result.automatic_exclusions =
            automatic_exclusion_patterns_from_files(&result.files, config);
        Ok(result)
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
        Self::discover_with_scope_excludes(root, root, excludes, None)
    }

    fn discover_with_scope_excludes(
        root: &Path,
        scope_base: &Path,
        excludes: &[String],
        scope_config: Option<&crate::tools::ScopeConfig>,
    ) -> Result<Self, std::io::Error> {
        let root = root.canonicalize()?;
        let scope_base = scope_base.canonicalize()?;
        let matchers = excludes
            .iter()
            .map(|pattern| {
                let absolute = scope_base.join(pattern.trim_start_matches('/'));
                globset::Glob::new(&absolute.to_string_lossy().replace('\\', "/"))
                    .map(|glob| glob.compile_matcher())
                    .map_err(std::io::Error::other)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let filter_root = root.clone();
        let scope_config = scope_config.cloned();
        let package_profiles = Arc::new(Mutex::new(BTreeMap::new()));
        let mut result = Self {
            root: root.clone(),
            ..Self::default()
        };
        if let Ok(mut diagnostics) = result.diagnostics.lock() {
            diagnostics.recursive_walks += 1;
        }
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
                let name = entry.file_name().to_str().unwrap_or_default();
                if is_universal_excluded_dir(name)
                    || matchers
                        .iter()
                        .any(|matcher| matcher.is_match(entry.path()))
                {
                    return false;
                }
                if !entry.file_type().is_some_and(|kind| kind.is_dir())
                    || relative.as_os_str().is_empty()
                {
                    return true;
                }
                scope_config.as_ref().is_none_or(|config| {
                    !is_inventory_generated_directory(entry.path(), name, config, &package_profiles)
                })
            });

        for entry in builder.build().filter_map(Result::ok) {
            if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                if let Ok(mut diagnostics) = result.diagnostics.lock() {
                    diagnostics.directories_visited += 1;
                }
                continue;
            }
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            if let Ok(mut diagnostics) = result.diagnostics.lock() {
                diagnostics.files_indexed += 1;
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

    /// Replaces this inventory's diagnostics and native-config contexts with the
    /// command-shared registry contexts.
    #[must_use]
    pub(crate) fn with_command_context(
        mut self,
        diagnostics: InventoryDiagnosticsHandle,
        native_config_cache: NativeConfigCacheHandle,
    ) -> Self {
        if let (Ok(existing), Ok(mut shared)) = (self.diagnostics.lock(), diagnostics.lock()) {
            shared.recursive_walks += existing.recursive_walks;
            shared.directories_visited += existing.directories_visited;
            shared.files_indexed += existing.files_indexed;
            shared.native_configs_parsed += existing.native_configs_parsed;
        }
        self.diagnostics = diagnostics;
        self.native_config_cache = native_config_cache;
        self
    }

    /// Returns deterministic diagnostics for this inventory.
    #[must_use]
    pub fn diagnostics(&self) -> InventoryDiagnostics {
        self.diagnostics
            .lock()
            .map_or_else(|_| InventoryDiagnostics::default(), |value| value.clone())
    }

    /// Returns the shared command diagnostics handle.
    #[must_use]
    pub fn diagnostics_handle(&self) -> InventoryDiagnosticsHandle {
        Arc::clone(&self.diagnostics)
    }

    /// Returns every indexed repository-relative file.
    #[must_use]
    pub const fn files(&self) -> &BTreeSet<PathBuf> {
        &self.files
    }

    /// Returns effective automatic exclusions activated by this inventory.
    #[must_use]
    pub fn automatic_exclusions(&self) -> &[String] {
        &self.automatic_exclusions
    }

    /// Builds an automatic plan while reusing an existing command inventory.
    ///
    /// # Errors
    ///
    /// * If policy or ownership resolution fails
    pub fn plan_from_inventory(
        registry: &ToolRegistry,
        capabilities: &[ToolCapability],
        discovery: &mut Self,
    ) -> Result<ToolPlan, std::io::Error> {
        plan_inventory(registry, capabilities, discovery)
    }

    /// Builds a plan for explicitly selected tools from one command inventory.
    ///
    /// # Errors
    ///
    /// * If configured scope patterns cannot be compiled
    pub fn explicit_plan(
        registry: &ToolRegistry,
        capabilities: &[ToolCapability],
        names: &[String],
        discovery: &mut Self,
    ) -> Result<ToolPlan, std::io::Error> {
        let mut tools = Vec::new();
        for name in names {
            let Some(entry) = TOOL_CATALOG.iter().find(|entry| entry.name == name) else {
                continue;
            };
            let Some(_tool) = registry.get(name) else {
                continue;
            };
            let policy = registry.config().tool_policy(name);
            let mut extensions = BTreeSet::new();
            for capability in capabilities {
                if entry.capabilities.contains(capability)
                    && policy.is_none_or(|policy| policy.enables(*capability))
                {
                    extensions.extend(entry.extensions(*capability));
                }
            }
            let global_scope = crate::tools::scope::ScopeMatcher::new(
                registry.working_dir(),
                discovery.automatic_exclusions(),
            )
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            let mut files = global_scope.filter_relative_files(discovery.files(), &extensions);
            if let Some(policy) = policy
                && (!policy.include.is_empty() || !policy.exclude.is_empty())
            {
                let tool_scope = crate::tools::scope::ScopeMatcher::with_patterns(
                    registry.working_dir(),
                    &policy.include,
                    &policy.exclude,
                )
                .map_err(|error| std::io::Error::other(error.to_string()))?;
                files = tool_scope.filter_relative_files(&files, &extensions);
            }
            tools.push(PlannedTool {
                name: name.clone(),
                evidence: SelectionEvidence {
                    kind: SelectionEvidenceKind::ClippierConfig,
                    path: PathBuf::from("command-line"),
                },
                files,
                format_extensions: entry.extensions(ToolCapability::Format),
                format_order: policy.and_then(|policy| policy.format_order),
            });
        }
        let mut diagnostics = discovery.diagnostics();
        diagnostics.files_assigned = tools.iter().map(|tool| tool.files.len()).sum();
        Ok(ToolPlan {
            tools: tools.clone(),
            unavailable: Vec::new(),
            effective_extensions: tools
                .iter()
                .map(|tool| {
                    (tool.name.clone(), {
                        let mut value = tool.format_extensions.clone();
                        value.extend(tool.files.iter().filter_map(|path| {
                            path.extension()
                                .and_then(|ext| ext.to_str())
                                .map(str::to_ascii_lowercase)
                        }));
                        value
                    })
                })
                .collect(),
            automatic_exclusions: discovery.automatic_exclusions().to_vec(),
            diagnostics,
        })
    }

    fn parse_toml_cached(&mut self, path: &Path) -> Option<&toml::Value> {
        if !self.parsed_toml.contains_key(path) {
            let absolute = self.root.join(path);
            let cached = self
                .native_config_cache
                .lock()
                .ok()
                .and_then(|cache| cache.toml.get(&absolute).cloned());
            let parsed = cached.unwrap_or_else(|| {
                let parsed = std::fs::read_to_string(&absolute)
                    .ok()
                    .and_then(|contents| toml::from_str::<toml::Value>(&contents).ok());
                if let Ok(mut diagnostics) = self.diagnostics.lock() {
                    diagnostics.native_configs_parsed += 1;
                }
                if let Ok(mut cache) = self.native_config_cache.lock() {
                    cache.toml.insert(absolute, parsed.clone());
                }
                parsed
            });
            self.parsed_toml.insert(path.to_path_buf(), parsed);
        }
        self.parsed_toml.get(path).and_then(Option::as_ref)
    }

    fn parse_json_cached(&mut self, path: &Path) -> Option<&serde_json::Value> {
        if !self.parsed_json.contains_key(path) {
            let absolute = self.root.join(path);
            let cached = self
                .native_config_cache
                .lock()
                .ok()
                .and_then(|cache| cache.json.get(&absolute).cloned());
            let parsed = cached.unwrap_or_else(|| {
                let parsed = std::fs::read_to_string(&absolute)
                    .ok()
                    .and_then(|contents| serde_json::from_str::<serde_json::Value>(&contents).ok());
                if let Ok(mut diagnostics) = self.diagnostics.lock() {
                    diagnostics.native_configs_parsed += 1;
                }
                if let Ok(mut cache) = self.native_config_cache.lock() {
                    cache.json.insert(absolute, parsed.clone());
                }
                parsed
            });
            self.parsed_json.insert(path.to_path_buf(), parsed);
        }
        self.parsed_json.get(path).and_then(Option::as_ref)
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
        &mut self,
        signal: super::EmbeddedConfigSignal,
    ) -> Option<PathBuf> {
        let paths = self.basenames.get(signal.manifest)?.clone();
        for path in paths {
            let configured = match signal.manifest {
                "pyproject.toml" => self.parse_toml_cached(&path).is_some_and(|value| {
                    let container = signal
                        .container
                        .map_or(Some(value), |container| value.get(container));
                    container.is_some_and(|value| value.get(signal.key).is_some())
                }),
                "package.json" => self.parse_json_cached(&path).is_some_and(|value| {
                    let container = signal
                        .container
                        .map_or(Some(value), |container| value.get(container));
                    container.is_some_and(|value| value.get(signal.key).is_some())
                }),
                _ => false,
            };
            if configured {
                return Some(path);
            }
        }
        None
    }

    fn embedded_config_for(&mut self, entry: &super::ToolCatalogEntry) -> Option<PathBuf> {
        super::embedded_config_signals(entry.name)
            .iter()
            .find_map(|signal| self.manifest_contains_tool_config(*signal))
    }

    /// Returns repository evidence for every relevant catalog tool, including tools
    /// which are not installed. Reuses the inventory and native configuration cache.
    #[must_use]
    pub fn tool_evidence(&mut self) -> BTreeMap<String, SelectionEvidence> {
        TOOL_CATALOG
            .iter()
            .filter_map(|entry| {
                self.evidence_for(entry.name)
                    .map(|evidence| (entry.name.to_owned(), evidence))
            })
            .collect()
    }

    fn evidence_for(&mut self, tool_name: &str) -> Option<SelectionEvidence> {
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
    /// Actual repository-relative files assigned to this tool.
    pub files: BTreeSet<PathBuf>,
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
    /// Effective per-tool extensions resolved during planning/probing.
    pub effective_extensions: BTreeMap<String, BTreeSet<String>>,
    /// Effective automatic exclusions activated by inventory evidence.
    pub automatic_exclusions: Vec<String>,
    /// Deterministic diagnostics for command-scoped inventory and planning.
    pub diagnostics: InventoryDiagnostics,
}

impl ToolPlan {
    /// Returns selected tool names.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|tool| tool.name.clone()).collect()
    }

    /// Automatic plan file assignments by selected tool.
    #[must_use]
    pub fn planned_files(&self) -> BTreeMap<String, BTreeSet<PathBuf>> {
        self.tools
            .iter()
            .map(|tool| (tool.name.clone(), tool.files.clone()))
            .collect()
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
    let scope_base = registry
        .config()
        .scope_base
        .as_deref()
        .unwrap_or_else(|| registry.working_dir());
    let mut discovery = RepositoryDiscovery::inventory_with_scope_base(
        registry.working_dir(),
        scope_base,
        &scope_config,
    )?
    .with_command_context(
        registry.diagnostics_handle(),
        registry.native_config_cache(),
    );
    plan_inventory(registry, capabilities, &mut discovery)
}

#[allow(clippy::too_many_lines)]
fn plan_inventory(
    registry: &ToolRegistry,
    capabilities: &[ToolCapability],
    discovery: &mut RepositoryDiscovery,
) -> Result<ToolPlan, std::io::Error> {
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

    let global_scope = crate::tools::scope::ScopeMatcher::new(
        registry.working_dir(),
        discovery.automatic_exclusions(),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
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
            let mut file_extensions = format_extensions.clone();
            if lint_selected {
                file_extensions.extend(entry.extensions(ToolCapability::Lint));
            }
            let mut files = global_scope.filter_relative_files(discovery.files(), &file_extensions);
            if let Some(tool) = registry.get(entry.name)
                && !tool.native_includes.is_empty()
            {
                let native_scope = crate::tools::scope::ScopeMatcher::with_patterns(
                    registry.working_dir(),
                    &tool.native_includes,
                    &[],
                )
                .map_err(|error| std::io::Error::other(error.to_string()))?;
                files = native_scope.filter_relative_files(&files, &file_extensions);
            }
            if let Some(policy) = policy
                && (!policy.include.is_empty() || !policy.exclude.is_empty())
            {
                let tool_scope = crate::tools::scope::ScopeMatcher::with_patterns(
                    registry.working_dir(),
                    &policy.include,
                    &policy.exclude,
                )
                .map_err(|error| std::io::Error::other(error.to_string()))?;
                files = tool_scope.filter_relative_files(&files, &file_extensions);
            }
            tools.push(PlannedTool {
                name: entry.name.to_string(),
                evidence,
                files,
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
    let mut diagnostics = discovery.diagnostics();
    diagnostics.files_assigned = tools.iter().map(|tool| tool.files.len()).sum();
    Ok(ToolPlan {
        tools: tools.clone(),
        unavailable,
        effective_extensions: tools
            .iter()
            .map(|tool| {
                (tool.name.clone(), {
                    let mut value = tool.format_extensions.clone();
                    value.extend(tool.files.iter().filter_map(|path| {
                        path.extension()
                            .and_then(|ext| ext.to_str())
                            .map(str::to_ascii_lowercase)
                    }));
                    value
                })
            })
            .collect(),
        automatic_exclusions: discovery.automatic_exclusions().to_vec(),
        diagnostics,
    })
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

fn is_inventory_generated_directory(
    directory: &Path,
    name: &str,
    config: &crate::tools::ScopeConfig,
    cache: &Mutex<BTreeMap<PathBuf, BTreeSet<&'static str>>>,
) -> bool {
    if !config.automatic_excludes {
        return false;
    }
    let parent = directory.parent().unwrap_or(directory);
    let profiles = cache.lock().map_or_else(
        |_| BTreeSet::new(),
        |mut cache| {
            cache
                .entry(parent.to_path_buf())
                .or_insert_with(|| {
                    crate::tools::scope::AUTOMATIC_EXCLUSION_PROFILES
                        .iter()
                        .filter(|profile| {
                            !config.disable_profiles.contains(profile.id)
                                && profile
                                    .manifests
                                    .iter()
                                    .any(|manifest| parent.join(manifest).is_file())
                        })
                        .map(|profile| profile.id)
                        .collect()
                })
                .clone()
        },
    );
    profiles.iter().any(|profile_id| {
        crate::tools::scope::AUTOMATIC_EXCLUSION_PROFILES
            .iter()
            .find(|profile| profile.id == *profile_id)
            .is_some_and(|profile| {
                profile.exclusions.iter().any(|pattern| {
                    pattern
                        .split('/')
                        .next()
                        .is_some_and(|component| component == name)
                })
            })
    })
}

fn is_universal_excluded_dir(name: &str) -> bool {
    matches!(name, ".git" | ".hg" | ".svn")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_anchors_configured_exclusions_to_scope_base() {
        let parent = tempfile::tempdir().unwrap();
        let scope_base = parent.path().join("workspace");
        let root = scope_base.join("packages/plugin");
        std::fs::create_dir_all(root.join("generated")).unwrap();
        std::fs::write(root.join("generated/reference.md"), "generated\n").unwrap();
        std::fs::write(root.join("README.md"), "# Included\n").unwrap();
        let config = crate::tools::ScopeConfig {
            exclude: vec!["/packages/plugin/generated/**".to_string()],
            ..Default::default()
        };

        let inventory =
            RepositoryDiscovery::inventory_with_scope_base(&root, &scope_base, &config).unwrap();

        assert!(inventory.files().contains(Path::new("README.md")));
        assert!(
            !inventory
                .files()
                .contains(Path::new("generated/reference.md"))
        );
    }

    #[test]
    fn inventory_prunes_root_anchored_configured_exclusions() {
        let root = tempfile::tempdir().unwrap();
        let excluded = root
            .path()
            .join("packages/plugin/permissions/autogenerated");
        std::fs::create_dir_all(&excluded).unwrap();
        std::fs::write(excluded.join("reference.md"), "generated\n").unwrap();
        std::fs::write(root.path().join("README.md"), "# Included\n").unwrap();
        let config = crate::tools::ScopeConfig {
            exclude: vec!["/packages/plugin/permissions/autogenerated/**".to_string()],
            ..Default::default()
        };

        let inventory = RepositoryDiscovery::inventory(root.path(), &config).unwrap();

        assert!(inventory.files().contains(Path::new("README.md")));
        assert!(!inventory.files().contains(Path::new(
            "packages/plugin/permissions/autogenerated/reference.md"
        )));
    }

    #[test]
    fn large_synthetic_inventory_has_bounded_work_independent_of_tool_count() {
        let root = tempfile::tempdir().unwrap();
        let package_count = 24;
        for index in 0..package_count {
            let package = root.path().join(format!("packages/package-{index}"));
            std::fs::create_dir_all(package.join("src")).unwrap();
            std::fs::create_dir_all(package.join("node_modules/dependency")).unwrap();
            std::fs::write(package.join("package.json"), "{}\n").unwrap();
            std::fs::write(package.join("src/index.js"), "const value = 1;\n").unwrap();
            std::fs::write(
                package.join("node_modules/dependency/index.js"),
                "generated\n",
            )
            .unwrap();
        }

        let inventory =
            RepositoryDiscovery::inventory(root.path(), &crate::tools::ScopeConfig::default())
                .unwrap();
        let diagnostics = inventory.diagnostics();

        assert_eq!(diagnostics.recursive_walks, 1);
        assert_eq!(diagnostics.files_indexed, package_count * 2);
        assert_eq!(inventory.files().len(), package_count * 2);
        assert!(
            inventory
                .files()
                .iter()
                .all(|path| !path.to_string_lossy().contains("node_modules"))
        );
    }

    #[test]
    fn all_files_plan_uses_one_walk_and_assigns_files_without_tool_walks() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(root.path().join("source.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.path().join("config.toml"), "key = 'value'\n").unwrap();
        let registry =
            ToolRegistry::new(crate::tools::ToolsConfig::default(), Some(root.path())).unwrap();
        let plan = plan_tools(&registry, &[ToolCapability::Format]).unwrap();

        assert_eq!(plan.diagnostics.recursive_walks, 1);
        assert!(plan.diagnostics.files_indexed >= 3);
        assert!(plan.diagnostics.files_assigned >= 2);
        assert!(
            plan.tools
                .iter()
                .find(|tool| tool.name == "rustfmt")
                .unwrap()
                .files
                .contains(Path::new("source.rs"))
        );
        assert!(
            plan.tools
                .iter()
                .find(|tool| tool.name == "taplo")
                .unwrap()
                .files
                .contains(Path::new("config.toml"))
        );
    }

    #[test]
    fn embedded_configuration_is_parsed_once_per_inventory() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("pyproject.toml"),
            "[tool.ruff]\nline-length=100\n[tool.black]\nline-length=100\n",
        )
        .unwrap();
        let mut discovery = RepositoryDiscovery::discover(root.path()).unwrap();
        assert!(discovery.evidence_for("ruff").is_some());
        assert!(discovery.evidence_for("black").is_some());
        assert_eq!(discovery.diagnostics().native_configs_parsed, 1);
    }

    #[test]
    fn embedded_manifest_configuration_is_native_evidence() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname = \"test\"\n[tool.ruff]\nline-length = 100\n",
        )
        .unwrap();
        let mut discovery = RepositoryDiscovery::discover(root.path()).unwrap();
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
        let mut discovery = RepositoryDiscovery::discover(root.path()).unwrap();
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
        let exclusions = crate::tools::EffectiveScope::resolve(
            root.path(),
            crate::tools::ScopeConfig::default(),
        )
        .exclusions;
        let discovery =
            RepositoryDiscovery::discover_with_excludes(root.path(), &exclusions).unwrap();
        assert!(discovery.matching_signal("Cargo.toml").is_some());
        assert!(discovery.matching_signal("rustfmt.toml").is_none());
    }
}
