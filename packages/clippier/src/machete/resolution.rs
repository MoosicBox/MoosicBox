//! Incremental lexical scopes, imports and conditional item ownership.
//! This is not rustc resolution: glob imports and generated bindings retain gaps.
use super::{AnalysisProfile, ConditionValue, EvidenceKind, evaluate_condition};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub(super) struct Observation {
    pub name: String,
    pub line: usize,
    pub column: usize,
    pub kind: EvidenceKind,
    pub conditions: Vec<String>,
    pub uncertainty: Vec<String>,
}

fn at(tokens: &[String], index: usize, expected: &str) -> bool {
    tokens.get(index).is_some_and(|token| token == expected)
}
fn identifier(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|ch| ch.is_alphabetic() || ch == '_')
}
fn close(tokens: &[String], start: usize) -> usize {
    let (open, end) = match tokens.get(start).map(String::as_str) {
        Some("(") => ("(", ")"),
        Some("[") => ("[", "]"),
        Some("{") => ("{", "}"),
        _ => return start,
    };
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token == open {
            depth += 1;
        }
        if token == end {
            depth -= 1;
            if depth == 0 {
                return index;
            }
        }
    }
    tokens.len().saturating_sub(1)
}
fn expression(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| {
            token
                .strip_prefix('"')
                .map_or_else(|| token.clone(), |value| format!("\"{value}\""))
        })
        .collect()
}

// cfg attributes attach to the following syntactic item, including its body.
// Unknown cfg_attr is retained, rather than treated as a false predicate.
pub(super) fn condition_tokens(
    tokens: Vec<String>,
    positions: &[usize],
    profile: Option<&AnalysisProfile>,
) -> (Vec<String>, Vec<usize>, Vec<Vec<String>>) {
    let mut conditions = vec![Vec::new(); tokens.len()];
    let mut disabled = BTreeSet::new();
    let mut index = 0;
    while index + 2 < tokens.len() {
        if !at(&tokens, index, "#") {
            index += 1;
            continue;
        }
        let bracket = if at(&tokens, index + 1, "!") {
            index + 2
        } else {
            index + 1
        };
        if !at(&tokens, bracket, "[") {
            index += 1;
            continue;
        }
        let end = close(&tokens, bracket);
        let attribute = tokens.get(bracket + 1).map(String::as_str);
        let mut predicate = None;
        if attribute == Some("cfg") && at(&tokens, bracket + 2, "(") {
            predicate = tokens
                .get(bracket + 3..end.saturating_sub(1))
                .map(expression);
        } else if attribute == Some("cfg_attr") && at(&tokens, bracket + 2, "(") {
            let mut depth = 0_usize;
            let comma = (bracket + 3..end).find(|&i| {
                match tokens[i].as_str() {
                    "(" => depth += 1,
                    ")" => depth = depth.saturating_sub(1),
                    _ => {}
                }
                depth == 0 && at(&tokens, i, ",")
            });
            if let Some(comma) = comma {
                let condition = expression(&tokens[bracket + 3..comma]);
                if at(&tokens, comma + 1, "cfg") && at(&tokens, comma + 2, "(") {
                    let inner_end = close(&tokens, comma + 2);
                    predicate = tokens
                        .get(comma + 3..inner_end)
                        .map(|inner| format!("any(not({condition}),{})", expression(inner)));
                }
            }
        }
        if let Some(predicate) = predicate {
            let item_end = conditional_item_end(&tokens, index, end, bracket == index + 2);
            let value = profile.map_or(ConditionValue::Unknown, |profile| {
                evaluate_condition(&predicate, profile)
            });
            for (cursor, enclosing) in conditions
                .iter_mut()
                .enumerate()
                .take(item_end + 1)
                .skip(index)
            {
                enclosing.push(predicate.clone());
                if value == ConditionValue::False {
                    disabled.insert(cursor);
                }
            }
        }
        index = end + 1;
    }
    let mut filtered = Vec::new();
    let mut offsets = Vec::new();
    let mut filtered_conditions = Vec::new();
    for (index, token) in tokens.into_iter().enumerate() {
        if !disabled.contains(&index) {
            filtered.push(token);
            offsets.push(positions.get(index).copied().unwrap_or(0));
            filtered_conditions.push(conditions[index].clone());
        }
    }
    (filtered, offsets, filtered_conditions)
}

