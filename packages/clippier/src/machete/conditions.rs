//! Offline conditional reasoning. No host cfg or compiler execution is inferred.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Meta, Token, parse::Parser, punctuated::Punctuated};

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
    /// Explicit positive cfg facts, e.g. `unix` or `target_os="linux"`.
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Atom {
    key: String,
    value: Option<String>,
}

fn single_valued(key: &str) -> bool {
    matches!(
        key,
        "target_arch"
            | "target_os"
            | "target_env"
            | "target_vendor"
            | "target_endian"
            | "target_pointer_width"
            | "target_abi"
    )
}

fn conflicts(left: &Atom, right: &Atom) -> bool {
    (left.key == right.key
        && single_valued(&left.key)
        && left.value.is_some()
        && right.value.is_some()
        && left.value != right.value)
        || (left.value.is_none()
            && right.value.is_none()
            && matches!(
                (left.key.as_str(), right.key.as_str()),
                ("unix", "windows") | ("windows", "unix")
            ))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Node {
    Atom(Atom),
    All(Vec<usize>),
    Any(Vec<usize>),
    Not(usize),
}

// Interned nodes preserve sharing between repeated conditions and their negations.
#[derive(Default)]
struct Graph {
    nodes: Vec<Node>,
    interned: BTreeMap<Node, usize>,
}
impl Graph {
    fn intern(&mut self, node: Node) -> Option<usize> {
        if let Some(id) = self.interned.get(&node) {
            return Some(*id);
        }
        if self.nodes.len() >= 4096 {
            return None;
        }
        let id = self.nodes.len();
        self.nodes.push(node.clone());
        self.interned.insert(node, id);
        Some(id)
    }
    fn parse(&mut self, text: &str) -> Option<usize> {
        // Bound nesting before invoking syn, which otherwise recursively parses lists.
        if text.len() > 65536 {
            return None;
        }
        let mut depth = 0usize;
        for byte in text.bytes() {
            if matches!(byte, b'(' | b'[' | b'{') {
                depth += 1;
                if depth > 64 {
                    return None;
                }
            }
            if matches!(byte, b')' | b']' | b'}') {
                depth = depth.checked_sub(1)?;
            }
        }
        self.meta(syn::parse_str(text).ok()?, 0)
    }
    fn meta(&mut self, meta: Meta, depth: usize) -> Option<usize> {
        if depth > 64 {
            return None;
        }
        let key = meta.path().get_ident()?.to_string();
        match meta {
            Meta::Path(_) => self.intern(Node::Atom(Atom { key, value: None })),
            Meta::NameValue(pair) => {
                let syn::Expr::Lit(literal) = pair.value else {
                    return None;
                };
                let syn::Lit::Str(value) = literal.lit else {
                    return None;
                };
                self.intern(Node::Atom(Atom {
                    key,
                    value: Some(value.value()),
                }))
            }
            Meta::List(list) => {
                let items = Punctuated::<Meta, Token![,]>::parse_terminated
                    .parse2(list.tokens)
                    .ok()?;
                let children = items
                    .into_iter()
                    .map(|item| self.meta(item, depth + 1))
                    .collect::<Option<Vec<_>>>()?;
                match key.as_str() {
                    "cfg" if children.len() == 1 => Some(children[0]),
                    "not" if children.len() == 1 => self.intern(Node::Not(children[0])),
                    "all" => self.intern(Node::All(children)),
                    "any" => self.intern(Node::Any(children)),
                    _ => None,
                }
            }
        }
    }
    fn evaluate(&self, root: usize, atom: &impl Fn(&Atom) -> ConditionValue) -> ConditionValue {
        // Nodes are inserted after their children; evaluate shared subexpressions once.
        let mut values: Vec<ConditionValue> = Vec::with_capacity(root + 1);
        for node in self.nodes.iter().take(root + 1) {
            let value = match node {
                Node::Atom(key) => atom(key),
                Node::Not(child) => values[*child].not(),
                Node::All(children) => children
                    .iter()
                    .fold(ConditionValue::True, |v, child| v.and(values[*child])),
                Node::Any(children) => children.iter().fold(ConditionValue::False, |v, child| {
                    v.not().and(values[*child].not()).not()
                }),
            };
            values.push(value);
        }
        values[root]
    }
}

/// Evaluate cfg using explicit facts only; absent custom/target facts stay unknown.
/// Contradictory authoritative facts and unsupported syntax return `Unknown`.
#[must_use]
pub fn evaluate_condition(expression: &str, profile: &AnalysisProfile) -> ConditionValue {
    let mut graph = Graph::default();
    let mut facts = BTreeSet::new();
    for fact in &profile.cfg {
        let Some(id) = graph.parse(fact) else {
            return ConditionValue::Unknown;
        };
        let Node::Atom(atom) = &graph.nodes[id] else {
            return ConditionValue::Unknown;
        };
        facts.insert(atom.clone());
    }
    if facts
        .iter()
        .any(|left| facts.iter().any(|right| conflicts(left, right)))
        || facts.iter().any(|fact| {
            fact.key == "feature"
                && fact
                    .value
                    .as_ref()
                    .is_some_and(|feature| !profile.features.contains(feature))
        })
    {
        return ConditionValue::Unknown;
    }
    // Cargo target table selectors may be literal triples, not cfg syntax.
    let expression = expression.trim();
    if expression.contains('-') && !expression.contains(['(', '=', '"']) {
        return profile
            .target
            .as_ref()
            .map_or(ConditionValue::Unknown, |target| {
                if target == expression {
                    ConditionValue::True
                } else {
                    ConditionValue::False
                }
            });
    }
    let Some(root) = graph.parse(expression) else {
        return ConditionValue::Unknown;
    };
    graph.evaluate(root, &|atom| {
        if facts.contains(atom) {
            return ConditionValue::True;
        }
        if atom.key == "feature"
            && let Some(feature) = &atom.value
        {
            return if profile.features.contains(feature) {
                ConditionValue::True
            } else {
                ConditionValue::False
            };
        }
        if facts.contains(atom) {
            ConditionValue::True
        } else if facts.iter().any(|fact| conflicts(atom, fact)) {
            ConditionValue::False
        } else {
            ConditionValue::Unknown
        }
    })
}

/// Result of bounded symbolic reachability, including explicit incomplete analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Satisfiability {
    /// A consistent assignment exists.
    Sat,
    /// All consistent assignments were ruled out.
    Unsat,
    /// Unsupported input or a parsing/search bound prevented a decision.
    Exhausted,
}

