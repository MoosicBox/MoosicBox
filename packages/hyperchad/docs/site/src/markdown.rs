//! Renderer-agnostic markdown presentation for documentation pages.

use hyperchad::color::Color;
use hyperchad::transformer::{Container, Element, HeaderSize, Number, TextDecoration};
use hyperchad::transformer_models::{
    FontWeight, LayoutDirection, LayoutOverflow, TextDecorationLine, TextDecorationStyle,
    WhiteSpace,
};

use crate::theme::Theme;

/// Theme-driven markdown style policy for documentation pages.
pub struct MarkdownStyle<'a> {
    theme: &'a Theme,
    font_family: &'a str,
}

impl<'a> MarkdownStyle<'a> {
    /// Create a markdown style policy from the docs theme and body font family.
    #[must_use]
    pub const fn new(theme: &'a Theme, font_family: &'a str) -> Self {
        Self { theme, font_family }
    }

    /// Apply this style policy to a rendered markdown container tree.
    pub fn apply(&self, container: &mut Container) {
        self.apply_inner(container, false);
    }

    fn apply_inner(&self, container: &mut Container, in_table_head: bool) {
        let child_in_table_head = in_table_head || matches!(container.element, Element::THead);
        for child in &mut container.children {
            self.apply_inner(child, child_in_table_head);
        }
        self.apply_container(container, in_table_head);
    }

    fn apply_container(&self, container: &mut Container, in_table_head: bool) {
        if has_class(container, "inline-code") {
            self.apply_inline_code(container);
        }

        if has_class(container, "markdown-link") {
            self.apply_link(container);
        }

        match &container.element {
            Element::Div if has_class(container, "markdown-p") => apply_paragraph(container),
            Element::Div if has_class(container, "markdown-code-block") => {
                self.apply_code_block(container);
            }
            Element::Div if has_class(container, "markdown-blockquote") => {
                self.apply_blockquote(container);
            }
            Element::Div if has_class(container, "markdown-hr") => self.apply_rule(container),
            Element::Heading { size } => self.apply_heading(container, *size),
            Element::UnorderedList | Element::OrderedList => apply_list(container),
            Element::ListItem => apply_list_item(container),
            Element::Image { .. } => apply_image(container),
            Element::Input { .. } => apply_task_marker(container),
            Element::Table => self.apply_table(container),
            Element::THead => apply_table_head(container, self.theme.surface),
            Element::TD { .. } if in_table_head => self.apply_table_header_cell(container),
            Element::TH { .. } => self.apply_table_header_cell(container),
            Element::TD { .. } => self.apply_table_cell(container),
            _ => {}
        }
    }

    fn apply_inline_code(&self, container: &mut Container) {
        container.font_family = Some(vec![self.theme.mono_font.to_string()]);
        container.background = Some(self.theme.surface);
        container.color = Some(self.theme.text_primary);
        container.padding_left = Some(Number::from(4));
        container.padding_right = Some(Number::from(4));
        container.padding_top = Some(Number::from(2));
        container.padding_bottom = Some(Number::from(2));
        set_radius(container, 3);
    }

    fn apply_link(&self, container: &mut Container) {
        container.color = Some(self.theme.accent);
        container.text_decoration = Some(TextDecoration {
            color: None,
            line: vec![TextDecorationLine::Underline],
            style: Some(TextDecorationStyle::Solid),
            thickness: None,
        });
    }

    fn apply_code_block(&self, container: &mut Container) {
        container.font_family = Some(vec![self.theme.mono_font.to_string()]);
        container.background = Some(self.theme.background);
        container.color = Some(self.theme.text_primary);
        container.padding_left = Some(Number::from(16));
        container.padding_right = Some(Number::from(16));
        container.padding_top = Some(Number::from(16));
        container.padding_bottom = Some(Number::from(16));
        container.margin_top = Some(Number::from(16));
        container.margin_bottom = Some(Number::from(16));
        container.overflow_x = LayoutOverflow::Scroll;
        container.white_space = Some(WhiteSpace::PreserveWrap);
        set_radius(container, 6);
        set_border(container, self.theme.border, 1);
    }

