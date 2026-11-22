//! simvar's terminal display: a progress gauge per running simulation, drawn with ratatui.
//!
//! The display is a [`SimObserver`]: everything it shows comes from [`SimEvent`]s, so a
//! harness only registers it (see [`spawn`]) and never feeds it directly.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    io,
    sync::{
        Arc, Mutex, PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyModifiers},
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::Style,
    widgets::{Block, Gauge, Paragraph},
};
use simvar_observer::{RunKind, SimEvent, SimObserver};

/// One worker's current run, as the display shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RunInfo {
    run: u64,
    seed: u64,
    step: u64,
    /// The run's length in steps, when it has one.
    steps: Option<u64>,
    progress: f64,
    failed: bool,
}

/// What the display knows, built from events.
#[derive(Debug, Default)]
struct State {
    /// Runs in the simulation, once it has started (`None` before, or when unbounded).
    runs: Option<u64>,
    completed: u64,
    /// Each worker's current or last run.
    workers: BTreeMap<u64, RunInfo>,
}

impl State {
    fn apply(&mut self, event: &SimEvent) {
        match *event {
            SimEvent::Started { runs, .. } => self.runs = runs,
            // Replay and shrink jobs run beside the fleet; the display shows only the fleet.
            SimEvent::RunStarted { kind, .. } if kind != RunKind::Fleet => {}
            SimEvent::RunStarted {
                id, seed, steps, ..
            } => {
                self.workers.insert(
                    id.worker,
                    RunInfo {
                        run: id.run,
                        seed,
                        step: 0,
                        steps,
                        progress: 0.0,
                        failed: false,
                    },
                );
            }
            // Jobs report under worker 0; they are not fleet runs.
            SimEvent::Progress { id, .. } | SimEvent::RunFinished { id, .. } if id.worker == 0 => {}
            SimEvent::Progress { id, step, progress } => {
                if let Some(info) = self.workers.get_mut(&id.worker) {
                    info.step = step;
                    info.progress = progress.unwrap_or(info.progress);
                }
            }
            SimEvent::RunFinished {
                id, success, steps, ..
            } => {
                self.completed += 1;
                if let Some(info) = self.workers.get_mut(&id.worker) {
                    info.step = steps;
                    if let Some(total) = info.steps {
                        #[allow(clippy::cast_precision_loss)]
                        let progress = (steps as f64 / total.max(1) as f64).clamp(0.0, 1.0);
                        info.progress = progress;
                    }
                    info.failed = !success;
                }
            }
            _ => {}
        }
    }
}

/// The running display: an observer, and the handle that ends it.
pub struct Display {
    state: Arc<RwLock<State>>,
    running: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<io::Result<()>>>>,
}

