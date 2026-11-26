//! A simulated kernel: terminals, pipes, processes and signals, for deterministic simulation.
//!
//! Each simulation run (a `switchy_simulation` context) has its own [`Kernel`]. Every
//! operation on it is a `switchy_thread` switch point, and anything that has to wait blocks on
//! the run's scheduler, so the interleaving of terminal readers, writers and simulated programs
//! is decided by the run's seed. Timeouts use the run's simulated clock.
//!
//! Programs are Rust closures registered by name ([`register_program`]); [`PtySlave::spawn`]
//! runs the one named by the command's first argument on a simulated thread, with a
//! [`Process`] for its standard streams, signals and children.
//!
//! What is modelled:
//!
//! * **Terminals**: bounded buffers in each direction, so a writer that is not read blocks; a
//!   line discipline subset (echo, `\r` to `\n` on input, `\n` to `\r\n` on output, `ctrl+c`
//!   interrupting the foreground group); canonical input, where typed lines are edited (erase,
//!   word erase, kill) before a program reads them a line at a time, and the end-of-file
//!   character ends a partial line or the input; window size changes with `SIGWINCH`; hang-up
//!   when the master closes; end of file on the master once nothing holds the slave side.
//! * **Personalities** ([`Personality`]): on Linux a terminal stays open while anything that
//!   inherited it is alive; on macOS it ends with its session. On Windows a terminal is a
//!   pseudo-console: it asks for the cursor position before its programs' output can flow,
//!   its output ends only when the console is closed, processes cannot be signalled from
//!   outside (only terminated, with exit code 1), and nothing has process groups.
//! * **Processes**: sessions, process groups, controlling terminals, zombies and reaping,
//!   exit codes and signals with per-signal handling, children that outlive their parent, and
//!   process ids reused as soon as nothing uses them any more.
//! * **Faults** ([`Faults`]): interrupted and short reads and writes on the master side,
//!   failed spawns, reads failing with an input/output error, writes stalling, processes
//!   killed from outside (as by an out-of-memory killer), signals delivered late, and
//!   pseudo-consoles whose startup cursor query never comes or comes late, at seeded rates.
//!
//! The kernel also keeps what really happened (every byte each terminal and pipe was written,
//! every signal delivered), for checking what a program under test observed; see [`snapshot`]
//! and [`terminal_written`].

mod programs;

pub use programs::install_default_programs;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::{OsStr, OsString},
    fmt, io,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use switchy_thread::simulator::{self as scheduler, Op, WaitKey};

use crate::{ChildExit, PtySize, Signal, SpawnCommand, Stdio};

/// How often, and how many times, a hung-up process is checked for having exited before it is
/// killed outright (as the native backend does).
const HANGUP_GRACE: Duration = Duration::from_millis(50);
const HANGUP_CHECKS: u32 = 5;

/// Which operating system's terminal behaviour to model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Personality {
    /// A terminal stays open while anything that inherited it is alive; buffers of 4 KiB.
    Linux,
    /// A terminal ends with its session; buffers of 1 KiB.
    MacOs,
    /// A pseudo-console (`ConPTY` with an inherited cursor): it writes `CSI 6 n` when opened
    /// and holds its programs' output until the master answers with a cursor report, which
    /// the programs never read. Its output ends when the master and the slave are both
    /// dropped (closing the console, which waits for remaining output to be read and ends the
    /// processes still attached), not when its programs end. Processes are terminated, never
    /// signalled: [`signal_group`] is unsupported, and an ended process reports an exit code
    /// (1 when killed), never a signal. Buffers of 4 KiB.
    Windows,
}

impl Personality {
    /// Whether this is [`Personality::Windows`].
    #[must_use]
    pub const fn is_windows(self) -> bool {
        matches!(self, Self::Windows)
    }
}

/// The exit code of a Windows console process ended by `ctrl+c` or by its console closing
/// (`STATUS_CONTROL_C_EXIT`).
const CONTROL_C_EXIT: u32 = 0xC000_013A;
/// The exit code a terminated Windows process reports (as the native backend kills).
const KILLED_EXIT_CODE: u32 = 1;
/// What a pseudo-console writes first: a cursor position request.
const CURSOR_QUERY: &[u8] = b"\x1b[6n";

/// Rates (each from 0 to 1) at which operations on the master side fail.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Faults {
    /// Reads and writes fail with [`io::ErrorKind::Interrupted`], and waits for readiness
    /// return early, not ready (as an interrupted `poll` does natively).
    pub interrupted: f64,
    /// Reads and writes transfer only part of what they could.
    pub short: f64,
    /// Spawning fails as if the system were out of processes.
    pub spawn: f64,
    /// How many times per simulated second, on average, something outside (an
    /// out-of-memory killer, an administrator) kills a random running process with
    /// `SIGKILL`. Recorded with [`SignalRecord::by_fault`].
    pub kill: f64,
    /// A signal one process sends another ([`signal_process`], [`signal_group`], killing a
    /// child) arrives after a random delay of up to [`SIGNAL_DELAY`] in simulated time.
    pub signal_delay: f64,
    /// A read from a terminal's master side fails with an input/output error.
    pub read_error: f64,
    /// A write to a terminal's master side stalls for a random time of up to [`WRITE_STALL`]
    /// in simulated time first (a slow consumer).
    pub write_stall: f64,
    /// A pseudo-console (Windows) never asks for the cursor position (its output flows at
    /// once), or asks only after a random delay of up to [`STARTUP_QUERY_DELAY`]: each half
    /// the time.
    pub startup_query: f64,
}

/// The longest a delayed signal ([`Faults::signal_delay`]) takes to arrive.
pub const SIGNAL_DELAY: Duration = Duration::from_millis(100);
/// The longest a stalled write ([`Faults::write_stall`]) stalls.
pub const WRITE_STALL: Duration = Duration::from_millis(200);
/// The longest a late startup cursor query ([`Faults::startup_query`]) waits.
pub const STARTUP_QUERY_DELAY: Duration = Duration::from_millis(500);

/// The simulated kernel's configuration for a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// Which operating system's terminal behaviour to model.
    pub personality: Personality,
    /// The probability that a new process gets an id an earlier one used.
    pub pid_reuse: f64,
    /// Fault rates.
    pub faults: Faults,
}

impl Config {
    /// The configuration from the environment, with unset choices drawn from the run's seed:
    ///
    /// * `SIMULATOR_PTY_PERSONALITY`: `linux`, `macos` or `windows`.
    /// * `SIMULATOR_PTY_PID_REUSE`: a probability (default 0.5).
    /// * `SIMULATOR_PTY_FAULTS`: a probability applied to interrupted and short transfers, and
    ///   a quarter of it to failed spawns (default 0). The other fault kinds are off unless a
    ///   simulation configures them.
    ///
    /// # Panics
    ///
    /// * If a variable is set to an invalid value
    #[must_use]
    pub fn from_env() -> Self {
        let personality = match std::env::var("SIMULATOR_PTY_PERSONALITY").as_deref() {
            Ok("linux") => Personality::Linux,
            Ok("macos") => Personality::MacOs,
            Ok("windows") => Personality::Windows,
            Ok(other) => {
                panic!("SIMULATOR_PTY_PERSONALITY must be linux|macos|windows, got '{other}'")
            }
            Err(_) => {
                let draw = lock(&switchy_simulation::stream("switchy_pty_personality")).below(3);
                match draw {
                    0 => Personality::Linux,
                    1 => Personality::MacOs,
                    _ => Personality::Windows,
                }
            }
        };
        let probability = |name: &str, default: f64| {
            std::env::var(name).map_or(default, |value| {
                value
                    .parse::<f64>()
                    .unwrap_or_else(|_| panic!("{name} must be a probability, got '{value}'"))
                    .clamp(0.0, 1.0)
            })
        };
        let fault = probability("SIMULATOR_PTY_FAULTS", 0.0);
        Self {
            personality,
            pid_reuse: probability("SIMULATOR_PTY_PID_REUSE", 0.5),
            faults: Faults {
                interrupted: fault,
                short: fault,
                spawn: fault / 4.0,
                ..Faults::default()
            },
        }
    }
}

/// How a process handles a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// The default: `SIGWINCH` is ignored, anything else ends the process.
    Default,
    /// Nothing happens (`SIGKILL` cannot be ignored).
    Ignore,
    /// The program receives it from [`Process::signals`].
    Catch,
}

/// A program: run with its process, returning its exit code.
pub type Program = Arc<dyn Fn(&mut Process) -> i32 + Send + Sync>;

/// A process's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessState {
    /// Running.
    Running,
    /// Exited, not yet waited for.
    Zombie(ChildExit),
}

/// The parent's ends of the pipes a spawn created, by the stream they connect.
#[derive(Debug, Default, Clone, Copy)]
struct SpawnedPipes {
    stdin: Option<u64>,
    stdout: Option<u64>,
    stderr: Option<u64>,
}

/// A read-only view of one process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSnapshot {
    /// A number unique to this process within the run (ids are reused; serials are not).
    pub serial: u64,
    /// The serial of its session's leader.
    pub session_serial: u64,
    /// Its id.
    pub pid: u32,
    /// The process that started it, if a simulated one did.
    pub parent: Option<u32>,
    /// Its process group.
    pub group: u32,
    /// Its session.
    pub session: u32,
    /// Its program's name.
    pub program: String,
    /// Running or exited.
    pub state: ProcessState,
    /// The terminal it holds open through its standard streams, if any.
    pub terminal: Option<String>,
    /// Every signal delivered to it, in order.
    pub signals: Vec<Signal>,
    /// Its working directory.
    pub cwd: PathBuf,
}

/// A read-only view of one terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSnapshot {
    /// Its slave device's simulated path.
    pub name: String,
    /// Its size.
    pub size: PtySize,
    /// Handles of its master side still open.
    pub master_handles: usize,
    /// Holders of its slave side (processes' streams and the slave handle).
    pub slave_holders: usize,
    /// Whether it hung up.
    pub hung_up: bool,
    /// Bytes written by processes and not yet read from the master.
    pub output_buffered: usize,
    /// Bytes typed and not yet read by processes.
    pub input_buffered: usize,
    /// Every byte processes wrote to it.
    pub written: usize,
}

/// A read-only view of a run's simulated kernel. Taking it never changes the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelSnapshot {
    /// Every process not yet reaped.
    pub processes: Vec<ProcessSnapshot>,
    /// Every terminal.
    pub terminals: Vec<TerminalSnapshot>,
    /// The personality modelled.
    pub personality: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    Terminal(u64),
    PipeRead(u64),
    PipeWrite(u64),
    Null,
}

/// What a terminal's reader gets next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Typed {
    Byte(u8),
    /// The end of a canonical line: a read returns no further.
    LineEnd,
    /// End of file: a read returns nothing.
    Eof,
}

/// The canonical-mode editing characters (the defaults on Linux and macOS).
const ERASE: u8 = 0x7f;
const WORD_ERASE: u8 = 0x17;
const KILL: u8 = 0x15;
const END_OF_FILE: u8 = 0x04;
/// What echoing an erased character writes: back, blank, back.
const ECHO_ERASE: &[u8] = b"\x08 \x08";

