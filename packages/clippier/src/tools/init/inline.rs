//! One inline, sectioned checklist. Only the active prompt rows are repainted.

use std::cell::Cell;
use std::io::{self, Write};

use bmux_tui::{
    buffer::Buffer,
    component::{Component, Constraints, LayoutCx},
    frame::Frame,
    geometry::{Rect, Size},
    paint::PaintCx,
};
use bmux_tui_components::checkbox::{CheckboxComponent, CheckboxState};
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

#[derive(Clone, Copy)]
enum Row {
    Section(usize),
    Choice(usize, usize),
}

fn checklist_rows(groups: &[Group]) -> (Vec<Row>, Vec<usize>) {
    let mut rows = Vec::new();
    let mut stops = Vec::new();
    for (group, section) in groups.iter().enumerate() {
        if section.choices.is_empty() {
            continue;
        }
        rows.push(Row::Section(group));
        for choice in 0..section.choices.len() {
            stops.push(rows.len());
            rows.push(Row::Choice(group, choice));
        }
    }
    (rows, stops)
}

fn viewport_start(start: usize, focus: usize, height: usize, count: usize) -> usize {
    let start = if focus < start {
        focus.saturating_sub(1)
    } else if focus >= start + height {
        focus + 1 - height
    } else {
        start
    };
    start.min(count.saturating_sub(height))
}

fn paint_row(
    row: Row,
    groups: &[Group],
    focused: bool,
    width: u16,
    output: &mut impl Write,
) -> io::Result<()> {
    let (label, checked) = match row {
        Row::Section(group) => (
            format!(
                "{} · {} · {} files",
                groups[group].title,
                if groups[group].formatting {
                    "Formatter"
                } else {
                    "Linter"
                },
                groups[group].files.len()
            ),
            None,
        ),
        Row::Choice(group, index) => {
            let choice = &groups[group].choices[index];
            (
                format!("{}{}", if focused { "› " } else { "  " }, choice.name),
                Some(choice.selected),
            )
        }
    };
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, 1));
    let mut frame = Frame::new(&mut buffer);
    let mut paint = PaintCx::new(&mut frame);
    if let Some(checked) = checked {
        let state = Cell::new(CheckboxState::new(checked));
        let component = CheckboxComponent::new("init-choice", &label, &state);
        let layout = component.layout(
            Constraints::tight(Size::new(width, 1)),
            &mut LayoutCx::new(),
        );
        component.paint(&layout, &mut paint);
    } else {
        paint.write_line(
            bmux_tui::paint::LocalRect::new(0, 0, width, 1),
            &bmux_tui::text::Line::from(label),
        );
    }
    execute!(
        output,
        cursor::MoveToColumn(0),
        Clear(ClearType::CurrentLine)
    )?;
    write!(
        output,
        "{}",
        if checked.is_none() {
            "\x1b[1;36m"
        } else if focused {
            "\x1b[36m"
        } else {
            "\x1b[0m"
        }
    )?;
    for cell in buffer.cells() {
        write!(output, "{}", cell.symbol)?;
    }
    write!(output, "\x1b[0m\r\n")
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

#[allow(clippy::too_many_lines)]
pub(super) fn select(groups: &mut [Group], output: &mut impl Write) -> io::Result<()> {
    let (rows, stops) = checklist_rows(groups);
    if stops.is_empty() {
        return Ok(());
    }
    writeln!(
        output,
        "\n↑/↓ or Tab navigate · Space toggle · Enter accept all · Esc cancel"
    )?;
    output.flush()?;
    terminal::enable_raw_mode()?;
    let guard = RawMode;
    let mut focus: usize = 0;
    let mut start = 0;
    let mut painted = 0;
    loop {
        clear_rows(output, painted)?;
        let (columns, height) = terminal::size()?;
        let visible = rows.len().min(usize::from(height.saturating_sub(8).max(1)));
        start = viewport_start(start, stops[focus], visible, rows.len());
        for (index, row) in rows.iter().enumerate().skip(start).take(visible) {
            paint_row(
                *row,
                groups,
                index == stops[focus],
                columns.saturating_sub(1).max(1),
                output,
            )?;
        }
        if let Row::Choice(group, index) = rows[stops[focus]] {
            let section = &groups[group];
            let choice = &section.choices[index];
            let examples = section
                .files
                .iter()
                .take(3)
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            for text in [
                format!(
                    "{} · {} · {} files",
                    choice.name,
                    if section.formatting {
                        "Formatter"
                    } else {
                        "Linter"
                    },
                    section.files.len()
                ),
                format!("Detected from: {}", choice.reason),
                format!("Examples: {examples}"),
            ] {
                let clipped = text
                    .chars()
                    .take(usize::from(columns.saturating_sub(1)))
                    .collect::<String>();
                write!(output, "\x1b[0m{clipped}\r\n")?;
            }
        }
        painted = u16::try_from(visible + 3).unwrap_or(u16::MAX);
        output.flush()?;
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
                    if let Row::Choice(group, index) = rows[stops[focus]] {
                        groups[group].choices[index].selected =
                            !groups[group].choices[index].selected;
                    }
                }
                KeyCode::Enter => break,
                _ => {}
            }
        }
    }
    clear_rows(output, painted)?;
    drop(guard);
    for group in groups {
        writeln!(output, "\n\x1b[1;36m{}\x1b[0m", group.title)?;
        for choice in &group.choices {
            writeln!(
                output,
                "  [{}] {} — {}",
                if choice.selected { "x" } else { " " },
                choice.name,
                choice.reason
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::recommendations::Choice;
    use super::*;

    #[test]
    fn navigation_crosses_sections_and_skips_headers() {
        let groups = (0..2)
            .map(|index| Group {
                title: index.to_string(),
                extensions: Vec::new(),
                files: Vec::new(),
                formatting: true,
                choices: vec![Choice {
                    name: index.to_string(),
                    reason: String::new(),
                    selected: false,
                }],
            })
            .collect::<Vec<_>>();
        let (rows, stops) = checklist_rows(&groups);
        assert_eq!(stops, [1, 3]);
        assert!(matches!(rows[stops[0]], Row::Choice(0, 0)));
        assert!(matches!(rows[stops[1]], Row::Choice(1, 0)));
    }

    #[test]
    fn viewport_tracks_focus_in_both_directions() {
        assert_eq!(viewport_start(0, 3, 8, 6), 0);
        assert_eq!(viewport_start(0, 8, 4, 12), 5);
        assert_eq!(viewport_start(5, 2, 4, 12), 1);
        assert_eq!(viewport_start(8, 11, 8, 12), 4);
    }
}
