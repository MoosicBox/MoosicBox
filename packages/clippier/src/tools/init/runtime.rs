//! Ordered input reduction and cadence-limited inline presentation.
use super::{
    inline::{render, stops},
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
}

impl Program for Setup<'_> {
    type Message = io::Error;
    type Error = io::Error;
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
                KeyCode::Up => self.focus = self.focus.saturating_sub(1),
                KeyCode::Tab if key.modifiers.shift => self.focus = self.focus.saturating_sub(1),
                KeyCode::Down | KeyCode::Tab => {
                    self.focus = (self.focus + 1).min(self.positions.len() - 1);
                }
                KeyCode::Home => self.focus = 0,
                KeyCode::End => self.focus = self.positions.len() - 1,
                KeyCode::Space | KeyCode::Char(' ') => {
                    let (g, c) = self.positions[self.focus];
                    self.groups[g].choices[c].selected = !self.groups[g].choices[c].selected;
                    changed = true;
                }
                KeyCode::Enter => {
                    self.accepted = true;
                    return Ok(Update::exit());
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
        let buffer = render(
            program.groups,
            self.header,
            program.positions[program.focus],
            size.width - 1,
            size.height - 1,
            &self.scroll,
        );
        self.timings.render += start.elapsed();
        let start = Instant::now();
        self.terminal.draw(&buffer)?;
        self.timings.output += start.elapsed();
        self.timings.frames += 1;
        if let Some(since) = program.pending_since.take() {
            self.timings.max_update_to_present =
                self.timings.max_update_to_present.max(since.elapsed());
        }
        Ok(PresentReport::default())
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