// The flags are the terminal's line discipline settings.
#[allow(clippy::struct_excessive_bools)]
struct Terminal {
    name: String,
    size: PtySize,
    /// Slave to master.
    output: VecDeque<u8>,
    /// Master to slave, ready to read.
    input: VecDeque<Typed>,
    /// In canonical mode, the line being typed, not yet readable.
    line: Vec<u8>,
    capacity: usize,
    master_handles: usize,
    slave_holders: usize,
    /// Windows: the master and slave objects still open; the console closes with the last.
    console_handles: usize,
    /// Readers of the master side still open.
    readers: usize,
    /// Windows: the console's cursor query is not answered yet, so output is held.
    awaiting_report: bool,
    /// Windows: what the master wrote so far of a cursor report.
    report: Vec<u8>,
    /// Windows: echoes of what was typed while the cursor query is unanswered, held with the
    /// rest of the console's output.
    held_echo: Vec<u8>,
    windows: bool,
    nonblocking: bool,
    took_writer: bool,
    session: Option<u32>,
    foreground: Option<u32>,
    /// Hung up; on Windows, the console was closed.
    hung_up: bool,
    echo: bool,
    isig: bool,
    icrnl: bool,
    icanon: bool,
    written: Vec<u8>,
}

impl Terminal {
    /// Whether reading the master side is at its end once its output is read: nothing holds
    /// the slave side (or it hung up); on Windows, the console was closed.
    const fn output_ended(&self) -> bool {
        if self.windows {
            self.hung_up
        } else {
            self.slave_holders == 0 || self.hung_up
        }
    }

    /// Whether the slave side's programs lost the master side: it hung up or every master
    /// handle closed; on Windows, the console was closed.
    const fn master_gone(&self) -> bool {
        if self.windows {
            self.hung_up
        } else {
            self.hung_up || self.master_handles == 0
        }
    }

    /// Bytes typed and not yet read.
    fn typed(&self) -> usize {
        self.input.len() + self.line.len()
    }

    /// Erase the last `count` characters of the line being typed, echoing it.
    fn erase(&mut self, count: usize) {
        for _ in 0..count.min(self.line.len()) {
            self.line.pop();
            if self.echo {
                self.echo_out(ECHO_ERASE);
            }
        }
    }

    /// Make the line typed so far readable, ending it as a line.
    fn end_line(&mut self) {
        self.input
            .extend(self.line.drain(..).map(Typed::Byte).chain([Typed::LineEnd]));
    }

    /// Echo `byte` as typed.
    fn echo_byte(&mut self, byte: u8) {
        if !self.echo {
            return;
        }
        if byte == b'\n' {
            self.echo_out(b"\r\n");
        } else {
            self.echo_out(&[byte]);
        }
    }

    /// Echo `bytes`: to the output, or (a console still waiting for its cursor report) held
    /// until the report comes, as the console renders nothing before.
    fn echo_out(&mut self, bytes: &[u8]) {
        if self.awaiting_report {
            self.held_echo.extend_from_slice(bytes);
        } else {
            self.output.extend(bytes);
        }
    }

    /// The console's cursor query was answered (or the console closed): what it held flows.
    fn release_held(&mut self) {
        self.awaiting_report = false;
        let held = std::mem::take(&mut self.held_echo);
        self.output.extend(held);
    }

    /// Take one typed byte through canonical editing.
    fn canonical(&mut self, byte: u8) {
        match byte {
            ERASE => self.erase(1),
            KILL => self.erase(self.line.len()),
            WORD_ERASE => {
                let spaces = self
                    .line
                    .iter()
                    .rev()
                    .take_while(|b| b.is_ascii_whitespace())
                    .count();
                let word = self.line[..self.line.len() - spaces]
                    .iter()
                    .rev()
                    .take_while(|b| !b.is_ascii_whitespace())
                    .count();
                self.erase(spaces + word);
            }
            END_OF_FILE if self.line.is_empty() => self.input.push_back(Typed::Eof),
            END_OF_FILE => self.end_line(),
            b'\n' => {
                self.line.push(b'\n');
                self.echo_byte(b'\n');
                self.end_line();
            }
            byte => {
                self.line.push(byte);
                self.echo_byte(byte);
            }
        }
    }
}

struct Pipe {
    buffer: VecDeque<u8>,
    capacity: usize,
    readers: usize,
    writers: usize,
    nonblocking: bool,
    written: Vec<u8>,
}

/// One delivered signal, kept for the whole run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalRecord {
    /// The receiving process's serial (see [`ProcessSnapshot::serial`]).
    pub serial: u64,
    /// Its id when the signal arrived.
    pub pid: u32,
    /// The serial of its session's leader.
    pub session_serial: u64,
    /// The signal.
    pub signal: Signal,
    /// Whether the outside killer fault ([`Faults::kill`]) sent it.
    pub by_fault: bool,
}

struct Proc {
    serial: u64,
    session_serial: u64,
    parent: Option<u32>,
    group: u32,
    session: u32,
    program: String,
    state: ProcessState,
    streams: [Stream; 3],
    controlling: Option<u64>,
    dispositions: BTreeMap<Signal, Disposition>,
    caught: VecDeque<Signal>,
    fatal: Option<Signal>,
    /// Whether a [`Child`] handle will wait for it; otherwise it is reaped when it exits.
    waited: bool,
    signals: Vec<Signal>,
    cwd: PathBuf,
}

struct State {
    config: Config,
    next_pid: u32,
    used_pids: BTreeSet<u32>,
    processes: BTreeMap<u32, Proc>,
    terminals: BTreeMap<u64, Terminal>,
    pipes: BTreeMap<u64, Pipe>,
    next_id: u64,
    programs: BTreeMap<String, Program>,
    io: BTreeMap<u32, ProcessIo>,
    signal_log: Vec<SignalRecord>,
    /// The last top-level spawn by each simulated thread.
    last_spawned: BTreeMap<usize, Spawned>,
    /// What happens later ([`Faults::signal_delay`], [`Faults::startup_query`]), in order of
    /// when.
    pending: Vec<Pending>,
}

/// Something the kernel does later.
#[derive(Debug, Clone, Copy)]
struct Pending {
    due: Instant,
    what: Later,
}

#[derive(Debug, Clone, Copy)]
enum Later {
    /// Deliver `signal` to `pid`, if it is still the process of `serial`.
    Signal {
        pid: u32,
        serial: u64,
        signal: Signal,
    },
    /// The outside killer ([`Faults::kill`]) strikes.
    Kill,
    /// A pseudo-console asks for the cursor position.
    CursorQuery { terminal: u64 },
}

/// Whether `bytes` are a cursor report (`ESC [ row ; column R`): `Some(true)` for a whole one,
/// `Some(false)` for the start of one, `None` when they cannot be one.
fn report_shape(bytes: &[u8]) -> Option<bool> {
    match bytes {
        [] | [0x1b] => Some(false),
        [0x1b, b'[', rest @ ..] => {
            let (body, complete) = match rest.split_last() {
                Some((b'R', body)) => (body, true),
                _ => (rest, false),
            };
            let mut fields = body.split(|byte| *byte == b';');
            let row = fields.next().unwrap_or_default();
            let column = fields.next();
            let digits = |field: &[u8]| field.iter().all(u8::is_ascii_digit);
            if fields.next().is_some()
                || !digits(row)
                || !column.is_none_or(digits)
                || body.len() > 16
            {
                None
            } else if complete {
                column.map(|_| true)
            } else {
                Some(false)
            }
        }
        _ => None,
    }
}

/// A run's simulated kernel.
pub struct Kernel {
    key: u64,
    rng: Arc<Mutex<switchy_simulation::Rng>>,
    state: Mutex<State>,
}

impl fmt::Debug for Kernel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Kernel").finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The current run's kernel, created on first use with [`Config::from_env`] unless
/// [`configure`] chose another configuration first.
#[must_use]
pub fn kernel() -> Arc<Kernel> {
    switchy_simulation::state_with(|| Kernel::new(Config::from_env()))
}

/// The current run's personality.
#[must_use]
pub fn personality() -> Personality {
    lock(&kernel().state).config.personality
}

/// Configure the current run's kernel. Must come before anything else uses it.
///
/// # Panics
///
/// * If the run's kernel already exists
pub fn configure(config: Config) {
    let mut fresh = false;
    let _ = switchy_simulation::state_with(|| {
        fresh = true;
        Kernel::new(config)
    });
    assert!(
        fresh,
        "the simulated kernel is already in use; configure it first"
    );
}

/// Whether this run has a program registered under `name` (see [`register_program`]).
#[must_use]
pub fn program_registered(name: &str) -> bool {
    lock(&kernel().state).programs.contains_key(name)
}

/// Register `program` for this run under `name`: spawning a command whose first argument is
/// `name` (or a path ending in `/name`) runs it.
pub fn register_program(name: &str, program: impl Fn(&mut Process) -> i32 + Send + Sync + 'static) {
    lock(&kernel().state)
        .programs
        .insert(name.to_owned(), Arc::new(program));
}

/// Observe what the run's kernel ended with (see [`switchy_simulation::digest`]): every
/// signal delivered, every byte each terminal and pipe was written, and the processes left.
fn observe_run_end() {
    let Some(kernel) = switchy_simulation::existing_state::<Kernel>() else {
        return;
    };
    let state = lock(&kernel.state);
    switchy_simulation::observe_debug("pty.signals", "every signal delivered", &state.signal_log);
    for (id, terminal) in &state.terminals {
        switchy_simulation::observe(
            "pty.terminal",
            &format!("{} (terminal {id}) written", terminal.name),
            &terminal.written,
        );
    }
    for (id, pipe) in &state.pipes {
        switchy_simulation::observe("pty.pipe", &format!("pipe {id} written"), &pipe.written);
    }
    drop(state);
    switchy_simulation::observe_debug("pty.processes", "processes left", &kernel.snapshot());
}

/// A read-only view of the current run's kernel.
#[must_use]
pub fn snapshot() -> KernelSnapshot {
    kernel().snapshot()
}

/// Every signal delivered in the current run so far, in order.
#[must_use]
pub fn signal_log() -> Vec<SignalRecord> {
    lock(&kernel().state).signal_log.clone()
}

/// The serial of the running or exited process with id `pid`; see
/// [`ProcessSnapshot::serial`].
#[must_use]
pub fn serial_of(pid: u32) -> Option<u64> {
    lock(&kernel().state).processes.get(&pid).map(|p| p.serial)
}

/// Every byte processes wrote to the terminal at simulated path `name`, in order.
#[must_use]
pub fn terminal_written(name: &str) -> Option<Vec<u8>> {
    let kernel = kernel();
    let state = lock(&kernel.state);
    state
        .terminals
        .values()
        .find(|t| t.name == name)
        .map(|t| t.written.clone())
}

/// Where a process's terminal output and piped standard output went, kept after it is reaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIo {
    /// The simulated path of the terminal it started on, if any.
    pub terminal: Option<String>,
    /// Its piped standard output, if it had one; see [`pipe_written`].
    pub stdout_pipe: Option<u64>,
}

/// A process started directly on a terminal, as recorded when it was spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawned {
    /// Its id.
    pub pid: u32,
    /// Its serial (see [`ProcessSnapshot::serial`]).
    pub serial: u64,
    /// Where its output goes.
    pub io: ProcessIo,
}

/// The last process the calling simulated thread started on a terminal. Exact even when the
/// process already exited and its id was reused, unlike looking it up by id.
#[must_use]
pub fn last_spawned() -> Option<Spawned> {
    let me = scheduler::scheduler().me();
    lock(&kernel().state).last_spawned.get(&me).cloned()
}

