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

// Tokenization keeps strings separate from identifiers. Structural collection below
// recognizes path roots; only unexpanded macro token trees retain arbitrary names.
pub(super) fn inspect(text: &str) -> Evidence {
    let mut result = Evidence::default();
    let tokens = tokenize(text, &mut result);
    collect_names(&tokens, &mut result.names);
    collect_inventory(&tokens, &mut result);
    result
}

fn tokenize(text: &str, result: &mut Evidence) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut stack = Vec::new();
    let mut tokens = Vec::new();
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
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
            continue;
        }
        if let Some(end) = literal_end(text, i, &mut tokens, result) {
            i = end;
            continue;
        }
        if bytes[i..].starts_with(b"r#")
            && bytes
                .get(i + 2)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_' || *b >= 128)
        {
            i += 2;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' || bytes[i] >= 128 {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] >= 128)
            {
                i += 1;
            }
            let name = text[start..i].to_owned();
            tokens.push(name);
            continue;
        }
        if bytes[i..].starts_with(b"::") {
            tokens.push("::".to_owned());
            i += 2;
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
    tokens
}

fn literal_end(
    text: &str,
    start: usize,
    tokens: &mut Vec<String>,
    result: &mut Evidence,
) -> Option<usize> {
    let bytes = text.as_bytes();
    // Raw string prefixes may be r, br or cr. Do not mistake r#ident for a string.
    let raw_start = if bytes[start..].starts_with(b"br") || bytes[start..].starts_with(b"cr") {
        start + 1
    } else {
        start
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
                return Some(end);
            }
            tokens.push(format!("\"{}", &text[content..end]));
            return Some(end + hashes + 1);
        }
    }
    if bytes[start] == b'"' {
        let content = start + 1;
        let mut end = content;
        while end < bytes.len() && bytes[end] != b'"' {
            if bytes[end] == b'\\' {
                end += 1;
            }
            end += 1;
        }
        if end >= bytes.len() {
            result.incomplete.insert("unterminated string".into());
            return Some(end);
        }
        tokens.push(format!("\"{}", &text[content..end]));
        return Some(end + 1);
    }
    // Distinguish character literals from lifetimes, including escaped chars.
    if bytes[start] == b'\'' {
        let mut end = start + 1;
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
            return Some(end + 1);
        }
    }
    None
}

fn collect_inventory(tokens: &[String], result: &mut Evidence) {
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
    }
}

fn identifier(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
}

fn at(tokens: &[String], index: usize, value: &str) -> bool {
    tokens.get(index).is_some_and(|token| token == value)
}

// A use tree introduces external roots only before its first path segment.
// Braces after a path inherit that path, whereas an outer group starts new roots.
fn import_names(tokens: &[String], start: usize, names: &mut BTreeSet<String>) -> usize {
    let mut groups = Vec::new();
    let mut root = true;
    let mut expecting = true;
    let mut index = start;
    while let Some(token) = tokens.get(index) {
        match token.as_str() {
            ";" => return index + 1,
            "{" => {
                groups.push(root);
                root &= expecting;
                expecting = true;
            }
            "}" => {
                let Some(parent) = groups.pop() else {
                    return index;
                };
                root = parent;
                expecting = false;
            }
            "," => expecting = true,
            "::" => {}
            _ => {
                if root && expecting && identifier(token) {
                    names.insert(token.clone());
                }
                expecting = false;
            }
        }
        index += 1;
    }
    index
}

fn path_tail(tokens: &[String], index: usize) -> bool {
    if index == 0 || !at(tokens, index - 1, "::") {
        return false;
    }
    let left_index = index.wrapping_sub(2);
    tokens.get(left_index).is_some_and(|left| {
        (identifier(left) && !matches!(left.as_str(), "return" | "break" | "yield" | "in" | "else"))
            || (left == ">" && (left_index == 0 || !at(tokens, left_index - 1, "-")))
            || matches!(left.as_str(), ")" | "]" | "::")
    })
}

fn collect_names(tokens: &[String], names: &mut BTreeSet<String>) {
    let mut index = 0;
    let mut macros = Vec::new();
    while let Some(token) = tokens.get(index) {
        if matches!(token.as_str(), "(" | "[" | "{") {
            let quoted = macros.last().copied().unwrap_or(false)
                || (index > 0 && at(tokens, index - 1, "!"))
                || (index >= 3 && at(tokens, index - 3, "macro_rules"));
            macros.push(quoted);
        }
        let quoted = macros.last().copied().unwrap_or(false);
        if !quoted && token == "use" {
            index = import_names(tokens, index + 1, names);
            continue;
        }
        if !quoted
            && token == "extern"
            && at(tokens, index + 1, "crate")
            && let Some(name) = tokens.get(index + 2).filter(|name| identifier(name))
        {
            names.insert(name.clone());
        }
        let path = at(tokens, index + 1, "::");
        let macro_call = at(tokens, index + 1, "!");
        let attribute = index >= 2
            && at(tokens, index - 1, "[")
            && (at(tokens, index - 2, "#")
                || (index >= 3 && at(tokens, index - 2, "!") && at(tokens, index - 3, "#")));
        if identifier(token)
            && (quoted
                || (!path_tail(tokens, index)
                    && (index == 0 || !at(tokens, index - 1, "."))
                    && (path || macro_call || attribute)))
        {
            names.insert(token.clone());
        }
        // Bare derive names are meaningful even without a qualified path. Other
        // attribute arguments (such as cfg flags) are not dependency evidence.
        if token == "derive"
            && at(tokens, index + 1, "(")
            && tokens[..index]
                .iter()
                .rposition(|s| s == "[")
                .is_some_and(|open| {
                    !tokens[open..index].iter().any(|s| s == "]")
                        && open > 0
                        && (at(tokens, open - 1, "#") || at(tokens, open - 1, "!"))
                })
        {
            let mut item = index + 2;
            while let Some(name) = tokens.get(item) {
                if name == ")" {
                    break;
                }
                if identifier(name) && !path_tail(tokens, item) {
                    names.insert(name.clone());
                }
                item += 1;
            }
        }
        if matches!(token.as_str(), ")" | "]" | "}") {
            macros.pop();
        }
        index += 1;
    }
}

