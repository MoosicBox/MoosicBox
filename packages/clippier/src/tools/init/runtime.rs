//! Ordered input reduction and cadence-limited inline presentation.
use super::{
    inline::{render_scene, stops},
    recommendations::Group,
};
use bmux_keyboard::KeyCode;
use bmux_tui::{event::Event, geometry::Size, inline::InlineTerminal};
use bmux_tui_components::scroll_view::ScrollViewState;
use bmux_tui_runtime::{
    PresentReport, Presenter, Program, ResetReason, Runtime, RuntimeConfig, RuntimeError,
    RuntimeEvent, TerminalInput, Update,
};
use std::{
    cell::Cell,
    io::{self, Write},
    time::{Duration, Instant},
};

pub(super) struct Setup<'a> {
    pub groups: &'a mut [Group],
    pub positions: Vec<(usize, usize)>,
    pub focus: usize,
    pub size: Size,
    pub accepted: bool,
    pub pending_since: Option<Instant>,
    pub hits: Vec<bmux_tui::hit::HitRegion>,
    pub pressed: Option<String>,
    pub hovered: Option<String>,
}

impl Program for Setup<'_> {
    type Message = io::Error;
    type Error = io::Error;
    #[allow(clippy::too_many_lines)]
    fn update(&mut self, event: RuntimeEvent<Self::Message>) -> io::Result<Update<Self::Message>> {
        if self.accepted {
            return Ok(Update::none());
        }
        let before = self.focus;
        let mut changed = false;
        match event {
            RuntimeEvent::Message(error) => return Err(error),
            RuntimeEvent::Terminal(Event::Resize(size)) => {
                changed = size != self.size;
                self.size = size;
                self.hits.clear();
                self.hovered = None;
                self.pressed = None;
            }
            RuntimeEvent::Terminal(Event::Mouse(mouse)) => {
                use bmux_tui::event::{MouseButton, MouseEventKind};
                let target = self
                    .hits
                    .iter()
                    .rev()
                    .filter(|hit| {
                        hit.area.contains(mouse.position)
                            && hit.enabled
                            && hit.visible
                            && (hit.id.as_str().starts_with("choice-")
                                || hit.id.as_str().starts_with("section-")
                                || hit.id.as_str().starts_with("action-"))
                    })
                    .max_by_key(|hit| (hit.layer, !hit.id.as_str().starts_with("section-")))
                    .map(|hit| hit.id.as_str().to_owned());
                if self.hovered != target {
                    self.hovered.clone_from(&target);
                    changed = true;
                }
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        self.pressed.clone_from(&target);
                        if let Some(id) = target {
                            let parts = id.split('-').collect::<Vec<_>>();
                            if let Some(index) =
                                parts.get(1).and_then(|value| value.parse::<usize>().ok())
                            {
                                if parts[0] == "action" {
                                    self.focus = self.positions.len() + index;
                                } else {
                                    let choice = if parts[0] == "choice" {
                                        parts
                                            .get(2)
                                            .and_then(|value| value.parse().ok())
                                            .unwrap_or(0)
                                    } else {
                                        0
                                    };
                                    if let Some(focus) = self
                                        .positions
                                        .iter()
                                        .position(|&stop| stop == (index, choice))
                                    {
                                        self.focus = focus;
                                    }
                                }
                            }
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if let Some(id) = self
                            .pressed
                            .take()
                            .filter(|pressed| Some(pressed) == target.as_ref())
                        {
                            if id.starts_with("choice-") && self.focus < self.positions.len() {
                                let (g, c) = self.positions[self.focus];
                                self.groups[g].choices[c].selected =
                                    !self.groups[g].choices[c].selected;
                                changed = true;
                            } else if id == "action-0" {
                                self.accepted = true;
                                return Ok(Update::exit());
                            } else if id == "action-1" {
                                return Err(io::Error::new(
                                    io::ErrorKind::Interrupted,
                                    "Setup cancelled; no files written",
                                ));
                            }
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        self.focus = (self.focus + 1).min(self.positions.len() + 1);
                    }
                    MouseEventKind::ScrollUp => self.focus = self.focus.saturating_sub(1),
                    _ => {}
                }
            }
            RuntimeEvent::Terminal(Event::Key(key)) => match key.key {
                KeyCode::Escape | KeyCode::Char('q') => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Setup cancelled; no files written",
                    ));
                }
                KeyCode::Char('c') if key.modifiers.ctrl => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Setup cancelled; no files written",
                    ));
                }
                KeyCode::Enter if key.modifiers.ctrl || self.focus == self.positions.len() => {
                    self.accepted = true;
                    return Ok(Update::exit());
                }
                KeyCode::Enter | KeyCode::Space | KeyCode::Char(' ')
                    if self.focus > self.positions.len() =>
                {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Setup cancelled; no files written",
                    ));
                }
                KeyCode::Space | KeyCode::Char(' ') if self.focus == self.positions.len() => {
                    self.accepted = true;
                    return Ok(Update::exit());
                }
                KeyCode::Left | KeyCode::Right
                    if self.size.width.saturating_sub(1) >= 120
                        && self.focus < self.positions.len() =>
                {
                    let columns = usize::from((self.size.width.saturating_sub(1) / 60).clamp(1, 3));
                    let groups = self
                        .groups
                        .iter()
                        .enumerate()
                        .filter(|(_, group)| !group.choices.is_empty())
                        .map(|(index, _)| index)
                        .collect::<Vec<_>>();
                    let per_column = groups.len().div_ceil(columns);
                    let (group, choice) = self.positions[self.focus];
                    let index = groups
                        .iter()
                        .position(|&value| value == group)
                        .expect("focused group");
                    let target = if key.key == KeyCode::Right {
                        index
                            .checked_add(per_column)
                            .filter(|&value| value < groups.len())
                    } else {
                        index.checked_sub(per_column)
                    };
                    if let Some(target) = target {
                        let group = groups[target];
                        let choice = choice.min(self.groups[group].choices.len() - 1);
                        self.focus = self
                            .positions
                            .iter()
                            .position(|&value| value == (group, choice))
                            .expect("focus stop");
                    }
                }
                KeyCode::Up | KeyCode::Left => self.focus = self.focus.saturating_sub(1),
                KeyCode::Tab if key.modifiers.shift => self.focus = self.focus.saturating_sub(1),
                KeyCode::Down | KeyCode::Right | KeyCode::Tab => {
                    self.focus = (self.focus + 1).min(self.positions.len() + 1);
                }
                KeyCode::Home => self.focus = 0,
                KeyCode::End => self.focus = self.positions.len() + 1,
                KeyCode::Enter | KeyCode::Space | KeyCode::Char(' ') => {
                    let (g, c) = self.positions[self.focus];
                    self.groups[g].choices[c].selected = !self.groups[g].choices[c].selected;
                    changed = true;
                }
                _ => {}
            },
            _ => {}
        }
        if changed || before != self.focus {
            self.pending_since.get_or_insert_with(Instant::now);
            Ok(Update::redraw())
        } else {
            Ok(Update::none())
        }
    }
}