/// Where the process started with id `pid` (most recently, as ids are reused) sends its
/// output. Recorded at spawn, so it stays available after the process is reaped.
#[must_use]
pub fn process_io(pid: u32) -> Option<ProcessIo> {
    lock(&kernel().state).io.get(&pid).cloned()
}

/// Every byte written to pipe `pipe`, in order.
#[must_use]
pub fn pipe_written(pipe: u64) -> Option<Vec<u8>> {
    lock(&kernel().state)
        .pipes
        .get(&pipe)
        .map(|p| p.written.clone())
}

/// Every byte processes wrote to process `pid`'s piped standard output, in order.
#[must_use]
pub fn stdout_written(pid: u32) -> Option<Vec<u8>> {
    process_io(pid)
        .and_then(|io| io.stdout_pipe)
        .and_then(pipe_written)
}

/// End every process still running (as `SIGKILL`), so their simulated threads finish. For the
/// end of a run.
pub fn kill_all() {
    let kernel = kernel();
    let mut state = lock(&kernel.state);
    let pids: Vec<u32> = state
        .processes
        .iter()
        .filter(|(_, p)| p.state == ProcessState::Running)
        .map(|(&pid, _)| pid)
        .collect();
    for pid in pids {
        Kernel::deliver(&mut state, pid, Signal::Kill);
    }
    drop(state);
    kernel.wake();
}

/// The system's description of a signal number.
#[must_use]
pub fn signal_name(signal: i32) -> String {
    match signal {
        1 => "Hangup".into(),
        2 => "Interrupt".into(),
        3 => "Quit".into(),
        9 => "Killed".into(),
        15 => "Terminated".into(),
        28 => "Window size changes".into(),
        other => format!("Signal {other}"),
    }
}

const fn signal_number(signal: Signal) -> i32 {
    match signal {
        Signal::Hangup => 1,
        Signal::Interrupt => 2,
        Signal::Quit => 3,
        Signal::Kill => 9,
        Signal::Terminate => 15,
        Signal::WindowChange => 28,
    }
}

/// The environment new commands start from: fixed, so runs do not depend on the real one.
#[must_use]
pub fn base_environment() -> Vec<(OsString, OsString)> {
    [
        ("HOME", "/home/sim"),
        ("PATH", "/usr/local/bin:/usr/bin:/bin"),
        ("SHELL", "/bin/sh"),
        ("USER", "sim"),
    ]
    .into_iter()
    .map(|(name, value)| (name.into(), value.into()))
    .collect()
}

/// Close the master side of the terminals named `terminals` (as [`ProcessIo::terminal`] names
/// them) at once, as the operating system does when the process holding them dies.
///
/// Each Unix terminal hangs up (its sessions get `SIGHUP`), each Windows console closes (its
/// processes end). Master handles that still exist act as closed.
pub fn close_masters(terminals: &[String]) {
    let kernel = kernel();
    kernel.switch();
    let mut state = lock(&kernel.state);
    let ids: Vec<u64> = state
        .terminals
        .iter()
        .filter(|(_, t)| terminals.contains(&t.name))
        .map(|(&id, _)| id)
        .collect();
    for terminal in ids {
        let Some(t) = state.terminals.get_mut(&terminal) else {
            continue;
        };
        if t.hung_up {
            continue;
        }
        // The last handle of each kind closes now.
        t.master_handles = 1;
        t.console_handles = 1;
        Kernel::close_master(&mut state, terminal);
        Kernel::close_console(&mut state, terminal);
    }
    drop(state);
    kernel.wake();
}

/// Signal process `pid`.
///
/// # Errors
///
/// * [`io::ErrorKind::NotFound`] if no running process has that id (as `ESRCH`)
/// * [`io::ErrorKind::Unsupported`] for any signal but [`Signal::Kill`] on Windows
pub fn signal_process(pid: u32, signal: Signal) -> io::Result<()> {
    let kernel = kernel();
    kernel.switch();
    let mut state = lock(&kernel.state);
    if state.config.personality.is_windows() && signal != Signal::Kill {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows processes can only be terminated",
        ));
    }
    if !state
        .processes
        .get(&pid)
        .is_some_and(|p| p.state == ProcessState::Running)
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no running process {pid}"),
        ));
    }
    kernel.send(&mut state, pid, signal);
    drop(state);
    kernel.wake();
    Ok(())
}

/// Signal every process in `group`.
///
/// # Errors
///
/// * [`io::ErrorKind::Unsupported`] on Windows, where processes have no groups; a group with
///   no processes counts as done.
pub fn signal_group(group: u32, signal: Signal) -> io::Result<()> {
    let kernel = kernel();
    kernel.switch();
    let mut state = lock(&kernel.state);
    if state.config.personality.is_windows() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "signalling a process group is not supported on Windows",
        ));
    }
    let members: Vec<u32> = state
        .processes
        .iter()
        .filter(|(_, p)| p.group == group && p.state == ProcessState::Running)
        .map(|(&pid, _)| pid)
        .collect();
    for pid in members {
        kernel.send(&mut state, pid, signal);
    }
    drop(state);
    kernel.wake();
    Ok(())
}

/// Where a process a simulated process starts goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// Its parent's process group.
    SameGroup,
    /// A process group of its own, in its parent's session.
    NewGroup,
    /// A session of its own (Unix), detached from any terminal.
    NewSession,
}

/// A wait's outcome.
enum Wait<T> {
    Done(T),
    Again,
}

impl Kernel {
    fn new(config: Config) -> Self {
        let kernel = Self::bare(config);
        let mut state = lock(&kernel.state);
        kernel.schedule_kill(&mut state);
        drop(state);
        kernel
    }

    fn bare(config: Config) -> Self {
        switchy_simulation::digest::on_run_end("switchy_pty", observe_run_end);
        let rng = switchy_simulation::stream("switchy_pty");
        let first = 100 + u32::try_from(lock(&rng).below(900)).unwrap_or(0);
        Self {
            key: scheduler::new_key(),
            rng,
            state: Mutex::new(State {
                config,
                next_pid: first,
                used_pids: BTreeSet::new(),
                processes: BTreeMap::new(),
                terminals: BTreeMap::new(),
                pipes: BTreeMap::new(),
                next_id: 1,
                programs: BTreeMap::new(),
                io: BTreeMap::new(),
                signal_log: Vec::new(),
                last_spawned: BTreeMap::new(),
                pending: Vec::new(),
            }),
        }
    }

    /// A switch point before any kernel operation, after which what is due happens: faults
    /// that strike from outside, and delayed deliveries.
    fn switch(&self) {
        if !std::thread::panicking() {
            scheduler::scheduler().yield_point(Op::Io);
            self.tick();
        }
    }

    /// Do what is due: maybe kill a random process ([`Faults::kill`]), and carry out pending
    /// deliveries whose time came.
    fn tick(&self) {
        let mut state = lock(&self.state);
        let mut changed = false;
        if !state.pending.is_empty() {
            let now = scheduler::deadline_after(Duration::ZERO);
            while state
                .pending
                .first()
                .is_some_and(|pending| now.is_some_and(|now| pending.due <= now))
            {
                let pending = state.pending.remove(0);
                match pending.what {
                    Later::Signal {
                        pid,
                        serial,
                        signal,
                    } => {
                        if state
                            .processes
                            .get(&pid)
                            .is_some_and(|p| p.serial == serial)
                        {
                            Self::deliver(&mut state, pid, signal);
                        }
                    }
                    Later::Kill => {
                        self.kill_one(&mut state);
                        self.schedule_kill(&mut state);
                    }
                    Later::CursorQuery { terminal } => {
                        // The console has held its programs' output since it opened.
                        if let Some(t) = state.terminals.get_mut(&terminal) {
                            t.output.extend(CURSOR_QUERY);
                        }
                    }
                }
                changed = true;
            }
        }
        drop(state);
        if changed {
            self.wake();
        }
    }

    /// The outside killer ([`Faults::kill`]): `SIGKILL` a random running process.
    fn kill_one(&self, state: &mut State) {
        let running: Vec<u32> = state
            .processes
            .iter()
            .filter(|(_, p)| p.state == ProcessState::Running && p.fatal.is_none())
            .map(|(&pid, _)| pid)
            .collect();
        if running.is_empty() {
            return;
        }
        let index = usize::try_from(lock(&self.rng).below(running.len() as u64)).unwrap_or(0);
        Self::deliver_from(state, running[index], Signal::Kill, true);
    }

    /// Schedule the outside killer's next strike, a random time (averaging `1 / kill`
    /// seconds) from now.
    fn schedule_kill(&self, state: &mut State) {
        let rate = state.config.faults.kill;
        if rate <= 0.0 {
            return;
        }
        // Up to twice the mean interval, in whole milliseconds.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bound = (2000.0 / rate).clamp(2.0, 1e12) as u64;
        let delay = Duration::from_millis(1 + lock(&self.rng).below(bound));
        Self::later(state, delay, Later::Kill);
    }

    /// When the next pending delivery is due.
    fn next_due(&self) -> Option<Instant> {
        lock(&self.state).pending.first().map(|pending| pending.due)
    }

    /// Schedule `what` for `delay` from now.
    fn later(state: &mut State, delay: Duration, what: Later) {
        let Some(due) = scheduler::deadline_after(delay) else {
            return;
        };
        let at = state.pending.partition_point(|pending| pending.due <= due);
        state.pending.insert(at, Pending { due, what });
    }

    /// Send `signal` from one process to `pid`: delivered now, or later under
    /// [`Faults::signal_delay`].
    fn send(&self, state: &mut State, pid: u32, signal: Signal) {
        if self.chance(state.config.faults.signal_delay)
            && let Some(serial) = state.processes.get(&pid).map(|p| p.serial)
        {
            let millis = SIGNAL_DELAY.as_millis().try_into().unwrap_or(u64::MAX);
            let delay = Duration::from_millis(1 + lock(&self.rng).below(millis));
            Self::later(
                state,
                delay,
                Later::Signal {
                    pid,
                    serial,
                    signal,
                },
            );
            return;
        }
        Self::deliver(state, pid, signal);
    }

    /// Wake everything waiting on the kernel; each rechecks what it waits for.
    fn wake(&self) {
        scheduler::scheduler().wake_all(WaitKey::Io(self.key));
    }

    /// Wait for the kernel to change, or until `deadline`. Returns whether it timed out.
    fn block(&self, deadline: Option<Instant>) -> bool {
        scheduler::scheduler().block(Op::Io, WaitKey::Io(self.key), deadline)
    }

    fn chance(&self, p: f64) -> bool {
        p > 0.0 && lock(&self.rng).chance(p)
    }

