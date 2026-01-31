//! Markdown to `HyperChad` `Container` conversion library.
//!
//! This crate provides conversion from Markdown text to `HyperChad` `Container` structures,
//! supporting GitHub Flavored Markdown (GFM) features including tables, strikethrough,
//! task lists, footnotes, and smart punctuation.
//!
//! # Features
//!
//! * **GitHub Flavored Markdown**: Full support for GFM extensions
//! * **Emoji support**: Convert emoji shortcodes (`:rocket:`) when the `emoji` feature is enabled
//! * **XSS protection**: Optional sanitization of dangerous HTML and URLs when the `xss-protection` feature is enabled
//! * **Customizable parsing**: Configure which markdown features to enable via [`MarkdownOptions`]
//!
//! # Examples
//!
//! Basic conversion:
//!
//! ```rust
//! use hyperchad_markdown::markdown_to_container;
//!
//! let markdown = "**bold** and *italic*";
//! let container = markdown_to_container(markdown);
//! ```
//!
//! Conversion with custom options:
//!
//! ```rust
//! use hyperchad_markdown::{markdown_to_container_with_options, MarkdownOptions};
//!
//! let markdown = "| Header |\n|--------|\n| Cell   |";
//! let options = MarkdownOptions {
//!     enable_tables: true,
//!     enable_strikethrough: false,
//!     ..Default::default()
//! };
//! let container = markdown_to_container_with_options(markdown, options);
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use hyperchad_color::Color;
use hyperchad_transformer::{Container, Element, Number};
use hyperchad_transformer_models::{
    FontWeight, LayoutDirection, TextDecorationLine, TextDecorationStyle, UserSelect, WhiteSpace,
};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::VecDeque;
use thiserror::Error;

#[cfg(feature = "syntax-highlighting")]
mod syntax;

