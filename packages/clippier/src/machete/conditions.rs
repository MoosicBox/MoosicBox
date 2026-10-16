//! Offline, three-valued conditional evaluation. No host cfg is inferred.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A named, finite analysis environment, not a global removal proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct AnalysisProfile {
    /// Explicit Cargo features; transitive local feature wiring is expanded offline.
    pub features: Vec<String>,
    /// Enable the manifest's default feature group.
    pub default_features: bool,
    /// Explicit Cargo target triple; absence leaves target predicates unknown.
    pub target: Option<String>,
    /// Explicit cfg facts, e.g. `unix` or `target_os="linux"`.
    pub cfg: Vec<String>,
}
impl Default for AnalysisProfile {
    fn default() -> Self {
        Self {
            features: Vec::new(),
            default_features: true,
            target: None,
            cfg: Vec::new(),
        }
    }
}

/// Conditional truth, retaining unsupported or unavailable facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConditionValue {
    /// Known active within the selected profile.
    True,
    /// Known inactive within the selected profile.
    False,
    /// Missing information or unsupported syntax.
    Unknown,
}
impl ConditionValue {
    pub(super) const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::True, Self::True) => Self::True,
            _ => Self::Unknown,
        }
    }
    const fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
}

/// Evaluate a Rust cfg expression using only explicitly supplied profile facts.
/// Unsupported expressions and unavailable target facts return `Unknown`.
#[must_use]
pub fn evaluate_condition(expression: &str, profile: &AnalysisProfile) -> ConditionValue {
    let expression = expression.trim();
    for operator in ["cfg", "all", "any", "not"] {
        if let Some(inner) = expression
            .strip_prefix(operator)
            .map(str::trim_start)
            .and_then(|s| s.strip_prefix('('))
            .and_then(|s| s.strip_suffix(')'))
        {
            let parts = split(inner);
            return match operator {
                "cfg" if parts.len() == 1 => evaluate_condition(parts[0], profile),
                "not" if parts.len() == 1 => evaluate_condition(parts[0], profile).not(),
                "all" => parts.iter().fold(ConditionValue::True, |value, part| {
                    value.and(evaluate_condition(part, profile))
                }),
                "any" => parts.iter().fold(ConditionValue::False, |value, part| {
                    value
                        .not()
                        .and(evaluate_condition(part, profile).not())
                        .not()
                }),
                _ => ConditionValue::Unknown,
            };
        }
    }
    let normalized = expression.replace(' ', "");
    if let Some(feature) = normalized
        .strip_prefix("feature=\"")
        .and_then(|s| s.strip_suffix('"'))
    {
        return if profile.features.iter().any(|f| f == feature) {
            ConditionValue::True
        } else {
            ConditionValue::False
        };
    }
    if profile
        .cfg
        .iter()
        .any(|fact| fact.replace(' ', "") == normalized)
    {
        return ConditionValue::True;
    }
    // An explicit fact for the same single-valued target key disproves alternatives.
    if let Some((key, _)) = normalized.split_once('=')
        && matches!(
            key,
            "target_arch"
                | "target_os"
                | "target_env"
                | "target_vendor"
                | "target_endian"
                | "target_pointer_width"
                | "target_abi"
        )
        && profile
            .cfg
            .iter()
            .any(|fact| fact.replace(' ', "").starts_with(&format!("{key}=")))
    {
        return ConditionValue::False;
    }
    if let Some(target) = &profile.target {
        if expression == target {
            return ConditionValue::True;
        }
        if !expression.contains(['(', '=', ' ']) && expression.contains('-') {
            return ConditionValue::False;
        }
    }
    ConditionValue::Unknown
}

fn split(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0_usize;
    let mut quoted = false;
    for (index, ch) in text.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth = depth.saturating_sub(1),
            ',' if !quoted && depth == 0 => {
                if !text[start..index].trim().is_empty() {
                    result.push(text[start..index].trim());
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    if !text[start..].trim().is_empty() {
        result.push(text[start..].trim());
    }
    result
}

pub(super) fn activate(
    profile: &AnalysisProfile,
    doc: &toml_edit::DocumentMut,
) -> (AnalysisProfile, BTreeSet<String>) {
    let mut profile = profile.clone();
    let mut pending = profile.features.clone();
    if profile.default_features {
        pending.push("default".into());
    }
    let mut visited = BTreeSet::new();
    let mut dependencies = BTreeSet::new();
    while let Some(feature) = pending.pop() {
        if !visited.insert(feature.clone()) {
            continue;
        }
        if let Some(values) = doc
            .get("features")
            .and_then(|f| f.get(&feature))
            .and_then(toml_edit::Item::as_array)
        {
            for value in values.iter().filter_map(toml_edit::Value::as_str) {
                if let Some(dep) = value.strip_prefix("dep:") {
                    dependencies.insert(dep.to_owned());
                } else if let Some((dep, _)) = value.split_once('/') {
                    if !dep.ends_with('?') {
                        dependencies.insert(dep.to_owned());
                    }
                } else {
                    pending.push(value.to_owned());
                }
            }
        } else {
            dependencies.insert(feature);
        }
    }
    profile.features = visited.into_iter().collect();
    (profile, dependencies)
}