#[derive(Default, Debug)]
struct Timings {
    frames: u64,
    render: Duration,
    output: Duration,
    max_update_to_present: Duration,
}
struct InlinePresenter<'a, W: Write> {
    terminal: InlineTerminal<W>,
    header: &'a [String],
    scroll: Cell<ScrollViewState>,
    timings: Timings,
}
impl<W: Write> Presenter<Setup<'_>> for InlinePresenter<'_, W> {
    type Error = io::Error;
    fn reset(&mut self, _: ResetReason) {}
    fn present(&mut self, program: &mut Setup<'_>) -> io::Result<PresentReport> {
        let size = program.size;
        if size.width < 2 || size.height < 2 {
            return Ok(PresentReport::default());
        }
        let start = Instant::now();
        let (buffer, hits) = render_scene(
            program.groups,
            self.header,
            program
                .positions
                .get(program.focus)
                .copied()
                .unwrap_or_else(|| (usize::MAX, program.focus - program.positions.len())),
            size.width - 1,
            size.height - 1,
            &self.scroll,
            program.hovered.as_deref(),
        );
        self.timings.render += start.elapsed();
        let start = Instant::now();
        self.terminal.draw(&buffer)?;
        program.hits = hits;
        self.timings.output += start.elapsed();
        self.timings.frames += 1;
        if let Some(since) = program.pending_since.take() {
            self.timings.max_update_to_present =
                self.timings.max_update_to_present.max(since.elapsed());
        }
        Ok(PresentReport::default())
    }
}

struct MouseCapture;
impl MouseCapture {
    fn enter() -> io::Result<Self> {
        crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
        Ok(Self)
    }
}
impl Drop for MouseCapture {
    fn drop(&mut self) {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    }
}

pub(super) fn run(
    groups: &mut [Group],
    header: &[String],
    output: &mut impl Write,
) -> io::Result<()> {
    let positions = stops(groups);
    if positions.is_empty() {
        return Ok(());
    }
    let _mouse = MouseCapture::enter()?;
    let (width, height) = crossterm::terminal::size()?;
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (timings, stats) = executor.block_on(async {
        let presenter = InlinePresenter {
            terminal: InlineTerminal::enter(&mut *output)?,
            header,
            scroll: Cell::new(ScrollViewState::new()),
            timings: Timings::default(),
        };
        let program = Setup {
            groups,
            positions,
            focus: 0,
            size: Size::new(width, height),
            accepted: false,
            pending_since: None,
            hits: Vec::new(),
            pressed: None,
            hovered: None,
        };
        let (runtime, handle) = Runtime::new(program, presenter, RuntimeConfig::default());
        let mut input = TerminalInput::start::<Setup<'_>>(handle, |error| error);
        let result = runtime.run().await;
        // Release the event stream before returning to line-oriented stdin prompts.
        input.shutdown().await;
        let mut finished = match result {
            Ok(output) => output,
            Err(RuntimeError::Program { error, .. } | RuntimeError::Presenter { error, .. }) => {
                return Err(error);
            }
        };
        finished.presenter.terminal.clear()?;
        let summary = (format!("{:?}", finished.presenter.timings), finished.stats);
        Ok(summary)
    })?;
    if std::env::var_os("CLIPPIER_INIT_PROFILE").is_some() {
        writeln!(output, "init profile: {timings}\nscheduler: {stats:?}")?;
    }
    Ok(())
}
