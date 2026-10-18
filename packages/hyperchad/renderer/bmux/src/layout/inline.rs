//! Inline formatting with grapheme-safe styling and original UTF-8 source maps.

use std::ops::Range;

use bmux_tui::text_width::display_width;
use unicode_segmentation::UnicodeSegmentation;

use super::{
    CellRect, Color, Container, Draft, Edges, Element, Engine, FontWeight, LayoutOptions,
    LeafMeasure, LogicalSize, MAX_EXTENT, Modifier, OverflowWrap, Style, TextAlign,
    TextDecorationLine, TextOverflow, Visibility, WhiteSpace, child_path, distributed, hidden,
    node_key, signed,
};

/// Inherited terminal text policy. Font size never changes cell advance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextStyle {
    pub style: Style,
    pub white_space: WhiteSpace,
    pub overflow_wrap: OverflowWrap,
    pub text_overflow: TextOverflow,
    pub text_align: TextAlign,
    pub visible: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            style: Style::default(),
            white_space: WhiteSpace::Normal,
            overflow_wrap: OverflowWrap::Normal,
            text_overflow: TextOverflow::Clip,
            text_align: TextAlign::Start,
            visible: true,
        }
    }
}

impl TextStyle {
    #[must_use]
    pub fn inherit(&self, node: &Container) -> Self {
        let mut result = self.clone();
        if let Some(color) = node.color {
            result.style.fg = Some(Color::Rgb(color.r, color.g, color.b));
        }
        if let Some(color) = node.background {
            result.style.bg = Some(Color::Rgb(color.r, color.g, color.b));
        }
        if let Some(weight) = node.font_weight {
            let bold = matches!(
                weight,
                FontWeight::SemiBold
                    | FontWeight::Bold
                    | FontWeight::ExtraBold
                    | FontWeight::Black
                    | FontWeight::Bolder
                    | FontWeight::Weight600
                    | FontWeight::Weight700
                    | FontWeight::Weight800
                    | FontWeight::Weight900
            ) || (weight != FontWeight::Lighter
                && matches!(weight, FontWeight::Medium | FontWeight::Weight500)
                && self.style.modifiers.contains(Modifier::BOLD));
            result.style.modifiers = if bold {
                result.style.modifiers.union(Modifier::BOLD)
            } else {
                result.style.modifiers.difference(Modifier::BOLD)
            };
        } else if matches!(node.element, Element::Heading { .. } | Element::TH { .. }) {
            result.style.modifiers = result.style.modifiers.union(Modifier::BOLD);
        }
        if let Some(decoration) = &node.text_decoration {
            for line in &decoration.line {
                match line {
                    TextDecorationLine::None => {
                        result.style.modifiers = result
                            .style
                            .modifiers
                            .difference(Modifier::UNDERLINE | Modifier::CROSSED_OUT);
                    }
                    TextDecorationLine::Underline | TextDecorationLine::Overline => {
                        result.style.modifiers |= Modifier::UNDERLINE;
                    }
                    TextDecorationLine::LineThrough => {
                        result.style.modifiers |= Modifier::CROSSED_OUT;
                    }
                    TextDecorationLine::Inherit => {}
                }
            }
        }
        if let Some(value) = node.white_space {
            result.white_space = value;
        }
        if let Some(value) = node.overflow_wrap {
            result.overflow_wrap = value;
        }
        if let Some(value) = node.text_overflow {
            result.text_overflow = value;
        }
        if let Some(value) = node.text_align {
            result.text_align = value;
        }
        if let Some(value) = node.visibility {
            result.visible = value == Visibility::Visible;
        }
        result
    }
}

/// Original UTF-8 bytes contributing to a displayed grapheme.
/// A grapheme crossing a style boundary may have multiple source ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRange {
    pub key: String,
    pub path: Vec<usize>,
    pub bytes: Range<usize>,
}

/// A terminal-atomic styled grapheme (or an expanded tab/collapsed space).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFragment {
    pub text: String,
    pub bounds: CellRect,
    pub style: Style,
    pub visible: bool,
    pub sources: Vec<SourceRange>,
}

pub(super) struct InlineSource<'a> {
    pub source: &'a Container,
    pub path: Vec<usize>,
    pub style: TextStyle,
}

pub(super) struct Formatted<'a> {
    pub size: LogicalSize,
    pub fragments: Vec<InlineFragment>,
    pub sources: Vec<InlineSource<'a>>,
}