#[derive(Default)]
struct Scope {
    parent: Option<usize>,
    // Names have lexical start positions (items: scope start; lets: declaration end).
    bindings: BTreeMap<String, usize>,
}
fn shadowed(scopes: &[Scope], mut scope: usize, name: &str, index: usize) -> bool {
    loop {
        if scopes[scope]
            .bindings
            .get(name)
            .is_some_and(|start| *start <= index)
        {
            return true;
        }
        if let Some(parent) = scopes[scope].parent {
            scope = parent;
        } else {
            return false;
        }
    }
}

#[path = "syntax.rs"]
pub mod syntax;

pub(super) fn resolve(
    text: &str,
    tokens: &[String],
    positions: &[usize],
    conditions: &[Vec<String>],
    precision: bool,
) -> (
    Vec<Observation>,
    BTreeSet<String>,
    Option<syntax::SyntaxIndex>,
) {
    resolve_parsed(text, tokens, positions, conditions, precision, None)
}

pub(super) fn resolve_parsed(
    text: &str,
    tokens: &[String],
    positions: &[usize],
    conditions: &[Vec<String>],
    precision: bool,
    parsed: Option<&syn::File>,
) -> (
    Vec<Observation>,
    BTreeSet<String>,
    Option<syntax::SyntaxIndex>,
) {
    if precision {
        let analyzed = parsed.map_or_else(
            || syntax::analyze(text, positions, conditions),
            |file| Ok(syntax::analyze_parsed(text, file, positions, conditions)),
        );
        match analyzed {
            Ok((mut observations, gaps, index)) => {
                // Macro token trees are intentionally not interpreted as Rust AST.
                // Retain their lexical evidence and expansion uncertainty.
                let (lexical, _) = lexical_resolve(text, tokens, positions, conditions, false);
                observations.extend(lexical.into_iter().filter(|o| !o.uncertainty.is_empty()));
                return (observations, gaps, Some(index));
            }
            Err(error) => {
                let (observations, mut gaps) =
                    lexical_resolve(text, tokens, positions, conditions, false);
                gaps.insert(format!("Rust syntax could not be indexed: {error}"));
                return (observations, gaps, None);
            }
        }
    }
    let (observations, gaps) = lexical_resolve(text, tokens, positions, conditions, false);
    (observations, gaps, None)
}

