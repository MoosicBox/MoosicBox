//! Terminal user interface (TUI) implementation.
//!
//! This module provides a terminal-based UI for monitoring simulation progress
//! when the `tui` feature is enabled. It displays real-time information about
//! running simulations including progress bars, step counts, and status.

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

/// Information about a running simulation displayed in the TUI.
#[derive(Debug, Clone, Copy)]
struct SimulationInfo {
    thread_id: u64,
    run_number: u64,
    step: u64,
    config: SimConfig,
    progress: f64,
    failed: bool,
}

/// Shared state for the TUI display.
///
/// This struct manages the state of all running simulations and the terminal
/// itself. It is thread-safe and can be cloned to share across threads.
#[derive(Debug, Clone)]
pub struct DisplayState {
    running: Arc<AtomicBool>,
    simulations: Arc<RwLock<Vec<SimulationInfo>>>,
    terminal: Arc<RwLock<Option<DefaultTerminal>>>,
    runs_completed: Arc<RwLock<u64>>,
}

impl DisplayState {
    /// Creates a new `DisplayState` with default values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            terminal: Arc::new(RwLock::new(None)),
            running: Arc::new(AtomicBool::new(true)),
            simulations: Arc::new(RwLock::new(vec![])),
            runs_completed: Arc::new(RwLock::new(0)),
        }
    }

    /// Increments the completed run counter.
    pub fn run_completed(&self) {
        let mut runs_completed = self.runs_completed.write().unwrap();
        *runs_completed += 1;
    }

    /// Updates the current step count for a simulation.
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

    /// Updates or creates simulation state for a specific thread.
    ///
    /// If a simulation with the given `thread_id` exists, updates its state.
    /// Otherwise, creates a new simulation entry and inserts it in sorted order.
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

    /// Draws the current state to the terminal.
    ///
    /// # Errors
    ///
    /// * Returns an error if the terminal has not been initialized
    /// * Returns an error if terminal drawing fails
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

    /// Returns the number of completed simulation runs.
    #[must_use]
    fn runs_completed(&self) -> u64 {
        *self.runs_completed.read().unwrap()
    }

    /// Signals the TUI to exit gracefully.
    pub fn exit(&self) {
        log::debug!("exiting the tui");
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Initializes the terminal for TUI display.
    ///
    /// # Errors
    ///
    /// * Returns an error if terminal initialization fails
    /// * Returns an error if terminal setup fails
    fn init_terminal(&self) -> std::io::Result<()> {
        let terminal = ratatui::try_init()?;
        log::debug!("PANIC HOOK OVERRODE");
        self.set_terminal(terminal)?;
        Ok(())
    }

    /// Sets the terminal instance and prepares it for display.
    ///
    /// # Errors
    ///
    /// * Returns an error if terminal clearing fails
    /// * Returns an error if terminal flushing fails
    /// * Returns an error if cursor positioning fails
    fn set_terminal(&self, mut terminal: DefaultTerminal) -> std::io::Result<()> {
        log::debug!("set_terminal");
        terminal.clear()?;
        terminal.flush()?;
        terminal.set_cursor_position(Position::ORIGIN)?;
        *self.terminal.write().unwrap() = Some(terminal);

        Ok(())
    }

    /// Restores the terminal to its original state.
    ///
    /// # Errors
    ///
    /// * Returns an error if cursor restoration fails
    fn restore(&self) -> std::io::Result<()> {
        log::debug!("restore");
        if let Some(terminal) = &mut *self.terminal.write().unwrap() {
            terminal.show_cursor()?;
        }
        ratatui::restore();

        Ok(())
    }
}

/// Spawns the TUI in a separate thread.
///
/// Starts the TUI event loop and rendering loop in a new thread. Blocks until
/// the TUI is initialized before returning the join handle.
#[must_use]
pub fn spawn(state: DisplayState) -> JoinHandle<std::io::Result<()>> {
    let (tx, rx) = oneshot::channel();

    let handle = std::thread::spawn(move || start(tx, &state));

    let _ = rx.recv();

    handle
}

/// Starts the TUI and runs the event and render loops.
///
/// Initializes the terminal, spawns the event loop, and runs the render loop
/// until the simulation completes or the user exits.
///
/// # Errors
///
/// * Returns an error if terminal initialization fails
/// * Returns an error if the event loop fails
/// * Returns an error if terminal restoration fails
/// * Returns an error if the render loop fails
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

/// Spawns the event handling loop in a separate thread.
///
/// Monitors keyboard input and handles Ctrl-C or 'q' to exit the simulation.
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

/// Runs the TUI render loop.
///
/// Continuously redraws the display until the simulation completes or is cancelled.
///
/// # Errors
///
/// * Returns an error if drawing fails
fn run(state: &DisplayState) -> std::io::Result<()> {
    while state.running.load(std::sync::atomic::Ordering::SeqCst) {
        state.draw()?;

        std::thread::sleep(Duration::from_millis(100));
    }
    log::debug!("run loop finished");
    Ok(())
}

/// Renders the TUI frame with simulation progress information.
///
/// Displays a grid of progress bars showing the status of all running simulations.
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

