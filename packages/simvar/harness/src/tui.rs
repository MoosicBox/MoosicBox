use std::{
    sync::{Arc, RwLock, atomic::AtomicBool},
    thread::JoinHandle,
    time::Duration,
};

use oneshot::Sender;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyModifiers},
    layout::{Alignment, Constraint, Direction, Layout, Position},
    style::Style,
    widgets::{Block, Gauge, Paragraph},
};

use crate::{RUNS, SimConfig, end_sim};

#[derive(Debug, Clone, Copy)]
struct SimulationInfo {
    thread_id: u64,
    run_number: u64,
    step: u64,
    config: SimConfig,
    progress: f64,
    failed: bool,
}

#[derive(Debug, Clone)]
pub struct DisplayState {
    running: Arc<AtomicBool>,
    simulations: Arc<RwLock<Vec<SimulationInfo>>>,
    terminal: Arc<RwLock<Option<DefaultTerminal>>>,
    runs_completed: Arc<RwLock<u64>>,
}

impl DisplayState {
    pub fn new() -> Self {
        Self {
            terminal: Arc::new(RwLock::new(None)),
            running: Arc::new(AtomicBool::new(true)),
            simulations: Arc::new(RwLock::new(vec![])),
            runs_completed: Arc::new(RwLock::new(0)),
        }
    }

    pub fn run_completed(&self) {
        let mut runs_completed = self.runs_completed.write().unwrap();
        *runs_completed += 1;
    }

    pub fn update_sim_step(&self, thread_id: u64, step: u64) {
        if let Some(existing) = self
            .simulations
            .write()
            .unwrap()
            .iter_mut()
            .find(|x| x.thread_id == thread_id)
        {
            existing.step = step;
        }
    }

    pub fn update_sim_state(
        &self,
        thread_id: u64,
        run_number: u64,
        config: SimConfig,
        progress: f64,
        failed: bool,
    ) {
        let mut binding = self.simulations.write().unwrap();

        if let Some(existing) = binding.iter_mut().find(|x| x.thread_id == thread_id) {
            existing.run_number = run_number;
            existing.config = config;
            existing.progress = progress;
            existing.failed = failed;
        } else {
            let mut index = None;

            for (i, sim) in binding.iter().enumerate() {
                if thread_id < sim.thread_id && index.is_none_or(|x| i < x) {
                    index = Some(i);
                }
            }

            let info = SimulationInfo {
                thread_id,
                run_number,
                step: 0,
                config,
                progress,
                failed,
            };

            if let Some(index) = index {
                binding.insert(index, info);
            } else {
                binding.push(info);
            }
        }
    }

    fn draw(&self) -> std::io::Result<()> {
        let mut binding = self.terminal.write().unwrap();

        binding
            .as_mut()
            .ok_or_else(|| {
                use std::io::{Error, ErrorKind};

                Error::new(
                    ErrorKind::Unsupported,
                    "terminal has not been created. call tui::start",
                )
            })?
            .draw(|frame| render(self, frame))?;

        drop(binding);

        Ok(())
    }

    fn runs_completed(&self) -> u64 {
        *self.runs_completed.read().unwrap()
    }

    pub fn exit(&self) {
        log::debug!("exiting the tui");
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn init_terminal(&self) -> std::io::Result<()> {
        let terminal = ratatui::try_init()?;
        log::debug!("PANIC HOOK OVERRODE");
        self.set_terminal(terminal)?;
        Ok(())
    }

    fn set_terminal(&self, mut terminal: DefaultTerminal) -> std::io::Result<()> {
        log::debug!("set_terminal");
        terminal.clear()?;
        terminal.flush()?;
        terminal.set_cursor_position(Position::ORIGIN)?;
        *self.terminal.write().unwrap() = Some(terminal);

        Ok(())
    }

    fn restore(&self) -> std::io::Result<()> {
        log::debug!("restore");
        if let Some(terminal) = &mut *self.terminal.write().unwrap() {
            terminal.show_cursor()?;
        }
        ratatui::restore();

        Ok(())
    }
}

pub fn spawn(state: DisplayState) -> JoinHandle<std::io::Result<()>> {
    let (tx, rx) = oneshot::channel();

    let handle = std::thread::spawn(move || start(tx, &state));

    let _ = rx.recv();

    handle
}

pub fn start(start_tx: Sender<()>, state: &DisplayState) -> std::io::Result<()> {
    state.init_terminal()?;
    start_tx.send(()).unwrap();
    let event_loop = spawn_event_loop(state);
    let result = run(state);
    state.restore()?;
    event_loop.join().unwrap()?;
    log::debug!("closing tui");

    result
}

fn spawn_event_loop(state: &DisplayState) -> JoinHandle<std::io::Result<()>> {
    let state = state.clone();

    std::thread::spawn(move || {
        while state.running.load(std::sync::atomic::Ordering::SeqCst) {
            if matches!(event::poll(Duration::from_millis(50)), Ok(true)) {
                match event::read()? {
                    Event::FocusGained
                    | Event::FocusLost
                    | Event::Mouse(..)
                    | Event::Paste(..)
                    | Event::Resize(..) => {}
                    Event::Key(key) => {
                        let exit = (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL))
                            || (key.code == KeyCode::Char('q') && key.modifiers.is_empty());
                        if exit {
                            state.exit();
                            end_sim();
                            return Ok::<_, std::io::Error>(());
                        }
                    }
                }
            }
        }
        log::debug!("read loop finished");

        Ok(())
    })
}

