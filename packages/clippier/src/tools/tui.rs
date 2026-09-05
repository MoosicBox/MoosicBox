//! Lightweight pane-based TUI for live tool output.

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use bmux_tui::{
    component::{Component, Constraints, LayoutCx, LayoutNode},
    composition::{Column, Flex, Row, TextBlock},
    crossterm::CrosstermTerminalGuard,
    geometry::{Rect, Size},
    paint::PaintCx,
    style::{Color, Modifier, Style},
    terminal::Terminal,
    text::{Line, Span},
};
use bmux_tui_components::pane::{Pane, PaneComponent, PaneState as SurfaceState, PaneStyles};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use std::cell::Cell;

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

    const fn reset(&mut self) {
        self.fg = None;
        self.bg = None;
        self.modifiers = Modifier::EMPTY;
    }

    fn apply_sgr_params(&mut self, params: &[i32]) {
        let mut index = 0_usize;
        while index < params.len() {
            let code = params[index];
            match code {
                0 => self.reset(),
                1 => self.modifiers = self.modifiers.union(Modifier::BOLD),
                2 => self.modifiers = self.modifiers.union(Modifier::DIM),
                3 => self.modifiers = self.modifiers.union(Modifier::ITALIC),
                4 => self.modifiers = self.modifiers.union(Modifier::UNDERLINE),
                5 | 6 => self.modifiers = self.modifiers.union(Modifier::SLOW_BLINK),
                7 => self.modifiers = self.modifiers.union(Modifier::REVERSED),
                8 => self.modifiers = self.modifiers.union(Modifier::HIDDEN),
                9 => self.modifiers = self.modifiers.union(Modifier::CROSSED_OUT),
                22 => {
                    self.modifiers = self
                        .modifiers
                        .difference(Modifier::BOLD.union(Modifier::DIM));
                }
                23 => self.modifiers = self.modifiers.difference(Modifier::ITALIC),
                24 => self.modifiers = self.modifiers.difference(Modifier::UNDERLINE),
                25 => self.modifiers = self.modifiers.difference(Modifier::SLOW_BLINK),
                27 => self.modifiers = self.modifiers.difference(Modifier::REVERSED),
                28 => self.modifiers = self.modifiers.difference(Modifier::HIDDEN),
                29 => self.modifiers = self.modifiers.difference(Modifier::CROSSED_OUT),
                30..=37 => self.fg = Some(Color::Indexed(to_u8(code - 30))),
                39 => self.fg = None,
                40..=47 => self.bg = Some(Color::Indexed(to_u8(code - 40))),
                49 => self.bg = None,
                90..=97 => self.fg = Some(Color::Indexed(to_u8(code - 90 + 8))),
                100..=107 => self.bg = Some(Color::Indexed(to_u8(code - 100 + 8))),
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
    lines: VecDeque<Line>,
    status: PaneStatus,
    scroll_offset: usize,
    ansi_state: AnsiStyleState,
    last_overwrite_at: Option<Instant>,
}

impl PaneState {
    fn new(display_name: String) -> Self {
        Self {
            display_name,
            lines: VecDeque::new(),
            status: PaneStatus::Pending,
            scroll_offset: 0,
            ansi_state: AnsiStyleState::default(),
            last_overwrite_at: None,
        }
    }

    fn push_line(&mut self, line: &str, overwrite: bool) {
        let parsed = parse_ansi_line(line, &mut self.ansi_state);
        if overwrite {
            self.last_overwrite_at = Some(Instant::now());
        }
        if overwrite {
            if let Some(last) = self.lines.back_mut() {
                *last = parsed;
            } else {
                self.lines.push_back(parsed);
            }
        } else {
            self.lines.push_back(parsed);
        }
        while self.lines.len() > 2_000 {
            let _ = self.lines.pop_front();
        }
    }

    const fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    const fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    const fn scroll_top(&mut self) {
        self.scroll_offset = usize::MAX;
    }

    const fn scroll_bottom(&mut self) {
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

#[allow(clippy::needless_pass_by_value)]
pub fn run_live_tui(
    tools: &[(String, String)],
    rx: Receiver<ToolEvent>,
    start_time: Instant,
) -> std::io::Result<TuiExit> {
    let mut panes: BTreeMap<String, PaneState> = tools
        .iter()
        .map(|(name, display_name)| (name.clone(), PaneState::new(display_name.clone())))
        .collect();

    let mut guard = CrosstermTerminalGuard::enter(std::io::stdout())?;
    let (width, height) = crossterm::terminal::size()?;
    let mut terminal = Terminal::new(
        guard.writer_mut().expect("active terminal guard"),
        Rect::new(0, 0, width, height),
    );
    let total = tools.len();
    let mut completed = 0_usize;
    let mut focused_index = 0_usize;
    let mut last_frame = Instant::now();

    let loop_result = loop {
        // Bound presentation to 20 FPS even when tool output never stops.
        std::thread::sleep(Duration::from_millis(50).saturating_sub(last_frame.elapsed()));
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
                let batch_start = Instant::now();
                for _ in 0..255 {
                    let Ok(event) = rx.try_recv() else {
                        break;
                    };
                    handle_event(event, &mut panes, &mut completed);
                    if batch_start.elapsed() >= Duration::from_millis(4) {
                        break;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break Ok(TuiExit::Completed),
        }

        let (width, height) = crossterm::terminal::size()?;
        if terminal.area() != Rect::new(0, 0, width, height) {
            terminal.resize(Rect::new(0, 0, width, height));
        }
        last_frame = Instant::now();
        terminal.draw(|frame| {
            render(frame, &panes, tools, completed, start_time, focused_index);
        })?;

        if completed >= total {
            break Ok(TuiExit::Completed);
        }
    };

    drop(terminal);
    guard.leave()?;
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
        ToolEvent::StdoutLine {
            tool_name,
            line,
            overwrite,
        }
        | ToolEvent::StderrLine {
            tool_name,
            line,
            overwrite,
        } => {
            if let Some(pane) = panes.get_mut(&tool_name) {
                pane.push_line(&line, overwrite);
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

struct LogBody<'a>(&'a PaneState);
impl Component for LogBody<'_> {
    fn layout(&self, constraints: Constraints, _: &mut LayoutCx) -> LayoutNode {
        LayoutNode::leaf(
            "log-body".into(),
            constraints.constrain(bmux_tui::component::LogicalSize::new(
                constraints.max_width(),
                self.0.lines.len(),
            )),
        )
    }
    fn paint(&self, layout: &LayoutNode, cx: &mut PaintCx<'_, '_>) {
        let height = layout.size.height;
        let start = self
            .0
            .lines
            .len()
            .saturating_sub(height.saturating_add(self.0.scroll_offset));
        for (row, line) in self.0.lines.iter().skip(start).take(height).enumerate() {
            cx.write_line(
                bmux_tui::paint::LocalRect::new(
                    0,
                    i64::try_from(row).unwrap_or(i64::MAX),
                    layout.size.width,
                    1,
                ),
                line,
            );
        }
    }
}

fn render(
    cx: &mut PaintCx<'_, '_>,
    panes: &BTreeMap<String, PaneState>,
    tools: &[(String, String)],
    completed: usize,
    start_time: Instant,
    focused_index: usize,
) {
    let now = Instant::now();
    let passed = panes
        .values()
        .filter(|pane| pane.status == PaneStatus::Passed)
        .count();
    let failed = panes
        .values()
        .filter(|pane| pane.status == PaneStatus::Failed)
        .count();
    let header = TextBlock::new(format!(
        "Clippier | {completed}/{} complete | {passed} passed | {failed} failed | {:.1?}",
        tools.len(),
        start_time.elapsed()
    ));
    let states = tools
        .iter()
        .map(|_| Cell::new(SurfaceState::new(Rect::new(0, 0, 0, 0))))
        .collect::<Vec<_>>();
    let columns = if tools.len() > 1 { 2 } else { 1 };
    let mut grid = Column::new().id("tools");
    for chunk in (0..tools.len()).collect::<Vec<_>>().chunks(columns) {
        let mut row = Row::new().alignment(bmux_tui::composition::VerticalAlignment::Stretch);
        for &index in chunk {
            let Some(pane) = panes.get(&tools[index].0) else {
                continue;
            };
            let color = if index == focused_index {
                Color::Cyan
            } else {
                match pane.status {
                    PaneStatus::Pending => Color::BrightBlack,
                    PaneStatus::Running => Color::Yellow,
                    PaneStatus::Passed => Color::Green,
                    PaneStatus::Failed => Color::Red,
                }
            };
            let title = format!(
                " {} [{} | {}{}]{} ",
                pane.display_name,
                status_label(pane.status),
                if pane.scroll_offset == 0 {
                    "tail"
                } else {
                    "scroll"
                },
                if pane_is_updating(pane.last_overwrite_at, now) {
                    " | updating"
                } else {
                    ""
                },
                if index == focused_index { " *" } else { "" }
            );
            row = row.flex(Flex::new(
                1,
                PaneComponent::new(
                    format!("tool-{index}"),
                    Pane::new().title(title).styles(PaneStyles {
                        border: Style::new().fg(color),
                        ..PaneStyles::default()
                    }),
                    &states[index],
                    LogBody(pane),
                ),
            ));
        }
        if chunk.len() < columns {
            row = row.flex(Flex::new(1, TextBlock::new("")));
        }
        grid = grid.flex(Flex::new(1, row));
    }
    let hints = TextBlock::new("q/Ctrl-C close · Tab switch · ↑↓/jk scroll · PgUp/PgDn · Home/End");
    let root = Column::new()
        .child(header)
        .flex(Flex::new(1, grid))
        .child(hints);
    let area = cx.area();
    let layout = root.layout(
        Constraints::tight(Size::new(area.width, area.height)),
        &mut LayoutCx::new(),
    );
    root.paint(&layout, cx);
}

fn parse_ansi_line(input: &str, state: &mut AnsiStyleState) -> Line {
    let bytes = input.as_bytes();
    let mut index = 0_usize;
    let mut segment_start = 0_usize;
    let mut spans: Vec<Span> = Vec::new();

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
            b'(' | b')' | b'*' | b'+' | b'-' | b'.' | b'/' => {
                index += 1;
                if index < bytes.len() {
                    index += 1;
                }
                segment_start = index;
            }
            _ => {
                index += 1;
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
        Line::from_spans(spans)
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
        } else if (*byte == b';' || *byte == b':')
            && let Some(value) = current.take()
        {
            values.push(value);
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
            let clamped = to_u8(value.clamp(0, 255));
            Some((Color::Indexed(clamped), 2))
        }
        2 => {
            if index + 3 >= params.len() {
                return None;
            }

            let r = to_u8(params[index + 1].clamp(0, 255));
            let g = to_u8(params[index + 2].clamp(0, 255));
            let b = to_u8(params[index + 3].clamp(0, 255));
            Some((Color::Rgb(r, g, b), 4))
        }
        _ => None,
    }
}

fn to_u8(value: i32) -> u8 {
    u8::try_from(value).unwrap_or(0)
}

fn pane_is_updating(last_overwrite_at: Option<Instant>, now: Instant) -> bool {
    last_overwrite_at
        .is_some_and(|instant| now.saturating_duration_since(instant) <= Duration::from_secs(1))
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
        assert_eq!(line.spans[0].content.as_str(), "hello");
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

    #[test]
    fn parse_ansi_line_ignores_charset_escape_sequence() {
        let mut state = AnsiStyleState::default();
        let line = parse_ansi_line("\u{1b}(Bhello", &mut state);

        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content.as_str(), "hello");
    }

    #[test]
    fn pane_push_line_overwrite_replaces_last_line() {
        let mut pane = PaneState::new("demo".to_string());
        pane.push_line("first", false);
        pane.push_line("second", true);

        assert_eq!(pane.lines.len(), 1);
        assert_eq!(pane.lines[0].spans[0].content.as_str(), "second");
    }

    #[test]
    fn component_grid_renders_status_and_tail_at_small_sizes() {
        use bmux_tui::{buffer::Buffer, frame::Frame};
        let tools = vec![("demo".into(), "Demo".into())];
        let mut pane = PaneState::new("Demo".into());
        for i in 0..100 {
            pane.push_line(&format!("line {i}"), false);
        }
        let panes = BTreeMap::from([("demo".into(), pane)]);
        for (width, height) in [(80, 24), (30, 10), (1, 1), (0, 0)] {
            let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
            render(
                &mut PaintCx::new(&mut Frame::new(&mut buffer)),
                &panes,
                &tools,
                0,
                Instant::now(),
                0,
            );
            if width == 80 {
                let text = buffer
                    .cells()
                    .iter()
                    .map(|cell| cell.symbol.as_str())
                    .collect::<String>();
                assert!(text.contains("line 99"));
                assert!(text.contains("Demo"));
            }
        }
    }

    #[test]
    fn multiple_tools_use_two_columns_even_on_narrow_terminals() {
        use bmux_tui::{buffer::Buffer, frame::Frame};
        let tools = (0..3)
            .map(|i| (format!("tool-{i}"), format!("Tool{i}")))
            .collect::<Vec<_>>();
        let panes = tools
            .iter()
            .map(|(id, name)| (id.clone(), PaneState::new(name.clone())))
            .collect();
        for width in [60, 80, 120] {
            let mut buffer = Buffer::empty(Rect::new(0, 0, width, 24));
            render(
                &mut PaintCx::new(&mut Frame::new(&mut buffer)),
                &panes,
                &tools,
                0,
                Instant::now(),
                0,
            );
            let rows = buffer
                .cells()
                .chunks(usize::from(width))
                .map(|row| {
                    row.iter()
                        .map(|cell| cell.symbol.as_str())
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            assert!(
                rows.iter()
                    .any(|row| row.contains("Tool0") && row.contains("Tool1")),
                "panes must be side by side at width {width}"
            );
            assert!(
                rows.iter().filter(|row| row.starts_with('│')).count() >= 16,
                "empty panes must fill their allocated grid height at width {width}"
            );
            let last = rows.iter().find(|row| row.contains("Tool2")).unwrap();
            assert!(
                last.chars().skip(usize::from(width / 2)).all(|c| c == ' '),
                "odd final pane must remain half-width"
            );
        }
    }

    #[test]
    fn pane_is_updating_true_with_recent_overwrite() {
        let now = Instant::now();
        assert!(pane_is_updating(Some(now), now));
        assert!(!pane_is_updating(None, now));
    }
}