    fn apply_blockquote(&self, container: &mut Container) {
        container.border_left = Some((self.theme.border, Number::from(4)));
        container.padding_left = Some(Number::from(16));
        container.margin_top = Some(Number::from(16));
        container.margin_bottom = Some(Number::from(16));
        container.color = Some(self.theme.text_muted);
    }

    fn apply_rule(&self, container: &mut Container) {
        container.height = Some(Number::from(1));
        container.background = Some(self.theme.border);
        container.margin_top = Some(Number::from(24));
        container.margin_bottom = Some(Number::from(24));
    }

    fn apply_heading(&self, container: &mut Container, size: HeaderSize) {
        let (margin_top, margin_bottom, font_size) = match size {
            HeaderSize::H1 => (32, 16, 32),
            HeaderSize::H2 => (24, 16, 24),
            HeaderSize::H3 => (24, 16, 20),
            HeaderSize::H4 => (16, 8, 16),
            HeaderSize::H5 => (16, 8, 14),
            HeaderSize::H6 => (16, 8, 13),
        };
        container.color = Some(if matches!(size, HeaderSize::H6) {
            self.theme.text_muted
        } else {
            self.theme.text_primary
        });
        container.font_family = Some(vec![self.theme.mono_font.to_string()]);
        container.font_weight = Some(FontWeight::Bold);
        container.margin_top = Some(Number::from(margin_top));
        container.margin_bottom = Some(Number::from(margin_bottom));
        container.font_size = Some(Number::from(font_size));
    }

    fn apply_table(&self, container: &mut Container) {
        container.margin_top = Some(Number::from(16));
        container.margin_bottom = Some(Number::from(16));
        container.min_width = Some(Number::IntegerPercent(100));
        container.overflow_x = LayoutOverflow::Scroll;
        container.border_top = Some((self.theme.border, Number::from(1)));
        container.border_left = Some((self.theme.border, Number::from(1)));
    }

    fn apply_table_header_cell(&self, container: &mut Container) {
        self.apply_table_cell(container);
        container.background = Some(self.theme.surface);
        container.color = Some(self.theme.text_primary);
        container.font_weight = Some(FontWeight::Bold);
    }

    fn apply_table_cell(&self, container: &mut Container) {
        container.padding_left = Some(Number::from(8));
        container.padding_right = Some(Number::from(8));
        container.padding_top = Some(Number::from(8));
        container.padding_bottom = Some(Number::from(8));
        container.border_right = Some((self.theme.border, Number::from(1)));
        container.border_bottom = Some((self.theme.border, Number::from(1)));
    }

    /// Apply body-level markdown text defaults to the root markdown container.
    pub fn apply_body(&self, container: &mut Container) {
        container.color = Some(self.theme.text_secondary);
        container.font_family = Some(vec![self.font_family.to_string()]);
        container.font_size = Some(Number::from(14));
        container.overflow_x = LayoutOverflow::Scroll;
    }
}

fn apply_paragraph(container: &mut Container) {
    container.margin_bottom = Some(Number::from(16));
}

fn apply_list_item(container: &mut Container) {
    container.margin_bottom = Some(Number::from(4));
}

fn apply_list(container: &mut Container) {
    container.margin_top = Some(Number::from(16));
    container.margin_bottom = Some(Number::from(16));
    container.padding_left = Some(Number::from(32));
    container.direction = LayoutDirection::Column;
}

const fn apply_table_head(container: &mut Container, background: Color) {
    container.background = Some(background);
}

fn apply_image(container: &mut Container) {
    container.max_width = Some(Number::IntegerPercent(100));
}

fn apply_task_marker(container: &mut Container) {
    container.margin_right = Some(Number::from(8));
}

fn set_radius(container: &mut Container, radius: i32) {
    let radius = Number::from(radius);
    container.border_top_left_radius = Some(radius.clone());
    container.border_top_right_radius = Some(radius.clone());
    container.border_bottom_left_radius = Some(radius.clone());
    container.border_bottom_right_radius = Some(radius);
}

fn set_border(container: &mut Container, color: Color, width: i32) {
    let border = (color, Number::from(width));
    container.border_top = Some(border.clone());
    container.border_right = Some(border.clone());
    container.border_bottom = Some(border.clone());
    container.border_left = Some(border);
}

fn has_class(container: &Container, class: &str) -> bool {
    container.classes.iter().any(|value| value == class)
}