#[derive(Clone)]
struct Glyph {
    text: String,
    width: u64,
    style: TextStyle,
    sources: Vec<SourceRange>,
}
impl Glyph {
    fn whitespace(&self) -> bool {
        self.text.chars().all(char::is_whitespace) && self.text != "\n"
    }
    fn collapsible(&self) -> bool {
        self.style.white_space == WhiteSpace::Normal && self.whitespace()
    }
    fn break_after(&self) -> bool {
        self.whitespace() || self.text == "-" || self.text == "\u{200b}"
    }
}

struct Character {
    bytes: Range<usize>,
    style: TextStyle,
    sources: Vec<SourceRange>,
}

fn collect<'a>(
    node: &'a Container,
    path: &[usize],
    inherited: &TextStyle,
    sources: &mut Vec<InlineSource<'a>>,
    raw: &mut String,
    characters: &mut Vec<Character>,
    previous_cr: &mut bool,
) {
    if hidden(node) {
        return;
    }
    let style = inherited.inherit(node);
    sources.push(InlineSource {
        source: node,
        path: path.into(),
        style: style.clone(),
    });
    if let Element::Text { value } | Element::Raw { value } = &node.element {
        for (index, character) in value.char_indices() {
            let source = SourceRange {
                key: node_key(node, path),
                path: path.into(),
                bytes: index..index + character.len_utf8(),
            };
            if character == '\n' && *previous_cr {
                if let Some(last) = characters.last_mut() {
                    last.sources.push(source);
                }
                *previous_cr = false;
                continue;
            }
            *previous_cr = character == '\r';
            let character = if character == '\r' {
                '\n'
            } else if character.is_control() && !matches!(character, '\n' | '\t') {
                '\u{fffd}'
            } else {
                character
            };
            let collapse = style.white_space == WhiteSpace::Normal
                && matches!(character, ' ' | '\t' | '\n' | '\u{000c}');
            if collapse
                && raw.ends_with(' ')
                && characters
                    .last()
                    .is_some_and(|last| last.style.white_space == WhiteSpace::Normal)
            {
                characters
                    .last_mut()
                    .expect("previous space")
                    .sources
                    .push(source);
                continue;
            }
            let start = raw.len();
            raw.push(if collapse { ' ' } else { character });
            characters.push(Character {
                bytes: start..raw.len(),
                style: style.clone(),
                sources: vec![source],
            });
        }
    }
    for (index, child) in node.children.iter().enumerate() {
        collect(
            child,
            &child_path(path, index),
            &style,
            sources,
            raw,
            characters,
            previous_cr,
        );
    }
}

fn glyphs(raw: &str, characters: &[Character]) -> Vec<Glyph> {
    let mut cursor = 0;
    raw.grapheme_indices(true)
        .map(|(start, text)| {
            let end = start + text.len();
            while cursor < characters.len() && characters[cursor].bytes.end <= start {
                cursor += 1;
            }
            let mut sources = Vec::new();
            let mut index = cursor;
            while index < characters.len() && characters[index].bytes.start < end {
                for source in &characters[index].sources {
                    if let Some(last) = sources.last_mut() {
                        let last: &mut SourceRange = last;
                        let previous_end = last.bytes.end;
                        let next_start = source.bytes.start;
                        if last.path == source.path && previous_end == next_start {
                            last.bytes.end = source.bytes.end;
                            continue;
                        }
                    }
                    sources.push(source.clone());
                }
                index += 1;
            }
            Glyph {
                text: text.into(),
                width: display_width(text) as u64,
                style: characters[cursor].style.clone(),
                sources,
            }
        })
        .collect()
}

fn trim_start(line: &mut Vec<Glyph>) {
    let count = line.iter().take_while(|glyph| glyph.collapsible()).count();
    line.drain(..count);
}
fn trim_end(line: &mut Vec<Glyph>) {
    while line.last().is_some_and(Glyph::collapsible) {
        line.pop();
    }
}
fn line_width(line: &[Glyph]) -> u64 {
    line.iter().map(|glyph| glyph.width).sum()
}

