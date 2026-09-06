//! Repository-driven setup choices, independent of terminal rendering.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{RepositoryDiscovery, SelectionEvidenceKind, TOOL_CATALOG, ToolsConfig};

#[derive(Debug)]
pub(super) struct Choice {
    pub name: String,
    pub reason: String,
    pub selected: bool,
    pub installed: Option<std::path::PathBuf>,
}

#[derive(Debug)]
pub(super) struct Group {
    pub title: String,
    pub choices: Vec<Choice>,
    pub extensions: Vec<String>,
    pub files: Vec<std::path::PathBuf>,
    pub formatting: bool,
}

#[allow(clippy::too_many_lines)]
pub(super) fn groups(inventory: &mut RepositoryDiscovery, config: &ToolsConfig) -> Vec<Group> {
    let evidence = inventory.tool_evidence();
    let extensions = inventory
        .files()
        .iter()
        .filter_map(|path| path.extension()?.to_str())
        .chain(
            TOOL_CATALOG
                .iter()
                .filter(|entry| {
                    config
                        .required
                        .iter()
                        .chain(&config.skip)
                        .any(|name| name == entry.name)
                        || config.tools.contains_key(entry.name)
                        || config.executables.contains_key(entry.name)
                })
                .flat_map(|entry| entry.format_extensions.iter().copied()),
        )
        .collect::<BTreeSet<_>>();
    let configured = |name: &str| {
        config
            .required
            .iter()
            .chain(&config.skip)
            .any(|item| item == name)
            || config.tools.contains_key(name)
            || config.executables.contains_key(name)
    };
    let active = |name: &str, capability: super::super::ToolCapability, extensions: &[&str]| {
        !config.skip.iter().any(|skip| skip == name)
            && config.tools.get(name).is_none_or(|policy| {
                policy.mode != super::super::ToolSelectionMode::Disabled
                    && (policy.capabilities.is_empty() || policy.capabilities.contains(&capability))
                    && (capability != super::super::ToolCapability::Format
                        || policy.format_extensions.is_empty()
                        || extensions.iter().any(|ext| {
                            policy
                                .format_extensions
                                .iter()
                                .any(|configured| configured == ext)
                        }))
            })
    };
    // Group extensions with identical alternatives; multi-language tools can win
    // more than one group, but are persisted only once.
    let mut families: BTreeMap<Vec<&str>, Vec<&str>> = BTreeMap::new();
    for extension in extensions {
        let alternatives = TOOL_CATALOG
            .iter()
            .filter(|entry| entry.format_extensions.contains(&extension))
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        if !alternatives.is_empty() {
            families.entry(alternatives).or_default().push(extension);
        }
    }
    let mut result = Vec::new();
    for (names, extensions) in families.into_iter().flat_map(|(names, extensions)| {
        // Existing per-extension choices must remain independently editable.
        if names.iter().any(|name| configured(name)) {
            extensions
                .into_iter()
                .map(|extension| (names.clone(), vec![extension]))
                .collect::<Vec<_>>()
        } else {
            vec![(names, extensions)]
        }
    }) {
        let winner = names
            .iter()
            .filter(|name| !config.skip.iter().any(|skip| skip == **name))
            .filter(|name| {
                config
                    .tools
                    .get(**name)
                    .is_none_or(|policy| policy.mode != super::super::ToolSelectionMode::Disabled)
            })
            .min_by_key(|name| {
                let strength = if configured(name) {
                    SelectionEvidenceKind::ClippierConfig
                } else {
                    evidence
                        .get(**name)
                        .map_or(SelectionEvidenceKind::Content, |fact| fact.kind)
                };
                let entry = super::super::tool_catalog_entry(name).expect("catalog candidate");
                (
                    std::cmp::Reverse(strength),
                    entry.formatter_priority,
                    **name,
                )
            })
            .copied();
        let choices = names
            .into_iter()
            .map(|name| Choice {
                name: name.to_owned(),
                reason: if configured(name) {
                    "Current clippier.toml configuration".to_owned()
                } else {
                    evidence.get(name).map_or_else(
                        || "source files; catalog alternative".to_owned(),
                        |fact| format!("{:?}: {}", fact.kind, fact.path.display()),
                    )
                },
                selected: if configured(name) {
                    active(name, super::super::ToolCapability::Format, &extensions)
                } else {
                    Some(name) == winner
                },
                installed: None,
            })
            .collect::<Vec<_>>();
        if !choices.is_empty() {
            let files = inventory
                .files()
                .iter()
                .filter(|path| {
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| extensions.contains(&ext))
                })
                .cloned()
                .collect();
            let names = extensions
                .iter()
                .map(|ext| language(ext))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            result.push(Group {
                title: names.join(" / "),
                choices,
                extensions: extensions.into_iter().map(str::to_owned).collect(),
                files,
                formatting: true,
            });
        }
    }
    let choices = TOOL_CATALOG
        .iter()
        .filter(|entry| {
            (configured(entry.name) || evidence.contains_key(entry.name))
                && entry
                    .capabilities
                    .contains(&super::super::ToolCapability::Lint)
        })
        .map(|entry| Choice {
            name: entry.name.to_owned(),
            reason: if configured(entry.name) {
                "Current clippier.toml configuration".to_owned()
            } else {
                evidence.get(entry.name).map_or_else(String::new, |fact| {
                    format!("{:?}: {}", fact.kind, fact.path.display())
                })
            },
            selected: active(entry.name, super::super::ToolCapability::Lint, &[]),
            installed: None,
        })
        .collect::<Vec<_>>();
    for choice in choices {
        let entry = super::super::tool_catalog_entry(&choice.name).expect("catalog tool");
        let extensions = entry
            .lint_extensions
            .iter()
            .map(|ext| (*ext).to_owned())
            .collect::<Vec<_>>();
        let files = inventory
            .files()
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| extensions.iter().any(|value| value == ext))
            })
            .cloned()
            .collect();
        result.push(Group {
            title: entry
                .lint_extensions
                .iter()
                .map(|ext| language(ext))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(" / "),
            choices: vec![choice],
            extensions,
            files,
            formatting: false,
        });
    }
    result.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then(right.formatting.cmp(&left.formatting))
    });
    result
}