fn solve(
    graph: &Graph,
    root: usize,
    atoms: &[Atom],
    assigned: &mut BTreeMap<Atom, bool>,
    budget: &mut usize,
) -> Satisfiability {
    if *budget == 0 {
        return Satisfiability::Exhausted;
    }
    *budget -= 1;
    match graph.evaluate(root, &|atom| {
        assigned.get(atom).map_or(ConditionValue::Unknown, |value| {
            if *value {
                ConditionValue::True
            } else {
                ConditionValue::False
            }
        })
    }) {
        ConditionValue::True => return Satisfiability::Sat,
        ConditionValue::False => return Satisfiability::Unsat,
        ConditionValue::Unknown => {}
    }
    let Some(atom) = atoms.iter().find(|atom| !assigned.contains_key(*atom)) else {
        return Satisfiability::Exhausted;
    };
    let mut exhausted = false;
    for value in [false, true] {
        if value
            && assigned
                .iter()
                .any(|(other, active)| *active && conflicts(atom, other))
        {
            continue;
        }
        assigned.insert(atom.clone(), value);
        let outcome = solve(graph, root, atoms, assigned, budget);
        assigned.remove(atom);
        match outcome {
            Satisfiability::Sat => return outcome,
            Satisfiability::Exhausted => exhausted = true,
            Satisfiability::Unsat => {}
        }
    }
    if exhausted {
        Satisfiability::Exhausted
    } else {
        Satisfiability::Unsat
    }
}