fn lexical_resolve(
    text: &str,
    tokens: &[String],
    positions: &[usize],
    conditions: &[Vec<String>],
    precision: bool,
) -> (Vec<Observation>, BTreeSet<String>) {
    let (scopes, token_scopes) = lexical_scopes(tokens);
    let (imports, import_tokens, scopes, mut gaps) =
        import_bindings(tokens, scopes, &token_scopes, conditions, precision);
    let derive_members = derive_members(tokens, precision, &mut gaps);
    let mut observations = Vec::new();
    let mut locations = Vec::with_capacity(positions.len());
    let mut characters = text.char_indices().peekable();
    let (mut line, mut column) = (1, 1);
    for &offset in positions {
        while let Some(&(position, ch)) = characters.peek() {
            if position >= offset {
                break;
            }
            characters.next();
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        locations.push((line, column));
    }
    let mut macro_stack = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token.as_str(), "(" | "[" | "{") {
            let quoted = macro_stack.last().copied().unwrap_or(false)
                || (index > 0 && at(tokens, index - 1, "!"))
                || (index >= 3 && at(tokens, index - 3, "macro_rules"));
            macro_stack.push(quoted);
        }
        let quoted = macro_stack.last().copied().unwrap_or(false);
        let import = imports.get(&index).copied();
        let tail = index >= 2 && at(tokens, index - 1, "::") && identifier(&tokens[index - 2]);
        let absolute = index > 0 && at(tokens, index - 1, "::") && !tail;
        let attribute = index >= 2 && at(tokens, index - 1, "[") && at(tokens, index - 2, "#");
        let external =
            index >= 2 && at(tokens, index - 2, "extern") && at(tokens, index - 1, "crate");
        let candidate = derive_members.contains(&index)
            || import.is_some()
            || external
            || (!import_tokens.contains(&index)
                && !tail
                && (at(tokens, index + 1, "::")
                    || at(tokens, index + 1, "!")
                    || attribute
                    || quoted));
        if candidate
            && identifier(token)
            && !matches!(token.as_str(), "crate" | "self" | "super" | "Self")
        {
            // Imported terminal bindings do not shadow their own import root.
            let own_binding = import.is_some() && !at(tokens, index + 1, "::");
            if !precision
                || absolute
                || own_binding
                || !shadowed(&scopes, token_scopes[index], token, index)
            {
                let (line, column) = locations.get(index).copied().unwrap_or((1, 1));
                observations.push(Observation {
                    name: token.clone(),
                    line,
                    column,
                    kind: import.unwrap_or_else(|| {
                        if quoted
                            || attribute
                            || derive_members.contains(&index)
                            || at(tokens, index + 1, "!")
                        {
                            EvidenceKind::Macro
                        } else {
                            EvidenceKind::Direct
                        }
                    }),
                    conditions: conditions.get(index).cloned().unwrap_or_default(),
                    uncertainty: if quoted {
                        vec!["unexpanded macro token; lexical retention only".into()]
                    } else {
                        Vec::new()
                    },
                });
            }
        }
        if matches!(token.as_str(), ")" | "]" | "}") {
            macro_stack.pop();
        }
    }
    (observations, gaps)
}

fn lexical_scopes(tokens: &[String]) -> (Vec<Scope>, Vec<usize>) {
    let mut scopes = vec![Scope::default()];
    let mut token_scopes = vec![0; tokens.len()];
    let mut current = 0;
    let mut enclosing = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        token_scopes[index] = current;
        if token == "{" {
            enclosing.push(current);
            scopes.push(Scope {
                parent: if index >= 2 && at(tokens, index - 2, "mod") {
                    None
                } else {
                    Some(current)
                },
                ..Scope::default()
            });
            current = scopes.len() - 1;
        }
        if token == "}" {
            current = enclosing.pop().unwrap_or(0);
        }
    }
    (scopes, token_scopes)
}

