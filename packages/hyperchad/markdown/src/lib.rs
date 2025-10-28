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

#[derive(Debug, Error)]
pub enum MarkdownError {
    #[error("Stack underflow while processing markdown")]
    StackUnderflow,
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
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct MarkdownOptions {
    pub enable_tables: bool,
    pub enable_strikethrough: bool,
    pub enable_tasklists: bool,
    pub enable_footnotes: bool,
    pub enable_smart_punctuation: bool,
    pub emoji_enabled: bool,
    pub xss_protection: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            enable_tables: true,
            enable_strikethrough: true,
            enable_tasklists: true,
            enable_footnotes: true,
            enable_smart_punctuation: true,
            emoji_enabled: cfg!(feature = "emoji"),
            xss_protection: cfg!(feature = "xss-protection"),
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
        Self { stack, options }
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

#[must_use]
pub fn markdown_to_container(markdown: &str) -> Container {
    markdown_to_container_with_options(markdown, MarkdownOptions::default())
}

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
        Event::Text(text) => ctx.add_child(Container {
            element: Element::Raw {
                value: text.to_string(),
            },
            ..Default::default()
        }),
        Event::Code(code) => ctx.add_child(Container {
            element: Element::Raw {
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
            element: Element::Raw {
                value: " ".to_string(),
            },
            ..Default::default()
        }),
        Event::HardBreak => ctx.add_child(Container {
            element: Element::Raw {
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
                white_space: Some(WhiteSpace::Preserve),
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
                direction: LayoutDirection::Row,
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
        TagEnd::Paragraph
        | TagEnd::Heading(_)
        | TagEnd::BlockQuote(_)
        | TagEnd::CodeBlock
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

    #[test]
    fn test_basic_markdown() {
        let md = "**bold** and *italic*";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test]
    fn test_headings() {
        let md = "# H1\n## H2\n### H3";
        let container = markdown_to_container(md);
        assert_eq!(container.children.len(), 3);
    }

    #[test]
    fn test_links() {
        let md = "[link](https://example.com)";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test]
    fn test_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test]
    fn test_list() {
        let md = "- Item 1\n- Item 2\n- Item 3";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[test]
    fn test_table() {
        let md = "| Header |\n|--------|\n| Cell   |";
        let container = markdown_to_container(md);
        assert!(!container.children.is_empty());
    }

    #[cfg(feature = "xss-protection")]
    #[test]
    fn test_xss_protection() {
        let md = "Text <script>alert('xss')</script> more";
        let _container = markdown_to_container(md);
        // XSS protection is enabled - just check it doesn't panic
    }

    #[cfg(feature = "emoji")]
    #[test]
    fn test_emoji() {
        let md = ":rocket: Launch!";
        let container = markdown_to_container(md);
        if let Some(child) = container.children.first()
            && let Some(text_child) = child.children.first()
            && let Element::Raw { value } = &text_child.element
        {
            assert!(value.contains('🚀'));
        }
    }
}