pub(super) fn apply_availability(
    groups: &mut [Group],
    installed: &BTreeMap<String, std::path::PathBuf>,
) {
    for group in groups {
        for choice in &mut group.choices {
            choice.installed = installed.get(&choice.name).cloned();
        }
        // Only source-only defaults may fall back; native/Clippier preferences win.
        if group.formatting
            && group.choices.iter().any(|choice| {
                choice.selected
                    && choice.installed.is_none()
                    && choice.reason.starts_with("source files;")
            })
            && let Some(winner) = group
                .choices
                .iter()
                .filter(|choice| choice.installed.is_some())
                .min_by_key(|choice| {
                    let entry =
                        super::super::tool_catalog_entry(&choice.name).expect("catalog candidate");
                    (entry.formatter_priority, &choice.name)
                })
                .map(|choice| choice.name.clone())
        {
            for choice in &mut group.choices {
                choice.selected = choice.name == winner;
                if choice.selected {
                    choice.reason.push_str("; installed fallback");
                }
            }
        }
    }
}

fn language(extension: &str) -> &str {
    match extension {
        "nix" => "Nix",
        "py" | "pyi" | "ipynb" => "Python",
        "js" | "jsx" => "JavaScript",
        "ts" | "tsx" => "TypeScript",
        "json" | "jsonc" => "JSON",
        "md" | "mdx" | "markdown" => "Markdown",
        "yml" | "yaml" => "YAML",
        "rs" => "Rust",
        "go" => "Go",
        "sh" | "bash" => "Shell",
        "lua" => "Lua",
        "toml" => "TOML",
        "css" | "scss" | "less" => "Stylesheets",
        "m" => "Objective-C",
        "c" | "cpp" | "h" | "hpp" | "cc" | "cxx" => "C / C++",
        other => other,
    }
}

pub(super) fn policies(groups: &[Group]) -> BTreeMap<String, super::super::ToolPolicy> {
    let mut policies = BTreeMap::<String, super::super::ToolPolicy>::new();
    for group in groups {
        for choice in group.choices.iter().filter(|choice| choice.selected) {
            let policy = policies.entry(choice.name.clone()).or_default();
            policy.mode = super::super::ToolSelectionMode::Enabled;
            if group.formatting {
                policy
                    .capabilities
                    .insert(super::super::ToolCapability::Format);
                policy
                    .format_extensions
                    .extend(group.extensions.iter().cloned());
            } else {
                policy
                    .capabilities
                    .insert(super::super::ToolCapability::Lint);
            }
        }
    }
    policies
}

pub(super) fn selections(groups: &[Group]) -> (Vec<String>, Vec<String>) {
    let mut selected = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for choice in groups.iter().flat_map(|group| &group.choices) {
        seen.insert(choice.name.clone());
        if choice.selected {
            selected.insert(choice.name.clone());
        }
    }
    let skipped = seen.difference(&selected).cloned().collect();
    (selected.into_iter().collect(), skipped)
}
