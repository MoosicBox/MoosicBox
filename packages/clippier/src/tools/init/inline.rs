//! Inline presentation of a measured BMUX component tree.

use std::cell::Cell;
use std::io::{self, Write};

use bmux_tui::{
    buffer::Buffer,
    component::{Component, Constraints, LayoutCx, LayoutId, LogicalSize},
    composition::Column,
    frame::Frame,
    geometry::{Rect, Size},
    paint::PaintCx,
};
use bmux_tui_components::{
    checkbox::{CheckboxComponent, CheckboxState},
    key_hint_bar::{KeyHint, KeyHintBarComponent},
    labeled_details::{DetailItem, LabeledDetailsComponent},
    pane::{Pane, PaneComponent, PaneState},
    scroll_view::{ScrollViewComponent, ScrollViewState},
};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, Clear, ClearType},
};

use super::recommendations::Group;

struct RawMode;
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), cursor::Show);
    }
}

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
    focus: (usize, usize),
    width: u16,
    height: u16,
    scroll: &mut ScrollViewState,
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
    let panes = groups
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
            choices = choices.child(CheckboxComponent::new(
                format!("choice-{g}-{c}"),
                &choice.name,
                &states[g][c],
            ));
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
            Pane::new().border(true).title(title),
            &panes[g],
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
        LabeledDetailsComponent::new("details", &details),
    );
    let hints = [
        KeyHint::new("↑/↓ Tab", "Move"),
        KeyHint::new("Space", "Toggle"),
        KeyHint::new("Enter", "Accept"),
        KeyHint::new("Esc", "Cancel"),
    ];
    let footer = KeyHintBarComponent::new("keys", &hints);
    let mut cx = LayoutCx::new();
    let detail_height = detail
        .layout(Constraints::for_width(width), &mut cx)
        .size
        .height
        .min(8);
    let footer_height = footer
        .layout(Constraints::for_width(width), &mut cx)
        .size
        .height;
    let viewport_height = usize::from(height)
        .saturating_sub(detail_height + footer_height + 2)
        .max(1);
    let content = sections.layout(Constraints::for_width(width), &mut cx);
    if let Some(rect) =
        content.find_logical_rect(&LayoutId::new(format!("choice-{}-{}", focus.0, focus.1)))
    {
        let offset = scroll.vertical_offset();
        if rect.y < offset {
            scroll.set_vertical_offset(rect.y.saturating_sub(1));
        } else if rect.y >= offset + viewport_height {
            scroll.set_vertical_offset(rect.y + 1 - viewport_height);
        }
    }
    let viewport = ScrollViewComponent::new(
        "checklist",
        LogicalSize::new(width, viewport_height),
        *scroll,
        sections,
    );
    let detail_view = ScrollViewComponent::new(
        "detail-view",
        LogicalSize::new(width, detail_height),
        ScrollViewState::new(),
        detail,
    );
    let root = Column::new()
        .id("init")
        .gap(1)
        .child(viewport)
        .child(detail_view)
        .child(footer);
    let layout = root.layout(Constraints::tight(Size::new(width, height)), &mut cx);
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    root.paint(&layout, &mut PaintCx::new(&mut Frame::new(&mut buffer)));
    buffer
}

// Inline transport only: components own geometry, clipping, and styling.
fn present(buffer: &Buffer, output: &mut impl Write) -> io::Result<()> {
    for row in buffer.cells().chunks(usize::from(buffer.area().width)) {
        execute!(
            output,
            cursor::MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        )?;
        for cell in row {
            if cell.is_wide_continuation() {
                continue;
            }
            write!(output, "\x1b[0m")?;
            for (modifier, code) in [
                (bmux_tui::style::Modifier::REVERSED, 7),
                (bmux_tui::style::Modifier::BOLD, 1),
                (bmux_tui::style::Modifier::DIM, 2),
                (bmux_tui::style::Modifier::UNDERLINE, 4),
            ] {
                if cell.style.modifiers.contains(modifier) {
                    write!(output, "\x1b[{code}m")?;
                }
            }
            write!(output, "{}", cell.symbol)?;
        }
        write!(output, "\x1b[0m\r\n")?;
    }
    output.flush()
}

fn clear_rows(output: &mut impl Write, painted: u16) -> io::Result<()> {
    if painted == 0 {
        return Ok(());
    }
    execute!(output, cursor::MoveUp(painted), cursor::MoveToColumn(0))?;
    for _ in 0..painted {
        execute!(output, Clear(ClearType::CurrentLine))?;
        write!(output, "\r\n")?;
    }
    execute!(output, cursor::MoveUp(painted))
}

pub(super) fn select(groups: &mut [Group], output: &mut impl Write) -> io::Result<()> {
    let stops = stops(groups);
    if stops.is_empty() {
        return Ok(());
    }
    output.flush()?;
    terminal::enable_raw_mode()?;
    let guard = RawMode;
    let mut focus = 0usize;
    let mut painted = 0;
    let mut scroll = ScrollViewState::new();
    loop {
        clear_rows(output, painted)?;
        let (width, height) = terminal::size()?;
        let buffer = render(
            groups,
            stops[focus],
            width.saturating_sub(1).max(1),
            height.saturating_sub(2).max(1),
            &mut scroll,
        );
        present(&buffer, output)?;
        painted = buffer.area().height;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key.code {
                KeyCode::Esc => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Setup cancelled; no files written",
                    ));
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Setup cancelled; no files written",
                    ));
                }
                KeyCode::Up | KeyCode::BackTab => focus = focus.saturating_sub(1),
                KeyCode::Down | KeyCode::Tab => focus = (focus + 1).min(stops.len() - 1),
                KeyCode::Home => focus = 0,
                KeyCode::End => focus = stops.len() - 1,
                KeyCode::Char(' ') => {
                    let (g, c) = stops[focus];
                    groups[g].choices[c].selected = !groups[g].choices[c].selected;
                }
                KeyCode::Enter => break,
                _ => {}
            }
        }
    }
    clear_rows(output, painted)?;
    drop(guard);
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
