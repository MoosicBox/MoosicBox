//! Bounded tree indexing and target resolution shared by action evaluation and forms.
use std::collections::BTreeMap;

use hyperchad_actions::{ElementTarget, Target};
use hyperchad_transformer::Container;

use super::{ActionError, ActionLimits, Source, Work};

pub(super) fn at<'a>(root: &'a Container, path: &[usize]) -> Option<&'a Container> {
    let mut node = root;
    for index in path {
        node = node.children.get(*index)?;
    }
    Some(node)
}
pub(super) fn at_mut<'a>(root: &'a mut Container, path: &[usize]) -> Option<&'a mut Container> {
    let mut node = root;
    for index in path {
        node = node.children.get_mut(*index)?;
    }
    Some(node)
}
pub(super) fn index(
    root: &Container,
    limits: &ActionLimits,
) -> Result<BTreeMap<String, Vec<usize>>, ActionError> {
    let mut result = BTreeMap::new();
    let mut stack = vec![(root, Vec::new())];
    while let Some((node, path)) = stack.pop() {
        if result.len() >= limits.max_nodes || path.len() > limits.max_depth {
            return Err(ActionError::Limit("document nodes/depth"));
        }
        let key = crate::layout::node_key(node, &path);
        if key.len() > limits.max_value_bytes {
            return Err(ActionError::Limit("node key"));
        }
        if result.insert(key, path.clone()).is_some() {
            return Err(ActionError::Invalid("duplicate node key".into()));
        }
        if node.children.len() > limits.max_nodes.saturating_sub(result.len()) {
            return Err(ActionError::Limit("document children"));
        }
        for (i, child) in node.children.iter().enumerate().rev() {
            let mut path = path.clone();
            path.push(i);
            stack.push((child, path));
        }
    }
    Ok(result)
}

impl Work<'_> {
    pub(super) fn target_text(
        &self,
        target: &Target,
        source: &Source,
    ) -> Result<String, ActionError> {
        match target {
            Target::Literal(text) => {
                self.text(text)?;
                Ok(text.clone())
            }
            Target::Ref(name) => self
                .engine
                .variables
                .get(&(source.key.clone(), name.clone()))
                .ok_or_else(|| ActionError::Missing(format!("variable {name}")))?
                .string(),
        }
    }
    pub(super) fn targets(
        &mut self,
        target: &ElementTarget,
        source: &Source,
        depth: usize,
    ) -> Result<Vec<Vec<usize>>, ActionError> {
        self.step(depth)?;
        let result = match target {
            ElementTarget::SelfTarget => vec![source.path.clone()],
            ElementTarget::LastChild => {
                let node = at(self.root, &source.path).ok_or(ActionError::InvalidSource)?;
                let index = node
                    .children
                    .len()
                    .checked_sub(1)
                    .ok_or_else(|| ActionError::Missing("last child".into()))?;
                let mut path = source.path.clone();
                path.push(index);
                vec![path]
            }
            ElementTarget::Selector(selector) => {
                let text = self.target_text(selector, source)?;
                return self.select(&text, source, depth + 1);
            }
            ElementTarget::ById(reference)
            | ElementTarget::Class(reference)
            | ElementTarget::ChildClass(reference) => {
                let text = self.target_text(reference, source)?;
                let mut found = Vec::new();
                let mut paths: Vec<_> = self.index.values().cloned().collect();
                paths.sort();
                for path in paths {
                    self.step(depth + 1)?;
                    let node = at(self.root, &path).ok_or(ActionError::InvalidSource)?;
                    let matched = match target {
                        ElementTarget::ById(_) => node.str_id.as_ref() == Some(&text),
                        ElementTarget::Class(_) => node.classes.contains(&text),
                        ElementTarget::ChildClass(_) => {
                            path.len() == source.path.len() + 1
                                && path.starts_with(&source.path)
                                && node.classes.contains(&text)
                        }
                        _ => false,
                    };
                    if matched {
                        if found.len() >= self.engine.limits.max_entries {
                            return Err(ActionError::Limit("target matches"));
                        }
                        found.push(path);
                    }
                }
                if matches!(target, ElementTarget::ById(_)) && found.len() > 1 {
                    return Err(ActionError::Invalid("ambiguous string ID".into()));
                }
                found
            }
            ElementTarget::Id(id) => {
                let mut found = Vec::new();
                for path in self.index.values() {
                    if at(self.root, path).is_some_and(|node| node.id == *id) {
                        found.push(path.clone());
                    }
                }
                if found.len() > 1 {
                    return Err(ActionError::Invalid("ambiguous numeric ID".into()));
                }
                found
            }
        };
        if result.is_empty() {
            return Err(ActionError::Missing("action target".into()));
        }
        Ok(result)
    }

    /// CSS subset with explicit rejection of unsupported syntax. Supports selector lists,
    /// ID/class/tag/attribute compound selectors and child/descendant combinators.
    pub(super) fn select(
        &mut self,
        selector: &str,
        source: &Source,
        depth: usize,
    ) -> Result<Vec<Vec<usize>>, ActionError> {
        self.text(selector)?;
        if matches!(selector, "self" | "this" | ":scope") {
            return Ok(vec![source.path.clone()]);
        }
        let groups = parse(selector)?;
        let mut result = Vec::new();
        let mut paths: Vec<_> = self.index.values().cloned().collect();
        paths.sort();
        for path in paths {
            self.step(depth)?;
            let mut matches = false;
            for group in &groups {
                if matches_chain(
                    self.root,
                    &path,
                    group,
                    source,
                    &mut self.steps,
                    &self.engine.limits,
                )? {
                    matches = true;
                    break;
                }
            }
            if matches {
                if result.len() >= self.engine.limits.max_entries {
                    return Err(ActionError::Limit("selector matches"));
                }
                result.push(path);
            }
        }
        if result.is_empty() {
            return Err(ActionError::Missing(format!("selector {selector}")));
        }
        Ok(result)
    }
}

