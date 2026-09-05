//! Repository-driven setup choices, independent of terminal rendering.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{RepositoryDiscovery, SelectionEvidenceKind, TOOL_CATALOG, ToolsConfig};

#[derive(Debug)]
pub(super) struct Choice {
    pub name: String,
    pub reason: String,
    pub selected: bool,
}

#[derive(Debug)]
pub(super) struct Group {
    pub title: String,
    pub choices: Vec<Choice>,
}

pub(super) fn groups(inventory: &mut RepositoryDiscovery, config: &ToolsConfig) -> Vec<Group> {
    let evidence = inventory.tool_evidence();
    let extensions = inventory
        .files()
        .iter()
        .filter_map(|path| path.extension()?.to_str())
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
    for (names, extensions) in families {
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
            .filter(|name| !configured(name))
            .map(|name| Choice {
                name: name.to_owned(),
                reason: evidence.get(name).map_or_else(
                    || "source files; catalog alternative".to_owned(),
                    |fact| format!("{:?}: {}", fact.kind, fact.path.display()),
                ),
                selected: Some(name) == winner,
            })
            .collect::<Vec<_>>();
        if !choices.is_empty() {
            result.push(Group {
                title: format!("Formatters · {}", extensions.join(", ")),
                choices,
            });
        }
    }
    let formatters = result
        .iter()
        .flat_map(|group| &group.choices)
        .map(|choice| choice.name.clone())
        .collect::<BTreeSet<_>>();
    let choices = evidence
        .into_iter()
        .filter(|(name, _)| !configured(name) && !formatters.contains(name))
        .map(|(name, fact)| Choice {
            name,
            reason: format!("{:?}: {}", fact.kind, fact.path.display()),
            selected: true,
        })
        .collect::<Vec<_>>();
    if !choices.is_empty() {
        result.push(Group {
            title: "Other detected tools / linters".to_owned(),
            choices,
        });
    }
    result
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