fn wrap(glyphs: Vec<Glyph>, width: u64, tab_width: u16) -> Vec<Vec<Glyph>> {
    let had_text = !glyphs.is_empty();
    let mut rows = Vec::new();
    let mut line = Vec::<Glyph>::new();
    let mut ended_newline = false;
    for mut glyph in glyphs {
        ended_newline = glyph.text == "\n";
        if ended_newline {
            trim_end(&mut line);
            rows.push(std::mem::take(&mut line));
            continue;
        }
        if line.is_empty() && glyph.collapsible() {
            continue;
        }
        if glyph.text == "\t" {
            glyph.width = u64::from(tab_width) - line_width(&line) % u64::from(tab_width);
            glyph.text = " ".repeat(usize::try_from(glyph.width).unwrap_or(usize::MAX));
        }
        if glyph.style.white_space != WhiteSpace::Preserve
            && line_width(&line).saturating_add(glyph.width) > width
            && !line.is_empty()
        {
            let break_at = line.iter().rposition(Glyph::break_after);
            if let Some(index) = break_at {
                let mut tail = line.split_off(index + 1);
                trim_end(&mut line);
                rows.push(std::mem::take(&mut line));
                trim_start(&mut tail);
                line = tail;
            }
            if line_width(&line).saturating_add(glyph.width) > width
                && !line.is_empty()
                && matches!(
                    glyph.style.overflow_wrap,
                    OverflowWrap::Anywhere | OverflowWrap::BreakWord
                )
            {
                trim_end(&mut line);
                rows.push(std::mem::take(&mut line));
            }
        }
        if line.is_empty() && glyph.collapsible() {
            continue;
        }
        line.push(glyph);
    }
    trim_end(&mut line);
    if !line.is_empty() || ended_newline || (had_text && rows.is_empty()) {
        rows.push(line);
    }
    rows
}

fn ellipsize(row: &mut Vec<Glyph>, width: u64, policy: &TextStyle) {
    let mut used = 0;
    let mut count = 0;
    for glyph in row.iter() {
        if used + glyph.width > width - 1 {
            break;
        }
        used += glyph.width;
        count += 1;
    }
    let omitted = row[count..]
        .iter()
        .flat_map(|glyph| glyph.sources.clone())
        .collect();
    row.truncate(count);
    row.push(Glyph {
        text: "…".into(),
        width: 1,
        style: policy.clone(),
        sources: omitted,
    });
}

pub(super) fn format<'a>(
    group: &[(&'a Container, Vec<usize>)],
    width: u64,
    inherited: &TextStyle,
    options: &LayoutOptions,
) -> Formatted<'a> {
    let mut raw = String::new();
    let mut characters = Vec::new();
    let mut sources = Vec::new();
    let mut previous_cr = false;
    for (source, path) in group {
        collect(
            source,
            path,
            inherited,
            &mut sources,
            &mut raw,
            &mut characters,
            &mut previous_cr,
        );
    }
    let rows = wrap(glyphs(&raw, &characters), width, options.tab_width);
    let height = rows.len() as u64;
    let mut fragments = Vec::new();
    let mut measured_width = 0;
    for (row_index, mut row) in rows.into_iter().enumerate() {
        let natural = line_width(&row);
        measured_width = measured_width.max(natural);
        let policy = row.first().map_or(inherited, |glyph| &glyph.style).clone();
        if policy.text_overflow == TextOverflow::Ellipsis
            && natural > width
            && policy.white_space == WhiteSpace::Preserve
            && width > 0
        {
            ellipsize(&mut row, width, &policy);
        }
        let used = line_width(&row);
        let spare = width.saturating_sub(used);
        let mut x = if width == MAX_EXTENT {
            0
        } else {
            match policy.text_align {
                TextAlign::Center => spare / 2,
                TextAlign::End => spare,
                _ => 0,
            }
        };
        let spaces = row.iter().filter(|glyph| glyph.collapsible()).count();
        let justify = width != MAX_EXTENT
            && policy.text_align == TextAlign::Justify
            && u64::try_from(row_index + 1).unwrap_or(u64::MAX) < height
            && spaces > 0;
        let mut space_index = 0;
        for glyph in row {
            let extra = if justify && glyph.collapsible() {
                space_index += 1;
                distributed(spare, space_index, spaces)
                    - distributed(spare, space_index - 1, spaces)
            } else {
                0
            };
            let text = if extra > 0 {
                format!(
                    "{}{}",
                    glyph.text,
                    " ".repeat(usize::try_from(extra).unwrap_or(usize::MAX))
                )
            } else {
                glyph.text
            };
            fragments.push(InlineFragment {
                text,
                bounds: CellRect::new(
                    signed(x),
                    i64::try_from(row_index).unwrap_or(i64::MAX),
                    glyph.width + extra,
                    1,
                ),
                style: glyph.style.style,
                visible: glyph.style.visible,
                sources: glyph.sources,
            });
            x += glyph.width + extra;
        }
    }
    Formatted {
        size: LogicalSize::new(measured_width, height),
        fragments,
        sources,
    }
}