impl std::fmt::Debug for Display {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Display")
            .field("running", &self.running.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl SimObserver for Display {
    fn event(&self, event: SimEvent) {
        self.state
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .apply(&event);
    }
}

impl Display {
    /// Stop drawing and give the terminal back. [`Self::join`] then returns.
    pub fn exit(&self) {
        log::debug!("exiting the tui");
        self.running.store(false, Ordering::SeqCst);
    }

    /// Wait until the display ends: the user quit, or [`Self::exit`] was called.
    ///
    /// # Errors
    ///
    /// * If drawing or restoring the terminal failed
    ///
    /// # Panics
    ///
    /// * If the display's thread panicked
    pub fn join(&self) -> io::Result<()> {
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        thread.map_or(Ok(()), |thread| {
            thread.join().expect("the display thread panicked")
        })
    }
}

/// Take over the terminal and start drawing.
///
/// `on_quit` runs when the user presses `q` or Ctrl-C (typically: end the simulation).
/// Register the result with the harness as an observer, and [`Display::join`] it once the
/// simulation is done.
///
/// # Errors
///
/// * If the terminal cannot be set up
///
/// # Panics
///
/// * If the display's input thread panics
pub fn spawn(on_quit: impl Fn() + Send + 'static) -> io::Result<Arc<Display>> {
    let state = Arc::new(RwLock::new(State::default()));
    let running = Arc::new(AtomicBool::new(true));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn({
        let state = Arc::clone(&state);
        let running = Arc::clone(&running);
        move || {
            let terminal = match init_terminal() {
                Ok(terminal) => {
                    let _ = ready_tx.send(Ok(()));
                    terminal
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return Ok(());
                }
            };
            let input = spawn_input_loop(Arc::clone(&running), on_quit);
            let result = draw_loop(terminal, &state, &running);
            ratatui::restore();
            input.join().expect("the input thread panicked")?;
            log::debug!("closing tui");
            result
        }
    });
    ready_rx
        .recv()
        .map_err(|_| io::Error::other("the display thread ended before starting"))??;
    Ok(Arc::new(Display {
        state,
        running,
        thread: Mutex::new(Some(thread)),
    }))
}

fn init_terminal() -> io::Result<DefaultTerminal> {
    let mut terminal = ratatui::try_init()?;
    terminal.clear()?;
    terminal.set_cursor_position(Position::ORIGIN)?;
    Ok(terminal)
}

/// Watch the keyboard for `q` and Ctrl-C until the display ends.
fn spawn_input_loop(
    running: Arc<AtomicBool>,
    on_quit: impl Fn() + Send + 'static,
) -> JoinHandle<io::Result<()>> {
    std::thread::spawn(move || {
        while running.load(Ordering::SeqCst) {
            if !event::poll(Duration::from_millis(50))? {
                continue;
            }
            if let Event::Key(key) = event::read()? {
                let quit = (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL))
                    || (key.code == KeyCode::Char('q') && key.modifiers.is_empty());
                if quit {
                    running.store(false, Ordering::SeqCst);
                    on_quit();
                }
            }
        }
        Ok(())
    })
}

fn draw_loop(
    mut terminal: DefaultTerminal,
    state: &RwLock<State>,
    running: &AtomicBool,
) -> io::Result<()> {
    while running.load(Ordering::SeqCst) {
        terminal.draw(|frame| {
            render(&state.read().unwrap_or_else(PoisonError::into_inner), frame);
        })?;
        std::thread::sleep(Duration::from_millis(100));
    }
    terminal.show_cursor()
}

/// The gauges' columns: as many gauges per column as fit, in as many columns as needed.
fn columns(area: Rect, count: usize, gauge_height: u16) -> (Vec<Rect>, usize) {
    let per_column = usize::from((area.height / gauge_height).max(1));
    let columns = count.div_ceil(per_column).max(1);
    let rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(std::iter::repeat_n(Constraint::Fill(1), columns))
        .split(area)
        .to_vec();
    (rects, per_column)
}

fn render(state: &State, frame: &mut Frame) {
    const GAUGE_HEIGHT: u16 = 3;
    let [header_area, gauges_area] = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(1), Constraint::Fill(1)])
        .areas(frame.area());

    let header = match state.runs {
        Some(runs) if runs > 1 => format!("Simulations {}/{runs}", state.completed),
        None => format!("Simulations {}", state.completed),
        _ => "Simulations".to_string(),
    };
    frame.render_widget(
        Paragraph::new(header).alignment(Alignment::Center),
        header_area,
    );

    let (columns, per_column) = columns(gauges_area, state.workers.len(), GAUGE_HEIGHT);
    let mut workers = state.workers.iter();
    for column in columns {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(std::iter::repeat_n(
                Constraint::Length(GAUGE_HEIGHT),
                per_column,
            ))
            .split(column);
        for (&area, (worker, info)) in rows.iter().zip(workers.by_ref()) {
            let style = if info.failed {
                Style::new().red()
            } else {
                Style::new().white()
            };
            let step = info.steps.map_or_else(
                || info.step.to_string(),
                |steps| format!("[{}/{steps}]", info.step),
            );
            let gauge = Gauge::default()
                .block(Block::bordered().title(format!(
                    "Thread {worker} / Run {} / Seed {} / Step {step}",
                    info.run, info.seed
                )))
                .gauge_style(style.on_black().italic())
                .ratio(info.progress.clamp(0.0, 1.0));
            frame.render_widget(gauge, area);
        }
    }
}