#[cfg(test)]
#[allow(clippy::significant_drop_tightening)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn default_config() -> SimConfig {
        SimConfig::new()
    }

    #[test_log::test]
    fn test_display_state_new_initializes_with_empty_simulations() {
        let state = DisplayState::new();

        assert!(state.simulations.read().unwrap().is_empty());
        assert!(state.running.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(*state.runs_completed.read().unwrap(), 0);
    }

    #[test_log::test]
    fn test_run_completed_increments_counter() {
        let state = DisplayState::new();

        assert_eq!(state.runs_completed(), 0);

        state.run_completed();
        assert_eq!(state.runs_completed(), 1);

        state.run_completed();
        state.run_completed();
        assert_eq!(state.runs_completed(), 3);
    }

    #[test_log::test]
    fn test_update_sim_state_adds_new_simulation() {
        let state = DisplayState::new();
        let config = default_config();

        state.update_sim_state(1, 1, config, 0.5, false);

        let simulations = state.simulations.read().unwrap();
        assert_eq!(simulations.len(), 1);
        assert_eq!(simulations[0].thread_id, 1);
        assert_eq!(simulations[0].run_number, 1);
        assert!((simulations[0].progress - 0.5).abs() < f64::EPSILON);
        assert!(!simulations[0].failed);
    }

    #[test_log::test]
    fn test_update_sim_state_updates_existing_simulation() {
        let state = DisplayState::new();
        let config = default_config();

        // Add initial state
        state.update_sim_state(1, 1, config, 0.25, false);

        // Update state
        state.update_sim_state(1, 2, config, 0.75, true);

        let simulations = state.simulations.read().unwrap();
        assert_eq!(simulations.len(), 1);
        assert_eq!(simulations[0].thread_id, 1);
        assert_eq!(simulations[0].run_number, 2);
        assert!((simulations[0].progress - 0.75).abs() < f64::EPSILON);
        assert!(simulations[0].failed);
    }

    #[test_log::test]
    fn test_update_sim_state_maintains_sorted_order_ascending() {
        let state = DisplayState::new();
        let config = default_config();

        // Add simulations in ascending order
        state.update_sim_state(1, 1, config, 0.1, false);
        state.update_sim_state(2, 1, config, 0.2, false);
        state.update_sim_state(3, 1, config, 0.3, false);

        let simulations = state.simulations.read().unwrap();
        assert_eq!(simulations.len(), 3);
        assert_eq!(simulations[0].thread_id, 1);
        assert_eq!(simulations[1].thread_id, 2);
        assert_eq!(simulations[2].thread_id, 3);
    }

    #[test_log::test]
    fn test_update_sim_state_maintains_sorted_order_descending() {
        let state = DisplayState::new();
        let config = default_config();

        // Add simulations in descending order
        state.update_sim_state(3, 1, config, 0.3, false);
        state.update_sim_state(2, 1, config, 0.2, false);
        state.update_sim_state(1, 1, config, 0.1, false);

        let simulations = state.simulations.read().unwrap();
        assert_eq!(simulations.len(), 3);
        assert_eq!(simulations[0].thread_id, 1);
        assert_eq!(simulations[1].thread_id, 2);
        assert_eq!(simulations[2].thread_id, 3);
    }

    #[test_log::test]
    fn test_update_sim_state_maintains_sorted_order_random() {
        let state = DisplayState::new();
        let config = default_config();

        // Add simulations in random order
        state.update_sim_state(5, 1, config, 0.5, false);
        state.update_sim_state(2, 1, config, 0.2, false);
        state.update_sim_state(8, 1, config, 0.8, false);
        state.update_sim_state(1, 1, config, 0.1, false);
        state.update_sim_state(4, 1, config, 0.4, false);

        let simulations = state.simulations.read().unwrap();
        assert_eq!(simulations.len(), 5);
        assert_eq!(simulations[0].thread_id, 1);
        assert_eq!(simulations[1].thread_id, 2);
        assert_eq!(simulations[2].thread_id, 4);
        assert_eq!(simulations[3].thread_id, 5);
        assert_eq!(simulations[4].thread_id, 8);
    }

    #[test_log::test]
    fn test_update_sim_step_updates_existing_simulation() {
        let state = DisplayState::new();
        let config = default_config();

        state.update_sim_state(1, 1, config, 0.0, false);

        // Initial step should be 0
        assert_eq!(state.simulations.read().unwrap()[0].step, 0);

        // Update step
        state.update_sim_step(1, 500);
        assert_eq!(state.simulations.read().unwrap()[0].step, 500);

        // Update step again
        state.update_sim_step(1, 1000);
        assert_eq!(state.simulations.read().unwrap()[0].step, 1000);
    }

    #[test_log::test]
    fn test_update_sim_step_does_nothing_for_nonexistent_thread() {
        let state = DisplayState::new();
        let config = default_config();

        state.update_sim_state(1, 1, config, 0.0, false);

        // Update step for non-existent thread
        state.update_sim_step(999, 500);

        // Original simulation should be unchanged
        let simulations = state.simulations.read().unwrap();
        assert_eq!(simulations.len(), 1);
        assert_eq!(simulations[0].thread_id, 1);
        assert_eq!(simulations[0].step, 0);
    }

    #[test_log::test]
    fn test_exit_sets_running_to_false() {
        let state = DisplayState::new();

        assert!(state.running.load(std::sync::atomic::Ordering::SeqCst));

        state.exit();

        assert!(!state.running.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test_log::test]
    fn test_display_state_clone_shares_state() {
        let state1 = DisplayState::new();
        let state2 = state1.clone();

        let config = default_config();
        state1.update_sim_state(1, 1, config, 0.5, false);

        // Clone should see the same simulation
        let simulations = state2.simulations.read().unwrap();
        assert_eq!(simulations.len(), 1);
        assert_eq!(simulations[0].thread_id, 1);
    }

    #[test_log::test]
    fn test_update_sim_state_preserves_config_values() {
        let state = DisplayState::new();
        let mut config = SimConfig::new();
        let _ = config.tcp_capacity(128).duration(Duration::from_secs(60));

        state.update_sim_state(1, 1, config, 0.0, false);

        let simulations = state.simulations.read().unwrap();
        assert_eq!(simulations[0].config.tcp_capacity, 128);
        assert_eq!(simulations[0].config.duration, Duration::from_secs(60));
    }
}
