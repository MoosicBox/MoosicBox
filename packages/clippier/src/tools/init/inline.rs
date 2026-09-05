//! Inline checkbox prompts. Only the active prompt rows are repainted.

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

pub(super) fn select(groups: &mut [Group], output: &mut impl Write) -> io::Result<()> {
    for group in groups {
        writeln!(output, "\n\x1b[1;36m{}\x1b[0m", group.title)?;
        writeln!(
            output,
            "↑/↓ navigate · Space toggle · Enter continue · Esc cancel"
        )?;
        output.flush()?;
        terminal::enable_raw_mode()?;
        let guard = RawMode;
        let mut focus: usize = 0;
        let mut painted = 0u16;
        loop {
            if painted > 0 {
                execute!(output, cursor::MoveUp(painted))?;
            }
            let (columns, rows) = terminal::size()?;
            let width = columns.saturating_sub(1).max(1);
            let visible = group
                .choices
                .len()
                .min(usize::from(rows.saturating_sub(4).max(1)));
            let start = focus.saturating_sub(visible - 1);
            for (index, choice) in group.choices.iter().enumerate().skip(start).take(visible) {
                let label = format!(
                    "{}{} — {}",
                    if index == focus { "› " } else { "  " },
                    choice.name,
                    choice.reason
                );
                let state = Cell::new(CheckboxState::new(choice.selected));
                let component = CheckboxComponent::new("init-choice", &label, &state);
                let layout = component.layout(
                    Constraints::tight(Size::new(width, 1)),
                    &mut LayoutCx::new(),
                );
                let mut buffer = Buffer::empty(Rect::new(0, 0, width, 1));
                component.paint(&layout, &mut PaintCx::new(&mut Frame::new(&mut buffer)));
                execute!(
                    output,
                    cursor::MoveToColumn(0),
                    Clear(ClearType::CurrentLine)
                )?;
                write!(output, "\x1b[36m")?;
                for cell in buffer.cells() {
                    write!(output, "{}", cell.symbol)?;
                }
                write!(output, "\x1b[0m\r\n")?;
            }
            painted = u16::try_from(visible).unwrap_or(u16::MAX);
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
                    KeyCode::Up => focus = focus.saturating_sub(1),
                    KeyCode::Down => focus = (focus + 1).min(group.choices.len() - 1),
                    KeyCode::Char(' ') => {
                        group.choices[focus].selected = !group.choices[focus].selected;
                    }
                    KeyCode::Enter => break,
                    _ => {}
                }
            }
        }
        execute!(output, cursor::MoveUp(painted), cursor::MoveToColumn(0))?;
        for _ in 0..painted {
            execute!(output, Clear(ClearType::CurrentLine))?;
            write!(output, "\r\n")?;
        }
        execute!(output, cursor::MoveUp(painted))?;
        drop(guard);
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