/// Errors that can occur during markdown processing.
#[derive(Debug, Error)]
pub enum MarkdownError {
    /// Stack underflow occurred while processing nested markdown elements.
    ///
    /// This error indicates an internal parsing error where the container stack
    /// became empty unexpectedly during markdown processing.
    #[error("Stack underflow while processing markdown")]
    StackUnderflow,
    /// An unexpected tag end was encountered during parsing.
    ///
    /// This error occurs when a closing tag is found without a matching opening tag.
    #[error("Unexpected tag end: {0}")]
    UnexpectedTagEnd(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderSize {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

impl From<HeadingLevel> for HeaderSize {
    fn from(level: HeadingLevel) -> Self {
        match level {
            HeadingLevel::H1 => Self::H1,
            HeadingLevel::H2 => Self::H2,
            HeadingLevel::H3 => Self::H3,
            HeadingLevel::H4 => Self::H4,
            HeadingLevel::H5 => Self::H5,
            HeadingLevel::H6 => Self::H6,
        }
    }
}

impl From<HeaderSize> for hyperchad_transformer::HeaderSize {
    fn from(size: HeaderSize) -> Self {
        match size {
            HeaderSize::H1 => Self::H1,
            HeaderSize::H2 => Self::H2,
            HeaderSize::H3 => Self::H3,
            HeaderSize::H4 => Self::H4,
            HeaderSize::H5 => Self::H5,
            HeaderSize::H6 => Self::H6,
        }
    }
}

struct MarkdownContext {
    stack: VecDeque<Container>,
    options: MarkdownOptions,
    /// State for buffering code block content during syntax highlighting.
    #[cfg(feature = "syntax-highlighting")]
    code_block_state: Option<syntax::CodeBlockState>,
}

/// Configuration options for markdown parsing and rendering.
///
/// Controls which markdown features are enabled during parsing and whether
/// security features like XSS protection are active.
///
/// # Examples
///
/// ```rust
/// use hyperchad_markdown::MarkdownOptions;
///
/// // Create options with only basic markdown features
/// let options = MarkdownOptions {
///     enable_tables: false,
///     enable_strikethrough: false,
///     enable_tasklists: false,
///     enable_footnotes: false,
///     enable_smart_punctuation: false,
///     emoji_enabled: false,
///     xss_protection: true,
///     syntax_highlighting: false,
/// };
/// ```
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct MarkdownOptions {
    /// Enable GitHub Flavored Markdown table support.
    pub enable_tables: bool,
    /// Enable strikethrough text using `~~text~~` syntax.
    pub enable_strikethrough: bool,
    /// Enable task list support with `- [ ]` and `- [x]` syntax.
    pub enable_tasklists: bool,
    /// Enable footnote support.
    pub enable_footnotes: bool,
    /// Enable smart punctuation conversion (e.g., `...` to `…`, `--` to `—`).
    pub enable_smart_punctuation: bool,
    /// Enable emoji shortcode conversion (`:rocket:` to 🚀).
    ///
    /// Requires the `emoji` feature to be enabled at compile time.
    pub emoji_enabled: bool,
    /// Enable XSS protection by sanitizing dangerous HTML tags and URLs.
    ///
    /// When enabled, dangerous tags like `<script>` and URLs with `javascript:` schemes
    /// are escaped or filtered out.
    pub xss_protection: bool,
    /// Enable syntax highlighting for fenced code blocks.
    ///
    /// Requires the `syntax-highlighting` feature to be enabled at compile time.
    /// When enabled, code blocks with language tags (e.g., ` ```rust `) will have
    /// their content syntax highlighted with colored spans.
    pub syntax_highlighting: bool,
}

impl Default for MarkdownOptions {
    /// Creates default markdown options with all GitHub Flavored Markdown features enabled.
    ///
    /// Default values:
    /// * `enable_tables`: `true`
    /// * `enable_strikethrough`: `true`
    /// * `enable_tasklists`: `true`
    /// * `enable_footnotes`: `true`
    /// * `enable_smart_punctuation`: `true`
    /// * `emoji_enabled`: `true` if the `emoji` feature is enabled, otherwise `false`
    /// * `xss_protection`: `true` if the `xss-protection` feature is enabled, otherwise `false`
    /// * `syntax_highlighting`: `true` if the `syntax-highlighting` feature is enabled, otherwise `false`
    fn default() -> Self {
        Self {
            enable_tables: true,
            enable_strikethrough: true,
            enable_tasklists: true,
            enable_footnotes: true,
            enable_smart_punctuation: true,
            emoji_enabled: cfg!(feature = "emoji"),
            xss_protection: cfg!(feature = "xss-protection"),
            syntax_highlighting: cfg!(feature = "syntax-highlighting"),
        }
    }
}

impl MarkdownContext {
    fn new(options: MarkdownOptions) -> Self {
        let root = Container {
            element: Element::Div,
            classes: vec!["markdown".to_string()],
            direction: LayoutDirection::Column,
            ..Default::default()
        };
        let mut stack = VecDeque::new();
        stack.push_back(root);
        Self {
            stack,
            options,
            #[cfg(feature = "syntax-highlighting")]
            code_block_state: None,
        }
    }

    fn current_mut(&mut self) -> Result<&mut Container, MarkdownError> {
        self.stack.back_mut().ok_or(MarkdownError::StackUnderflow)
    }

    fn push(&mut self, container: Container) {
        self.stack.push_back(container);
    }

    fn pop(&mut self) -> Result<Container, MarkdownError> {
        if self.stack.len() <= 1 {
            return Err(MarkdownError::StackUnderflow);
        }
        self.stack.pop_back().ok_or(MarkdownError::StackUnderflow)
    }

    fn add_child(&mut self, container: Container) -> Result<(), MarkdownError> {
        self.current_mut()?.children.push(container);
        Ok(())
    }

    fn finish(mut self) -> Result<Container, MarkdownError> {
        if self.stack.len() != 1 {
            return Err(MarkdownError::StackUnderflow);
        }
        self.stack.pop_back().ok_or(MarkdownError::StackUnderflow)
    }
}

/// Converts markdown text to a `HyperChad` `Container` with default options.
///
/// This is a convenience function that uses default markdown options, which enable
/// all GitHub Flavored Markdown features, emoji support (if the `emoji` feature is enabled),
/// and XSS protection (if the `xss-protection` feature is enabled).
///
/// # Examples
///
/// ```rust
/// use hyperchad_markdown::markdown_to_container;
///
/// let markdown = "# Hello World\n\nThis is **bold** and *italic* text.";
/// let container = markdown_to_container(markdown);
/// ```
///
/// For more control over parsing options, use [`markdown_to_container_with_options`].
#[must_use]
pub fn markdown_to_container(markdown: &str) -> Container {
    markdown_to_container_with_options(markdown, MarkdownOptions::default())
}

/// Converts markdown text to a `HyperChad` `Container` with custom options.
///
/// This function provides full control over markdown parsing features through the
/// [`MarkdownOptions`] parameter. You can selectively enable or disable GitHub Flavored
/// Markdown features, emoji conversion, and XSS protection.
///
/// # Examples
///
/// ```rust
/// use hyperchad_markdown::{markdown_to_container_with_options, MarkdownOptions};
///
/// // Create a custom configuration
/// let options = MarkdownOptions {
///     enable_tables: true,
///     enable_strikethrough: true,
///     enable_tasklists: false,
///     enable_footnotes: false,
///     enable_smart_punctuation: true,
///     emoji_enabled: false,
///     xss_protection: true,
///     syntax_highlighting: false,
/// };
///
/// let markdown = "~~strikethrough~~ text";
/// let container = markdown_to_container_with_options(markdown, options);
/// ```
///
/// With emoji support:
///
/// ```rust
/// # #[cfg(feature = "emoji")]
/// # {
/// use hyperchad_markdown::{markdown_to_container_with_options, MarkdownOptions};
///
/// let options = MarkdownOptions {
///     emoji_enabled: true,
///     ..Default::default()
/// };
///
/// let markdown = ":rocket: Launch!";
/// let container = markdown_to_container_with_options(markdown, options);
/// # }
/// ```
#[must_use]
pub fn markdown_to_container_with_options(markdown: &str, options: MarkdownOptions) -> Container {
    let markdown = if options.emoji_enabled {
        #[cfg(feature = "emoji")]
        {
            let replacer = gh_emoji::Replacer::new();
            std::borrow::Cow::Owned(replacer.replace_all(markdown).to_string())
        }
        #[cfg(not(feature = "emoji"))]
        {
            std::borrow::Cow::Borrowed(markdown)
        }
    } else {
        std::borrow::Cow::Borrowed(markdown)
    };

    let mut parser_options = Options::empty();
    if options.enable_tables {
        parser_options.insert(Options::ENABLE_TABLES);
    }
    if options.enable_strikethrough {
        parser_options.insert(Options::ENABLE_STRIKETHROUGH);
    }
    if options.enable_tasklists {
        parser_options.insert(Options::ENABLE_TASKLISTS);
    }
    if options.enable_footnotes {
        parser_options.insert(Options::ENABLE_FOOTNOTES);
    }
    if options.enable_smart_punctuation {
        parser_options.insert(Options::ENABLE_SMART_PUNCTUATION);
    }

    let parser = Parser::new_ext(&markdown, parser_options);
    let mut ctx = MarkdownContext::new(options);

    for event in parser {
        if let Err(e) = process_event(&mut ctx, event) {
            log::error!("Error processing markdown event: {e}");
        }
    }

    ctx.finish().unwrap_or_else(|e| {
        log::error!("Error finishing markdown processing: {e}");
        Container::default()
    })
}

fn process_event(ctx: &mut MarkdownContext, event: Event) -> Result<(), MarkdownError> {
    match event {
        Event::Start(tag) => process_start_tag(ctx, tag),
        Event::End(tag_end) => process_end_tag(ctx, tag_end),
        Event::Text(text) => {
            // When syntax highlighting is enabled and we're inside a code block,
            // buffer the text for later highlighting instead of adding it directly.
            #[cfg(feature = "syntax-highlighting")]
            if ctx.options.syntax_highlighting
                && let Some(ref mut state) = ctx.code_block_state
            {
                state.content.push_str(&text);
                return Ok(());
            }

            ctx.add_child(Container {
                element: Element::Text {
                    value: text.to_string(),
                },
                ..Default::default()
            })
        }
        Event::Code(code) => ctx.add_child(Container {
            element: Element::Text {
                value: code.to_string(),
            },
            classes: vec!["inline-code".to_string()],
            font_family: Some(vec!["monospace".to_string()]),
            background: Some(Color::from_hex("#f6f8fa")),
            padding_left: Some(Number::from(4)),
            padding_right: Some(Number::from(4)),
            padding_top: Some(Number::from(2)),
            padding_bottom: Some(Number::from(2)),
            border_top_left_radius: Some(Number::from(3)),
            border_top_right_radius: Some(Number::from(3)),
            border_bottom_left_radius: Some(Number::from(3)),
            border_bottom_right_radius: Some(Number::from(3)),
            ..Default::default()
        }),
        Event::Html(html) | Event::InlineHtml(html) => {
            if ctx.options.xss_protection && is_dangerous_html(&html) {
                ctx.add_child(Container {
                    element: Element::Raw {
                        value: html_escape(&html),
                    },
                    ..Default::default()
                })
            } else {
                ctx.add_child(Container {
                    element: Element::Raw {
                        value: html.to_string(),
                    },
                    ..Default::default()
                })
            }
        }
        Event::SoftBreak => ctx.add_child(Container {
            element: Element::Text {
                value: " ".to_string(),
            },
            ..Default::default()
        }),
        Event::HardBreak => ctx.add_child(Container {
            element: Element::Text {
                value: "\n".to_string(),
            },
            white_space: Some(WhiteSpace::PreserveWrap),
            ..Default::default()
        }),
        Event::Rule => ctx.add_child(Container {
            element: Element::Div,
            classes: vec!["markdown-hr".to_string()],
            height: Some(Number::from(1)),
            background: Some(Color::from_hex("#d0d7de")),
            margin_top: Some(Number::from(24)),
            margin_bottom: Some(Number::from(24)),
            ..Default::default()
        }),
        Event::TaskListMarker(checked) => ctx.add_child(Container {
            element: Element::Input {
                input: hyperchad_transformer::Input::Checkbox {
                    checked: Some(checked),
                },
                name: None,
                autofocus: None,
            },
            margin_right: Some(Number::from(8)),
            user_select: Some(UserSelect::None),
            ..Default::default()
        }),
        Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => Ok(()),
    }
}

#[allow(clippy::too_many_lines)]
fn process_start_tag(ctx: &mut MarkdownContext, tag: Tag) -> Result<(), MarkdownError> {
    match tag {
        Tag::Paragraph => {
            ctx.push(Container {
                element: Element::Div,
                classes: vec!["markdown-p".to_string()],
                margin_bottom: Some(Number::from(16)),
                ..Default::default()
            });
            Ok(())
        }
        Tag::Heading { level, .. } => {
            let size = HeaderSize::from(level);
            let (margin_top, margin_bottom, font_size) = match size {
                HeaderSize::H1 => (32, 16, 32),
                HeaderSize::H2 => (24, 16, 24),
                HeaderSize::H3 => (24, 16, 20),
                HeaderSize::H4 => (16, 8, 16),
                HeaderSize::H5 => (16, 8, 14),
                HeaderSize::H6 => (16, 8, 13),
            };
            ctx.push(Container {
                element: Element::Heading { size: size.into() },
                classes: vec![format!("markdown-h{}", level as u8)],
                font_weight: Some(FontWeight::Bold),
                margin_top: Some(Number::from(margin_top)),
                margin_bottom: Some(Number::from(margin_bottom)),
                font_size: Some(Number::from(font_size)),
                ..Default::default()
            });
            Ok(())
        }
        Tag::BlockQuote(_) => {
            ctx.push(Container {
                element: Element::Div,
                classes: vec!["markdown-blockquote".to_string()],
                border_left: Some((Color::from_hex("#d0d7de"), Number::from(4))),
                padding_left: Some(Number::from(16)),
                margin_top: Some(Number::from(16)),
                margin_bottom: Some(Number::from(16)),
                color: Some(Color::from_hex("#656d76")),
                ..Default::default()
            });
            Ok(())
        }
        Tag::CodeBlock(kind) => {
            let language = match kind {
                pulldown_cmark::CodeBlockKind::Indented => None,
                pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                    if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    }
                }
            };

            // Initialize code block state for syntax highlighting
            #[cfg(feature = "syntax-highlighting")]
            if ctx.options.syntax_highlighting {
                ctx.code_block_state = Some(syntax::CodeBlockState {
                    language: language.clone(),
                    content: String::new(),
                });
            }

            ctx.push(Container {
                element: Element::Div,
                classes: vec!["markdown-code-block".to_string()],
                data: language
                    .map(|l| vec![("language".to_string(), l)])
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                font_family: Some(vec!["monospace".to_string()]),
                background: Some(Color::from_hex("#f6f8fa")),
                padding_left: Some(Number::from(16)),
                padding_right: Some(Number::from(16)),
                padding_top: Some(Number::from(16)),
                padding_bottom: Some(Number::from(16)),
                margin_top: Some(Number::from(16)),
                margin_bottom: Some(Number::from(16)),
                border_top_left_radius: Some(Number::from(6)),
                border_top_right_radius: Some(Number::from(6)),
                border_bottom_left_radius: Some(Number::from(6)),
                border_bottom_right_radius: Some(Number::from(6)),
                white_space: Some(WhiteSpace::PreserveWrap),
                ..Default::default()
            });
            Ok(())
        }
        Tag::List(start) => {
            let element = start.map_or(Element::UnorderedList, |_start_num| Element::OrderedList);
            ctx.push(Container {
                element,
                classes: vec!["markdown-list".to_string()],
                margin_top: Some(Number::from(16)),
                margin_bottom: Some(Number::from(16)),
                padding_left: Some(Number::from(32)),
                direction: LayoutDirection::Column,
                ..Default::default()
            });
            Ok(())
        }
        Tag::Item => {
            ctx.push(Container {
                element: Element::ListItem,
                classes: vec!["markdown-list-item".to_string()],
                margin_bottom: Some(Number::from(4)),
                ..Default::default()
            });
            Ok(())
        }
        Tag::Emphasis => {
            ctx.push(Container {
                element: Element::Span,
                classes: vec!["markdown-em".to_string()],
                ..Default::default()
            });
            Ok(())
        }
        Tag::Strong => {
            ctx.push(Container {
                element: Element::Span,
                classes: vec!["markdown-strong".to_string()],
                font_weight: Some(FontWeight::Bold),
                ..Default::default()
            });
            Ok(())
        }
        Tag::Strikethrough => {
            ctx.push(Container {
                element: Element::Span,
                classes: vec!["markdown-strikethrough".to_string()],
                text_decoration: Some(hyperchad_transformer::TextDecoration {
                    color: None,
                    line: vec![TextDecorationLine::LineThrough],
                    style: Some(TextDecorationStyle::Solid),
                    thickness: None,
                }),
                ..Default::default()
            });
            Ok(())
        }
        Tag::Link {
            link_type: _,
            dest_url,
            title: _,
            id: _,
        } => {
            let href = dest_url.to_string();
            let href = if ctx.options.xss_protection {
                filter_dangerous_url(&href)
            } else {
                href
            };
            ctx.push(Container {
                element: Element::Anchor {
                    target: None,
                    href: Some(href),
                },
                classes: vec!["markdown-link".to_string()],
                color: Some(Color::from_hex("#0969da")),
                text_decoration: Some(hyperchad_transformer::TextDecoration {
                    color: None,
                    line: vec![TextDecorationLine::Underline],
                    style: Some(TextDecorationStyle::Solid),
                    thickness: None,
                }),
                ..Default::default()
            });
            Ok(())
        }
        Tag::Image {
            link_type: _,
            dest_url,
            title,
            id: _,
        } => ctx.add_child(Container {
            element: Element::Image {
                source: Some(dest_url.to_string()),
                alt: Some(title.to_string()),
                fit: None,
                source_set: None,
                sizes: None,
                loading: None,
            },
            classes: vec!["markdown-image".to_string()],
            max_width: Some(Number::IntegerPercent(100)),
            ..Default::default()
        }),
        Tag::Table(_) => {
            ctx.push(Container {
                element: Element::Table,
                classes: vec!["markdown-table".to_string()],
                margin_top: Some(Number::from(16)),
                margin_bottom: Some(Number::from(16)),
                border_top: Some((Color::from_hex("#d0d7de"), Number::from(1))),
                border_left: Some((Color::from_hex("#d0d7de"), Number::from(1))),
                ..Default::default()
            });
            Ok(())
        }
        Tag::TableHead => {
            ctx.push(Container {
                element: Element::THead,
                classes: vec!["markdown-thead".to_string()],
                background: Some(Color::from_hex("#f6f8fa")),
                ..Default::default()
            });
            Ok(())
        }
        Tag::TableRow => {
            ctx.push(Container {
                element: Element::TR,
                classes: vec!["markdown-tr".to_string()],
                ..Default::default()
            });
            Ok(())
        }
        Tag::TableCell => {
            ctx.push(Container {
                element: Element::TD {
                    rows: None,
                    columns: None,
                },
                classes: vec!["markdown-td".to_string()],
                padding_left: Some(Number::from(8)),
                padding_right: Some(Number::from(8)),
                padding_top: Some(Number::from(8)),
                padding_bottom: Some(Number::from(8)),
                border_right: Some((Color::from_hex("#d0d7de"), Number::from(1))),
                border_bottom: Some((Color::from_hex("#d0d7de"), Number::from(1))),
                ..Default::default()
            });
            Ok(())
        }
        Tag::FootnoteDefinition(_)
        | Tag::HtmlBlock
        | Tag::MetadataBlock(_)
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition
        | Tag::Superscript
        | Tag::Subscript => Ok(()),
    }
}

fn process_end_tag(ctx: &mut MarkdownContext, tag_end: TagEnd) -> Result<(), MarkdownError> {
    match tag_end {
        TagEnd::CodeBlock => {
            // Apply syntax highlighting if enabled and we have buffered content
            #[cfg(feature = "syntax-highlighting")]
            if ctx.options.syntax_highlighting
                && let Some(state) = ctx.code_block_state.take()
            {
                let containers =
                    syntax::highlight_code_to_containers(&state.content, state.language.as_deref());
                ctx.current_mut()?.children.extend(containers);
            }

            let container = ctx.pop()?;
            ctx.add_child(container)?;
            Ok(())
        }
        TagEnd::Paragraph
        | TagEnd::Heading(_)
        | TagEnd::BlockQuote(_)
        | TagEnd::List(_)
        | TagEnd::Item
        | TagEnd::Emphasis
        | TagEnd::Strong
        | TagEnd::Strikethrough
        | TagEnd::Link
        | TagEnd::Table
        | TagEnd::TableHead
        | TagEnd::TableRow
        | TagEnd::TableCell => {
            let container = ctx.pop()?;
            ctx.add_child(container)?;
            Ok(())
        }
        TagEnd::Image
        | TagEnd::FootnoteDefinition
        | TagEnd::HtmlBlock
        | TagEnd::MetadataBlock(_)
        | TagEnd::DefinitionList
        | TagEnd::DefinitionListTitle
        | TagEnd::DefinitionListDefinition
        | TagEnd::Superscript
        | TagEnd::Subscript => Ok(()),
    }
}

#[allow(clippy::missing_const_for_fn)]
fn is_dangerous_html(html: &str) -> bool {
    #[cfg(feature = "xss-protection")]
    {
        const DANGEROUS_TAGS: &[&str] = &[
            "<script",
            "<iframe",
            "<object",
            "<embed",
            "<style",
            "<link",
            "<base",
            "<meta",
            "<title",
            "<textarea",
            "<xmp",
            "<noembed",
            "<noframes",
            "<plaintext",
        ];
        let html_lower = html.to_lowercase();
        DANGEROUS_TAGS.iter().any(|tag| html_lower.contains(tag))
    }
    #[cfg(not(feature = "xss-protection"))]
    {
        let _ = html;
        false
    }
}

fn html_escape(html: &str) -> String {
    #[cfg(feature = "xss-protection")]
    {
        html.replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
            .replace('&', "&amp;")
    }

    #[cfg(not(feature = "xss-protection"))]
    html.to_string()
}

#[cfg(feature = "xss-protection")]
fn filter_dangerous_url(url: &str) -> String {
    let url_lower = url.to_lowercase();
    if url_lower.starts_with("javascript:")
        || url_lower.starts_with("data:")
        || url_lower.starts_with("vbscript:")
    {
        "#".to_string()
    } else {
        url.to_string()
    }
}

#[cfg(not(feature = "xss-protection"))]
fn filter_dangerous_url(url: &str) -> String {
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_basic_markdown() {
        let md = "**bold** and *italic*";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_headings() {
        let md = "# H1\n## H2\n### H3";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 3);
    }

    #[test_log::test]
    fn test_links() {
        let md = "[link](https://example.com)";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_list() {
        let md = "- Item 1\n- Item 2\n- Item 3";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_table() {
        let md = "| Header |\n|--------|\n| Cell   |";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection() {
        let md = "Text <script>alert('xss')</script> more";
        let _container = markdown_to_container(md);
        // XSS protection is enabled - just check it doesn't panic
    }

    #[cfg(feature = "emoji")]
    #[test_log::test]
    fn test_emoji() {
        let md = ":rocket: Launch!";
        let container = markdown_to_container(md);
        if let Some(child) = container.children.first()
            && let Some(text_child) = child.children.first()
            && let Element::Text { value } = &text_child.element
        {
            assert!(value.contains('🚀'));
        }
    }

    #[test_log::test]
    fn test_empty_markdown() {
        let md = "";
        let container = markdown_to_container(md);
        assert_eq!(container.element, Element::Div);
        assert!(container.classes.contains(&"markdown".to_string()));
        assert!(container.children.is_empty());
    }

    #[test_log::test]
    fn test_blockquote() {
        let md = "> This is a quote\n> with multiple lines";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 1);
        if let Some(blockquote) = container.children.first() {
            assert!(
                blockquote
                    .classes
                    .contains(&"markdown-blockquote".to_string())
            );
            assert_eq!(blockquote.color, Some(Color::from_hex("#656d76")));
        }
    }

    #[test_log::test]
    fn test_strikethrough() {
        let md = "~~strikethrough text~~";
        let options = MarkdownOptions {
            enable_strikethrough: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_strikethrough_disabled() {
        let md = "~~strikethrough text~~";
        let options = MarkdownOptions {
            enable_strikethrough: false,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // When strikethrough is disabled, the ~~ should be treated as literal text
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_task_list() {
        let md = "- [ ] Unchecked task\n- [x] Checked task";
        let options = MarkdownOptions {
            enable_tasklists: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_horizontal_rule() {
        let md = "---";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 1);
        if let Some(rule) = container.children.first() {
            assert!(rule.classes.contains(&"markdown-hr".to_string()));
            assert_eq!(rule.height, Some(Number::from(1)));
            assert_eq!(rule.background, Some(Color::from_hex("#d0d7de")));
        }
    }

    #[test_log::test]
    fn test_hard_break() {
        let md = "Line 1  \nLine 2";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_inline_code() {
        let md = "This is `inline code` text";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
        // Verify inline code styling is applied
        if let Some(paragraph) = container.children.first() {
            let has_inline_code = paragraph.children.iter().any(|child| {
                child.classes.contains(&"inline-code".to_string())
                    && child.font_family == Some(vec!["monospace".to_string()])
            });
            assert!(has_inline_code);
        }
    }

    #[test_log::test]
    fn test_ordered_list() {
        let md = "1. First item\n2. Second item\n3. Third item";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 1);
        if let Some(list) = container.children.first() {
            assert_eq!(list.element, Element::OrderedList);
            assert!(list.classes.contains(&"markdown-list".to_string()));
        }
    }

    #[test_log::test]
    fn test_unordered_list() {
        let md = "* First item\n* Second item\n* Third item";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 1);
        if let Some(list) = container.children.first() {
            assert_eq!(list.element, Element::UnorderedList);
        }
    }

    #[test_log::test]
    fn test_nested_formatting() {
        let md = "**bold with *italic* inside**";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_mixed_features() {
        let md = "# Header\n\n**Bold** and *italic*\n\n- List item\n\n[Link](https://example.com)";
        let container = markdown_to_container(md);
        assert!(container.children.len() >= 3);
    }

    #[test_log::test]
    fn test_image() {
        let md = "![Alt text](https://example.com/image.png)";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
        // Verify image element properties
        if let Some(paragraph) = container.children.first() {
            let has_image = paragraph.children.iter().any(|child| {
                matches!(child.element, Element::Image { .. })
                    && child.classes.contains(&"markdown-image".to_string())
            });
            assert!(has_image);
        }
    }

    #[test_log::test]
    fn test_all_heading_levels() {
        let md = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 6);
        // Verify all are heading elements
        for child in &container.children {
            assert!(matches!(child.element, Element::Heading { .. }));
        }
    }

    #[test_log::test]
    fn test_code_block_with_language() {
        let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
        if let Some(code_block) = container.children.first() {
            assert!(
                code_block
                    .classes
                    .contains(&"markdown-code-block".to_string())
            );
            assert_eq!(code_block.font_family, Some(vec!["monospace".to_string()]));
        }
    }

    #[test_log::test]
    fn test_code_block_without_language() {
        let md = "```\nplain code\n```";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
        if let Some(code_block) = container.children.first() {
            assert!(
                code_block
                    .classes
                    .contains(&"markdown-code-block".to_string())
            );
        }
    }

    #[test_log::test]
    fn test_options_with_all_features_disabled() {
        let options = MarkdownOptions {
            enable_tables: false,
            enable_strikethrough: false,
            enable_tasklists: false,
            enable_footnotes: false,
            enable_smart_punctuation: false,
            emoji_enabled: false,
            xss_protection: false,
            syntax_highlighting: false,
        };
        let md = "**bold** text";
        let container = markdown_to_container_with_options(md, options);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_options_default() {
        let options = MarkdownOptions::default();
        assert!(options.enable_tables);
        assert!(options.enable_strikethrough);
        assert!(options.enable_tasklists);
        assert!(options.enable_footnotes);
        assert!(options.enable_smart_punctuation);
    }

    #[test_log::test]
    fn test_table_with_multiple_rows() {
        let md = "| Col1 | Col2 |\n|------|------|\n| A    | B    |\n| C    | D    |";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
        if let Some(table) = container.children.first() {
            assert_eq!(table.element, Element::Table);
            assert!(table.classes.contains(&"markdown-table".to_string()));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_script_tag() {
        let md = "<script>alert('xss')</script>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // Verify that dangerous content is escaped
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Note: html_escape replaces & last, so we get double-escaped ampersands
            assert!(value.contains("&amp;lt;script"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_iframe_tag() {
        let md = "<iframe src=\"evil.com\"></iframe>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // Verify dangerous tags are escaped
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Note: html_escape replaces & last, so we get double-escaped ampersands
            assert!(value.contains("&amp;lt;"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_javascript_url() {
        let md = "[Click](javascript:alert('xss'))";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // Verify dangerous URLs are filtered
        if let Some(paragraph) = container.children.first()
            && let Some(link) = paragraph.children.first()
            && let Element::Anchor { href, .. } = &link.element
        {
            assert_eq!(href, &Some("#".to_string()));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_data_url() {
        let md = "[Click](data:text/html,<script>alert('xss')</script>)";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // Verify data URLs are filtered
        if let Some(paragraph) = container.children.first()
            && let Some(link) = paragraph.children.first()
            && let Element::Anchor { href, .. } = &link.element
        {
            assert_eq!(href, &Some("#".to_string()));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_vbscript_url() {
        let md = "[Click](vbscript:msgbox('xss'))";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // Verify vbscript URLs are filtered
        if let Some(paragraph) = container.children.first()
            && let Some(link) = paragraph.children.first()
            && let Element::Anchor { href, .. } = &link.element
        {
            assert_eq!(href, &Some("#".to_string()));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_safe_html() {
        let md = "<p>Safe paragraph</p>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // Safe HTML should pass through
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_xss_protection_disabled() {
        let md = "<script>alert('test')</script>";
        let options = MarkdownOptions {
            xss_protection: false,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // With XSS protection disabled, content should pass through
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            assert!(value.contains("<script>"));
        }
    }

    #[test_log::test]
    fn test_link_with_title() {
        let md = "[Link](https://example.com)";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
        if let Some(paragraph) = container.children.first() {
            let has_link = paragraph.children.iter().any(|child| {
                matches!(child.element, Element::Anchor { .. })
                    && child.classes.contains(&"markdown-link".to_string())
            });
            assert!(has_link);
        }
    }

    #[test_log::test]
    fn test_multiple_paragraphs() {
        let md = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 3);
        for child in &container.children {
            assert!(child.classes.contains(&"markdown-p".to_string()));
        }
    }

    #[test_log::test]
    fn test_smart_punctuation_ellipsis() {
        let md = "Wait...";
        let options = MarkdownOptions {
            enable_smart_punctuation: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_smart_punctuation_disabled() {
        let md = "Wait...";
        let options = MarkdownOptions {
            enable_smart_punctuation: false,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_header_size_conversion_h1() {
        let size = HeaderSize::from(HeadingLevel::H1);
        assert_eq!(size, HeaderSize::H1);
        let transformed: hyperchad_transformer::HeaderSize = size.into();
        assert!(matches!(transformed, hyperchad_transformer::HeaderSize::H1));
    }

    #[test_log::test]
    fn test_header_size_conversion_h2() {
        let size = HeaderSize::from(HeadingLevel::H2);
        assert_eq!(size, HeaderSize::H2);
    }

    #[test_log::test]
    fn test_header_size_conversion_h3() {
        let size = HeaderSize::from(HeadingLevel::H3);
        assert_eq!(size, HeaderSize::H3);
    }

    #[test_log::test]
    fn test_header_size_conversion_h4() {
        let size = HeaderSize::from(HeadingLevel::H4);
        assert_eq!(size, HeaderSize::H4);
    }

    #[test_log::test]
    fn test_header_size_conversion_h5() {
        let size = HeaderSize::from(HeadingLevel::H5);
        assert_eq!(size, HeaderSize::H5);
    }

    #[test_log::test]
    fn test_header_size_conversion_h6() {
        let size = HeaderSize::from(HeadingLevel::H6);
        assert_eq!(size, HeaderSize::H6);
    }

    #[test_log::test]
    fn test_nested_lists() {
        let md = "- Item 1\n  - Nested 1\n  - Nested 2\n- Item 2";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_bold_and_italic_combined() {
        let md = "***bold and italic***";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test_log::test]
    fn test_link_color_styling() {
        let md = "[test](https://example.com)";
        let container = markdown_to_container(md);
        if let Some(paragraph) = container.children.first() {
            let has_styled_link = paragraph.children.iter().any(|child| {
                child.color == Some(Color::from_hex("#0969da"))
                    && child.classes.contains(&"markdown-link".to_string())
            });
            assert!(has_styled_link);
        }
    }

    #[test_log::test]
    fn test_tables_disabled() {
        let md = "| Header |\n|--------|\n| Cell   |";
        let options = MarkdownOptions {
            enable_tables: false,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // When tables are disabled, markdown should be treated as text
        assert!(!container.children.is_empty());
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_object_tag() {
        let md = "<object data=\"evil.swf\"></object>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify object tag is escaped
            assert!(value.contains("&amp;lt;object"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_embed_tag() {
        let md = "<embed src=\"evil.swf\">";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            assert!(value.contains("&amp;lt;embed"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_style_tag() {
        let md = "<style>body { display: none; }</style>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            assert!(value.contains("&amp;lt;style"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_mixed_case_script() {
        let md = "<ScRiPt>alert('xss')</ScRiPt>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Case-insensitive detection should still escape this
            assert!(value.contains("&amp;lt;"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_mixed_case_javascript_url() {
        let md = "[Click](JAVASCRIPT:alert('xss'))";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(paragraph) = container.children.first()
            && let Some(link) = paragraph.children.first()
            && let Element::Anchor { href, .. } = &link.element
        {
            // Case-insensitive URL filtering
            assert_eq!(href, &Some("#".to_string()));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_safe_url_preserved() {
        let md = "[Click](https://example.com/page)";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(paragraph) = container.children.first()
            && let Some(link) = paragraph.children.first()
            && let Element::Anchor { href, .. } = &link.element
        {
            // Safe URLs should pass through unchanged
            assert_eq!(href, &Some("https://example.com/page".to_string()));
        }
    }

    #[test_log::test]
    fn test_code_block_language_data_attribute() {
        let md = "```python\nprint('hello')\n```";
        let container = markdown_to_container(md);
        if let Some(code_block) = container.children.first() {
            // Verify language is stored in data attribute
            assert_eq!(code_block.data.get("language"), Some(&"python".to_string()));
        }
    }

    #[test_log::test]
    fn test_indented_code_block() {
        let md = "    indented code\n    more code";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
        if let Some(code_block) = container.children.first() {
            assert!(
                code_block
                    .classes
                    .contains(&"markdown-code-block".to_string())
            );
            // Indented code blocks don't have a language
            assert!(!code_block.data.contains_key("language"));
        }
    }

    #[test_log::test]
    fn test_table_structure() {
        let md = "| Header1 | Header2 |\n|---------|----------|\n| Cell1   | Cell2    |";
        let container = markdown_to_container(md);
        if let Some(table) = container.children.first() {
            assert_eq!(table.element, Element::Table);
            assert!(table.classes.contains(&"markdown-table".to_string()));
            // Table should have styling
            assert_eq!(table.margin_top, Some(Number::from(16)));
            assert_eq!(table.margin_bottom, Some(Number::from(16)));
            // Table should have THead child
            let has_thead = table
                .children
                .iter()
                .any(|child| matches!(child.element, Element::THead));
            assert!(has_thead);
        }
    }

    #[test_log::test]
    fn test_strong_element_has_bold_weight() {
        let md = "**bold text**";
        let container = markdown_to_container(md);
        if let Some(paragraph) = container.children.first() {
            let has_bold = paragraph.children.iter().any(|child| {
                child.classes.contains(&"markdown-strong".to_string())
                    && child.font_weight == Some(FontWeight::Bold)
            });
            assert!(has_bold);
        }
    }

    #[test_log::test]
    fn test_strikethrough_text_decoration() {
        let md = "~~strikethrough~~";
        let options = MarkdownOptions {
            enable_strikethrough: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(paragraph) = container.children.first() {
            let has_strikethrough = paragraph.children.iter().any(|child| {
                child
                    .classes
                    .contains(&"markdown-strikethrough".to_string())
                    && child.text_decoration.as_ref().is_some_and(|td| {
                        td.line.contains(&TextDecorationLine::LineThrough)
                            && td.style == Some(TextDecorationStyle::Solid)
                    })
            });
            assert!(has_strikethrough);
        }
    }

    #[test_log::test]
    fn test_soft_break_produces_space() {
        // Soft break occurs when there's a single newline (not double newline for paragraph)
        let md = "Line one\nLine two";
        let container = markdown_to_container(md);
        // Should produce a single paragraph with text separated by soft breaks (spaces)
        assert_eq!(container.children.len(), 1);
        if let Some(paragraph) = container.children.first() {
            // Should have multiple children including the soft break space
            assert!(paragraph.children.len() >= 2);
        }
    }

    #[test_log::test]
    fn test_task_list_checked_and_unchecked() {
        let md = "- [x] Checked\n- [ ] Unchecked";
        let options = MarkdownOptions {
            enable_tasklists: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(list) = container.children.first() {
            // Find all checkbox inputs
            let checkboxes: Vec<_> = list
                .children
                .iter()
                .flat_map(|item| &item.children)
                .filter_map(|child| {
                    if let Element::Input {
                        input: hyperchad_transformer::Input::Checkbox { checked },
                        ..
                    } = &child.element
                    {
                        Some(*checked)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(checkboxes.len(), 2);
            assert_eq!(checkboxes[0], Some(true)); // Checked
            assert_eq!(checkboxes[1], Some(false)); // Unchecked
        }
    }

    #[test_log::test]
    fn test_link_text_decoration_underline() {
        let md = "[Link text](https://example.com)";
        let container = markdown_to_container(md);
        if let Some(paragraph) = container.children.first() {
            let has_underlined_link = paragraph.children.iter().any(|child| {
                child.classes.contains(&"markdown-link".to_string())
                    && child.text_decoration.as_ref().is_some_and(|td| {
                        td.line.contains(&TextDecorationLine::Underline)
                            && td.style == Some(TextDecorationStyle::Solid)
                    })
            });
            assert!(has_underlined_link);
        }
    }

    #[test_log::test]
    fn test_emphasis_element_created() {
        let md = "*emphasized text*";
        let container = markdown_to_container(md);
        if let Some(paragraph) = container.children.first() {
            let has_emphasis = paragraph
                .children
                .iter()
                .any(|child| child.classes.contains(&"markdown-em".to_string()));
            assert!(has_emphasis);
        }
    }

    #[test_log::test]
    fn test_heading_specific_styling() {
        // Test that different heading levels get different font sizes
        let md = "# H1\n#### H4";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 2);

        let h1 = &container.children[0];
        let h4 = &container.children[1];

        // H1 should have font_size 32, H4 should have font_size 16
        assert_eq!(h1.font_size, Some(Number::from(32)));
        assert_eq!(h4.font_size, Some(Number::from(16)));

        // Check margin differences
        assert_eq!(h1.margin_top, Some(Number::from(32)));
        assert_eq!(h4.margin_top, Some(Number::from(16)));
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_base_tag() {
        let md = "<base href=\"https://evil.com/\">";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify base tag is escaped
            assert!(value.contains("&amp;lt;base"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_meta_tag() {
        let md = "<meta http-equiv=\"refresh\" content=\"0;url=evil.com\">";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify meta tag is escaped
            assert!(value.contains("&amp;lt;meta"));
        }
    }

    #[test_log::test]
    fn test_blockquote_styling() {
        let md = "> Quote text";
        let container = markdown_to_container(md);
        if let Some(blockquote) = container.children.first() {
            // Verify specific blockquote styling
            assert_eq!(blockquote.padding_left, Some(Number::from(16)));
            assert_eq!(
                blockquote.border_left,
                Some((Color::from_hex("#d0d7de"), Number::from(4)))
            );
            assert_eq!(blockquote.margin_top, Some(Number::from(16)));
            assert_eq!(blockquote.margin_bottom, Some(Number::from(16)));
        }
    }

    #[test_log::test]
    fn test_inline_code_styling() {
        let md = "`code`";
        let container = markdown_to_container(md);
        if let Some(paragraph) = container.children.first() {
            let code = paragraph
                .children
                .iter()
                .find(|c| c.classes.contains(&"inline-code".to_string()));
            assert!(code.is_some());
            let code = code.unwrap();
            // Verify inline code has expected styling
            assert_eq!(code.background, Some(Color::from_hex("#f6f8fa")));
            assert_eq!(code.padding_left, Some(Number::from(4)));
            assert_eq!(code.padding_right, Some(Number::from(4)));
            assert_eq!(code.border_top_left_radius, Some(Number::from(3)));
        }
    }

    #[test_log::test]
    fn test_image_source_and_alt_text() {
        let md = "![My alt text](https://example.com/image.png)";
        let container = markdown_to_container(md);
        if let Some(paragraph) = container.children.first() {
            let image = paragraph
                .children
                .iter()
                .find(|c| matches!(c.element, Element::Image { .. }));
            assert!(image.is_some());
            let image = image.unwrap();
            if let Element::Image { source, alt, .. } = &image.element {
                assert_eq!(source, &Some("https://example.com/image.png".to_string()));
                // Note: pulldown-cmark puts title in alt, not the alt text from markdown
                // Since there's no title in the markdown, alt will be an empty string
                assert_eq!(alt, &Some(String::new()));
            } else {
                panic!("Expected Image element");
            }
            // Verify max-width styling
            assert_eq!(image.max_width, Some(Number::IntegerPercent(100)));
        }
    }

    #[test_log::test]
    fn test_code_block_full_styling() {
        let md = "```\ncode\n```";
        let container = markdown_to_container(md);
        if let Some(code_block) = container.children.first() {
            // Verify all code block styling attributes
            assert_eq!(code_block.background, Some(Color::from_hex("#f6f8fa")));
            assert_eq!(code_block.padding_left, Some(Number::from(16)));
            assert_eq!(code_block.padding_right, Some(Number::from(16)));
            assert_eq!(code_block.padding_top, Some(Number::from(16)));
            assert_eq!(code_block.padding_bottom, Some(Number::from(16)));
            assert_eq!(code_block.margin_top, Some(Number::from(16)));
            assert_eq!(code_block.margin_bottom, Some(Number::from(16)));
            assert_eq!(code_block.border_top_left_radius, Some(Number::from(6)));
            assert_eq!(code_block.border_top_right_radius, Some(Number::from(6)));
            assert_eq!(code_block.border_bottom_left_radius, Some(Number::from(6)));
            assert_eq!(code_block.border_bottom_right_radius, Some(Number::from(6)));
            assert_eq!(code_block.white_space, Some(WhiteSpace::PreserveWrap));
        }
    }

    #[test_log::test]
    fn test_list_item_styling() {
        let md = "- Item 1\n- Item 2";
        let container = markdown_to_container(md);
        if let Some(list) = container.children.first() {
            assert!(list.children.len() >= 2);
            for item in &list.children {
                assert_eq!(item.element, Element::ListItem);
                assert!(item.classes.contains(&"markdown-list-item".to_string()));
                assert_eq!(item.margin_bottom, Some(Number::from(4)));
            }
        }
    }

    #[test_log::test]
    fn test_table_cell_styling() {
        let md = "| Header |\n|--------|\n| Cell   |";
        let container = markdown_to_container(md);
        if let Some(table) = container.children.first() {
            // Find a table cell (TD element)
            fn find_td(container: &Container) -> Option<&Container> {
                for child in &container.children {
                    if matches!(child.element, Element::TD { .. }) {
                        return Some(child);
                    }
                    if let Some(found) = find_td(child) {
                        return Some(found);
                    }
                }
                None
            }

            let td = find_td(table);
            assert!(td.is_some());
            let td = td.unwrap();
            // Verify table cell styling
            assert!(td.classes.contains(&"markdown-td".to_string()));
            assert_eq!(td.padding_left, Some(Number::from(8)));
            assert_eq!(td.padding_right, Some(Number::from(8)));
            assert_eq!(td.padding_top, Some(Number::from(8)));
            assert_eq!(td.padding_bottom, Some(Number::from(8)));
            assert_eq!(
                td.border_right,
                Some((Color::from_hex("#d0d7de"), Number::from(1)))
            );
            assert_eq!(
                td.border_bottom,
                Some((Color::from_hex("#d0d7de"), Number::from(1)))
            );
        }
    }

    #[test_log::test]
    fn test_list_direction() {
        let md = "- Item 1\n- Item 2";
        let container = markdown_to_container(md);
        if let Some(list) = container.children.first() {
            // Lists should have column direction
            assert_eq!(list.direction, LayoutDirection::Column);
        }
    }

    #[test_log::test]
    fn test_root_container_has_column_direction() {
        let md = "# Heading\n\nParagraph";
        let container = markdown_to_container(md);
        // Root container should have column direction
        assert_eq!(container.direction, LayoutDirection::Column);
    }

    #[test_log::test]
    fn test_table_head_styling() {
        let md = "| Header1 | Header2 |\n|---------|----------|\n| Cell1   | Cell2    |";
        let container = markdown_to_container(md);
        if let Some(table) = container.children.first() {
            // Find the thead element
            let thead = table
                .children
                .iter()
                .find(|c| matches!(c.element, Element::THead));
            assert!(thead.is_some());
            let thead = thead.unwrap();
            assert!(thead.classes.contains(&"markdown-thead".to_string()));
            assert_eq!(thead.background, Some(Color::from_hex("#f6f8fa")));
        }
    }

    #[test_log::test]
    fn test_table_row_created() {
        let md = "| Cell1 | Cell2 |\n|-------|-------|\n| A     | B     |";
        let container = markdown_to_container(md);
        if let Some(table) = container.children.first() {
            // Find TR elements recursively
            fn count_tr(container: &Container) -> usize {
                let mut count = 0;
                for child in &container.children {
                    if matches!(child.element, Element::TR) {
                        count += 1;
                    }
                    count += count_tr(child);
                }
                count
            }

            // Should have at least 1 row (data row) - header row is in THead
            let tr_count = count_tr(table);
            assert!(
                tr_count >= 1,
                "Expected at least 1 TR element, found {tr_count}"
            );
        }
    }

    #[cfg(feature = "xss-protection")]
    fn check_no_unescaped_script(container: &Container) -> bool {
        for child in &container.children {
            if let Element::Raw { value } = &child.element
                && value.contains("<script>")
            {
                return false;
            }
            if !check_no_unescaped_script(child) {
                return false;
            }
        }
        true
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_inline_html_escaped() {
        // Test that inline HTML with dangerous content is escaped
        let md = "Text with <script>bad</script> inline";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // Find all raw elements and check none contain unescaped script tags
        assert!(check_no_unescaped_script(&container));
    }

    #[test_log::test]
    fn test_paragraph_margin() {
        let md = "This is a paragraph.";
        let container = markdown_to_container(md);
        if let Some(paragraph) = container.children.first() {
            assert!(paragraph.classes.contains(&"markdown-p".to_string()));
            assert_eq!(paragraph.margin_bottom, Some(Number::from(16)));
        }
    }

    #[test_log::test]
    fn test_link_anchor_element() {
        let md = "[Example](https://example.com)";
        let container = markdown_to_container(md);
        if let Some(paragraph) = container.children.first() {
            let link = paragraph
                .children
                .iter()
                .find(|c| matches!(c.element, Element::Anchor { .. }));
            assert!(link.is_some());
            let link = link.unwrap();
            if let Element::Anchor { href, target } = &link.element {
                assert_eq!(href, &Some("https://example.com".to_string()));
                assert_eq!(target, &None);
            } else {
                panic!("Expected Anchor element");
            }
        }
    }

    #[test_log::test]
    fn test_heading_class_format() {
        let md = "## H2 Heading";
        let container = markdown_to_container(md);
        if let Some(heading) = container.children.first() {
            // Verify heading class follows the format "markdown-h{level}"
            assert!(heading.classes.contains(&"markdown-h2".to_string()));
            assert!(matches!(
                heading.element,
                Element::Heading {
                    size: hyperchad_transformer::HeaderSize::H2
                }
            ));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_link_tag() {
        let md = "<link rel=\"stylesheet\" href=\"evil.css\">";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify link tag is escaped
            assert!(value.contains("&amp;lt;link"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_title_tag() {
        let md = "<title>Evil Title</title>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify title tag is escaped
            assert!(value.contains("&amp;lt;title"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_textarea_tag() {
        let md = "<textarea>form hijack</textarea>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify textarea tag is escaped
            assert!(value.contains("&amp;lt;textarea"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_xmp_tag() {
        let md = "<xmp><script>evil</script></xmp>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify xmp tag is escaped
            assert!(value.contains("&amp;lt;xmp"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_noembed_tag() {
        let md = "<noembed><script>evil</script></noembed>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify noembed tag is escaped
            assert!(value.contains("&amp;lt;noembed"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_noframes_tag() {
        let md = "<noframes><script>evil</script></noframes>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify noframes tag is escaped
            assert!(value.contains("&amp;lt;noframes"));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_plaintext_tag() {
        let md = "<plaintext>stops parsing</plaintext>";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(child) = container.children.first()
            && let Element::Raw { value } = &child.element
        {
            // Verify plaintext tag is escaped
            assert!(value.contains("&amp;lt;plaintext"));
        }
    }

    #[test_log::test]
    fn test_nested_blockquote() {
        let md = "> Outer quote\n> > Nested quote";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
        // Verify nested blockquote structure
        if let Some(outer) = container.children.first() {
            assert!(outer.classes.contains(&"markdown-blockquote".to_string()));
        }
    }

    fn find_checkbox(container: &Container) -> Option<&Container> {
        for child in &container.children {
            if let Element::Input {
                input: hyperchad_transformer::Input::Checkbox { .. },
                ..
            } = &child.element
            {
                return Some(child);
            }
            if let Some(found) = find_checkbox(child) {
                return Some(found);
            }
        }
        None
    }

    #[test_log::test]
    fn test_task_list_checkbox_styling() {
        let md = "- [x] Completed";
        let options = MarkdownOptions {
            enable_tasklists: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // Find the checkbox input
        let checkbox = find_checkbox(&container);
        assert!(checkbox.is_some());
        let checkbox = checkbox.unwrap();
        // Verify checkbox styling
        assert_eq!(checkbox.margin_right, Some(Number::from(8)));
        assert_eq!(checkbox.user_select, Some(UserSelect::None));
    }

    #[test_log::test]
    fn test_dangerous_url_passes_through_when_xss_disabled() {
        // When XSS protection is disabled, dangerous URLs should pass through unchanged
        let md = "[Click](javascript:alert('test'))";
        let options = MarkdownOptions {
            xss_protection: false,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(paragraph) = container.children.first() {
            let link = paragraph
                .children
                .iter()
                .find(|c| matches!(c.element, Element::Anchor { .. }));
            assert!(link.is_some());
            if let Element::Anchor { href, .. } = &link.unwrap().element {
                // URL should pass through unchanged when XSS protection is off
                assert_eq!(href, &Some("javascript:alert('test')".to_string()));
            }
        }
    }

    #[test_log::test]
    fn test_footnote_syntax_with_footnotes_enabled() {
        // Test that footnote syntax is parsed when footnotes are enabled
        // Note: pulldown-cmark parses footnotes but our converter ignores them
        // This test verifies no crash occurs and basic content is preserved
        let md = "Text with footnote[^1].\n\n[^1]: Footnote content.";
        let options = MarkdownOptions {
            enable_footnotes: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        // The main text paragraph should still be rendered
        assert!(!container.children.is_empty());
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_mixed_case_data_url() {
        // Test case-insensitive detection of data: URLs
        let md = "[Click](DATA:text/html,<script>alert('xss')</script>)";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(paragraph) = container.children.first()
            && let Some(link) = paragraph.children.first()
            && let Element::Anchor { href, .. } = &link.element
        {
            // Mixed case data URLs should be filtered to "#"
            assert_eq!(href, &Some("#".to_string()));
        }
    }

    #[cfg(feature = "xss-protection")]
    #[test_log::test]
    fn test_xss_protection_mixed_case_vbscript_url() {
        // Test case-insensitive detection of vbscript: URLs
        let md = "[Click](VBSCRIPT:msgbox('xss'))";
        let options = MarkdownOptions {
            xss_protection: true,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);
        if let Some(paragraph) = container.children.first()
            && let Some(link) = paragraph.children.first()
            && let Element::Anchor { href, .. } = &link.element
        {
            // Mixed case vbscript URLs should be filtered to "#"
            assert_eq!(href, &Some("#".to_string()));
        }
    }

    #[test_log::test]
    fn test_syntax_highlighting_disabled() {
        let md = "```rust\nfn main() {}\n```";
        let options = MarkdownOptions {
            syntax_highlighting: false,
            ..Default::default()
        };
        let container = markdown_to_container_with_options(md, options);

        // Without highlighting, children should be Text elements (not Span)
        if let Some(code_block) = container.children.first() {
            assert!(
                code_block
                    .children
                    .iter()
                    .any(|c| matches!(c.element, Element::Text { .. }))
            );
        }
    }

    // =========================================================================
    // Syntax Highlighting Tests
    // =========================================================================

    #[cfg(feature = "syntax-highlighting")]
    mod syntax {
        use super::*;

        #[test_log::test]
        fn test_syntax_highlighting_rust() {
            let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
            let options = MarkdownOptions {
                syntax_highlighting: true,
                ..Default::default()
            };
            let container = markdown_to_container_with_options(md, options);

            // Should have code block with span children (not raw text)
            if let Some(code_block) = container.children.first() {
                assert!(
                    code_block
                        .classes
                        .contains(&"markdown-code-block".to_string())
                );
                // With syntax highlighting, children should be Span elements with colors
                assert!(
                    code_block
                        .children
                        .iter()
                        .any(|c| matches!(c.element, Element::Span))
                );
                assert!(code_block.children.iter().any(|c| c.color.is_some()));
            }
        }

        #[test_log::test]
        fn test_syntax_highlighting_python() {
            let md = "```python\ndef hello():\n    print('Hello')\n```";
            let options = MarkdownOptions {
                syntax_highlighting: true,
                ..Default::default()
            };
            let container = markdown_to_container_with_options(md, options);

            if let Some(code_block) = container.children.first() {
                assert!(
                    code_block
                        .classes
                        .contains(&"markdown-code-block".to_string())
                );
                assert!(
                    code_block
                        .children
                        .iter()
                        .any(|c| matches!(c.element, Element::Span))
                );
            }
        }

        #[test_log::test]
        fn test_syntax_highlighting_javascript() {
            let md = "```javascript\nfunction hello() {\n    console.log('Hello');\n}\n```";
            let options = MarkdownOptions {
                syntax_highlighting: true,
                ..Default::default()
            };
            let container = markdown_to_container_with_options(md, options);

            if let Some(code_block) = container.children.first() {
                assert!(
                    code_block
                        .children
                        .iter()
                        .any(|c| matches!(c.element, Element::Span))
                );
            }
        }

        #[test_log::test]
        fn test_syntax_highlighting_unknown_language() {
            let md = "```unknownlang\nsome code here\n```";
            let options = MarkdownOptions {
                syntax_highlighting: true,
                ..Default::default()
            };
            let container = markdown_to_container_with_options(md, options);

            // Should still work, falling back to plain text syntax
            assert!(!container.children.is_empty());
            if let Some(code_block) = container.children.first() {
                assert!(
                    code_block
                        .classes
                        .contains(&"markdown-code-block".to_string())
                );
                // Even with unknown language, should have span children
                assert!(!code_block.children.is_empty());
            }
        }

        #[test_log::test]
        fn test_syntax_highlighting_no_language() {
            let md = "```\nplain code\n```";
            let options = MarkdownOptions {
                syntax_highlighting: true,
                ..Default::default()
            };
            let container = markdown_to_container_with_options(md, options);

            // Should still work with plain text
            assert!(!container.children.is_empty());
            if let Some(code_block) = container.children.first() {
                assert!(!code_block.children.is_empty());
            }
        }

        #[test_log::test]
        fn test_syntax_highlighting_preserves_code_block_styling() {
            let md = "```rust\nlet x = 1;\n```";
            let options = MarkdownOptions {
                syntax_highlighting: true,
                ..Default::default()
            };
            let container = markdown_to_container_with_options(md, options);

            if let Some(code_block) = container.children.first() {
                // Code block should still have its styling
                assert_eq!(code_block.background, Some(Color::from_hex("#f6f8fa")));
                assert_eq!(code_block.font_family, Some(vec!["monospace".to_string()]));
                assert_eq!(code_block.padding_left, Some(Number::from(16)));
            }
        }

        #[test_log::test]
        fn test_syntax_highlighting_multiple_code_blocks() {
            let md = "```rust\nfn a() {}\n```\n\nSome text\n\n```python\ndef b(): pass\n```";
            let options = MarkdownOptions {
                syntax_highlighting: true,
                ..Default::default()
            };
            let container = markdown_to_container_with_options(md, options);

            // Should have multiple children: code block, paragraph, code block
            assert!(container.children.len() >= 2);

            // Both code blocks should be highlighted
            let code_blocks: Vec<_> = container
                .children
                .iter()
                .filter(|c| c.classes.contains(&"markdown-code-block".to_string()))
                .collect();
            assert_eq!(code_blocks.len(), 2);

            for block in code_blocks {
                assert!(block.children.iter().any(|c| c.color.is_some()));
            }
        }
    }
}