pub(super) fn plain_size(
    text: &str,
    width: u64,
    white_space: WhiteSpace,
    overflow_wrap: OverflowWrap,
    tab_width: u16,
) -> LogicalSize {
    let container = Container {
        element: Element::Text { value: text.into() },
        white_space: Some(white_space),
        overflow_wrap: Some(overflow_wrap),
        ..Container::default()
    };
    let options = LayoutOptions {
        tab_width,
        ..LayoutOptions::default()
    };
    format(
        &[(&container, Vec::new())],
        width,
        &TextStyle::default(),
        &options,
    )
    .size
}

impl<M: LeafMeasure> Engine<'_, M> {
    pub(super) fn inline_nodes<'a>(
        sources: &[InlineSource<'a>],
        fragments: &mut Vec<InlineFragment>,
        skip: Option<&[usize]>,
    ) -> Vec<Draft<'a>> {
        fn build<'a>(
            index: usize,
            sources: &[InlineSource<'a>],
            fragments: &[InlineFragment],
            parent_origin: (i64, i64),
        ) -> Draft<'a> {
            let source = &sources[index];
            let regions: Vec<_> = fragments
                .iter()
                .filter(|fragment| {
                    fragment
                        .sources
                        .iter()
                        .any(|range| range.path.starts_with(&source.path))
                })
                .map(|fragment| fragment.bounds)
                .collect();
            let bounds = regions
                .iter()
                .copied()
                .reduce(CellRect::union)
                .unwrap_or_default();
            let children = sources
                .iter()
                .enumerate()
                .filter(|(_, child)| {
                    child.path.len() == source.path.len() + 1
                        && child.path.starts_with(&source.path)
                })
                .map(|(index, _)| build(index, sources, fragments, (bounds.x, bounds.y)))
                .collect();
            let text = if matches!(source.source.element, Element::Anchor { .. }) {
                fragments
                    .iter()
                    .filter(|fragment| {
                        inline_anchor_owner(sources, fragment) == Some(source.path.as_slice())
                    })
                    .cloned()
                    .map(|mut fragment| {
                        fragment.bounds = fragment.bounds.translated(-bounds.x, -bounds.y);
                        fragment
                    })
                    .collect()
            } else {
                Vec::new()
            };
            // Inline metadata shares the formatting context's background. Painting
            // it again after the context's glyphs erases links and styled spans.
            let mut style = source.style.clone();
            style.style.bg = source
                .source
                .background
                .map(|color| Color::Rgb(color.r, color.g, color.b));
            Draft {
                source: source.source,
                path: source.path.clone(),
                key: node_key(source.source, &source.path),
                rect: bounds.translated(-parent_origin.0, -parent_origin.1),
                content: CellRect::new(0, 0, bounds.width, bounds.height),
                extents: LogicalSize::new(bounds.width, bounds.height),
                margin: Edges::default(),
                children,
                text,
                regions: regions
                    .into_iter()
                    .map(|rect| rect.translated(-bounds.x, -bounds.y))
                    .collect(),
                style,
            }
        }
        let nodes = sources
            .iter()
            .enumerate()
            .filter(|(_, source)| {
                if skip == Some(source.path.as_slice()) {
                    return false;
                }
                let parent = &source.path[..source.path.len().saturating_sub(1)];
                skip == Some(parent) || !sources.iter().any(|other| other.path == parent)
            })
            .map(|(index, _)| build(index, sources, fragments, (0, 0)))
            .collect();
        fragments.retain(|fragment| {
            inline_anchor_owner(sources, fragment).is_none_or(|owner| skip == Some(owner))
        });
        nodes
    }
}

// The deepest anchor owns a glyph; its metadata descendants never repaint it.
fn inline_anchor_owner<'a>(
    sources: &'a [InlineSource<'_>],
    fragment: &InlineFragment,
) -> Option<&'a [usize]> {
    sources
        .iter()
        .filter(|source| {
            matches!(source.source.element, Element::Anchor { .. })
                && fragment
                    .sources
                    .iter()
                    .any(|range| range.path.starts_with(&source.path))
        })
        .max_by_key(|source| source.path.len())
        .map(|source| source.path.as_slice())
}