    /// Repeat `attempt` until it is done, blocking between attempts (until `deadline`).
    /// `None` when the deadline passed first.
    fn wait_for<T>(
        &self,
        deadline: Option<Instant>,
        mut attempt: impl FnMut(&mut State) -> Wait<T>,
    ) -> Option<T> {
        loop {
            {
                let mut state = lock(&self.state);
                if let Wait::Done(value) = attempt(&mut state) {
                    drop(state);
                    self.wake();
                    return Some(value);
                }
            }
            // Wake for the next pending delivery too, so it happens while everything waits.
            let due = self.next_due();
            let wake_at = match (deadline, due) {
                (Some(deadline), Some(due)) => Some(deadline.min(due)),
                (deadline, due) => deadline.or(due),
            };
            if self.block(wake_at) {
                self.tick();
                let reached = deadline.is_some_and(|deadline| {
                    scheduler::deadline_after(Duration::ZERO).is_some_and(|now| now >= deadline)
                });
                if !reached {
                    continue;
                }
                let mut state = lock(&self.state);
                return match attempt(&mut state) {
                    Wait::Done(value) => {
                        drop(state);
                        self.wake();
                        Some(value)
                    }
                    Wait::Again => None,
                };
            }
        }
    }

    fn snapshot(&self) -> KernelSnapshot {
        let state = lock(&self.state);
        let snapshot = KernelSnapshot {
            processes: state
                .processes
                .iter()
                .map(|(&pid, p)| ProcessSnapshot {
                    serial: p.serial,
                    session_serial: p.session_serial,
                    pid,
                    parent: p.parent,
                    group: p.group,
                    session: p.session,
                    program: p.program.clone(),
                    state: p.state.clone(),
                    terminal: p.streams.iter().find_map(|s| match s {
                        Stream::Terminal(t) => state.terminals.get(t).map(|t| t.name.clone()),
                        _ => None,
                    }),
                    signals: p.signals.clone(),
                    cwd: p.cwd.clone(),
                })
                .collect(),
            terminals: state
                .terminals
                .values()
                .map(|t| TerminalSnapshot {
                    name: t.name.clone(),
                    size: t.size,
                    master_handles: t.master_handles,
                    slave_holders: t.slave_holders,
                    hung_up: t.hung_up,
                    output_buffered: t.output.len(),
                    input_buffered: t.typed(),
                    written: t.written.len(),
                })
                .collect(),
            personality: format!("{:?}", state.config.personality),
        };
        drop(state);
        snapshot
    }

    const fn capacity(state: &State) -> usize {
        match state.config.personality {
            Personality::Linux | Personality::Windows => 4096,
            Personality::MacOs => 1024,
        }
    }

    const fn new_id(state: &mut State) -> u64 {
        let id = state.next_id;
        state.next_id += 1;
        id
    }

    /// A process id: an earlier one nothing uses any more, sometimes, or a new one.
    fn allocate_pid(&self, state: &mut State) -> u32 {
        let in_use: BTreeSet<u32> = state
            .processes
            .iter()
            .flat_map(|(&pid, p)| [pid, p.group, p.session])
            .collect();
        let free: Vec<u32> = state
            .used_pids
            .iter()
            .copied()
            .filter(|pid| !in_use.contains(pid))
            .collect();
        if !free.is_empty() && self.chance(state.config.pid_reuse) {
            let index = lock(&self.rng).index(free.len());
            return free[index];
        }
        let pid = state.next_pid;
        state.next_pid += 1 + u32::try_from(lock(&self.rng).below(3)).unwrap_or(0);
        state.used_pids.insert(pid);
        pid
    }

    fn hold(state: &mut State, stream: Stream) {
        match stream {
            Stream::Terminal(t) => {
                if let Some(t) = state.terminals.get_mut(&t) {
                    t.slave_holders += 1;
                }
            }
            Stream::PipeRead(p) => {
                if let Some(p) = state.pipes.get_mut(&p) {
                    p.readers += 1;
                }
            }
            Stream::PipeWrite(p) => {
                if let Some(p) = state.pipes.get_mut(&p) {
                    p.writers += 1;
                }
            }
            Stream::Null => {}
        }
    }

    fn release(state: &mut State, stream: Stream) {
        match stream {
            Stream::Terminal(t) => {
                if let Some(t) = state.terminals.get_mut(&t) {
                    t.slave_holders = t.slave_holders.saturating_sub(1);
                }
            }
            Stream::PipeRead(p) => {
                if let Some(p) = state.pipes.get_mut(&p) {
                    p.readers = p.readers.saturating_sub(1);
                }
            }
            Stream::PipeWrite(p) => {
                if let Some(p) = state.pipes.get_mut(&p) {
                    p.writers = p.writers.saturating_sub(1);
                }
            }
            Stream::Null => {}
        }
    }

    /// Deliver `signal` to `pid`, as its handling says.
    fn deliver(state: &mut State, pid: u32, signal: Signal) {
        Self::deliver_from(state, pid, signal, false);
    }

    /// Deliver `signal` to `pid`, as its handling says, recording whether a fault sent it.
    fn deliver_from(state: &mut State, pid: u32, signal: Signal, by_fault: bool) {
        let Some(process) = state.processes.get_mut(&pid) else {
            return;
        };
        if process.state != ProcessState::Running {
            return;
        }
        process.signals.push(signal);
        let record = SignalRecord {
            serial: process.serial,
            pid,
            session_serial: process.session_serial,
            signal,
            by_fault,
        };
        state.signal_log.push(record);
        let Some(process) = state.processes.get_mut(&pid) else {
            return;
        };
        let disposition = if signal == Signal::Kill {
            Disposition::Default
        } else {
            process
                .dispositions
                .get(&signal)
                .copied()
                .unwrap_or(Disposition::Default)
        };
        match disposition {
            Disposition::Ignore => {}
            Disposition::Catch => process.caught.push_back(signal),
            Disposition::Default => {
                if signal != Signal::WindowChange && process.fatal.is_none() {
                    process.fatal = Some(signal);
                }
            }
        }
    }

    /// Signal every running process whose controlling terminal is `terminal` and that is in its
    /// foreground group (when it has one).
    fn signal_foreground(state: &mut State, terminal: u64, signal: Signal) {
        let foreground = state.terminals.get(&terminal).and_then(|t| t.foreground);
        let targets: Vec<u32> = state
            .processes
            .iter()
            .filter(|(_, p)| {
                p.state == ProcessState::Running
                    && p.controlling == Some(terminal)
                    && foreground.is_none_or(|g| g == p.group)
            })
            .map(|(&pid, _)| pid)
            .collect();
        for pid in targets {
            Self::deliver(state, pid, signal);
        }
    }

    /// Record that `pid` ended, releasing what it held.
    fn exit(&self, pid: u32, exit: ChildExit) {
        let mut state = lock(&self.state);
        let Some(process) = state.processes.get_mut(&pid) else {
            return;
        };
        switchy_simulation::observe_debug(
            "pty.exit",
            &format!("pid {pid} (serial {})", process.serial),
            &exit,
        );
        process.state = ProcessState::Zombie(exit);
        let streams = process.streams;
        let controlling = process.controlling;
        let leader = process.session == pid;
        let waited = process.waited;
        for stream in streams {
            Self::release(&mut state, stream);
        }
        // A console's processes outlive the first one (Windows has no controlling process).
        if leader
            && !state.config.personality.is_windows()
            && let Some(terminal) = controlling
        {
            // The controlling process exited: its terminal hangs up on the foreground group.
            Self::signal_foreground(&mut state, terminal, Signal::Hangup);
            if state.config.personality == Personality::MacOs
                && let Some(t) = state.terminals.get_mut(&terminal)
            {
                t.hung_up = true;
            }
            // And the terminal is no longer the session's (POSIX): it names no session or
            // foreground group, and the session's other members lose it as their controlling
            // terminal. The session's id may be reused once it is empty; the terminal must not
            // reach whoever reuses it.
            if let Some(t) = state.terminals.get_mut(&terminal)
                && t.session == Some(pid)
            {
                t.session = None;
                t.foreground = None;
            }
            for process in state.processes.values_mut() {
                if process.session == pid && process.controlling == Some(terminal) {
                    process.controlling = None;
                }
            }
        }
        if !waited {
            state.processes.remove(&pid);
        }
        drop(state);
        self.wake();
    }

    fn close_master(state: &mut State, terminal: u64) {
        let Some(t) = state.terminals.get_mut(&terminal) else {
            return;
        };
        t.master_handles = t.master_handles.saturating_sub(1);
        if t.master_handles == 0 && !t.hung_up && !t.windows {
            t.hung_up = true;
            let session = t.session;
            let targets: Vec<u32> = state
                .processes
                .iter()
                .filter(|(_, p)| {
                    p.state == ProcessState::Running
                        && (p.controlling == Some(terminal) || Some(p.session) == session)
                })
                .map(|(&pid, _)| pid)
                .collect();
            for pid in targets {
                Self::deliver(state, pid, Signal::Hangup);
            }
        }
    }

    /// The master or slave object of `terminal` closed. On Windows the console closes with the
    /// last of them: the processes still attached to it end (`CTRL_CLOSE_EVENT`), and its
    /// output ends once read. Returns whether this closed a console.
    fn close_console(state: &mut State, terminal: u64) -> bool {
        let Some(t) = state.terminals.get_mut(&terminal) else {
            return false;
        };
        t.console_handles = t.console_handles.saturating_sub(1);
        if !t.windows || t.console_handles > 0 || t.hung_up {
            return false;
        }
        t.hung_up = true;
        t.release_held();
        let targets: Vec<u32> = state
            .processes
            .iter()
            .filter(|(_, p)| p.state == ProcessState::Running && p.controlling == Some(terminal))
            .map(|(&pid, _)| pid)
            .collect();
        // `CTRL_CLOSE_EVENT`: a handler may run, but the system ends the process when it
        // returns (or after a few seconds); it cannot be ignored as a hang-up can.
        for pid in targets {
            Self::deliver(state, pid, Signal::Hangup);
            if let Some(process) = state.processes.get_mut(&pid)
                && process.state == ProcessState::Running
                && process.fatal.is_none()
            {
                process.fatal = Some(Signal::Hangup);
            }
        }
        true
    }

    /// Closing a console waits until its remaining output is read (or nothing reads it).
    fn wait_console_drained(&self, terminal: u64) {
        if std::thread::panicking() {
            return;
        }
        let _ = self.wait_for(None, |state| match state.terminals.get(&terminal) {
            Some(t) if !t.output.is_empty() && t.readers > 0 => Wait::Again,
            _ => Wait::Done(()),
        });
    }

    /// Windows: take `byte`, typed while the console's cursor query is unanswered, as part of
    /// a cursor report (`ESC [ row ; column R`). Returns the bytes to type normally instead:
    /// none while a report may still be under way, every withheld byte once it cannot be one.
    fn cursor_report(t: &mut Terminal, byte: u8) -> Vec<u8> {
        if t.report.is_empty() && byte != 0x1b {
            return vec![byte];
        }
        t.report.push(byte);
        match report_shape(&t.report) {
            Some(true) => {
                t.report.clear();
                t.release_held();
                Vec::new()
            }
            Some(false) => Vec::new(),
            None => std::mem::take(&mut t.report),
        }
    }

    /// Process `bytes` typed at the master through the line discipline, as far as there is
    /// room. Returns how many were consumed.
    fn type_input(state: &mut State, terminal: u64, bytes: &[u8]) -> usize {
        let mut consumed = 0;
        for &byte in bytes {
            let Some(t) = state.terminals.get_mut(&terminal) else {
                break;
            };
            if t.awaiting_report {
                let typed = Self::cursor_report(t, byte);
                consumed += 1;
                // Withheld bytes that were not a report are typed after all, room or not.
                for byte in typed {
                    Self::type_byte(state, terminal, byte);
                }
                continue;
            }
            if !(t.isig && (byte == 0x03 || byte == 0x1c)) && t.typed() >= t.capacity {
                break;
            }
            Self::type_byte(state, terminal, byte);
            consumed += 1;
        }
        consumed
    }