#[cfg(test)]
mod tests {
    use simvar_observer::RunId;

    use super::*;

    fn started(worker: u64, run: u64, seed: u64, steps: Option<u64>) -> SimEvent {
        SimEvent::RunStarted {
            id: RunId { worker, run },
            seed,
            props: vec![],
            steps,
            worker,
            kind: RunKind::Fleet,
            epoch_offset: 0,
        }
    }

    fn finished(worker: u64, run: u64, success: bool, steps: u64) -> SimEvent {
        SimEvent::RunFinished {
            id: RunId { worker, run },
            seed: 0,
            success,
            report: String::new(),
            steps,
            real_time: std::time::Duration::ZERO,
            sim_time: std::time::Duration::ZERO,
        }
    }

    #[test]
    fn a_run_appears_under_its_worker_and_follows_progress() {
        let mut state = State::default();
        state.apply(&SimEvent::Started {
            runs: Some(4),
            max_parallel: 1,
        });
        state.apply(&started(2, 1, 42, Some(1000)));
        state.apply(&SimEvent::Progress {
            id: RunId { worker: 2, run: 1 },
            step: 500,
            progress: Some(0.5),
        });
        let info = state.workers[&2];
        assert_eq!((info.run, info.seed, info.step), (1, 42, 500));
        assert!((info.progress - 0.5).abs() < f64::EPSILON);
        assert_eq!(state.runs, Some(4));
    }

    #[test]
    fn a_worker_shows_its_latest_run_and_finished_runs_are_counted() {
        let mut state = State::default();
        state.apply(&started(1, 1, 7, Some(100)));
        state.apply(&finished(1, 1, true, 100));
        state.apply(&started(1, 3, 8, Some(100)));
        state.apply(&finished(1, 3, false, 40));
        let info = state.workers[&1];
        assert_eq!((info.run, info.seed, info.step), (3, 8, 40));
        assert!(info.failed);
        assert!((info.progress - 0.4).abs() < f64::EPSILON);
        assert_eq!(state.completed, 2);
    }

    #[test]
    fn jobs_beside_the_fleet_are_not_shown() {
        let mut state = State::default();
        state.apply(&SimEvent::RunStarted {
            id: RunId { worker: 0, run: 1 },
            seed: 5,
            props: vec![],
            steps: None,
            worker: 0,
            kind: RunKind::Replay,
            epoch_offset: 0,
        });
        state.apply(&finished(0, 1, false, 10));
        assert!(state.workers.is_empty());
        assert_eq!(state.completed, 0);
    }

    #[test]
    fn an_unbounded_simulation_counts_its_runs() {
        let mut state = State::default();
        state.apply(&SimEvent::Started {
            runs: None,
            max_parallel: 1,
        });
        state.apply(&started(1, 1, 3, None));
        state.apply(&finished(1, 1, true, 5));
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&state, frame)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(text.contains("Simulations 1"), "{text}");
    }

    #[test]
    fn workers_are_listed_in_order() {
        let mut state = State::default();
        for worker in [3, 1, 2] {
            state.apply(&started(worker, worker, 0, None));
        }
        assert_eq!(state.workers.keys().copied().collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn gauges_fill_columns_top_to_bottom() {
        let area = Rect::new(0, 0, 90, 10);
        assert_eq!(columns(area, 3, 3).0.len(), 1);
        let (rects, per_column) = columns(area, 7, 3);
        assert_eq!((rects.len(), per_column), (3, 3));
        assert_eq!(columns(area, 0, 3).0.len(), 1);
    }

    #[test]
    fn rendering_shows_each_run() {
        let mut state = State::default();
        state.apply(&SimEvent::Started {
            runs: Some(2),
            max_parallel: 1,
        });
        state.apply(&started(1, 1, 42, None));
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&state, frame)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(text.contains("Simulations 0/2"), "{text}");
        assert!(text.contains("Run 1 / Seed 42"), "{text}");
    }
}
