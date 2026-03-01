//! Syntax highlighting support for code blocks.
//!
//! This module provides syntax highlighting functionality using the `syntect` library.
//! It is only available when the `syntax-highlighting` feature is enabled.

use std::sync::LazyLock;

use hyperchad_color::Color;
use hyperchad_transformer::{Container, Element};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// State for buffering code block content during syntax highlighting.
///
/// This struct holds the intermediate state while parsing a fenced code block,
/// accumulating the code content and storing the optional language identifier
/// for later syntax highlighting.
pub struct CodeBlockState {
    /// The language of the code block (if specified).
    pub language: Option<String>,
    /// Buffered code content.
    pub content: String,
}

/// Highlights code and returns containers with styled spans.
///
/// Takes source code and an optional language identifier, and produces
/// a vector of `Container` elements with syntax highlighting applied.
/// Each token is wrapped in a `Span` element with the appropriate color
/// based on the syntax highlighting theme.
///
/// If the language is not recognized, falls back to plain text syntax.
#[must_use]
pub fn highlight_code_to_containers(code: &str, language: Option<&str>) -> Vec<Container> {
    let syntax = language
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