    /// Take one typed byte through the line discipline: signal characters, `\r` to `\n`, and
    /// canonical editing or raw input.
    fn type_byte(state: &mut State, terminal: u64, byte: u8) {
        let Some(t) = state.terminals.get_mut(&terminal) else {
            return;
        };
        if t.isig && (byte == 0x03 || byte == 0x1c) {
            let signal = if byte == 0x03 {
                Signal::Interrupt
            } else {
                Signal::Quit
            };
            t.input.clear();
            t.line.clear();
            if t.echo {
                t.echo_out(if byte == 0x03 { b"^C" } else { b"^\\" });
            }
            Self::signal_foreground(state, terminal, signal);
            return;
        }
        let byte = if t.icrnl && byte == b'\r' {
            b'\n'
        } else {
            byte
        };
        if t.icanon {
            t.canonical(byte);
        } else {
            t.input.push_back(Typed::Byte(byte));
            t.echo_byte(byte);
        }
    }

    fn open(&self, size: PtySize, inherit_cursor: bool) -> u64 {
        self.switch();
        let mut state = lock(&self.state);
        let id = Self::new_id(&mut state);
        let capacity = Self::capacity(&state);
        let windows = state.config.personality.is_windows();
        // A pseudo-console inheriting the cursor asks where it is before anything else: at
        // once, or (under the startup query fault) never or late.
        let (query, held) = if !windows || !inherit_cursor {
            (false, false)
        } else if self.chance(state.config.faults.startup_query) {
            if lock(&self.rng).chance(0.5) {
                (false, false)
            } else {
                let millis = STARTUP_QUERY_DELAY
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX);
                let delay = Duration::from_millis(1 + lock(&self.rng).below(millis));
                Self::later(&mut state, delay, Later::CursorQuery { terminal: id });
                (false, true)
            }
        } else {
            (true, true)
        };
        state.terminals.insert(
            id,
            Terminal {
                name: format!("/dev/simpts/{id}"),
                size,
                output: if query {
                    CURSOR_QUERY.iter().copied().collect()
                } else {
                    VecDeque::new()
                },
                input: VecDeque::new(),
                line: Vec::new(),
                capacity,
                // The master and the slave handle.
                master_handles: 1,
                slave_holders: 1,
                console_handles: 2,
                readers: 0,
                awaiting_report: held,
                report: Vec::new(),
                held_echo: Vec::new(),
                windows,
                nonblocking: false,
                took_writer: false,
                session: None,
                foreground: None,
                hung_up: false,
                echo: true,
                isig: true,
                icrnl: true,
                icanon: true,
                written: Vec::new(),
            },
        );
        drop(state);
        id
    }

    fn new_pipe(state: &mut State) -> u64 {
        let id = Self::new_id(state);
        let capacity = Self::capacity(state) * 4;
        state.pipes.insert(
            id,
            Pipe {
                buffer: VecDeque::new(),
                capacity,
                readers: 0,
                writers: 0,
                nonblocking: false,
                written: Vec::new(),
            },
        );
        id
    }

    /// Start `spec`'s program. `parent` is the simulated process starting it, if one is.
    // One sequence: resolve the program, allocate the process and its streams, start it.
    #[allow(clippy::too_many_lines)]
    fn spawn(
        self: &Arc<Self>,
        spec: &SpawnCommand,
        terminal: Option<u64>,
        parent: Option<(u32, [Stream; 3], Placement)>,
    ) -> io::Result<(u32, SpawnedPipes)> {
        self.switch();
        let argv: Vec<String> = spec
            .argv()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let name = argv
            .first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no program to run"))?;
        let base = name.rsplit('/').next().unwrap_or(name).to_owned();
        let mut state = lock(&self.state);
        // A process a fatal signal already ended cannot start children (natively, fork fails
        // while a fatal signal is pending), so a group kill cannot miss a child it races with.
        if let Some((parent_pid, _, _)) = parent
            && state
                .processes
                .get(&parent_pid)
                .is_some_and(|p| p.fatal.is_some())
        {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        let Some(program) = state.programs.get(&base).cloned() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "cannot run {name}: not found on PATH {}",
                    spec.get_env("PATH")
                        .map_or_else(Default::default, OsStr::to_string_lossy)
                ),
            ));
        };
        let windows = state.config.personality.is_windows();
        // Windows: a process has the console on all of its streams or on none of them.
        let detached = if parent.is_none() && windows {
            let streams: [Stdio; 3] = spec.stdio().into();
            let on_console = streams
                .iter()
                .filter(|stdio| **stdio == Stdio::Terminal)
                .count();
            if on_console == 1 || on_console == 2 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "a Windows process has the console on all of its streams or on none",
                ));
            }
            on_console == 0
        } else {
            false
        };
        if self.chance(state.config.faults.spawn) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "simulated fault: cannot start another process",
            ));
        }
        let pid = self.allocate_pid(&mut state);
        let serial = Self::new_id(&mut state);
        switchy_simulation::observe(
            "pty.spawn",
            &format!(
                "pid {pid} (serial {serial}) by {:?}: {argv:?}",
                parent.map(|(pid, _, _)| pid)
            ),
            b"",
        );
        let session_serial = match parent {
            // A new session (Unix) is its own; a Windows process stays in its parent's job.
            Some((_, _, Placement::NewSession)) if !windows => serial,
            Some((p, _, _)) => state.processes[&p].session_serial,
            None => serial,
        };
        let mut pipes = SpawnedPipes::default();
        let (group, session, streams, controlling, inherited) =
            if let Some((parent_pid, parent_streams, placement)) = parent {
                let parent = &state.processes[&parent_pid];
                match placement {
                    Placement::SameGroup | Placement::NewGroup => (
                        if placement == Placement::NewGroup {
                            pid
                        } else {
                            parent.group
                        },
                        parent.session,
                        parent_streams,
                        parent.controlling,
                        parent.dispositions.clone(),
                    ),
                    // Detached, as a daemon is: no terminal, its streams on the null device.
                    Placement::NewSession => (
                        pid,
                        if windows { parent.session } else { pid },
                        [Stream::Null; 3],
                        None,
                        BTreeMap::new(),
                    ),
                }
            } else {
                {
                    let terminal = terminal.expect("a top-level spawn has a terminal");
                    let (stdin, stdout, stderr) = spec.stdio();
                    let stream =
                        |stdio: Stdio, slot: &mut Option<u64>, input: bool, state: &mut State| {
                            match stdio {
                                Stdio::Terminal => Stream::Terminal(terminal),
                                Stdio::Null => Stream::Null,
                                Stdio::Piped => {
                                    let pipe = Self::new_pipe(state);
                                    *slot = Some(pipe);
                                    if input {
                                        Stream::PipeRead(pipe)
                                    } else {
                                        Stream::PipeWrite(pipe)
                                    }
                                }
                            }
                        };
                    let streams = [
                        stream(stdin, &mut pipes.stdin, true, &mut state),
                        stream(stdout, &mut pipes.stdout, false, &mut state),
                        stream(stderr, &mut pipes.stderr, false, &mut state),
                    ];
                    // A Windows process with its streams redirected runs without the console.
                    let controlling =
                        (spec.is_controlling_terminal() && !detached).then_some(terminal);
                    if let Some(t) = controlling.and_then(|t| state.terminals.get_mut(&t)) {
                        t.session = Some(pid);
                        t.foreground = Some(pid);
                        if !t.windows {
                            t.hung_up = false;
                        }
                    }
                    (pid, pid, streams, controlling, BTreeMap::new())
                }
            };
        for stream in streams {
            Self::hold(&mut state, stream);
        }
        // The parent holds the other ends of new pipes.
        if let Some(pipe) = pipes.stdin {
            Self::hold(&mut state, Stream::PipeWrite(pipe));
        }
        for pipe in [pipes.stdout, pipes.stderr].into_iter().flatten() {
            Self::hold(&mut state, Stream::PipeRead(pipe));
        }
        if parent.is_none() {
            let terminal = terminal.and_then(|t| state.terminals.get(&t).map(|t| t.name.clone()));
            let io = ProcessIo {
                terminal,
                stdout_pipe: pipes.stdout,
            };
            state.io.insert(pid, io.clone());
            let spawner = scheduler::scheduler().me();
            state
                .last_spawned
                .insert(spawner, Spawned { pid, serial, io });
        }
        let cwd = spec
            .get_cwd()
            .map_or_else(|| PathBuf::from("/home/sim"), PathBuf::from);
        state.processes.insert(
            pid,
            Proc {
                serial,
                session_serial,
                parent: parent.map(|(p, _, _)| p),
                group,
                session,
                program: base.clone(),
                state: ProcessState::Running,
                streams,
                controlling,
                dispositions: inherited,
                caught: VecDeque::new(),
                fatal: None,
                waited: parent.is_none(),
                signals: Vec::new(),
                cwd: cwd.clone(),
            },
        );
        drop(state);
        let kernel = Arc::clone(self);
        let env: Vec<(String, String)> = spec
            .utf8_environment()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        // If the program never runs (its thread killed before it started, or never started),
        // the process ends as killed: it must not run forever without a program.
        let unborn = Unborn {
            kernel: Arc::clone(&kernel),
            pid: Some(pid),
        };
        switchy_thread::thread::Builder::new()
            .name(format!("proc-{pid}-{base}"))
            // A process outlives the threads that started it (a server that crashes, a runtime
            // shut down): its program belongs to no thread group.
            .group(0)
            // The kernel's own spawn fault decides whether a process starts.
            .fault_free(true)
            .spawn(move || {
                unborn.born();
                let mut process = Process {
                    kernel: Arc::clone(&kernel),
                    pid,
                    argv,
                    env,
                    cwd,
                };
                let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| program(&mut process)));
                let exit = match outcome {
                    Ok(code) => ChildExit::with_exit_code(u32::from_ne_bytes(code.to_ne_bytes())),
                    Err(payload) => match payload.downcast::<Ending>() {
                        Ok(ending) => match *ending {
                            Ending::Exit(code) => {
                                ChildExit::with_exit_code(u32::from_ne_bytes(code.to_ne_bytes()))
                            }
                            Ending::Signal(signal)
                                if lock(&kernel.state).config.personality.is_windows() =>
                            {
                                // Windows reports how a process ended as an exit code.
                                ChildExit::with_exit_code(if signal == Signal::Kill {
                                    KILLED_EXIT_CODE
                                } else {
                                    CONTROL_C_EXIT
                                })
                            }
                            Ending::Signal(signal) => ChildExit::signaled(
                                signal_number(signal),
                                &signal_name(signal_number(signal)),
                            ),
                        },
                        Err(payload) => {
                            kernel.exit(pid, ChildExit::with_exit_code(101));
                            std::panic::resume_unwind(payload);
                        }
                    },
                };
                kernel.exit(pid, exit);
            })?;
        Ok((pid, pipes))
    }
}

/// A process whose program has not started: ends it as killed if dropped (its program never
/// runs).
struct Unborn {
    kernel: Arc<Kernel>,
    /// `None` once the program starts.
    pid: Option<u32>,
}

impl Unborn {
    /// The program started: the process ends as it does.
    fn born(mut self) {
        self.pid = None;
    }
}

