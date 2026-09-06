//! Inline presentation of a measured BMUX component tree.

use std::cell::Cell;
use std::io::{self, Write};

use bmux_tui::{
    buffer::Buffer,
    component::{Component, Constraints, LayoutCx, LogicalSize},
    composition::{Column, Flex, Row, TextBlock},
    frame::Frame,
    geometry::{Rect, Size},
    paint::PaintCx,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use bmux_tui_components::{
    button::{ButtonComponent, ButtonState, ButtonStyles},
    checkbox::{CheckboxComponent, CheckboxState, CheckboxStyles},
    key_hint_bar::{KeyHint, KeyHintBarComponent, KeyHintBarStyles},
    labeled_details::{DetailItem, LabeledDetailsComponent, LabeledDetailsStyles},
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
    let columns = usize::from((width / 60).clamp(1, 3));
    let mut row = Row::new().gap(1);
    let mut count = 0;
    for (g, group) in groups.iter().enumerate() {
        if group.choices.is_empty() {
            continue;
        }
        let accent = if group.formatting {
            Color::Cyan
        } else {
            Color::Magenta
        };
        let mut choices = Column::new().id(format!("choices-{g}"));
        for (c, choice) in group.choices.iter().enumerate() {
            let availability = Line::from_spans(vec![
                Span::styled(" · ", Style::new().fg(Color::BrightBlack)),
                Span::styled(
                    if choice.installed.is_some() {
                        "installed"
                    } else {
                        "not found"
                    },
                    Style::new().fg(if choice.installed.is_some() {
                        Color::Green
                    } else {
                        Color::Yellow
                    }),
                ),
                Span::styled(
                    if choice.active { " · active" } else { "" },
                    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]);
            choices = choices.child(
                Row::new()
                    .child(
                        CheckboxComponent::new(
                            format!("choice-{g}-{c}"),
                            &choice.name,
                            &states[g][c],
                        )
                        .styles(CheckboxStyles {
                            normal: Style::new().fg(if choice.selected {
                                Color::Green
                            } else {
                                Color::Default
                            }),
                            focused: Style::new()
                                .fg(Color::Black)
                                .bg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            ..CheckboxStyles::default()
                        }),
                    )
                    .child(TextBlock::new(Text::from_lines(vec![availability]))),
            );
        }
        let title = Line::from_spans(vec![
            Span::styled(
                format!(" {} ", group.title),
                Style::new().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "· {} ",
                    if group.formatting {
                        "Formatter"
                    } else {
                        "Linter"
                    }
                ),
                Style::new().fg(accent),
            ),
            Span::styled(
                format!("· {} files ", group.files.len()),
                Style::new().fg(Color::Yellow),
            ),
        ]);
        row = row.flex(Flex::new(
            1,
            PaneComponent::new(
                format!("section-{g}"),
                Pane::new().border(true).title(title).styles(PaneStyles {
                    border: Style::new().fg(if g == focus.0 {
                        accent
                    } else {
                        Color::BrightBlack
                    }),
                    ..PaneStyles::default()
                }),
                &panel_states[g],
                choices,
            ),
        ));
        count += 1;
        if count == columns {
            sections = sections.child(row);
            row = Row::new().gap(1);
            count = 0;
        }
    }
    if count > 0 {
        for _ in count..columns {
            row = row.flex(Flex::new(1, TextBlock::new("")));
        }
        sections = sections.child(row);
    }
    let detail_focus = if focus.0 == usize::MAX {
        *stops(groups).last().expect("nonempty checklist")
    } else {
        focus
    };
    let section = &groups[detail_focus.0];
    let choice = &section.choices[detail_focus.1];
    let details = vec![
        DetailItem::new(
            "Capability",
            format!(
                "{} · {}",
                if section.formatting {
                    "Formatter"
                } else {
                    "Linter"
                },
                choice.installed.as_ref().map_or_else(
                    || "not found; still selectable".to_owned(),
                    |path| format!("installed: {}", path.display())
                )
            ),
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
        Pane::new()
            .border(true)
            .title(Line::from_spans(vec![Span::styled(
                format!(" {} ", choice.name),
                Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            )]))
            .styles(PaneStyles {
                border: Style::new().fg(Color::Blue),
                ..PaneStyles::default()
            }),
        &detail_state,
        LabeledDetailsComponent::new("details", &details)
            .item_spacing(false)
            .styles(LabeledDetailsStyles {
                label: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                value: Style::new(),
                continuation: Style::new().fg(Color::BrightBlack),
            }),
    );
    let hints = [
        KeyHint::new("↑↓", "Move"),
        KeyHint::new("Space/↵", "Toggle"),
        KeyHint::new("Ctrl+↵", "Submit"),
        KeyHint::new("Tab/↵", "Focus/Activate"),
        KeyHint::new("Esc/q", "Cancel"),
    ];
    let footer = KeyHintBarComponent::new("keys", &hints).styles(KeyHintBarStyles {
        key: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        label: Style::new(),
        separator: Style::new().fg(Color::BrightBlack),
        ..KeyHintBarStyles::default()
    });
    let mut cx = LayoutCx::new();
    let viewport = ScrollViewComponent::new(
        "checklist",
        LogicalSize::new(width, 0),
        scroll.get(),
        sections,
    )
    .retain_state(scroll);
    let positions = stops(groups);
    let viewport = if focus.0 == usize::MAX {
        viewport
    } else if positions.first() == Some(&focus) {
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
        let context = Text::from_lines(
            header
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let color = match index {
                        0 => Color::Cyan,
                        1 => Color::Green,
                        _ => Color::Default,
                    };
                    Line::from_spans(vec![Span::styled(line, Style::new().fg(color))])
                })
                .collect::<Vec<_>>(),
        );
        root = root.child(TextBlock::new(context).id("header"));
    }
    let button_states = [0, 1].map(|index| {
        let mut state = ButtonState::new();
        state.set_focused(focus == (usize::MAX, index));
        Cell::new(state)
    });
    let mut buttons = Row::new().id("actions");
    for (index, label, color) in [(0, "Submit", Color::Green), (1, "Cancel", Color::Red)] {
        buttons = buttons.child(
            ButtonComponent::new(format!("action-{index}"), label, &button_states[index]).styles(
                ButtonStyles {
                    normal: Style::new().fg(color),
                    focused: Style::new()
                        .fg(Color::Black)
                        .bg(color)
                        .add_modifier(Modifier::BOLD),
                    ..ButtonStyles::default()
                },
            ),
        );
    }
    let root = root
        .flex(Flex::new(3, viewport))
        .child(TextBlock::new("").id("details-spacing"))
        .flex(Flex::new(1, detail_view))
        .child(buttons)
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
