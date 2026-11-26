//! Where operating systems differ, the simulator's personalities must behave as the real
//! systems do. Each scenario here runs natively (`--features native`, on the host's operating
//! system) and in simulation (`--features simulator`, under every personality: Linux, macOS and
//! Windows on any host). The assertions are shared and follow [`switchy_pty::behavior`], so a
//! scenario that passes natively on an operating system and in simulation under its personality
//! shows that the personality models it.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::io::{Read as _, Write as _};
use std::time::Duration;

use switchy_pty::{PtyInput, PtyMaster, PtyPair, PtySize, SpawnCommand};
use switchy_thread::sync::mpsc;

/// How long anything may take natively (simulated time is much shorter).
const TIMEOUT: Duration = Duration::from_secs(20);
/// How long to watch for something that must not happen.
const QUIET: Duration = Duration::from_millis(500);
const CURSOR_QUERY: &[u8] = b"\x1b[6n";

/// A program for each world: a Unix shell command, a Windows `cmd` command, a simulated
/// program.
fn program(unix: &str, windows: &str, simulated: &[&str]) -> SpawnCommand {
    if cfg!(feature = "simulator") {
        let mut command = SpawnCommand::new(simulated[0]);
        command.args(&simulated[1..]);
        command
    } else if cfg!(windows) {
        let mut command = SpawnCommand::new("cmd");
        command.args(["/C", windows]);
        command
    } else {
        let mut command = SpawnCommand::new("/bin/sh");
        command.args(["-c", unix]);
        command
    }
}

fn printing_hello() -> SpawnCommand {
    program("echo hello", "echo hello", &["echo", "hello"])
}

fn long_running() -> SpawnCommand {
    program(
        "sleep 30",
        "ping -n 31 127.0.0.1 > nul",
        &["sleep", "30000"],
    )
}

/// A terminal whose output a thread collects; `None` marks its end.
struct Terminal {
    master: Option<PtyMaster>,
    writer: Option<PtyInput>,
    output: mpsc::Receiver<Option<Vec<u8>>>,
    seen: Vec<u8>,
    ended: bool,
}