impl Drop for Unborn {
    fn drop(&mut self) {
        let Some(pid) = self.pid else {
            return;
        };
        let exit = if lock(&self.kernel.state).config.personality.is_windows() {
            ChildExit::with_exit_code(KILLED_EXIT_CODE)
        } else {
            ChildExit::signaled(signal_number(Signal::Kill), &signal_name(9))
        };
        self.kernel.exit(pid, exit);
    }
}

/// How a program's run ended early: unwound out of the program by [`Process`].
enum Ending {
    Exit(i32),
    Signal(Signal),
}

/// What a simulated program sees of its process: standard streams, signals, children.
///
/// Every method is a switch point. When a signal ends the process, the method it is in (or
/// the next one it calls) does not return: the program unwinds and the process exits.
pub struct Process {
    kernel: Arc<Kernel>,
    pid: u32,
    argv: Vec<String>,
    env: Vec<(String, String)>,
    cwd: PathBuf,
}

impl fmt::Debug for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Process")
            .field("pid", &self.pid)
            .field("argv", &self.argv)
            .finish_non_exhaustive()
    }
}

impl Process {
    /// Its process id.
    #[must_use]
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    /// The program's arguments, starting with its name.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.argv
    }

    /// The value of environment variable `name`.
    #[must_use]
    pub fn env(&self, name: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The working directory.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Unwind if a signal ended the process.
    fn check(&self, state: &State) {
        if let Some(signal) = state.processes.get(&self.pid).and_then(|p| p.fatal) {
            std::panic::resume_unwind(Box::new(Ending::Signal(signal)));
        }
    }

    fn streams(&self) -> [Stream; 3] {
        lock(&self.kernel.state).processes[&self.pid].streams
    }

    /// Read standard input, waiting for something to read. `Ok(0)` is end of input.
    ///
    /// # Errors
    ///
    /// * [`io::Error`] if standard input fails
    pub fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.kernel.switch();
        let stream = self.streams()[0];
        let pid = self.pid;
        let read = self.kernel.wait_for(None, |state| {
            if let Some(signal) = state.processes.get(&pid).and_then(|p| p.fatal) {
                return Wait::Done(Err(signal));
            }
            match stream {
                Stream::Null | Stream::PipeWrite(_) => Wait::Done(Ok(0)),
                Stream::Terminal(t) => {
                    let Some(t) = state.terminals.get_mut(&t) else {
                        return Wait::Done(Ok(0));
                    };
                    let mut read = 0;
                    loop {
                        match t.input.front().copied() {
                            Some(Typed::Byte(byte)) if read < buffer.len() => {
                                buffer[read] = byte;
                                t.input.pop_front();
                                read += 1;
                            }
                            // A canonical read returns one line at most.
                            Some(Typed::LineEnd) => {
                                t.input.pop_front();
                                if read > 0 {
                                    break;
                                }
                            }
                            Some(Typed::Eof) if read == 0 => {
                                t.input.pop_front();
                                return Wait::Done(Ok(0));
                            }
                            _ => break,
                        }
                    }
                    if read > 0 {
                        Wait::Done(Ok(read))
                    } else if t.master_gone() {
                        Wait::Done(Ok(0))
                    } else {
                        Wait::Again
                    }
                }
                Stream::PipeRead(p) => {
                    let Some(p) = state.pipes.get_mut(&p) else {
                        return Wait::Done(Ok(0));
                    };
                    let mut read = 0;
                    while read < buffer.len()
                        && let Some(byte) = p.buffer.pop_front()
                    {
                        buffer[read] = byte;
                        read += 1;
                    }
                    if read > 0 || p.writers == 0 {
                        Wait::Done(Ok(read))
                    } else {
                        Wait::Again
                    }
                }
            }
        });
        match read {
            Some(Ok(read)) => Ok(read),
            Some(Err(signal)) => std::panic::resume_unwind(Box::new(Ending::Signal(signal))),
            None => unreachable!("reads without a deadline do not time out"),
        }
    }

    /// Write all of `bytes` to standard output (`fd` 1) or error (`fd` 2), waiting while the
    /// terminal or pipe is full.
    ///
    /// # Errors
    ///
    /// * [`io::ErrorKind::BrokenPipe`] when nothing reads the stream any more (a hung-up
    ///   terminal, or a pipe whose reader is gone)
    pub fn write_fd(&mut self, fd: usize, bytes: &[u8]) -> io::Result<()> {
        self.kernel.switch();
        let stream = self.streams()[fd.clamp(1, 2)];
        let pid = self.pid;
        let mut rest = bytes;
        while !rest.is_empty() {
            let written = self.kernel.wait_for(None, |state| {
                if let Some(signal) = state.processes.get(&pid).and_then(|p| p.fatal) {
                    return Wait::Done(Err(Ok(signal)));
                }
                match stream {
                    Stream::Null | Stream::PipeRead(_) => Wait::Done(Ok(rest.len())),
                    Stream::Terminal(t) => {
                        let Some(t) = state.terminals.get_mut(&t) else {
                            return Wait::Done(Err(Err(io::ErrorKind::BrokenPipe)));
                        };
                        if t.master_gone() {
                            return Wait::Done(Err(Err(io::ErrorKind::BrokenPipe)));
                        }
                        // A pseudo-console holds output until its cursor query is answered.
                        if t.awaiting_report {
                            return Wait::Again;
                        }
                        let mut written = 0;
                        for &byte in rest {
                            let translated: &[u8] = if byte == b'\n' { b"\r\n" } else { &[byte] };
                            if t.output.len() + translated.len() > t.capacity {
                                break;
                            }
                            t.output.extend(translated);
                            t.written.extend(translated);
                            written += 1;
                        }
                        if written > 0 {
                            Wait::Done(Ok(written))
                        } else {
                            Wait::Again
                        }
                    }
                    Stream::PipeWrite(p) => {
                        let Some(p) = state.pipes.get_mut(&p) else {
                            return Wait::Done(Err(Err(io::ErrorKind::BrokenPipe)));
                        };
                        if p.readers == 0 {
                            return Wait::Done(Err(Err(io::ErrorKind::BrokenPipe)));
                        }
                        let room = p.capacity.saturating_sub(p.buffer.len()).min(rest.len());
                        if room == 0 {
                            return Wait::Again;
                        }
                        p.buffer.extend(&rest[..room]);
                        p.written.extend(&rest[..room]);
                        Wait::Done(Ok(room))
                    }
                }
            });
            match written {
                Some(Ok(written)) => rest = &rest[written..],
                Some(Err(Ok(signal))) => {
                    std::panic::resume_unwind(Box::new(Ending::Signal(signal)))
                }
                Some(Err(Err(kind))) => return Err(io::Error::from(kind)),
                None => unreachable!("writes without a deadline do not time out"),
            }
        }
        Ok(())
    }

    /// Write all of `bytes` to standard output; see [`Self::write_fd`].
    ///
    /// # Errors
    ///
    /// * As [`Self::write_fd`]
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write_fd(1, bytes)
    }

    /// Choose how the process handles `signal`. Children started later inherit it.
    pub fn handle(&mut self, signal: Signal, disposition: Disposition) {
        self.kernel.switch();
        let mut state = lock(&self.kernel.state);
        self.check(&state);
        if let Some(process) = state.processes.get_mut(&self.pid) {
            process.dispositions.insert(signal, disposition);
        }
    }

    /// Caught signals not yet taken, oldest first.
    pub fn signals(&mut self) -> Vec<Signal> {
        self.kernel.switch();
        let mut state = lock(&self.kernel.state);
        self.check(&state);
        state
            .processes
            .get_mut(&self.pid)
            .map_or_else(Vec::new, |p| p.caught.drain(..).collect())
    }

    /// Wait at most `timeout` of simulated time for a caught signal; it is taken.
    pub fn wait_signal(&mut self, timeout: Duration) -> Option<Signal> {
        self.kernel.switch();
        let pid = self.pid;
        let deadline = scheduler::deadline_after(timeout);
        let caught = self.kernel.wait_for(deadline, |state| {
            let Some(process) = state.processes.get_mut(&pid) else {
                return Wait::Done(None);
            };
            if process.fatal.is_some() {
                return Wait::Done(None);
            }
            process
                .caught
                .pop_front()
                .map_or(Wait::Again, |s| Wait::Done(Some(s)))
        });
        self.check(&lock(&self.kernel.state));
        caught.flatten()
    }

    /// Sleep for `duration` of simulated time. A signal that ends the process ends it.
    pub fn sleep(&mut self, duration: Duration) {
        self.kernel.switch();
        let pid = self.pid;
        let deadline = scheduler::deadline_after(duration);
        let _ = self.kernel.wait_for(deadline, |state| {
            if state.processes.get(&pid).is_some_and(|p| p.fatal.is_some()) {
                Wait::Done(())
            } else {
                Wait::Again
            }
        });
        self.check(&lock(&self.kernel.state));
    }

    /// The size of the process's terminal (its controlling terminal, or the one on its
    /// standard input).
    #[must_use]
    pub fn terminal_size(&self) -> Option<PtySize> {
        let state = lock(&self.kernel.state);
        let process = state.processes.get(&self.pid)?;
        let terminal = process.controlling.or_else(|| {
            process.streams.iter().find_map(|s| match s {
                Stream::Terminal(t) => Some(*t),
                _ => None,
            })
        })?;
        state.terminals.get(&terminal).map(|t| t.size)
    }

    /// Turn the terminal's line discipline off (`raw`) or back on: echo, `ctrl+c` handling,
    /// `\r` translation and canonical input (a line typed so far becomes readable).
    pub fn set_raw(&mut self, raw: bool) {
        self.kernel.switch();
        let mut state = lock(&self.kernel.state);
        self.check(&state);
        let terminal = state.processes.get(&self.pid).and_then(|p| p.controlling);
        if let Some(t) = terminal.and_then(|t| state.terminals.get_mut(&t)) {
            t.echo = !raw;
            t.isig = !raw;
            t.icrnl = !raw;
            t.icanon = !raw;
            if raw {
                let line = std::mem::take(&mut t.line);
                t.input.extend(line.into_iter().map(Typed::Byte));
            }
        }
        drop(state);
    }

    /// Start a child running `argv` with this process's standard streams. With `new_group`
    /// it leads a process group of its own (as a shell's background job does); otherwise it
    /// joins this one's. Nobody waits for it: it is reaped when it exits.
    ///
    /// # Errors
    ///
    /// * [`io::ErrorKind::NotFound`] when no program has that name
    pub fn spawn(&mut self, argv: &[&str], new_group: bool) -> io::Result<u32> {
        let mut command = SpawnCommand::new(argv.first().copied().unwrap_or_default());
        command.args(&argv[1..]).env_clear();
        for (name, value) in &self.env {
            command.env(name, value);
        }
        command.cwd(&self.cwd);
        let streams = self.streams();
        let spawned = self.kernel.spawn(
            &command,
            None,
            Some((
                self.pid,
                streams,
                if new_group {
                    Placement::NewGroup
                } else {
                    Placement::SameGroup
                },
            )),
        );
        // Refused because a signal ended this process: end it now.
        self.check(&lock(&self.kernel.state));
        Ok(spawned?.0)
    }

    /// Start a child running `argv` detached, as a daemon does (`setsid` with its standard
    /// streams on the null device): it leads a new session with no controlling terminal, so
    /// its parent's terminal and session ending do not touch it. On Windows it is a detached
    /// process that stays in its parent's job (a console process cannot leave it).
    ///
    /// # Errors
    ///
    /// * [`io::ErrorKind::NotFound`] when no program has that name
    pub fn spawn_session(&mut self, argv: &[&str]) -> io::Result<u32> {
        let mut command = SpawnCommand::new(argv.first().copied().unwrap_or_default());
        command.args(&argv[1..]).env_clear();
        for (name, value) in &self.env {
            command.env(name, value);
        }
        command.cwd(&self.cwd);
        let spawned = self.kernel.spawn(
            &command,
            None,
            Some((self.pid, [Stream::Null; 3], Placement::NewSession)),
        );
        self.check(&lock(&self.kernel.state));
        Ok(spawned?.0)
    }

    /// End the process with exit code `code`.
    pub fn exit(&mut self, code: i32) -> ! {
        std::panic::resume_unwind(Box::new(Ending::Exit(code)))
    }
}