fn import_bindings(
    tokens: &[String],
    mut scopes: Vec<Scope>,
    token_scopes: &[usize],
    conditions: &[Vec<String>],
    precision: bool,
) -> (
    BTreeMap<usize, EvidenceKind>,
    BTreeSet<usize>,
    Vec<Scope>,
    BTreeSet<String>,
) {
    let mut gaps = BTreeSet::new();
    let mut imports = BTreeMap::<usize, EvidenceKind>::new();
    let mut import_tokens = BTreeSet::new();
    let mut index = 0;
    let mut macro_stack = Vec::new();
    while index < tokens.len() {
        if matches!(tokens[index].as_str(), "(" | "[" | "{") {
            let quoted = macro_stack.last().copied().unwrap_or(false)
                || (index > 0 && at(tokens, index - 1, "!"))
                || (index >= 3 && at(tokens, index - 3, "macro_rules"));
            macro_stack.push(quoted);
        }
        let quoted = macro_stack.last().copied().unwrap_or(false);
        if matches!(tokens[index].as_str(), ")" | "]" | "}") {
            macro_stack.pop();
        }
        if quoted {
            index += 1;
            continue;
        }
        let scope = token_scopes[index];
        let token = tokens[index].as_str();
        if matches!(
            token,
            "mod" | "struct" | "enum" | "union" | "trait" | "type"
        ) && conditions.get(index).is_none_or(Vec::is_empty)
            && let Some(name) = tokens.get(index + 1).filter(|name| identifier(name))
        {
            scopes[scope].bindings.insert(name.clone(), 0);
        }
        if token == "use" {
            let end = (index + 1..tokens.len())
                .find(|&i| at(tokens, i, ";"))
                .unwrap_or(tokens.len());
            let kind = if index > 0 && at(tokens, index - 1, "pub") {
                EvidenceKind::Reexport
            } else {
                EvidenceKind::Import
            };
            let mut expecting_root = true;
            let mut nesting = 0_usize;
            for cursor in index + 1..end {
                import_tokens.insert(cursor);
                let name = &tokens[cursor];
                if name == "*" {
                    gaps.insert("glob import may introduce dependency-shadowing bindings".into());
                }
                if name == "{" {
                    nesting += 1;
                    continue;
                }
                if name == "}" {
                    nesting = nesting.saturating_sub(1);
                    continue;
                }
                if name == "," && nesting == 1 && at(tokens, index + 1, "{") {
                    expecting_root = true;
                    continue;
                }
                if name == "as" {
                    if let Some(alias) = tokens.get(cursor + 1) {
                        if cursor == index + 2 && conditions.get(index).is_none_or(Vec::is_empty) {
                            scopes[scope].bindings.insert(alias.clone(), 0);
                        } else if precision {
                            // Only the parse-error fallback reaches this branch.
                            // Successful AST indexing resolves aliases directly.
                            gaps.insert(format!(
                                "imported alias {alias} requires syntax resolution"
                            ));
                        }
                    }
                    continue;
                }
                if identifier(name) && expecting_root {
                    imports.insert(cursor, kind);
                    expecting_root = false;
                }
            }
        }
        // Generic parameters and pattern-dependent bindings require compiler namespaces.
        if precision
            && token == "fn"
            && let Some(open) =
                (index + 1..tokens.len()).find(|&i| at(tokens, i, "(") || at(tokens, i, "{"))
            && tokens[index + 1..open].iter().any(|token| token == "<")
        {
            gaps.insert("generic parameter namespace resolution is not supported".into());
        }
        index += 1;
    }
    (imports, import_tokens, scopes, gaps)
}

fn derive_members(
    tokens: &[String],
    precision: bool,
    gaps: &mut BTreeSet<String>,
) -> BTreeSet<usize> {
    let mut derive_members = BTreeSet::new();
    for (index, token) in tokens.iter().enumerate() {
        if token == "derive"
            && at(tokens, index + 1, "(")
            && tokens[..index]
                .iter()
                .rposition(|token| token == "[")
                .is_some_and(|open| {
                    open > 0
                        && at(tokens, open - 1, "#")
                        && !tokens[open..index].iter().any(|token| token == "]")
                })
        {
            let end = close(tokens, index + 1);
            for cursor in index + 2..end {
                if identifier(&tokens[cursor]) && !(cursor > 0 && at(tokens, cursor - 1, "::")) {
                    derive_members.insert(cursor);
                }
            }
        }
        if precision
            && matches!(token.as_str(), "concat_idents" | "paste")
            && at(tokens, index + 1, "!")
        {
            gaps.insert("identifier-generating macro requires expansion".into());
        }
    }
    derive_members
}

fn conditional_item_end(tokens: &[String], index: usize, end: usize, inner: bool) -> usize {
    let mut item_end = tokens.len().saturating_sub(1);
    if inner {
        let mut blocks = Vec::new();
        for cursor in 0..index {
            if at(tokens, cursor, "{") {
                blocks.push(cursor);
            }
            if at(tokens, cursor, "}") {
                blocks.pop();
            }
        }
        if let Some(open) = blocks.last() {
            item_end = close(tokens, *open);
        }
    } else {
        let mut cursor = end + 1;
        while cursor < tokens.len() {
            if at(tokens, cursor, "[") || at(tokens, cursor, "(") {
                cursor = close(tokens, cursor) + 1;
                continue;
            }
            if at(tokens, cursor, "{") {
                item_end = close(tokens, cursor);
                break;
            }
            if at(tokens, cursor, ";") {
                item_end = cursor;
                break;
            }
            cursor += 1;
        }
    }
    item_end
}
