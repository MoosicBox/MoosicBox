//! Syntax highlighting support for code blocks.
//!
//! This module provides syntax highlighting functionality using the `syntect` library.
//! It is only available when the `syntax-highlighting` feature is enabled.

use std::{collections::BTreeMap, sync::LazyLock};

use hyperchad_color::Color;
use hyperchad_transformer::{Container, Element};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
static LANGUAGE_ALIASES: LazyLock<BTreeMap<&'static str, &'static str>> = LazyLock::new(|| {
    BTreeMap::from([
        ("bash", "bash"),
        ("c", "c"),
        ("c++", "cpp"),
        ("cpp", "cpp"),
        ("csharp", "cs"),
        ("css", "css"),
        ("dockerfile", "dockerfile"),
        ("go", "go"),
        ("html", "html"),
        ("java", "java"),
        ("javascript", "js"),
        ("js", "js"),
        ("jsx", "jsx"),
        ("json", "json"),
        ("kt", "kotlin"),
        ("kts", "kotlin"),
        ("kotlin", "kotlin"),
        ("md", "markdown"),
        ("markdown", "markdown"),
        ("patch", "diff"),
        ("py", "python"),
        ("python", "python"),
        ("rb", "ruby"),
        ("rs", "rust"),
        ("ruby", "ruby"),
        ("rust", "rust"),
        ("sh", "bash"),
        ("shell", "bash"),
        ("toml", "toml"),
        ("ts", "ts"),
        ("tsx", "tsx"),
        ("typescript", "ts"),
        ("yaml", "yaml"),
        ("yml", "yaml"),
        ("zsh", "bash"),
    ])
});

/// State for buffering code block content during syntax highlighting.
pub struct CodeBlockState {
    /// The language of the code block (if specified).
    pub language: Option<String>,
    /// Buffered code content.
    pub content: String,
}

/// Normalize a Markdown code-fence info string into a syntax token.
#[must_use]
pub fn normalize_language(language: &str) -> Option<String> {
    let token = language
        .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';' || ch == '{')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();

    if token.is_empty() {
        return None;
    }

    Some(
        LANGUAGE_ALIASES
            .get(token.as_str())
            .copied()
            .unwrap_or(token.as_str())
            .to_owned(),
    )
}

/// Highlights code and returns containers with styled spans.
///
/// Takes source code and an optional language identifier, and produces
/// a vector of `Container` elements with syntax highlighting applied.
/// Each token is wrapped in a `Span` element with the appropriate color
/// based on the syntax highlighting theme.
///
/// If the language is not recognized, falls back to plain text syntax.
///
/// # Panics
///
/// Panics if the built-in `base16-ocean.dark` theme cannot be found in
/// the loaded `syntect` theme set.
#[must_use]
pub fn highlight_code_to_containers(code: &str, language: Option<&str>) -> Vec<Container> {
    let normalized_language = language.and_then(normalize_language);
    let syntax = normalized_language
        .as_deref()
        .and_then(|lang| SYNTAX_SET.find_syntax_by_token(lang))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut containers = Vec::new();
    for line in LinesWithEndings::from(code) {
        if let Ok(ranges) = highlighter.highlight_line(line, &SYNTAX_SET) {
            for (style, text) in ranges {
                containers.push(Container {
                    element: Element::Span,
                    color: Some(Color {
                        r: style.foreground.r,
                        g: style.foreground.g,
                        b: style.foreground.b,
                        a: Some(style.foreground.a),
                    }),
                    children: vec![Container {
                        element: Element::Text {
                            value: text.to_string(),
                        },
                        ..Default::default()
                    }],
                    ..Default::default()
                });
            }
        }
    }
    containers
}

#[cfg(test)]
mod tests {
    use super::normalize_language;

    #[test]
    fn normalizes_language_aliases() {
        assert_eq!(normalize_language("rs"), Some("rust".to_owned()));
        assert_eq!(normalize_language("rust ignore"), Some("rust".to_owned()));
        assert_eq!(normalize_language(".ts"), Some("ts".to_owned()));
        assert_eq!(normalize_language("zsh"), Some("bash".to_owned()));
        assert_eq!(normalize_language(""), None);
    }
}