fn run(state: &DisplayState) -> std::io::Result<()> {
    while state.running.load(std::sync::atomic::Ordering::SeqCst) {
        state.draw()?;

        std::thread::sleep(Duration::from_millis(100));
    }
    log::debug!("run loop finished");
    Ok(())
}

#[allow(clippy::similar_names, clippy::too_many_lines)]
fn render(state: &DisplayState, frame: &mut Frame) {
    let area = frame.area();

    log::trace!("render: start frame.size=({}, {})", area.width, area.height);

    let simulations = state.simulations.read().unwrap();

    let [header_area, gauges_area] = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(1), Constraint::Fill(1)])
        .areas(area);

    log::trace!(
        "render: header_area.size=({}, {}) gauges_area.size=({}, {})",
        header_area.width,
        header_area.height,
        gauges_area.width,
        gauges_area.height,
    );

    let runs = *RUNS;
    let header = if runs > 1 {
        format!("Simulations {}/{runs}", state.runs_completed())
    } else {
        "Simulations".to_string()
    };
    let header_widget = Paragraph::new(header).alignment(Alignment::Center);

    frame.render_widget(header_widget, header_area);

    if simulations.is_empty() {
        return;
    }

    let height = gauges_area.height;
    let gauge_height: u16 = 3;
    let gauge_margin: u16 = 0;
    let (gauges_height, gauges_per_height) = {
        let mut current_height = 0;
        let mut gauge_count = 0;

        while (gauge_count as usize) < simulations.len() {
            current_height += gauge_height;

            if current_height >= height {
                break;
            }

            gauge_count += 1;
            current_height += gauge_margin;
        }

        let gauge_count = std::cmp::max(1, gauge_count);
        let gauges_height = gauge_count * gauge_height + ((gauge_count - 1) * gauge_margin);

        (gauges_height, gauge_count)
    };

    let required_height = {
        let mut required_height = 0;
        let mut gauge_count = 0;

        while (gauge_count as usize) < simulations.len() {
            required_height += gauges_height;
            gauge_count += gauges_per_height;
        }

        required_height
    };

    log::trace!(
        "\
        render: \
        height={height} \
        gauge_height={gauge_height} \
        gauge_margin={gauge_margin} \
        required_height={required_height} \
        gauges_per_height={gauges_per_height} \
        gauges_height={gauges_height}\
        "
    );

    let columns = {
        let mut required_height = required_height;
        let mut columns = vec![];

        while required_height >= height {
            columns.push(Constraint::Fill(1));
            required_height -= gauges_height;
        }

        columns.push(Constraint::Fill(1));
        columns
    };

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints(&columns)
        .split(gauges_area);

    let mut remaining_height = required_height;
    let mut offset = 0;

    log::trace!(
        "render: rendering columns={} remaining_height={remaining_height}",
        columns.len()
    );

    for (i, &column) in columns.iter().enumerate() {
        let rows = gauges_per_height;
        remaining_height -= gauges_height;

        log::trace!(
            "render: rows={rows} remaining_height={remaining_height} offset={offset} column.size=({}, {})",
            column.width,
            column.height,
        );

        let gauges_areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(std::iter::repeat_n(
                Constraint::Length(gauge_height),
                rows as usize,
            ))
            .split(column);

        for (&area, sim) in gauges_areas.iter().zip(simulations.iter().skip(offset)) {
            log::trace!("render: render col={i}, sim={}", sim.thread_id);

            let style = Style::new();
            let style = if sim.failed {
                style.red()
            } else {
                style.white()
            };
            let style = style.on_black().italic();

            let gauge = Gauge::default()
                .block(Block::bordered().title(format!(
                    "Thread {} / Run {} / Seed {} / Step {step}",
                    sim.thread_id,
                    sim.run_number,
                    sim.config.seed,
                    step = if sim.config.duration < Duration::MAX {
                        format!("[{}/{}]", sim.step, sim.config.duration.as_millis())
                    } else {
                        sim.step.to_string()
                    }
                )))
                .gauge_style(style)
                .ratio(sim.progress);

            frame.render_widget(gauge, area);
        }

        offset += rows as usize;
    }

    drop(simulations);

    log::trace!("render: end");
}