fn satisfiability(conditions: &[String], without_test: bool) -> Satisfiability {
    if conditions.len() > 4096 {
        return Satisfiability::Exhausted;
    }
    let mut graph = Graph::default();
    let Some(mut roots) = conditions
        .iter()
        .map(|condition| graph.parse(condition))
        .collect::<Option<Vec<_>>>()
    else {
        return Satisfiability::Exhausted;
    };
    if without_test {
        let Some(root) = graph.parse("not(test)") else {
            return Satisfiability::Exhausted;
        };
        roots.push(root);
    }
    let Some(root) = graph.intern(Node::All(roots)) else {
        return Satisfiability::Exhausted;
    };
    let atoms = graph
        .nodes
        .iter()
        .filter_map(|node| {
            if let Node::Atom(atom) = node {
                Some(atom.clone())
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    // Bound recursion as well as total search work. Exhaustion never proves absence.
    if atoms.len() > 128 {
        return Satisfiability::Exhausted;
    }
    solve(&graph, root, &atoms, &mut BTreeMap::new(), &mut 16384)
}

/// Invocation-local memoization of profile-independent Boolean queries.
/// Keys intern sorted, deduplicated conjunctions without rewriting string literals.
/// At most 256 conjunctions and 1 MiB of source keys are retained; eviction only
/// affects performance. No profile evaluation or filesystem state is cached.
#[derive(Default)]
pub(super) struct ConditionQueries {
    conjunctions: BTreeMap<Vec<String>, [Option<Satisfiability>; 2]>,
    key_bytes: usize,
}

impl ConditionQueries {
    /// Return explicit reachability, including unsupported or exhausted analysis.
    pub(super) fn satisfiability(&mut self, conditions: &[String]) -> Satisfiability {
        self.query(conditions, false)
    }

    /// Reachability after adding `not(test)`. Only `Unsat` proves test implication.
    pub(super) fn without_test(&mut self, conditions: &[String]) -> Satisfiability {
        self.query(conditions, true)
    }

    fn query(&mut self, conditions: &[String], without_test: bool) -> Satisfiability {
        if conditions.len() > 4096 {
            return Satisfiability::Exhausted;
        }
        let Some(bytes) = conditions
            .iter()
            .try_fold(0usize, |sum, value| sum.checked_add(value.len()))
        else {
            return Satisfiability::Exhausted;
        };
        if bytes > 65536 {
            return Satisfiability::Exhausted;
        }
        let mut key = conditions
            .iter()
            .map(|value| value.trim().to_owned())
            .collect::<Vec<_>>();
        key.sort_unstable();
        key.dedup();
        let mode = usize::from(without_test);
        if let Some(Some(result)) = self.conjunctions.get(&key).map(|results| results[mode]) {
            return result;
        }
        let result = satisfiability(&key, without_test);
        if !self.conjunctions.contains_key(&key) {
            if self.conjunctions.len() >= 256 || self.key_bytes + bytes > 1024 * 1024 {
                self.conjunctions.clear();
                self.key_bytes = 0;
            }
            self.key_bytes += bytes;
        }
        self.conjunctions.entry(key).or_insert([None; 2])[mode] = Some(result);
        result
    }
}

/// Bounded reachability retaining unsupported/budget-exhausted outcomes for reporting.
#[cfg(test)]
pub(super) fn condition_satisfiability(conditions: &[String]) -> Satisfiability {
    ConditionQueries::default().satisfiability(conditions)
}

/// Whether the conjunction might be reachable. Invalid/over-budget input stays reachable.
#[cfg(test)]
pub(super) fn condition_satisfiable(conditions: &[String]) -> bool {
    condition_satisfiability(conditions) != Satisfiability::Unsat
}

/// Prove that the conjunction implies `test`; unsupported/exhausted queries return false.
/// An unreachable conjunction implies `test` vacuously; check reachability separately.
#[cfg(test)]
pub(super) fn condition_requires_test(conditions: &[String]) -> bool {
    ConditionQueries::default().without_test(conditions) == Satisfiability::Unsat
}

/// Manifest-local fixed point, not a cross-package Cargo resolver or unit graph.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct FeatureActivation {
    pub local_features: BTreeSet<String>,
    /// Dependencies activated or referenced by forwarding in some manifest context.
    /// Declaration kind/target availability must still be checked by the unit graph.
    pub dependencies: BTreeSet<String>,
    pub dependency_features: BTreeMap<String, BTreeSet<String>>,
    /// Unknown local feature names; callers must report an invalid/incomplete profile.
    pub unresolved_features: BTreeSet<String>,
}

/// Expand local cycles and strong/weak forwarding independently of visitation order.
/// Nonoptional declarations are available for forwarding, but not usage evidence alone.
/// This is a union across declaration kinds and selectors, not proof that a dependency
/// is available in a particular compilation unit. Callers must apply unit ownership.
pub(super) fn activate_features(
    profile: &AnalysisProfile,
    doc: &toml_edit::DocumentMut,
) -> FeatureActivation {
    let mut result = FeatureActivation::default();
    let (mut available, optional) = dependency_availability(doc);
    let features = doc.get("features").and_then(toml_edit::Item::as_table_like);
    let suppressed = features
        .into_iter()
        .flat_map(|features| features.iter())
        .filter_map(|(_, value)| value.as_array())
        .flat_map(|array| array.iter())
        .filter_map(toml_edit::Value::as_str)
        .filter_map(|value| value.strip_prefix("dep:"))
        .collect::<BTreeSet<_>>();
    let mut pending = profile.features.clone();
    if profile.default_features && features.is_some_and(|features| features.contains_key("default"))
    {
        pending.push("default".into());
    }
    let mut visited = BTreeSet::new();
    let mut weak = BTreeSet::new();
    while let Some(feature) = pending.pop() {
        if !visited.insert(feature.clone()) {
            continue;
        }
        if let Some(dep) = feature.strip_prefix("dep:") {
            result.dependencies.insert(dep.to_owned());
            available.insert(dep.to_owned());
        } else if let Some((dep, forwarded)) = feature.split_once('/') {
            if let Some(dep) = dep.strip_suffix('?') {
                weak.insert((dep.to_owned(), forwarded.to_owned()));
            } else {
                available.insert(dep.to_owned());
                result.dependencies.insert(dep.to_owned());
                if optional.contains(dep)
                    && !suppressed.contains(dep)
                    && !features.is_some_and(|features| features.contains_key(dep))
                {
                    pending.push(dep.to_owned());
                }
                result
                    .dependency_features
                    .entry(dep.to_owned())
                    .or_default()
                    .insert(forwarded.to_owned());
            }
        } else if let Some(values) = features
            .and_then(|features| features.get(&feature))
            .and_then(toml_edit::Item::as_array)
        {
            result.local_features.insert(feature);
            pending.extend(
                values
                    .iter()
                    .filter_map(toml_edit::Value::as_str)
                    .map(str::to_owned),
            );
        } else if optional.contains(&feature) && !suppressed.contains(feature.as_str()) {
            result.local_features.insert(feature.clone());
            available.insert(feature.clone());
            result.dependencies.insert(feature);
        } else {
            result.unresolved_features.insert(feature);
        }
    }
    for (dep, feature) in weak {
        if available.contains(&dep) {
            result.dependencies.insert(dep.clone());
            result
                .dependency_features
                .entry(dep)
                .or_default()
                .insert(feature);
        }
    }
    result
}

fn dependency_availability(doc: &toml_edit::DocumentMut) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut available = BTreeSet::new();
    let mut optional = BTreeSet::new();
    let mut collect = |item: &toml_edit::Item| {
        if let Some(table) = item.as_table_like() {
            for (name, value) in table.iter() {
                if value
                    .get("optional")
                    .and_then(toml_edit::Item::as_bool)
                    .unwrap_or(false)
                {
                    optional.insert(name.to_owned());
                } else {
                    available.insert(name.to_owned());
                }
            }
        }
    };
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(item) = doc.get(section) {
            collect(item);
        }
    }
    if let Some(targets) = doc.get("target").and_then(toml_edit::Item::as_table_like) {
        for (_, target) in targets.iter() {
            for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(item) = target.get(section) {
                    collect(item);
                }
            }
        }
    }
    (available, optional)
}