fn words(text: &str, names: &mut BTreeSet<String>) {
    names.extend(
        text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    );
}

#[cfg(test)]
mod tests {
    use super::inspect;

    fn references(source: &str, name: &str) -> bool {
        inspect(source)
            .names
            .iter()
            .any(|found| found.eq_ignore_ascii_case(name))
    }

    #[test]
    fn imports_only_retain_external_roots() {
        for source in [
            "use not_futures::futures::stuff;",
            "use not_futures::{futures::{Thing}, other};",
            "pub use { first::{futures}, not_futures::futures::{Thing}, last };",
            "use other as futures;",
            "use { other as futures };",
            "use :futures;",
            "use : :futures;",
            "use x; let futures = 1; futures as usize;",
            "fn futures() {} struct futures; type futures = usize;",
            "fn f() { object.futures::<Thing>(); }",
            "self::futures::run(); crate::futures::run(); super::futures::run();",
        ] {
            assert!(!references(source, "futures"), "{source}");
        }
        for source in [
            "use futures;",
            "use ::futures;",
            "use futures::{self};",
            "use futures::*;",
            "use futures::future;",
            "use futures as renamed;",
            "use {futures as renamed};",
            "pub use {other::{nested}, futures, last};",
            "pub use {other::{Thing as _, nested::*}, futures as renamed, last};",
            "pub use {::futures::{Thing}, other};",
            "use r#futures::Thing;",
            "use {other::{Thing},\n futures\n as\n renamed};",
            "extern crate futures;",
            "extern crate futures as renamed;",
            "futures::run();",
            "::futures::run();",
            "return ::futures::run();",
            "let value: ::futures::Thing;",
            "let value:/* gap */::futures::Thing;",
            "fn f() -> ::futures::Thing {}",
            "<T as futures::Trait>::run();",
            "fn f(_: Option<futures::Thing>) {}",
            "r#futures::run();",
            "Futures::run();",
            "use Futures as renamed;",
        ] {
            assert!(references(source, "futures"), "{source}");
        }
    }

    #[test]
    fn literals_and_all_comment_styles_are_not_references() {
        for source in [
            "// futures::run();",
            "/// futures::run();",
            "//! futures::run();",
            "/* futures::run(); /* nested */ */",
            "/** futures::run(); */",
            "/*! futures::run(); */",
            "#[doc = \"futures::run();\"]",
            "const S: &str = \"futures::run();\";",
            "const S: &str = r###\"futures::run();\"###;",
            "let s = br#\"futures::run();\"#; let t = cr#\"futures::run();\"#;",
            "let s = b\"futures::run();\"; let t = c\"futures::run();\";",
            "call!(\"futures::run();\");",
            "#[cfg(feature = \"futures\")] fn f() {}",
        ] {
            assert!(!references(source, "futures"), "{source}");
        }
        assert!(references(
            "/* gap */ futures /* gap */ :: run();",
            "futures"
        ));
        assert!(!references(
            "let c = 'x'; let s = \"escaped \\\" futures::run()\";",
            "futures"
        ));
    }

    #[test]
    fn attributes_derives_and_macro_tokens_keep_meaningful_evidence() {
        for source in [
            "#[futures::attribute] struct Item;",
            "#[::futures::attribute] struct Item;",
            "#![futures::attribute]",
            "#[derive(futures::Trait)] struct Item;",
            "#[derive(futures)] struct Item;",
            "#[cfg_attr(test, derive(futures))] struct Item;",
            "futures!();",
            "futures::go!{}",
            "quote!(futures);",
            "quote! { not_futures::futures::Thing }",
            "macro_rules! generate { () => { futures } }",
            "#[serde(crate = \"futures\")] struct Item;",
        ] {
            assert!(references(source, "futures"), "{source}");
        }
        for source in [
            "#[other::futures] struct Item;",
            "#[derive(other::futures)] struct Item;",
            "fn derive(futures: usize) {}",
            "#[cfg(futures)] struct Item;",
        ] {
            assert!(!references(source, "futures"), "{source}");
        }
        for source in [
            "#[derive(Serialize, Deserialize)] struct Item;",
            "#[derive(Error)] struct Item;",
        ] {
            let names = inspect(source).names;
            assert!(names.contains("Serialize") || names.contains("Error"));
        }
    }

    #[test]
    fn token_tree_inventory_and_malformed_source_remain_protected() {
        let evidence =
            inspect("quote! { mod missing; } mod real; #[path = \"custom.rs\"] mod custom;");
        assert_eq!(evidence.modules.into_iter().collect::<Vec<_>>(), ["real"]);
        assert!(evidence.includes.contains("custom.rs"));
        assert!(!inspect("dep::go! {").incomplete.is_empty());
        assert!(!inspect("/* unfinished").incomplete.is_empty());
        assert!(!inspect("\"unfinished").incomplete.is_empty());
    }
}