impl Terminal {
    fn open(pair: &PtyPair) -> mpsc::Receiver<Option<Vec<u8>>> {
        let mut reader = pair.master.try_clone_reader().unwrap();
        let (sender, output) = mpsc::channel();
        switchy_thread::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => {
                        let _ = sender.send(None);
                        return;
                    }
                    Ok(read) => {
                        if sender.send(Some(buffer[..read].to_vec())).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        output
    }

    fn new(pair: PtyPair) -> (Self, switchy_pty::PtySlave) {
        let output = Self::open(&pair);
        let PtyPair { slave, master } = pair;
        let writer = master.take_writer().ok();
        (
            Self {
                master: Some(master),
                writer,
                output,
                seen: Vec::new(),
                ended: false,
            },
            slave,
        )
    }

    /// Read until `done` holds for what was read, the output ends, or `within` passes; whether
    /// `done` held.
    fn read_until(&mut self, within: Duration, done: impl Fn(&[u8]) -> bool) -> bool {
        let deadline = switchy_time::instant_now() + within;
        while !done(&self.seen) && !self.ended {
            let left = deadline.saturating_duration_since(switchy_time::instant_now());
            match self.output.recv_timeout(left) {
                Ok(Some(bytes)) => self.seen.extend(bytes),
                Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => self.ended = true,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
            }
        }
        done(&self.seen)
    }

    /// Answer the console's cursor query, where the backend asks one.
    fn answer_startup_query(&mut self) {
        if switchy_pty::behavior().startup_cursor_query {
            assert!(
                self.read_until(TIMEOUT, |seen| contains(seen, CURSOR_QUERY)),
                "the console asked for the cursor: {:?}",
                String::from_utf8_lossy(&self.seen)
            );
            let writer = self.writer.as_mut().unwrap();
            writer.write_all(b"\x1b[1;1R").unwrap();
            writer.flush().unwrap();
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn position(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(not(feature = "simulator"))]
fn scenario(test: impl Fn()) {
    test();
}

/// Every personality, on any host.
#[cfg(feature = "simulator")]
fn scenario(test: impl Fn()) {
    use switchy_pty::simulator::{self, Config, Faults, Personality};
    for personality in [Personality::Linux, Personality::MacOs, Personality::Windows] {
        for seed in 0..8 {
            let outcome = switchy_thread::simulator::run(seed, || {
                switchy_time::simulator::set_step_multiplier(1);
                simulator::configure(Config {
                    personality,
                    pid_reuse: 0.5,
                    faults: Faults::default(),
                });
                simulator::install_default_programs();
                test();
                simulator::kill_all();
                switchy_thread::simulator::run_until_idle();
            });
            if let Err(panic) = outcome.result {
                eprintln!("{personality:?}, seed {seed} failed");
                std::panic::resume_unwind(panic);
            }
        }
    }
}

#[test]
fn output_waits_for_the_startup_query_where_the_console_asks_one() {
    scenario(|| {
        let pair = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let (mut terminal, slave) = Terminal::new(pair);
        let mut child = slave.spawn(&printing_hello()).unwrap();
        if switchy_pty::behavior().startup_cursor_query {
            assert!(terminal.read_until(TIMEOUT, |seen| contains(seen, CURSOR_QUERY)));
            // Held until answered.
            assert!(!terminal.read_until(QUIET, |seen| contains(seen, b"hello")));
            terminal.answer_startup_query();
        }
        assert!(
            terminal.read_until(TIMEOUT, |seen| contains(seen, b"hello")),
            "{:?}",
            String::from_utf8_lossy(&terminal.seen)
        );
        if let Some(query) = position(&terminal.seen, CURSOR_QUERY) {
            assert!(switchy_pty::behavior().startup_cursor_query);
            assert!(query < position(&terminal.seen, b"hello").unwrap());
        }
        assert_eq!(child.wait().unwrap().exit_code(), 0);
        drop(slave);
    });
}

#[test]
fn output_ends_with_the_programs_or_with_the_console() {
    scenario(|| {
        let pair = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let (mut terminal, slave) = Terminal::new(pair);
        let mut child = slave.spawn(&printing_hello()).unwrap();
        terminal.answer_startup_query();
        child.wait().unwrap();
        drop(slave);
        if switchy_pty::behavior().output_ends_with_console {
            terminal.read_until(QUIET, |_| false);
            assert!(!terminal.ended, "output ended before the console closed");
            drop(terminal.writer.take());
            drop(terminal.master.take());
        }
        terminal.read_until(TIMEOUT, |_| false);
        assert!(terminal.ended, "output never ended");
    });
}

#[test]
fn killed_processes_report_signals_or_exit_codes() {
    scenario(|| {
        let pair = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let (mut terminal, slave) = Terminal::new(pair);
        let mut child = slave.spawn(&long_running()).unwrap();
        terminal.answer_startup_query();
        child.kill().unwrap();
        let exit = child.wait().unwrap();
        if switchy_pty::behavior().process_groups {
            assert!(exit.signal().is_some(), "{exit:?}");
        } else {
            assert_eq!(exit.signal(), None, "{exit:?}");
            assert_eq!(exit.exit_code(), 1, "{exit:?}");
        }
        drop(slave);
    });
}

#[test]
fn process_groups_and_terminal_devices_exist_only_where_described() {
    scenario(|| {
        let behavior = switchy_pty::behavior();
        let pair = PtyPair::open(PtySize::new(80, 24)).unwrap();
        assert_eq!(pair.master.tty_name().is_some(), behavior.terminal_device);
        let (mut terminal, slave) = Terminal::new(pair);
        let mut child = slave.spawn(&long_running()).unwrap();
        terminal.answer_startup_query();
        let master = terminal.master.as_ref().unwrap();
        assert_eq!(
            master.foreground_process_group().is_some(),
            behavior.terminal_device
        );
        let signalled = switchy_pty::signal_group(child.id(), switchy_pty::Signal::Kill);
        if behavior.process_groups {
            signalled.unwrap();
        } else {
            assert_eq!(
                signalled.unwrap_err().kind(),
                std::io::ErrorKind::Unsupported
            );
            child.kill().unwrap();
        }
        child.wait().unwrap();
        drop(slave);
    });
}

/// A program writing `out` to standard output and `err` to standard error, a line each.
fn printing_out_and_err() -> SpawnCommand {
    program(
        "echo out; echo err >&2",
        "echo out& echo err 1>&2",
        &["out-err", "out", "err"],
    )
}

fn read_trimmed(reader: &mut impl std::io::Read) -> String {
    let mut text = String::new();
    reader.read_to_string(&mut text).unwrap();
    text.trim().to_owned()
}

#[test]
fn redirected_streams_have_pipes_of_their_own() {
    use switchy_pty::Stdio;
    scenario(|| {
        let pair = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut command = printing_out_and_err();
        command
            .stdin(Stdio::Null)
            .stdout(Stdio::Piped)
            .stderr(Stdio::Piped);
        let mut child = pair.slave.spawn(&command).unwrap();
        let mut stdout = child.take_stdout().unwrap();
        let mut stderr = child.take_stderr().unwrap();
        assert_eq!(read_trimmed(&mut stdout), "out");
        assert_eq!(read_trimmed(&mut stderr), "err");
        assert_eq!(child.wait().unwrap().exit_code(), 0);
        drop(pair);
    });
}

#[test]
fn the_console_is_on_all_streams_or_none_where_described() {
    use switchy_pty::Stdio;
    scenario(|| {
        let behavior = switchy_pty::behavior();
        let pair = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let mut command = printing_hello();
        command.stdout(Stdio::Piped);
        let spawned = pair.slave.spawn(&command);
        if behavior.console_on_all_streams_or_none {
            assert_eq!(spawned.unwrap_err().kind(), std::io::ErrorKind::Unsupported);
        } else {
            let mut child = spawned.unwrap();
            assert_eq!(read_trimmed(&mut child.take_stdout().unwrap()), "hello");
            child.wait().unwrap();
        }
        drop(pair);
    });
}

#[test]
fn jobs_exist_where_processes_have_no_groups() {
    scenario(|| {
        let behavior = switchy_pty::behavior();
        let job = switchy_pty::PtyJob::new();
        if behavior.process_groups {
            assert_eq!(job.unwrap_err().kind(), std::io::ErrorKind::Unsupported);
            return;
        }
        let job = job.unwrap();
        let pair = PtyPair::open(PtySize::new(80, 24)).unwrap();
        let (mut terminal, slave) = Terminal::new(pair);
        let mut child = slave.spawn_in(&long_running(), &job).unwrap();
        terminal.answer_startup_query();
        job.terminate().unwrap();
        assert_eq!(child.wait().unwrap().exit_code(), 1);
        drop(slave);
    });
}
