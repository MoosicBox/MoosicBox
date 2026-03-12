//! Lightweight pane-based TUI for live tool output.

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    crossterm::event::{self, Event, KeyCode, KeyModifiers},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
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

#[derive(Clone, Debug, Default)]
struct AnsiStyleState {
    fg: Option<Color>,
    bg: Option<Color>,
    modifiers: Modifier,
}

impl AnsiStyleState {
    fn as_style(&self) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        style.add_modifier(self.modifiers)
    }

    fn reset(&mut self) {
        self.fg = None;
        self.bg = None;
        self.modifiers = Modifier::empty();
    }

    fn apply_sgr_params(&mut self, params: &[i32]) {
        let mut index = 0_usize;
        while index < params.len() {
            let code = params[index];
            match code {
                0 => self.reset(),
                1 => self.modifiers.insert(Modifier::BOLD),
                2 => self.modifiers.insert(Modifier::DIM),
                3 => self.modifiers.insert(Modifier::ITALIC),
                4 => self.modifiers.insert(Modifier::UNDERLINED),
                5 => self.modifiers.insert(Modifier::SLOW_BLINK),
                6 => self.modifiers.insert(Modifier::RAPID_BLINK),
                7 => self.modifiers.insert(Modifier::REVERSED),
                8 => self.modifiers.insert(Modifier::HIDDEN),
                9 => self.modifiers.insert(Modifier::CROSSED_OUT),
                22 => self.modifiers.remove(Modifier::BOLD | Modifier::DIM),
                23 => self.modifiers.remove(Modifier::ITALIC),
                24 => self.modifiers.remove(Modifier::UNDERLINED),
                25 => self
                    .modifiers
                    .remove(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK),
                27 => self.modifiers.remove(Modifier::REVERSED),
                28 => self.modifiers.remove(Modifier::HIDDEN),
                29 => self.modifiers.remove(Modifier::CROSSED_OUT),
                30..=37 => self.fg = Some(Color::Indexed((code - 30) as u8)),
                39 => self.fg = None,
                40..=47 => self.bg = Some(Color::Indexed((code - 40) as u8)),
                49 => self.bg = None,
                90..=97 => self.fg = Some(Color::Indexed((code - 90 + 8) as u8)),
                100..=107 => self.bg = Some(Color::Indexed((code - 100 + 8) as u8)),
                38 | 48 => {
                    let is_foreground = code == 38;
                    if let Some((color, consumed)) = parse_extended_color(params, index + 1) {
                        if is_foreground {
                            self.fg = Some(color);
                        } else {
                            self.bg = Some(color);
                        }
                        index += consumed;
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }
}

struct PaneState {
    display_name: String,
    lines: VecDeque<Line<'static>>,
    status: PaneStatus,
    scroll_offset: usize,
    ansi_state: AnsiStyleState,
}

impl PaneState {
    fn new(display_name: String) -> Self {
        Self {
            display_name,
            lines: VecDeque::new(),
            status: PaneStatus::Pending,
            scroll_offset: 0,
            ansi_state: AnsiStyleState::default(),
        }
    }

    fn push_line(&mut self, line: String) {
        self.lines
            .push_back(parse_ansi_line(&line, &mut self.ansi_state));
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
                let status_color = match pane.status {
                    PaneStatus::Pending => Color::DarkGray,
                    PaneStatus::Running => Color::Yellow,
                    PaneStatus::Passed => Color::Green,
                    PaneStatus::Failed => Color::Red,
                };
                let is_focused = tool_index == focused_index;

                let max_lines = col_area.height.saturating_sub(2) as usize;
                let total_lines = pane.lines.len();
                let max_start = total_lines.saturating_sub(max_lines);
                let distance_from_tail = max_lines.saturating_add(pane.scroll_offset);
                let desired_start = total_lines.saturating_sub(distance_from_tail);
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
                    Text::raw("")
                } else {
                    Text::from(visible_lines)
                };

                let border_style = if is_focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(status_color)
                };
                let scroll_label = if pane.scroll_offset == 0 {
                    "tail"
                } else {
                    "scroll"
                };

                let paragraph = Paragraph::new(content).block(
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

fn parse_ansi_line(input: &str, state: &mut AnsiStyleState) -> Line<'static> {
    let bytes = input.as_bytes();
    let mut index = 0_usize;
    let mut segment_start = 0_usize;
    let mut spans: Vec<Span<'static>> = Vec::new();

    while index < bytes.len() {
        if bytes[index] != 0x1B {
            index += 1;
            continue;
        }

        if segment_start < index {
            spans.push(Span::styled(
                input[segment_start..index].to_string(),
                state.as_style(),
            ));
        }

        index += 1;
        if index >= bytes.len() {
            break;
        }

        match bytes[index] {
            b'[' => {
                index += 1;
                let params_start = index;
                while index < bytes.len() && !(0x40..=0x7E).contains(&bytes[index]) {
                    index += 1;
                }

                if index < bytes.len() {
                    let final_byte = bytes[index];
                    if final_byte == b'm' {
                        let params = parse_sgr_params(&bytes[params_start..index]);
                        state.apply_sgr_params(&params);
                    }
                    index += 1;
                }
                segment_start = index;
            }
            b']' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == 0x07 {
                        index += 1;
                        break;
                    }
                    if index + 1 < bytes.len() && bytes[index] == 0x1B && bytes[index + 1] == b'\\'
                    {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
                segment_start = index;
            }
            _ => {
                segment_start = index;
            }
        }
    }

    if segment_start < input.len() {
        spans.push(Span::styled(
            input[segment_start..].to_string(),
            state.as_style(),
        ));
    }

    if spans.is_empty() {
        Line::from(String::new())
    } else {
        Line::from(spans)
    }
}

fn parse_sgr_params(params: &[u8]) -> Vec<i32> {
    if params.is_empty() {
        return vec![0];
    }

    let mut values = Vec::new();
    let mut current: Option<i32> = None;

    for byte in params {
        if byte.is_ascii_digit() {
            let digit = i32::from(byte - b'0');
            let prior = current.unwrap_or(0);
            current = Some(prior.saturating_mul(10).saturating_add(digit));
        } else if *byte == b';' || *byte == b':' {
            if let Some(value) = current.take() {
                values.push(value);
            }
        }
    }

    if let Some(value) = current {
        values.push(value);
    }

    if values.is_empty() { vec![0] } else { values }
}

fn parse_extended_color(params: &[i32], index: usize) -> Option<(Color, usize)> {
    if index >= params.len() {
        return None;
    }

    match params[index] {
        5 => {
            if index + 1 >= params.len() {
                return None;
            }

            let value = params[index + 1];
            let clamped = value.clamp(0, 255) as u8;
            Some((Color::Indexed(clamped), 2))
        }
        2 => {
            if index + 3 >= params.len() {
                return None;
            }

            let r = params[index + 1].clamp(0, 255) as u8;
            let g = params[index + 2].clamp(0, 255) as u8;
            let b = params[index + 3].clamp(0, 255) as u8;
            Some((Color::Rgb(r, g, b), 4))
        }
        _ => None,
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
    fn parse_ansi_line_applies_basic_color() {
        let mut state = AnsiStyleState::default();
        let line = parse_ansi_line("\u{1b}[31mhello\u{1b}[0m", &mut state);

        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content.as_ref(), "hello");
        assert_eq!(line.spans[0].style.fg, Some(Color::Indexed(1)));
        assert_eq!(state.fg, None);
    }

    #[test]
    fn parse_ansi_line_persists_style_across_lines() {
        let mut state = AnsiStyleState::default();
        let first = parse_ansi_line("\u{1b}[32mgreen", &mut state);
        let second = parse_ansi_line("still green", &mut state);
        let _third = parse_ansi_line("\u{1b}[0mreset", &mut state);

        assert_eq!(first.spans[0].style.fg, Some(Color::Indexed(2)));
        assert_eq!(second.spans[0].style.fg, Some(Color::Indexed(2)));
        assert_eq!(state.fg, None);
    }

    #[test]
    fn parse_ansi_line_supports_truecolor() {
        let mut state = AnsiStyleState::default();
        let line = parse_ansi_line("\u{1b}[38;2;12;34;56mcolor", &mut state);

        assert_eq!(line.spans[0].style.fg, Some(Color::Rgb(12, 34, 56)));
    }
}
