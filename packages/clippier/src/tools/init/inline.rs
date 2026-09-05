//! Inline presentation of a measured BMUX component tree.

use std::cell::Cell;
use std::io::{self, Write};

use bmux_tui::{
    buffer::Buffer,
    component::{Component, Constraints, LayoutCx, LogicalSize},
    composition::{Column, Flex, TextBlock},
    frame::Frame,
    geometry::{Rect, Size},
    paint::PaintCx,
    style::{Color, Modifier, Style},
};
use bmux_tui_components::{
    checkbox::{CheckboxComponent, CheckboxState, CheckboxStyles},
    key_hint_bar::{KeyHint, KeyHintBarComponent},
    labeled_details::{DetailItem, LabeledDetailsComponent},
    pane::{Pane, PaneComponent, PaneState, PaneStyles},
    scroll_view::{ScrollViewComponent, ScrollViewState},
};

use super::recommendations::Group;

pub(super) fn stops(groups: &[Group]) -> Vec<(usize, usize)> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(group, section)| (0..section.choices.len()).map(move |choice| (group, choice)))
        .collect()
}

#[allow(clippy::too_many_lines)]
pub(super) fn render(
    groups: &[Group],
    header: &[String],
    focus: (usize, usize),
    width: u16,
    height: u16,
    scroll: &Cell<ScrollViewState>,
) -> Buffer {
    let states = groups
        .iter()
        .enumerate()
        .map(|(g, group)| {
            group
                .choices
                .iter()
                .enumerate()
                .map(|(c, choice)| {
                    let mut state = CheckboxState::new(choice.selected);
                    state.set_focused((g, c) == focus);
                    Cell::new(state)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let panel_states = groups
        .iter()
        .map(|_| Cell::new(PaneState::new(Rect::new(0, 0, 0, 0))))
        .collect::<Vec<_>>();
    let mut sections = Column::new().id("sections").gap(1);
    for (g, group) in groups.iter().enumerate() {
        if group.choices.is_empty() {
            continue;
        }
        let mut choices = Column::new().id(format!("choices-{g}"));
        for (c, choice) in group.choices.iter().enumerate() {
            choices = choices.child(
                CheckboxComponent::new(format!("choice-{g}-{c}"), &choice.name, &states[g][c])
                    .styles(CheckboxStyles {
                        normal: Style::new().fg(if choice.selected {
                            Color::Green
                        } else {
                            Color::Default
                        }),
                        focused: Style::new()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                            .add_modifier(Modifier::REVERSED),
                        ..CheckboxStyles::default()
                    }),
            );
        }
        let title = format!(
            " {} · {} · {} files ",
            group.title,
            if group.formatting {
                "Formatter"
            } else {
                "Linter"
            },
            group.files.len()
        );
        sections = sections.child(PaneComponent::new(
            format!("section-{g}"),
            Pane::new().border(true).title(title).styles(PaneStyles {
                border: Style::new().fg(if g == focus.0 {
                    Color::Cyan
                } else {
                    Color::BrightBlack
                }),
                ..PaneStyles::default()
            }),
            &panel_states[g],
            choices,
        ));
    }
    let section = &groups[focus.0];
    let choice = &section.choices[focus.1];
    let details = vec![
        DetailItem::new(
            "Capability",
            if section.formatting {
                "Formatter"
            } else {
                "Linter"
            },
        ),
        DetailItem::new("Evidence", &choice.reason),
        DetailItem::new(
            "Coverage",
            format!(
                "{} candidate files · {}",
                section.files.len(),
                section.extensions.join(", ")
            ),
        ),
        DetailItem::new(
            "Examples",
            section
                .files
                .iter()
                .take(3)
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ),
    ];
    let detail_state = Cell::new(PaneState::new(Rect::new(0, 0, 0, 0)));
    let detail = PaneComponent::new(
        "detail-pane",
        Pane::new().border(true).title(format!(" {} ", choice.name)),
        &detail_state,
        LabeledDetailsComponent::new("details", &details).item_spacing(false),
    );
    let hints = [
        KeyHint::new("↑↓", "Move"),
        KeyHint::new("Space", "Toggle"),
        KeyHint::new("↵", "Accept"),
        KeyHint::new("Esc", "Cancel"),
    ];
    let footer = KeyHintBarComponent::new("keys", &hints);
    let mut cx = LayoutCx::new();
    let viewport = ScrollViewComponent::new(
        "checklist",
        LogicalSize::new(width, 0),
        scroll.get(),
        sections,
    )
    .retain_state(scroll);
    let positions = stops(groups);
    let viewport = if positions.first() == Some(&focus) {
        viewport.reveal(format!("section-{}.surface", focus.0))
    } else if positions.last() == Some(&focus) {
        viewport.reveal_end(format!("section-{}.surface", focus.0))
    } else {
        viewport.reveal(format!("choice-{}-{}", focus.0, focus.1))
    };
    let detail_view = ScrollViewComponent::new(
        "detail-view",
        LogicalSize::new(width, 0),
        ScrollViewState::new(),
        detail,
    );
    let mut root = Column::new().id("init");
    if !header.is_empty() {
        root = root.child(TextBlock::new(super::branding::logo(width, height)).id("logo"));
        root = root.child(TextBlock::new(header.join("\n")).id("header"));
    }
    let root = root
        .flex(Flex::new(3, viewport))
        .flex(Flex::new(1, detail_view))
        .child(footer);
    let layout = root.layout(Constraints::tight(Size::new(width, height)), &mut cx);
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    root.paint(&layout, &mut PaintCx::new(&mut Frame::new(&mut buffer)));
    buffer
}

pub(super) fn select(
    groups: &mut [Group],
    header: &[String],
    output: &mut impl Write,
) -> io::Result<()> {
    let stops = stops(groups);
    if stops.is_empty() {
        return Ok(());
    }
    output.flush()?;
    super::runtime::run(groups, header, output)?;
    for group in groups {
        writeln!(
            output,
            "\n{} · {}",
            group.title,
            if group.formatting {
                "Formatter"
            } else {
                "Linter"
            }
        )?;
        for choice in &group.choices {
            writeln!(
                output,
                "  [{}] {}",
                if choice.selected { "x" } else { " " },
                choice.name
            )?;
        }
    }
    Ok(())
}
