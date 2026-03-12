//! Lightweight pane-based TUI for live tool output.

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    crossterm::event::{self, Event, KeyCode, KeyModifiers},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};

use crate::tools::runner::ToolEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneStatus {
    Pending,
    Running,
    Passed,
    Failed,
}

struct PaneState {
    display_name: String,
    lines: VecDeque<String>,
    status: PaneStatus,
}

impl PaneState {
    fn new(display_name: String) -> Self {
        Self {
            display_name,
            lines: VecDeque::new(),
            status: PaneStatus::Pending,
        }
    }

    fn push_line(&mut self, line: String) {
        self.lines.push_back(strip_ansi_sequences(&line));
        while self.lines.len() > 2_000 {
            let _ = self.lines.pop_front();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuiExit {
    Completed,
    UserClosed,
}

pub fn run_live_tui(
    tools: &[(String, String)],
    rx: Receiver<ToolEvent>,
    start_time: Instant,
) -> std::io::Result<TuiExit> {
    let mut panes: BTreeMap<String, PaneState> = tools
        .iter()
        .map(|(name, display_name)| (name.clone(), PaneState::new(display_name.clone())))
        .collect();

    let mut terminal = ratatui::try_init()?;
    let total = tools.len();
    let mut completed = 0_usize;

    let loop_result = loop {
        if user_requested_close()? {
            break Ok(TuiExit::UserClosed);
        }

        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                handle_event(event, &mut panes, &mut completed);
                while let Ok(event) = rx.try_recv() {
                    handle_event(event, &mut panes, &mut completed);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break Ok(TuiExit::Completed),
        }

        terminal.draw(|frame| render(frame, &panes, tools, completed, total, start_time))?;

        if completed >= total {
            break Ok(TuiExit::Completed);
        }
    };

    ratatui::restore();
    loop_result
}

fn user_requested_close() -> std::io::Result<bool> {
    if !event::poll(Duration::from_millis(0))? {
        return Ok(false);
    }

    let should_close = match event::read()? {
        Event::Key(key) => {
            (key.code == KeyCode::Char('q') && key.modifiers.is_empty())
                || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        }
        Event::FocusGained
        | Event::FocusLost
        | Event::Mouse(..)
        | Event::Paste(..)
        | Event::Resize(..) => false,
    };

    Ok(should_close)
}

fn strip_ansi_sequences(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\u{1B}' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                let mut prev_was_esc = false;
                for c in chars.by_ref() {
                    if c == '\u{7}' {
                        break;
                    }
                    if prev_was_esc && c == '\\' {
                        break;
                    }
                    prev_was_esc = c == '\u{1B}';
                }
            }
            Some(_) | None => {}
        }
    }

    output
}

fn handle_event(event: ToolEvent, panes: &mut BTreeMap<String, PaneState>, completed: &mut usize) {
    match event {
        ToolEvent::Started {
            tool_name,
            display_name,
        } => {
            if let Some(pane) = panes.get_mut(&tool_name) {
                pane.status = PaneStatus::Running;
                pane.display_name = display_name;
            }
        }
        ToolEvent::StdoutLine { tool_name, line } | ToolEvent::StderrLine { tool_name, line } => {
            if let Some(pane) = panes.get_mut(&tool_name) {
                pane.push_line(line);
            }
        }
        ToolEvent::Finished { tool_name, success } => {
            if let Some(pane) = panes.get_mut(&tool_name) {
                pane.status = if success {
                    PaneStatus::Passed
                } else {
                    PaneStatus::Failed
                };
                *completed += 1;
            }
        }
    }
}

fn render(
    frame: &mut Frame,
    panes: &BTreeMap<String, PaneState>,
    tools: &[(String, String)],
    completed: usize,
    total: usize,
    start_time: Instant,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(frame.area());

    let running = panes
        .values()
        .filter(|pane| pane.status == PaneStatus::Running)
        .count();
    let passed = panes
        .values()
        .filter(|pane| pane.status == PaneStatus::Passed)
        .count();
    let failed = panes
        .values()
        .filter(|pane| pane.status == PaneStatus::Failed)
        .count();

    let header = Paragraph::new(format!(
        "clippier live output | total: {total} running: {running} passed: {passed} failed: {failed} done: {completed}/{total} elapsed: {:.1?}",
        start_time.elapsed()
    ))
    .block(Block::default().borders(Borders::ALL).title("Status"));
    frame.render_widget(header, chunks[0]);

    render_panes(frame, chunks[1], panes, tools);
}

fn render_panes(
    frame: &mut Frame,
    area: Rect,
    panes: &BTreeMap<String, PaneState>,
    tools: &[(String, String)],
) {
    if tools.is_empty() {
        return;
    }

    let columns: usize = if tools.len() > 1 { 2 } else { 1 };
    let rows = tools.len().div_ceil(columns);

    let row_constraints: Vec<Constraint> = (0..rows)
        .map(|_| Constraint::Ratio(1, rows as u32))
        .collect();
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    for (row_index, row_area) in row_areas.iter().enumerate() {
        let col_constraints: Vec<Constraint> = (0..columns)
            .map(|_| Constraint::Ratio(1, columns as u32))
            .collect();
        let col_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row_area);

        for (col_index, col_area) in col_areas.iter().enumerate() {
            let tool_index = (row_index * columns) + col_index;
            if let Some((tool_name, _)) = tools.get(tool_index)
                && let Some(pane) = panes.get(tool_name)
            {
                let style = match pane.status {
                    PaneStatus::Pending => Style::default().fg(Color::DarkGray),
                    PaneStatus::Running => Style::default().fg(Color::Yellow),
                    PaneStatus::Passed => Style::default().fg(Color::Green),
                    PaneStatus::Failed => Style::default().fg(Color::Red),
                };

                let max_lines = col_area.height.saturating_sub(2) as usize;
                let visible_lines = if pane.lines.len() > max_lines {
                    pane.lines
                        .iter()
                        .skip(pane.lines.len() - max_lines)
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    pane.lines.iter().cloned().collect::<Vec<_>>()
                };

                let content = if visible_lines.is_empty() {
                    String::new()
                } else {
                    visible_lines.join("\n")
                };

                let paragraph = Paragraph::new(content).style(style).block(
                    Block::default().borders(Borders::ALL).title(format!(
                        "{} [{}]",
                        pane.display_name,
                        status_label(pane.status)
                    )),
                );
                frame.render_widget(paragraph, *col_area);
            }
        }
    }
}

const fn status_label(status: PaneStatus) -> &'static str {
    match status {
        PaneStatus::Pending => "pending",
        PaneStatus::Running => "running",
        PaneStatus::Passed => "pass",
        PaneStatus::Failed => "fail",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_sequences_removes_csi_codes() {
        let input = "\u{1b}[1m\u{1b}[92mhello\u{1b}[0m world";
        assert_eq!(strip_ansi_sequences(input), "hello world");
    }
}