pub(super) fn activate(
    profile: &AnalysisProfile,
    doc: &toml_edit::DocumentMut,
) -> (AnalysisProfile, BTreeSet<String>) {
    let activated = activate_features(profile, doc);
    let mut profile = profile.clone();
    profile.features = activated.local_features.into_iter().collect();
    (profile, activated.dependencies)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn conditions(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn query_session_interns_conjunctions_and_separates_modes() {
        let mut queries = ConditionQueries::default();
        let original = conditions(&[" test ", "custom", "test"]);
        assert_eq!(queries.satisfiability(&original), Satisfiability::Sat);
        assert_eq!(queries.without_test(&original), Satisfiability::Unsat);
        assert_eq!(
            queries.satisfiability(&conditions(&["custom", "test"])),
            Satisfiability::Sat
        );
        assert_eq!(queries.conjunctions.len(), 1);
        assert_eq!(
            queries.conjunctions.values().next(),
            Some(&[Some(Satisfiability::Sat), Some(Satisfiability::Unsat)])
        );
        assert_eq!(
            queries.satisfiability(&conditions(&["feature=\"a b\"", "not(feature=\"ab\")"])),
            Satisfiability::Sat
        );
    }

    #[test]
    fn query_session_preserves_invalid_and_exhausted_outcomes() {
        let mut queries = ConditionQueries::default();
        for invalid in [
            "not(test, custom)",
            "unsupported(test)",
            "all(test,,custom)",
            "feature=42",
            "",
        ] {
            let input = conditions(&[invalid]);
            assert_eq!(queries.satisfiability(&input), Satisfiability::Exhausted);
            assert_eq!(queries.without_test(&input), Satisfiability::Exhausted);
            assert_eq!(queries.satisfiability(&input), Satisfiability::Exhausted);
        }
        let large = vec!["x".repeat(65537)];
        assert_eq!(queries.satisfiability(&large), Satisfiability::Exhausted);
        assert_eq!(
            queries.satisfiability(&vec!["test".into(); 4097]),
            Satisfiability::Exhausted
        );
        let many = (0..129)
            .map(|index| format!("custom_{index}"))
            .collect::<Vec<_>>();
        assert_eq!(queries.satisfiability(&many), Satisfiability::Exhausted);
        assert_eq!(queries.without_test(&many), Satisfiability::Exhausted);
    }

    #[test]
    fn search_budget_exhaustion_is_not_an_unsatisfiability_proof() {
        let mut graph = Graph::default();
        let root = graph.parse("all(test, custom)").unwrap();
        let atoms = vec![
            Atom {
                key: "test".into(),
                value: None,
            },
            Atom {
                key: "custom".into(),
                value: None,
            },
        ];
        assert_eq!(
            solve(&graph, root, &atoms, &mut BTreeMap::new(), &mut 0),
            Satisfiability::Exhausted
        );
        assert_eq!(
            solve(&graph, root, &atoms, &mut BTreeMap::new(), &mut 1),
            Satisfiability::Exhausted
        );
        assert_eq!(
            solve(&graph, root, &atoms, &mut BTreeMap::new(), &mut 16),
            Satisfiability::Sat
        );
    }

    #[test]
    fn query_session_eviction_does_not_change_semantics() {
        let mut queries = ConditionQueries::default();
        for index in 0..300 {
            assert_eq!(
                queries.satisfiability(&[format!("custom_{index}")]),
                Satisfiability::Sat
            );
            assert!(queries.conjunctions.len() <= 256);
            assert!(queries.key_bytes <= 1024 * 1024);
        }
        assert_eq!(
            queries.satisfiability(&conditions(&["custom_0", "not(custom_0)"])),
            Satisfiability::Unsat
        );
        assert!(ConditionQueries::default().conjunctions.is_empty());
    }

    #[test]
    fn compound_test_implication_and_contradictions() {
        assert!(condition_requires_test(&conditions(&[
            "all(test, feature=\"logic\")"
        ])));
        assert!(condition_requires_test(&conditions(&[
            "not(any(not(test), custom))"
        ])));
        assert!(!condition_requires_test(&conditions(&[
            "any(test, custom)"
        ])));
        assert!(!condition_requires_test(&conditions(&["not(test)"])));
        assert!(!condition_satisfiable(&conditions(&[
            "custom",
            "not(custom)"
        ])));
        assert!(condition_satisfiable(&conditions(&[
            "any()",
            "unsupported(x)"
        ])));
        assert!(!condition_satisfiable(&conditions(&["any()\n"])));
        assert!(condition_satisfiable(&conditions(&["all()\n"])));
        assert!(!condition_requires_test(&conditions(&[
            "not(test, custom)"
        ])));
    }
    #[test]
    fn target_constraints_are_selective() {
        assert!(!condition_satisfiable(&conditions(&[
            "target_os=\"linux\"",
            "target_os=\"windows\""
        ])));
        assert!(!condition_satisfiable(&conditions(&["unix", "windows"])));
        assert!(condition_satisfiable(&conditions(&[
            "target_feature=\"sse\"",
            "target_feature=\"avx\""
        ])));
        assert!(condition_satisfiable(&conditions(&[
            "feature=\"one\"",
            "feature=\"two\""
        ])));
        assert!(condition_satisfiable(&conditions(&[
            "target_os",
            "target_os=\"linux\""
        ])));
        assert!(condition_requires_test(&conditions(&[
            "any(test, target_os=\"linux\")",
            "target_os=\"windows\""
        ])));
    }
    #[test]
    fn explicit_facts_and_literals_preserve_unknown() {
        let mut profile = AnalysisProfile {
            features: vec!["with space".into()],
            cfg: vec!["target_os = \"linux\"".into()],
            ..AnalysisProfile::default()
        };
        assert_eq!(
            evaluate_condition("feature = r#\"with space\"#", &profile),
            ConditionValue::True
        );
        assert_eq!(
            evaluate_condition("target_os=\"windows\"", &profile),
            ConditionValue::False
        );
        assert_eq!(
            evaluate_condition("not(custom)", &profile),
            ConditionValue::Unknown
        );
        assert_eq!(
            evaluate_condition("all(custom, any())", &profile),
            ConditionValue::False
        );
        assert_eq!(
            evaluate_condition("cfg(any(all(), custom))", &profile),
            ConditionValue::True
        );
        assert_eq!(
            evaluate_condition("all(custom,,test)", &profile),
            ConditionValue::Unknown
        );
        profile.cfg.push("target_os=\"windows\"".into());
        assert_eq!(
            evaluate_condition("target_os=\"linux\"", &profile),
            ConditionValue::Unknown
        );
    }
    #[test]
    fn exhaustion_is_conservative() {
        let many = (0..129)
            .map(|index| format!("custom_{index}"))
            .collect::<Vec<_>>();
        assert!(condition_satisfiable(&many));
        assert_eq!(condition_satisfiability(&many), Satisfiability::Exhausted);
        assert!(!condition_requires_test(&many));
        let deep = format!("{}test{}", "not(".repeat(65), ")".repeat(65));
        assert!(condition_satisfiable(&[deep]));
    }
    #[test]
    fn target_triples_do_not_invent_cfg_facts() {
        let mut profile = AnalysisProfile {
            target: Some("x86_64-unknown-linux-gnu".into()),
            ..AnalysisProfile::default()
        };
        assert_eq!(
            evaluate_condition("x86_64-unknown-linux-gnu", &profile),
            ConditionValue::True
        );
        assert_eq!(
            evaluate_condition("aarch64-unknown-linux-gnu", &profile),
            ConditionValue::False
        );
        assert_eq!(
            evaluate_condition("target_os=\"linux\"", &profile),
            ConditionValue::Unknown
        );
        profile.cfg = vec!["feature=\"missing\"".into()];
        assert_eq!(
            evaluate_condition("feature=\"missing\"", &profile),
            ConditionValue::Unknown
        );
    }

    #[test]
    fn forwarding_cycles_and_weak_order() {
        let doc: toml_edit::DocumentMut = r#"
[dependencies]
opt = { version = "1.0.0", optional = true }
always = "1.0.0"
[features]
default = ["enable", "weak", "cycle"]
enable = ["dep:opt"]
weak = ["opt?/serde", "always?/std"]
cycle = ["default"]
"#
        .parse()
        .unwrap();
        let activated = activate_features(&AnalysisProfile::default(), &doc);
        assert_eq!(
            activated.dependency_features["opt"],
            BTreeSet::from(["serde".into()])
        );
        assert_eq!(
            activated.dependency_features["always"],
            BTreeSet::from(["std".into()])
        );
        assert!(!activated.local_features.contains("opt"));
        for features in [
            vec!["weak".into(), "enable".into()],
            vec!["enable".into(), "weak".into()],
        ] {
            let result = activate_features(
                &AnalysisProfile {
                    features,
                    default_features: false,
                    ..AnalysisProfile::default()
                },
                &doc,
            );
            assert_eq!(result.dependency_features, activated.dependency_features);
        }
        let weak_only = activate_features(
            &AnalysisProfile {
                features: vec!["weak".into()],
                default_features: false,
                ..AnalysisProfile::default()
            },
            &doc,
        );
        assert!(!weak_only.dependencies.contains("opt"));
        assert!(!weak_only.dependency_features.contains_key("opt"));
    }
    #[test]
    fn implicit_features_and_strong_forwarding() {
        let doc: toml_edit::DocumentMut = r#"
[dependencies]
implicit = { version = "1.0.0", optional = true }
explicit = { version = "1.0.0", optional = true }
[features]
expose = ["dep:explicit"]
strong = ["explicit/std"]
"#
        .parse()
        .unwrap();
        let result = activate_features(
            &AnalysisProfile {
                features: vec![
                    "implicit".into(),
                    "explicit".into(),
                    "unknown".into(),
                    "strong".into(),
                ],
                default_features: false,
                ..AnalysisProfile::default()
            },
            &doc,
        );
        assert!(result.local_features.contains("implicit"));
        assert!(!result.local_features.contains("explicit"));
        assert!(!result.dependencies.contains("unknown"));
        assert!(result.unresolved_features.contains("unknown"));
        let empty: toml_edit::DocumentMut = "".parse().unwrap();
        assert!(
            activate_features(&AnalysisProfile::default(), &empty)
                .dependencies
                .is_empty()
        );
        assert!(result.dependencies.contains("explicit"));
        assert!(result.dependency_features["explicit"].contains("std"));
    }
}
