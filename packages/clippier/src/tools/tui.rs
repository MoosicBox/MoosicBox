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
    scroll_offset: usize,
}

impl PaneState {
    fn new(display_name: String) -> Self {
        Self {
            display_name,
            lines: VecDeque::new(),
            status: PaneStatus::Pending,
            scroll_offset: 0,
        }
    }

    fn push_line(&mut self, line: String) {
        self.lines.push_back(strip_ansi_sequences(&line));
        while self.lines.len() > 2_000 {
            let _ = self.lines.pop_front();
        }
    }

    fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    fn scroll_top(&mut self) {
        self.scroll_offset = usize::MAX;
    }

    fn scroll_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuiExit {
    Completed,
    UserClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UserAction {
    None,
    Close,
    FocusNext,
    FocusPrev,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    ScrollTop,
    ScrollBottom,
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
    let mut focused_index = 0_usize;

    let loop_result = loop {
        let action = read_user_action()?;
        if action == UserAction::Close {
            break Ok(TuiExit::UserClosed);
        }
        apply_user_action(action, &mut panes, tools, &mut focused_index);

        if focused_index >= tools.len() && !tools.is_empty() {
            focused_index = tools.len() - 1;
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

        terminal.draw(|frame| {
            render(
                frame,
                &panes,
                tools,
                completed,
                total,
                start_time,
                focused_index,
            )
        })?;

        if completed >= total {
            break Ok(TuiExit::Completed);
        }
    };

    ratatui::restore();
    loop_result
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

fn read_user_action() -> std::io::Result<UserAction> {
    if !event::poll(Duration::from_millis(0))? {
        return Ok(UserAction::None);
    }

    let action = match event::read()? {
        Event::Key(key) => {
            if (key.code == KeyCode::Char('q') && key.modifiers.is_empty())
                || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                UserAction::Close
            } else if key.code == KeyCode::Tab || key.code == KeyCode::Right {
                UserAction::FocusNext
            } else if key.code == KeyCode::BackTab || key.code == KeyCode::Left {
                UserAction::FocusPrev
            } else if key.code == KeyCode::Up || key.code == KeyCode::Char('k') {
                UserAction::ScrollUp
            } else if key.code == KeyCode::Down || key.code == KeyCode::Char('j') {
                UserAction::ScrollDown
            } else if key.code == KeyCode::PageUp {
                UserAction::PageUp
            } else if key.code == KeyCode::PageDown {
                UserAction::PageDown
            } else if key.code == KeyCode::Home || key.code == KeyCode::Char('g') {
                UserAction::ScrollTop
            } else if key.code == KeyCode::End || key.code == KeyCode::Char('G') {
                UserAction::ScrollBottom
            } else {
                UserAction::None
            }
        }
        Event::FocusGained
        | Event::FocusLost
        | Event::Mouse(..)
        | Event::Paste(..)
        | Event::Resize(..) => UserAction::None,
    };

    Ok(action)
}

fn apply_user_action(
    action: UserAction,
    panes: &mut BTreeMap<String, PaneState>,
    tools: &[(String, String)],
    focused_index: &mut usize,
) {
    if tools.is_empty() {
        return;
    }

    match action {
        UserAction::FocusNext => {
            *focused_index = (*focused_index + 1) % tools.len();
        }
        UserAction::FocusPrev => {
            *focused_index = if *focused_index == 0 {
                tools.len() - 1
            } else {
                *focused_index - 1
            };
        }
        UserAction::ScrollUp
        | UserAction::ScrollDown
        | UserAction::PageUp
        | UserAction::PageDown
        | UserAction::ScrollTop
        | UserAction::ScrollBottom => {
            if let Some((tool_name, _)) = tools.get(*focused_index)
                && let Some(pane) = panes.get_mut(tool_name)
            {
                match action {
                    UserAction::ScrollUp => pane.scroll_up(1),
                    UserAction::ScrollDown => pane.scroll_down(1),
                    UserAction::PageUp => pane.scroll_up(10),
                    UserAction::PageDown => pane.scroll_down(10),
                    UserAction::ScrollTop => pane.scroll_top(),
                    UserAction::ScrollBottom => pane.scroll_bottom(),
                    UserAction::None
                    | UserAction::Close
                    | UserAction::FocusNext
                    | UserAction::FocusPrev => {}
                }
            }
        }
        UserAction::None | UserAction::Close => {}
    }
}

fn render(
    frame: &mut Frame,
    panes: &BTreeMap<String, PaneState>,
    tools: &[(String, String)],
    completed: usize,
    total: usize,
    start_time: Instant,
    focused_index: usize,
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
        "clippier live output | total: {total} running: {running} passed: {passed} failed: {failed} done: {completed}/{total} elapsed: {:.1?} | keys: q/ctrl-c close, tab switch, j/k scroll",
        start_time.elapsed()
    ))
    .block(Block::default().borders(Borders::ALL).title("Status"));
    frame.render_widget(header, chunks[0]);

    render_panes(frame, chunks[1], panes, tools, focused_index);
}

fn render_panes(
    frame: &mut Frame,
    area: Rect,
    panes: &BTreeMap<String, PaneState>,
    tools: &[(String, String)],
    focused_index: usize,
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
                let is_focused = tool_index == focused_index;

                let max_lines = col_area.height.saturating_sub(2) as usize;
                let total_lines = pane.lines.len();
                let max_start = total_lines.saturating_sub(max_lines);
                let desired_start = total_lines.saturating_sub(max_lines + pane.scroll_offset);
                let start = desired_start.min(max_start);

                let visible_lines = if total_lines > max_lines {
                    pane.lines
                        .iter()
                        .skip(start)
                        .take(max_lines)
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

                let border_style = if is_focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                };
                let scroll_label = if pane.scroll_offset == 0 {
                    "tail"
                } else {
                    "scroll"
                };

                let paragraph = Paragraph::new(content).style(style).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(border_style)
                        .title(format!(
                            "{} [{} | {}]{}",
                            pane.display_name,
                            status_label(pane.status),
                            scroll_label,
                            if is_focused { " *" } else { "" }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_sequences_removes_csi_codes() {
        let input = "\u{1b}[1m\u{1b}[92mhello\u{1b}[0m world";
        assert_eq!(strip_ansi_sequences(input), "hello world");
    }
}
