//! Rust-aware lexical evidence, deliberately without expansion or cfg solving.
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Evidence {
    pub names: BTreeSet<String>,
    pub incomplete: BTreeSet<String>,
    pub advisory: BTreeSet<String>,
    pub uncertain: BTreeSet<String>,
    pub includes: BTreeSet<String>,
    pub modules: BTreeSet<String>,
}

// Keep identifiers (including raw identifiers), ignore ordinary strings/comments,
// and balance delimiters. Counting every identifier is deliberately conservative:
// shadowing or an unexpanded macro may retain a dependency, never justify deletion.
pub(super) fn inspect(text: &str) -> Evidence {
    let mut result = Evidence::default();
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut stack = Vec::new();
    let mut tokens = Vec::new();
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            if bytes[start..].starts_with(b"///") || bytes[start..].starts_with(b"//!") {
                words(&text[start..i], &mut result.names);
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            let start = i;
            let mut depth = 1;
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth > 0 {
                result
                    .incomplete
                    .insert("unterminated block comment".into());
            }
            if bytes[start..].starts_with(b"/**") || bytes[start..].starts_with(b"/*!") {
                words(&text[start..i], &mut result.names);
            }
            continue;
        }
        // Raw string prefixes may be r, br or cr. Do not mistake r#ident for a string.
        let raw_start = if bytes[i..].starts_with(b"br") || bytes[i..].starts_with(b"cr") {
            i + 1
        } else {
            i
        };
        if bytes.get(raw_start) == Some(&b'r') {
            let mut quote = raw_start + 1;
            while bytes.get(quote) == Some(&b'#') {
                quote += 1;
            }
            if bytes.get(quote) == Some(&b'"') {
                let hashes = quote - raw_start - 1;
                let content = quote + 1;
                let mut end = content;
                while end < bytes.len()
                    && !(bytes[end] == b'"'
                        && bytes
                            .get(end + 1..end + 1 + hashes)
                            .is_some_and(|s| s.iter().all(|b| *b == b'#')))
                {
                    end += 1;
                }
                if end == bytes.len() {
                    result.incomplete.insert("unterminated raw string".into());
                    i = end;
                } else {
                    tokens.push(format!("\"{}", &text[content..end]));
                    i = end + hashes + 1;
                }
                continue;
            }
        }
        if bytes[i] == b'"' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i >= bytes.len() {
                result.incomplete.insert("unterminated string".into());
            } else {
                tokens.push(format!("\"{}", &text[start..i]));
                i += 1;
            }
            continue;
        }
        // Distinguish character literals from lifetimes, including escaped chars.
        if bytes[i] == b'\'' {
            let mut end = i + 1;
            if bytes.get(end) == Some(&b'\\') {
                end += 2;
                if bytes.get(end) == Some(&b'{') {
                    while end < bytes.len() && bytes[end] != b'}' {
                        end += 1;
                    }
                    end += 1;
                }
            } else if end < bytes.len() {
                end += text[end..].chars().next().map_or(0, char::len_utf8);
            }
            if bytes.get(end) == Some(&b'\'') {
                i = end + 1;
                continue;
            }
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' || bytes[i] >= 128 {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] >= 128)
            {
                i += 1;
            }
            let name = text[start..i].to_owned();
            result.names.insert(name.clone());
            tokens.push(name);
            continue;
        }
        let token = bytes[i] as char;
        match token {
            '(' | '[' | '{' => stack.push(token),
            ')' | ']' | '}' => {
                let expected = match token {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if stack.pop() != Some(expected) {
                    result
                        .incomplete
                        .insert("unbalanced source delimiters".into());
                }
            }
            _ => {}
        }
        if !token.is_whitespace() {
            tokens.push(token.to_string());
        }
        i += 1;
    }
    if !stack.is_empty() {
        result
            .incomplete
            .insert("unbalanced source delimiters".into());
    }
    let mut explicit_module_path = false;
    let mut macro_delimiters = Vec::new();
    let mut inline_modules: Vec<Option<String>> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        // Token trees are lexical evidence, not declarations in this source file.
        if matches!(token.as_str(), "(" | "[" | "{") {
            let in_macro = macro_delimiters.last().copied().unwrap_or(false)
                || tokens
                    .get(index.wrapping_sub(1))
                    .is_some_and(|previous| previous == "!")
                || (index >= 3 && tokens[index - 3] == "macro_rules");
            macro_delimiters.push(in_macro);
            let inline = if token == "{" && !in_macro && index >= 2 && tokens[index - 2] == "mod" {
                Some(tokens[index - 1].clone())
            } else {
                None
            };
            inline_modules.push(inline);
        }
        let quoted = macro_delimiters.last().copied().unwrap_or(false);
        if matches!(token.as_str(), ")" | "]" | "}") {
            macro_delimiters.pop();
            inline_modules.pop();
        }
        if token == "mod" && !quoted {
            if tokens.get(index + 2).is_some_and(|s| s == ";")
                && !explicit_module_path
                && let Some(name) = tokens.get(index + 1)
            {
                let mut parts = inline_modules
                    .iter()
                    .filter_map(Clone::clone)
                    .collect::<Vec<_>>();
                parts.push(name.clone());
                result.modules.insert(parts.join("/"));
            }
            explicit_module_path = false;
        }
        // Literal include! / include_str! / include_bytes! files are protected too.
        if matches!(token.as_str(), "include" | "include_str" | "include_bytes")
            && tokens.get(index + 1).is_some_and(|s| s == "!")
        {
            if let Some(path) = tokens.get(index + 3).and_then(|s| s.strip_prefix('"')) {
                result.includes.insert(path.to_owned());
            } else if token == "include" {
                result
                    .uncertain
                    .insert("Rust include requires deferred expansion".into());
            } else {
                result
                    .advisory
                    .insert("text/byte include requires deferred expansion (advisory)".into());
            }
        }
        // Attribute-specified module paths and crate overrides are string values.
        let module_path_attribute = !quoted
            && token == "path"
            && tokens[..index]
                .iter()
                .rposition(|token| token == "#")
                .is_some_and(|start| {
                    let attribute = &tokens[start + 1..index];
                    attribute.first().is_some_and(|token| token == "[")
                        && !attribute.iter().any(|token| token == "]")
                        && (attribute.len() == 1
                            || attribute.get(1).is_some_and(|token| token == "cfg_attr"))
                });
        if (module_path_attribute || token == "crate")
            && tokens.get(index + 1).is_some_and(|s| s == "=")
            && let Some(value) = tokens.get(index + 2).and_then(|s| s.strip_prefix('"'))
        {
            if module_path_attribute {
                explicit_module_path = true;
                result.includes.insert(value.to_owned());
            } else {
                words(value, &mut result.names);
            }
        }
        if token == "doc"
            && tokens.get(index + 1).is_some_and(|s| s == "=")
            && let Some(value) = tokens.get(index + 2).and_then(|s| s.strip_prefix('"'))
        {
            words(value, &mut result.names);
        }
    }
    result
}

fn words(text: &str, names: &mut BTreeSet<String>) {
    names.extend(
        text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    );
}