#[derive(Debug)]
struct Simple {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<(String, Option<String>)>,
    scope: bool,
}
#[derive(Debug)]
struct Segment {
    simple: Simple,
    child: bool,
}
// Tokenization is kept together so rejected syntax cannot bypass separator validation.
#[allow(clippy::too_many_lines)]
fn parse(selector: &str) -> Result<Vec<Vec<Segment>>, ActionError> {
    let bad = || ActionError::Selector(selector.into());
    let mut groups = Vec::new();
    // Commas inside attribute values are deliberately rejected instead of misparsed.
    for group in selector.split(',') {
        let mut segments = Vec::new();
        let mut rest = group.trim();
        let mut child = rest.strip_prefix('>').is_some_and(|tail| {
            segments.push(Segment {
                simple: Simple {
                    tag: None,
                    id: None,
                    classes: Vec::new(),
                    attrs: Vec::new(),
                    scope: true,
                },
                child: false,
            });
            rest = tail.trim_start();
            true
        });
        while !rest.is_empty() {
            let mut simple = Simple {
                tag: None,
                id: None,
                classes: Vec::new(),
                attrs: Vec::new(),
                scope: false,
            };
            let initial = rest;
            if let Some(tail) = rest.strip_prefix(":scope") {
                simple.scope = true;
                rest = tail;
            } else if let Some(tail) = rest.strip_prefix('*') {
                rest = tail;
            } else if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                let len = ident_len(rest);
                simple.tag = Some(rest[..len].to_ascii_lowercase());
                rest = &rest[len..];
            }
            loop {
                if rest.starts_with('#') || rest.starts_with('.') {
                    let id = rest.starts_with('#');
                    rest = &rest[1..];
                    let len = ident_len(rest);
                    if len == 0 {
                        return Err(bad());
                    }
                    let value = rest[..len].to_owned();
                    rest = &rest[len..];
                    if id {
                        if simple.id.replace(value).is_some() {
                            return Err(bad());
                        }
                    } else {
                        simple.classes.push(value);
                    }
                } else if let Some(tail) = rest.strip_prefix('[') {
                    let end = tail.find(']').ok_or_else(bad)?;
                    let attr = tail[..end].trim();
                    let (name, value) = if let Some((name, value)) = attr.split_once('=') {
                        let value = value.trim();
                        let value = if value.starts_with('"') && value.ends_with('"')
                            || value.starts_with('\'') && value.ends_with('\'')
                        {
                            &value[1..value.len() - 1]
                        } else {
                            value
                        };
                        (name.trim(), Some(value.to_owned()))
                    } else {
                        (attr, None)
                    };
                    if name.is_empty()
                        || !name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    {
                        return Err(bad());
                    }
                    simple.attrs.push((name.into(), value));
                    rest = &tail[end + 1..];
                } else {
                    break;
                }
            }
            if rest.len() == initial.len() {
                return Err(bad());
            }
            segments.push(Segment { simple, child });
            let had_space = rest.starts_with(char::is_whitespace);
            rest = rest.trim_start();
            if let Some(tail) = rest.strip_prefix('>') {
                rest = tail.trim_start();
                child = true;
                if rest.is_empty() {
                    return Err(bad());
                }
            } else if !rest.is_empty() && !had_space {
                return Err(bad());
            } else {
                child = false;
            }
        }
        if segments.is_empty() {
            return Err(bad());
        }
        groups.push(segments);
    }
    Ok(groups)
}
fn ident_len(text: &str) -> usize {
    text.bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        .count()
}
fn matches_chain(
    root: &Container,
    path: &[usize],
    chain: &[Segment],
    source: &Source,
    steps: &mut usize,
    limits: &ActionLimits,
) -> Result<bool, ActionError> {
    *steps = steps.saturating_add(1);
    if *steps > limits.max_steps || chain.len() > limits.max_depth {
        return Err(ActionError::Limit("selector steps/depth"));
    }
    let Some(last) = chain.last() else {
        return Ok(true);
    };
    let node = at(root, path).ok_or(ActionError::InvalidSource)?;
    if !simple_matches(node, path, &last.simple, source) {
        return Ok(false);
    }
    if chain.len() == 1 {
        return Ok(true);
    }
    if path.is_empty() {
        return Ok(false);
    }
    let rest = &chain[..chain.len() - 1];
    if last.child {
        return matches_chain(root, &path[..path.len() - 1], rest, source, steps, limits);
    }
    for length in (0..path.len()).rev() {
        if matches_chain(root, &path[..length], rest, source, steps, limits)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn simple_matches(node: &Container, path: &[usize], simple: &Simple, source: &Source) -> bool {
    if simple.scope && path != source.path {
        return false;
    }
    if simple
        .id
        .as_ref()
        .is_some_and(|id| node.str_id.as_ref() != Some(id))
    {
        return false;
    }
    if simple
        .classes
        .iter()
        .any(|class| !node.classes.contains(class))
    {
        return false;
    }
    if simple
        .tag
        .as_ref()
        .is_some_and(|tag| tag_name(&node.element) != tag)
    {
        return false;
    }
    simple.attrs.iter().all(|(name, expected)| {
        let value = match name.as_str() {
            "id" => node.str_id.as_deref(),
            name if name.starts_with("data-") => node.data.get(&name[5..]).map(String::as_str),
            _ => None,
        };
        value.is_some_and(|value| expected.as_ref().is_none_or(|e| e == value))
    })
}
const fn tag_name(element: &hyperchad_transformer::Element) -> &'static str {
    use hyperchad_transformer::Element;
    match element {
        Element::Div => "div",
        Element::Raw { .. } | Element::Text { .. } => "#text",
        Element::Aside => "aside",
        Element::Main => "main",
        Element::Header => "header",
        Element::Footer => "footer",
        Element::Section => "section",
        Element::Form { .. } => "form",
        Element::Span => "span",
        Element::Input { .. } => "input",
        Element::Button { .. } => "button",
        Element::Image { .. } => "img",
        Element::Anchor { .. } => "a",
        Element::Heading { .. } => "heading",
        Element::UnorderedList => "ul",
        Element::OrderedList => "ol",
        Element::ListItem => "li",
        Element::Table => "table",
        Element::THead => "thead",
        Element::TH { .. } => "th",
        Element::TBody => "tbody",
        Element::TR => "tr",
        Element::TD { .. } => "td",
        Element::Canvas => "canvas",
        Element::Textarea { .. } => "textarea",
        Element::Details { .. } => "details",
        Element::Summary => "summary",
        Element::Select { .. } => "select",
        Element::Option { .. } => "option",
    }
}