/// The backend's types behind the crate's public wrappers.
// Signatures are uniform with the native backends, which need the results, exclusive access
// and handles that a simulated kernel does not.
#[allow(
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_ref_mut,
    clippy::unused_self
)]
pub(crate) mod backend {
    use std::{
        collections::BTreeSet,
        io,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
        time::Duration,
    };

    use switchy_thread::simulator as scheduler;

    use super::{
        ChildExit, HANGUP_CHECKS, HANGUP_GRACE, Kernel, ProcessState, PtySize, Signal,
        SpawnCommand, State, Stream, Wait, kernel, lock,
    };
    pub use super::{base_environment, signal_group};

    /// The current run's personality's behaviour.
    pub fn behavior() -> crate::Behavior {
        crate::Behavior::of(super::personality().is_windows())
    }
    // Signals in std exit statuses exist only on Unix.
    #[cfg(unix)]
    pub use super::signal_name;

    pub fn open(size: PtySize, options: crate::PtyOptions) -> io::Result<(Master, Slave)> {
        let kernel = kernel();
        let terminal = kernel.open(size, options.inherit_cursor);
        let name = (!lock(&kernel.state).config.personality.is_windows())
            .then(|| PathBuf::from(format!("/dev/simpts/{terminal}")));
        Ok((
            Master {
                kernel: Arc::clone(&kernel),
                terminal,
                name,
            },
            Slave { kernel, terminal },
        ))
    }

    #[derive(Debug)]
    pub struct Master {
        kernel: Arc<Kernel>,
        terminal: u64,
        name: Option<PathBuf>,
    }

    impl Drop for Master {
        fn drop(&mut self) {
            let mut state = lock(&self.kernel.state);
            Kernel::close_master(&mut state, self.terminal);
            let closed = Kernel::close_console(&mut state, self.terminal);
            drop(state);
            self.kernel.wake();
            if closed {
                self.kernel.wait_console_drained(self.terminal);
            }
        }
    }

    impl Master {
        fn open_handle(&self) -> io::Result<()> {
            self.kernel.switch();
            let mut state = lock(&self.kernel.state);
            let t = state
                .terminals
                .get_mut(&self.terminal)
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            t.master_handles += 1;
            drop(state);
            Ok(())
        }

        pub fn resize(&self, size: PtySize) -> io::Result<()> {
            self.kernel.switch();
            let mut state = lock(&self.kernel.state);
            if let Some(t) = state.terminals.get_mut(&self.terminal) {
                t.size = size;
            }
            Kernel::signal_foreground(&mut state, self.terminal, Signal::WindowChange);
            drop(state);
            self.kernel.wake();
            Ok(())
        }

        pub fn size(&self) -> io::Result<PtySize> {
            self.kernel.switch();
            lock(&self.kernel.state)
                .terminals
                .get(&self.terminal)
                .map(|t| t.size)
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }

        pub fn try_clone_reader(&self) -> io::Result<Reader> {
            self.open_handle()?;
            if let Some(t) = lock(&self.kernel.state).terminals.get_mut(&self.terminal) {
                t.readers += 1;
            }
            Ok(Reader {
                kernel: Arc::clone(&self.kernel),
                terminal: self.terminal,
            })
        }

        pub fn take_writer(&self) -> io::Result<Input> {
            {
                let mut state = lock(&self.kernel.state);
                let t = state
                    .terminals
                    .get_mut(&self.terminal)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
                if t.took_writer {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "the terminal's writer was already taken",
                    ));
                }
                t.took_writer = true;
                drop(state);
            }
            self.open_handle()?;
            Ok(Input {
                kernel: Arc::clone(&self.kernel),
                terminal: self.terminal,
                end_on_drop: true,
            })
        }

        pub fn clone_writer(&self) -> io::Result<Input> {
            self.open_handle()?;
            Ok(Input {
                kernel: Arc::clone(&self.kernel),
                terminal: self.terminal,
                end_on_drop: false,
            })
        }

        pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
            self.kernel.switch();
            if let Some(t) = lock(&self.kernel.state).terminals.get_mut(&self.terminal) {
                t.nonblocking = nonblocking;
            }
            Ok(())
        }

        pub fn has_peer(&self) -> bool {
            self.kernel.switch();
            lock(&self.kernel.state)
                .terminals
                .get(&self.terminal)
                .is_some_and(|t| t.windows || (t.slave_holders > 0 && !t.hung_up))
        }

        pub fn tty_name(&self) -> Option<&Path> {
            // A pseudo-console has no device path.
            self.name.as_deref()
        }

        pub fn foreground_process_group(&self) -> Option<i32> {
            lock(&self.kernel.state)
                .terminals
                .get(&self.terminal)
                .filter(|t| !t.windows)
                .and_then(|t| t.foreground)
                .and_then(|g| i32::try_from(g).ok())
        }
    }

    #[derive(Debug)]
    pub struct Reader {
        kernel: Arc<Kernel>,
        terminal: u64,
    }

    impl Drop for Reader {
        fn drop(&mut self) {
            let mut state = lock(&self.kernel.state);
            if let Some(t) = state.terminals.get_mut(&self.terminal) {
                t.readers = t.readers.saturating_sub(1);
            }
            Kernel::close_master(&mut state, self.terminal);
            drop(state);
            self.kernel.wake();
        }
    }

    /// Whether `terminal` has output to read or is at its end.
    fn readable(state: &State, terminal: u64) -> bool {
        state.terminals.get(&terminal).is_none_or(|t| {
            !t.output.is_empty()
                || t.output_ended()
                || (!t.windows && t.hung_up && t.master_handles > 0)
        })
    }

    impl Reader {
        pub fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.kernel.switch();
            let faults = lock(&self.kernel.state).config.faults;
            if self.kernel.chance(faults.interrupted) {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            if self.kernel.chance(faults.read_error) {
                return Err(io::Error::other("input/output error"));
            }
            let limit = if buffer.len() > 1 && self.kernel.chance(faults.short) {
                1 + usize::try_from(lock(&self.kernel.rng).below(buffer.len() as u64 - 1))
                    .unwrap_or(0)
            } else {
                buffer.len()
            };
            let terminal = self.terminal;
            let mut first = true;
            self.kernel
                .wait_for(None, |state| {
                    let Some(t) = state.terminals.get_mut(&terminal) else {
                        return Wait::Done(Ok(0));
                    };
                    if !t.output.is_empty() {
                        let count = t.output.len().min(limit);
                        for (slot, byte) in buffer.iter_mut().zip(t.output.drain(..count)) {
                            *slot = byte;
                        }
                        return Wait::Done(Ok(count));
                    }
                    if t.output_ended() {
                        // A terminal nothing holds open reads as end of file (EIO natively).
                        return Wait::Done(Ok(0));
                    }
                    if t.nonblocking && first {
                        return Wait::Done(Err(io::Error::from(io::ErrorKind::WouldBlock)));
                    }
                    first = false;
                    Wait::Again
                })
                .unwrap_or_else(|| unreachable!("reads without a deadline do not time out"))
        }

        pub fn wait_readable(&self, timeout: Duration) -> io::Result<bool> {
            self.kernel.switch();
            if self
                .kernel
                .chance(lock(&self.kernel.state).config.faults.interrupted)
            {
                // An interrupted poll: the native backends report it as not ready.
                return Ok(false);
            }
            let terminal = self.terminal;
            Ok(self
                .kernel
                .wait_for(scheduler::deadline_after(timeout), |state| {
                    if readable(state, terminal) {
                        Wait::Done(())
                    } else {
                        Wait::Again
                    }
                })
                .is_some())
        }
    }

    #[derive(Debug)]
    pub struct Input {
        kernel: Arc<Kernel>,
        terminal: u64,
        end_on_drop: bool,
    }

    impl Drop for Input {
        fn drop(&mut self) {
            let mut state = lock(&self.kernel.state);
            if self.end_on_drop {
                // A newline, then the end-of-file character: it only acts at the start of a line.
                let _ = Kernel::type_input(&mut state, self.terminal, b"\n\x04");
            }
            Kernel::close_master(&mut state, self.terminal);
            drop(state);
            self.kernel.wake();
        }
    }

    impl Input {
        pub fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.kernel.switch();
            if bytes.is_empty() {
                return Ok(0);
            }
            let faults = lock(&self.kernel.state).config.faults;
            if self.kernel.chance(faults.interrupted) {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            if self.kernel.chance(faults.write_stall) {
                let millis = super::WRITE_STALL
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX);
                let stall = Duration::from_millis(1 + lock(&self.kernel.rng).below(millis));
                if let Some(until) = scheduler::deadline_after(stall) {
                    while !self.kernel.block(Some(until)) {}
                }
            }
            let limit = if bytes.len() > 1 && self.kernel.chance(faults.short) {
                1 + usize::try_from(lock(&self.kernel.rng).below(bytes.len() as u64 - 1))
                    .unwrap_or(0)
            } else {
                bytes.len()
            };
            let terminal = self.terminal;
            let mut first = true;
            self.kernel
                .wait_for(None, |state| {
                    let Some(t) = state.terminals.get(&terminal) else {
                        return Wait::Done(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
                    };
                    if t.output_ended() {
                        return Wait::Done(Err(io::Error::other("input/output error")));
                    }
                    let nonblocking = t.nonblocking;
                    let consumed = Kernel::type_input(state, terminal, &bytes[..limit]);
                    if consumed > 0 {
                        return Wait::Done(Ok(consumed));
                    }
                    if nonblocking && first {
                        return Wait::Done(Err(io::Error::from(io::ErrorKind::WouldBlock)));
                    }
                    first = false;
                    Wait::Again
                })
                .unwrap_or_else(|| unreachable!("writes without a deadline do not time out"))
        }

        // Uniform with the native backends.
        #[allow(clippy::unnecessary_wraps, clippy::unused_self)]
        pub const fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }

        pub fn wait_writable(&self, timeout: Duration) -> io::Result<bool> {
            self.kernel.switch();
            if self
                .kernel
                .chance(lock(&self.kernel.state).config.faults.interrupted)
            {
                // An interrupted poll: the native backends report it as not ready.
                return Ok(false);
            }
            let terminal = self.terminal;
            Ok(self
                .kernel
                .wait_for(scheduler::deadline_after(timeout), |state| {
                    match state.terminals.get(&terminal) {
                        Some(t) if t.typed() >= t.capacity && !t.output_ended() => Wait::Again,
                        _ => Wait::Done(()),
                    }
                })
                .is_some())
        }
    }

    #[derive(Debug)]
    pub struct Slave {
        kernel: Arc<Kernel>,
        terminal: u64,
    }

    impl Drop for Slave {
        fn drop(&mut self) {
            let mut state = lock(&self.kernel.state);
            Kernel::release(&mut state, Stream::Terminal(self.terminal));
            let closed = Kernel::close_console(&mut state, self.terminal);
            drop(state);
            self.kernel.wake();
            if closed {
                self.kernel.wait_console_drained(self.terminal);
            }
        }
    }

    impl Slave {
        /// Start `spec` as [`Self::spawn`] does, in `job`.
        pub fn spawn_in(&self, spec: &SpawnCommand, job: &Job) -> io::Result<Child> {
            let child = self.spawn(spec)?;
            job.adopt(child.pid);
            Ok(child)
        }

        pub fn spawn(&self, spec: &SpawnCommand) -> io::Result<Child> {
            let (pid, pipes) = self.kernel.spawn(spec, Some(self.terminal), None)?;
            let reader = |pipe| PipeReader {
                kernel: Arc::clone(&self.kernel),
                pipe,
            };
            Ok(Child {
                kernel: Arc::clone(&self.kernel),
                pid,
                stdin: pipes.stdin.map(|pipe| PipeWriter {
                    kernel: Arc::clone(&self.kernel),
                    pipe,
                }),
                stdout: pipes.stdout.map(reader),
                stderr: pipes.stderr.map(reader),
            })
        }
    }

    /// The processes spawned into a job and everything they start, as a kill-on-close Job
    /// Object holds them on Windows: the kernel's sessions of its processes (a Windows process
    /// stays in the session it was created in, so its descendants are the job's too).
    #[derive(Debug)]
    pub struct Job {
        kernel: Arc<Kernel>,
        sessions: Mutex<BTreeSet<u64>>,
    }

    impl Job {
        pub fn new() -> io::Result<Self> {
            let kernel = kernel();
            if !lock(&kernel.state).config.personality.is_windows() {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "jobs exist on Windows only (Unix processes have groups)",
                ));
            }
            Ok(Self {
                kernel,
                sessions: Mutex::new(BTreeSet::new()),
            })
        }

        fn adopt(&self, pid: u32) {
            let session = lock(&self.kernel.state)
                .processes
                .get(&pid)
                .map(|process| process.session_serial);
            if let Some(session) = session {
                lock(&self.sessions).insert(session);
            }
        }

        /// End every running process in the job, at once (exit code 1, as Windows'
        /// `TerminateJobObject` here).
        pub fn terminate(&self) -> io::Result<()> {
            self.kernel.switch();
            let sessions = lock(&self.sessions).clone();
            let mut state = lock(&self.kernel.state);
            let members: Vec<u32> = state
                .processes
                .iter()
                .filter(|(_, p)| {
                    p.state == ProcessState::Running && sessions.contains(&p.session_serial)
                })
                .map(|(pid, _)| *pid)
                .collect();
            for pid in members {
                Kernel::deliver(&mut state, pid, Signal::Kill);
            }
            drop(state);
            self.kernel.wake();
            Ok(())
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // Kill on close: the last handle gone, what is left ends.
            let _ = self.terminate();
        }
    }

    #[derive(Debug)]
    pub struct Child {
        kernel: Arc<Kernel>,
        pid: u32,
        stdin: Option<PipeWriter>,
        stdout: Option<PipeReader>,
        stderr: Option<PipeReader>,
    }

    impl Drop for Child {
        fn drop(&mut self) {
            // Nobody will wait for it any more: reap it now if it exited, or when it does.
            let mut state = lock(&self.kernel.state);
            if let Some(process) = state.processes.get_mut(&self.pid) {
                process.waited = false;
                if matches!(process.state, ProcessState::Zombie(_)) {
                    state.processes.remove(&self.pid);
                }
            }
        }
    }

    impl Child {
        pub const fn id(&self) -> u32 {
            self.pid
        }

        pub const fn take_stdin(&mut self) -> Option<PipeWriter> {
            self.stdin.take()
        }

        pub const fn take_stdout(&mut self) -> Option<PipeReader> {
            self.stdout.take()
        }

        pub const fn take_stderr(&mut self) -> Option<PipeReader> {
            self.stderr.take()
        }

        fn reap(state: &mut State, pid: u32) -> Option<ChildExit> {
            match state.processes.get(&pid).map(|p| p.state.clone()) {
                Some(ProcessState::Zombie(exit)) => {
                    state.processes.remove(&pid);
                    Some(exit)
                }
                _ => None,
            }
        }

        pub fn try_wait(&mut self) -> io::Result<Option<ChildExit>> {
            self.kernel.switch();
            let mut state = lock(&self.kernel.state);
            if !state.processes.contains_key(&self.pid) {
                return Err(io::Error::other("the process was already waited for"));
            }
            let exit = Self::reap(&mut state, self.pid);
            drop(state);
            Ok(exit)
        }

        pub fn wait(&mut self) -> io::Result<ChildExit> {
            self.kernel.switch();
            let pid = self.pid;
            self.kernel
                .wait_for(None, |state| {
                    if !state.processes.contains_key(&pid) {
                        return Wait::Done(Err(io::Error::other(
                            "the process was already waited for",
                        )));
                    }
                    Self::reap(state, pid).map_or(Wait::Again, |exit| Wait::Done(Ok(exit)))
                })
                .unwrap_or_else(|| unreachable!("waits without a deadline do not time out"))
        }

        pub fn wait_unreaped(&mut self) -> io::Result<ChildExit> {
            self.kernel.switch();
            let pid = self.pid;
            self.kernel
                .wait_for(None, |state| {
                    state.processes.get(&pid).map_or_else(
                        || Wait::Done(Err(io::Error::other("the process was already waited for"))),
                        |process| match &process.state {
                            ProcessState::Zombie(exit) => Wait::Done(Ok(exit.clone())),
                            ProcessState::Running => Wait::Again,
                        },
                    )
                })
                .unwrap_or_else(|| unreachable!("waits without a deadline do not time out"))
        }

        pub fn kill(&mut self) -> io::Result<()> {
            if self.exited()? {
                return Ok(());
            }
            // Windows terminates at once.
            if lock(&self.kernel.state).config.personality.is_windows() {
                signal_pid(&self.kernel, self.pid, Signal::Kill);
                return Ok(());
            }
            signal_pid(&self.kernel, self.pid, Signal::Hangup);
            for check in 0..HANGUP_CHECKS {
                if check > 0 {
                    switchy_thread::thread::sleep(HANGUP_GRACE);
                }
                if self.exited()? {
                    return Ok(());
                }
            }
            signal_pid(&self.kernel, self.pid, Signal::Kill);
            Ok(())
        }

        fn exited(&self) -> io::Result<bool> {
            self.kernel.switch();
            let state = lock(&self.kernel.state);
            Ok(state
                .processes
                .get(&self.pid)
                .is_none_or(|p| p.state != ProcessState::Running))
        }

        // Fallible on every platform: Windows duplicates a handle.
        #[allow(clippy::unnecessary_wraps)]
        pub fn killer(&self) -> io::Result<Killer> {
            Ok(Killer {
                kernel: Arc::clone(&self.kernel),
                pid: self.pid,
            })
        }
    }

    fn signal_pid(kernel: &Kernel, pid: u32, signal: Signal) {
        kernel.switch();
        let mut state = lock(&kernel.state);
        kernel.send(&mut state, pid, signal);
        drop(state);
        kernel.wake();
    }

    #[derive(Debug, Clone)]
    pub struct Killer {
        kernel: Arc<Kernel>,
        pid: u32,
    }

    impl Killer {
        // Fallible on every platform: the native backends signal the operating system.
        #[allow(clippy::unnecessary_wraps)]
        pub fn kill(&self) -> io::Result<()> {
            let signal = if lock(&self.kernel.state).config.personality.is_windows() {
                Signal::Kill
            } else {
                Signal::Hangup
            };
            signal_pid(&self.kernel, self.pid, signal);
            Ok(())
        }
    }

    #[derive(Debug)]
    pub struct PipeReader {
        kernel: Arc<Kernel>,
        pipe: u64,
    }

    impl Drop for PipeReader {
        fn drop(&mut self) {
            let mut state = lock(&self.kernel.state);
            Kernel::release(&mut state, Stream::PipeRead(self.pipe));
            drop(state);
            self.kernel.wake();
        }
    }

    impl PipeReader {
        pub fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.kernel.switch();
            let pipe = self.pipe;
            let mut first = true;
            self.kernel
                .wait_for(None, |state| {
                    let Some(p) = state.pipes.get_mut(&pipe) else {
                        return Wait::Done(Ok(0));
                    };
                    let mut read = 0;
                    while read < buffer.len()
                        && let Some(byte) = p.buffer.pop_front()
                    {
                        buffer[read] = byte;
                        read += 1;
                    }
                    if read > 0 || p.writers == 0 {
                        return Wait::Done(Ok(read));
                    }
                    if p.nonblocking && first {
                        return Wait::Done(Err(io::Error::from(io::ErrorKind::WouldBlock)));
                    }
                    first = false;
                    Wait::Again
                })
                .unwrap_or_else(|| unreachable!("reads without a deadline do not time out"))
        }

        pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
            self.kernel.switch();
            if let Some(p) = lock(&self.kernel.state).pipes.get_mut(&self.pipe) {
                p.nonblocking = nonblocking;
            }
            Ok(())
        }

        pub fn wait_readable(&self, timeout: Duration) -> io::Result<bool> {
            self.kernel.switch();
            let pipe = self.pipe;
            Ok(self
                .kernel
                .wait_for(scheduler::deadline_after(timeout), |state| {
                    match state.pipes.get(&pipe) {
                        Some(p) if p.buffer.is_empty() && p.writers > 0 => Wait::Again,
                        _ => Wait::Done(()),
                    }
                })
                .is_some())
        }
    }

    #[derive(Debug)]
    pub struct PipeWriter {
        kernel: Arc<Kernel>,
        pipe: u64,
    }

    impl Drop for PipeWriter {
        fn drop(&mut self) {
            let mut state = lock(&self.kernel.state);
            Kernel::release(&mut state, Stream::PipeWrite(self.pipe));
            drop(state);
            self.kernel.wake();
        }
    }

    impl PipeWriter {
        pub fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.kernel.switch();
            if bytes.is_empty() {
                return Ok(0);
            }
            let pipe = self.pipe;
            self.kernel
                .wait_for(None, |state| {
                    let Some(p) = state.pipes.get_mut(&pipe) else {
                        return Wait::Done(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
                    };
                    if p.readers == 0 {
                        return Wait::Done(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
                    }
                    let room = p.capacity.saturating_sub(p.buffer.len()).min(bytes.len());
                    if room == 0 {
                        return Wait::Again;
                    }
                    p.buffer.extend(&bytes[..room]);
                    p.written.extend(&bytes[..room]);
                    Wait::Done(Ok(room))
                })
                .unwrap_or_else(|| unreachable!("writes without a deadline do not time out"))
        }

        // Uniform with the native backends.
        #[allow(clippy::unnecessary_wraps, clippy::unused_self)]
        pub const fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
