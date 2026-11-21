//! Pseudo-terminals and the processes running on them.
//!
//! A [`PtyPair`] is both ends of a new terminal: the [`PtyMaster`] side to read output from,
//! write input to and resize, and the [`PtySlave`] side to start processes on with
//! [`PtySlave::spawn`]. Nothing runs in the background: reading, writing, waiting and cleanup
//! are the caller's, so it can supervise processes, hand the terminal to a helper by path, or
//! keep it open after a process exits.
//!
//! A [`SpawnCommand`] describes a process. Like a login terminal it starts from the caller's
//! environment, captured when the command is created, and every change to it can be read back.
//! Each of its standard streams can be the terminal, a pipe or nothing ([`Stdio`]).
//!
//! # Backends
//!
//! * `native`: on Unix an `openpty` terminal; on Windows a pseudo-console (`ConPTY`), where the
//!   console stays open until the master and slave are both dropped, and dropping the last of
//!   them may wait for the console to flush its remaining output to the reader.
//! * `simulator` (takes precedence): an in-memory terminal and process table driven by the
//!   current simulation run, with simulated programs instead of executables.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![cfg(any(unix, windows))]

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;

use std::time::Duration;

#[cfg(all(feature = "native", not(feature = "simulator"), unix))]
#[path = "unix.rs"]
mod native;
#[cfg(all(feature = "native", not(feature = "simulator"), windows))]
#[path = "windows.rs"]
mod native;
#[cfg(feature = "simulator")]
pub mod simulator;
#[cfg(feature = "simulator")]
use simulator::backend as native;

/// Terminal size in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PtySize {
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
}

impl PtySize {
    /// A size of `cols` columns by `rows` rows; zero is raised to one.
    #[must_use]
    pub const fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: if cols == 0 { 1 } else { cols },
            rows: if rows == 0 { 1 } else { rows },
        }
    }
}

/// Where one of a spawned process's standard streams goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stdio {
    /// The terminal.
    #[default]
    Terminal,
    /// A pipe to this process: see [`PtyChild::take_stdin`] and [`PtyChild::take_stdout`].
    Piped,
    /// Nothing: reads see end of file and writes are discarded.
    Null,
}

/// A signal for [`signal_group`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Signal {
    /// `SIGHUP`: the terminal closed.
    Hangup,
    /// `SIGINT`: interrupt, as `ctrl+c`.
    Interrupt,
    /// `SIGKILL`: end at once.
    Kill,
    /// `SIGTERM`: please end.
    Terminate,
    /// `SIGQUIT`: quit, as `ctrl+\\`.
    Quit,
    /// `SIGWINCH`: the window size changed. Ignored unless handled.
    WindowChange,
}

/// Send `signal` to every process in process group `group`. A group with no processes left
/// counts as done.
///
/// # Errors
/// * [`io::Error`] when the group cannot be signalled.
/// * [`io::ErrorKind::Unsupported`] where processes have no groups (Windows).
pub fn signal_group(group: u32, signal: Signal) -> io::Result<()> {
    native::signal_group(group, signal)
}

/// A new pseudo-terminal's two ends. The slave is listed first so it is dropped first.
#[derive(Debug)]
pub struct PtyPair {
    /// The side processes run on.
    pub slave: PtySlave,
    /// The side that reads their output, writes their input and resizes the terminal.
    pub master: PtyMaster,
}

impl PtyPair {
    /// Open a new pseudo-terminal of `size`.
    ///
    /// # Errors
    /// * [`io::Error`] when the system cannot allocate a terminal.
    pub fn open(size: PtySize) -> io::Result<Self> {
        let (master, slave) = native::open(size)?;
        Ok(Self {
            slave: PtySlave(slave),
            master: PtyMaster(master),
        })
    }
}

/// The controlling side of a pseudo-terminal. Safe to share between threads.
#[derive(Debug)]
pub struct PtyMaster(native::Master);

impl PtyMaster {
    /// Resize the terminal; processes on it are notified as by a window resize.
    ///
    /// # Errors
    /// * [`io::Error`] when the terminal rejects the size.
    pub fn resize(&self, size: PtySize) -> io::Result<()> {
        self.0.resize(size)
    }

    /// The terminal's current size.
    ///
    /// # Errors
    /// * [`io::Error`] when the size cannot be read.
    pub fn size(&self) -> io::Result<PtySize> {
        self.0.size()
    }

    /// A new handle reading what processes write to the terminal. Reads block until output
    /// arrives, and return end of file once nothing holds the terminal open any more (on
    /// Windows, once the console is closed).
    ///
    /// # Errors
    /// * [`io::Error`] when the handle cannot be duplicated.
    pub fn try_clone_reader(&self) -> io::Result<PtyReader> {
        self.0.try_clone_reader().map(PtyReader)
    }

    /// The handle writing to the terminal as typing would. Available once.
    ///
    /// On Unix, dropping it types a newline and the terminal's end-of-file character (when one
    /// is set), so a shell reading the terminal sees its input end.
    ///
    /// # Errors
    /// * [`io::ErrorKind::AlreadyExists`] when the writer was already taken.
    /// * [`io::Error`] when the handle cannot be duplicated.
    pub fn take_writer(&self) -> io::Result<PtyInput> {
        self.0.take_writer().map(PtyInput)
    }

    /// Another handle writing to the terminal, which (unlike [`Self::take_writer`]) types
    /// nothing when dropped. Any number may be made.
    ///
    /// # Errors
    /// * [`io::Error`] when the handle cannot be duplicated.
    pub fn clone_writer(&self) -> io::Result<PtyInput> {
        self.0.clone_writer().map(PtyInput)
    }

    /// Make reads and writes on every handle of this terminal return
    /// [`io::ErrorKind::WouldBlock`] instead of waiting; wait with
    /// [`PtyReader::wait_readable`] and [`PtyInput::wait_writable`] instead.
    ///
    /// # Errors
    /// * [`io::Error`] when the mode cannot be changed.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }

    /// Whether anything still holds the terminal's slave side open: a process started on it,
    /// or something it started, or the [`PtySlave`] itself.
    #[must_use]
    // Constant only on some backends.
    #[allow(clippy::missing_const_for_fn)]
    pub fn has_peer(&self) -> bool {
        self.0.has_peer()
    }

    /// Path of the terminal's slave device, for handing the terminal to another process.
    #[cfg(unix)]
    #[must_use]
    pub fn tty_name(&self) -> Option<&std::path::Path> {
        self.0.tty_name()
    }

    /// The terminal's foreground process group, when it has one.
    #[cfg(unix)]
    #[must_use]
    pub fn foreground_process_group(&self) -> Option<i32> {
        self.0.foreground_process_group()
    }
}

#[cfg(all(unix, not(feature = "simulator")))]
impl std::os::fd::AsFd for PtyMaster {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

#[cfg(all(unix, not(feature = "simulator")))]
impl std::os::fd::AsRawFd for PtyMaster {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.0.as_fd().as_raw_fd()
    }
}

/// The side of a pseudo-terminal processes run on.
#[derive(Debug)]
pub struct PtySlave(native::Slave);

impl PtySlave {
    /// Start `command` with this terminal as its standard input, output and error.
    ///
    /// On Unix the process starts a new session with the terminal as its controlling terminal,
    /// from default signal handling and without descriptors this process left inheritable. On
    /// Windows it is attached to the pseudo-console.
    ///
    /// The program is looked up on the command's `PATH` when it names no directory. The working
    /// directory is the command's when that is a directory, and otherwise the home directory
    /// (on Windows, `USERPROFILE`, or this process's own when that is unset too).
    ///
    /// # Errors
    /// * [`io::ErrorKind::NotFound`] or [`io::ErrorKind::PermissionDenied`] when the program
    ///   cannot be found or run, with the places looked at in the message.
    /// * [`io::Error`] when the process cannot be started.
    pub fn spawn(&self, command: &SpawnCommand) -> io::Result<PtyChild> {
        self.0.spawn(command).map(PtyChild)
    }

    /// Start `command` as [`Self::spawn`] does, inside the caller's Job Object `job`.
    ///
    /// The process is assigned to the job as it is created, so it cannot run, or start anything,
    /// outside the job. The job's limits are the caller's: a kill-on-close job ends the process
    /// when the caller's last handle to it closes. Nested jobs are allowed.
    ///
    /// The command line differs from [`Self::spawn`] in one case: when the program is `cmd`,
    /// the argument after `/C` or `/K` is the script and is passed verbatim, wrapped as
    /// `/S /C "script"`, because `cmd` parses it as shell source rather than as C runtime
    /// arguments. Only options may precede `/C` or `/K`, and the script must be the last
    /// argument.
    ///
    /// With [`SpawnCommand::exact_environment`], the environment block holds exactly the
    /// command's variables. Their names are checked (one leading `=` is allowed, as in `cmd`'s
    /// `=C:`), and two names that Windows considers equal are an error. The block is ordered as
    /// Windows orders names.
    ///
    /// # Errors
    /// As [`Self::spawn`], and [`io::ErrorKind::InvalidInput`] for a malformed `cmd` script
    /// invocation or an invalid exact environment.
    #[cfg(all(windows, not(feature = "simulator")))]
    pub fn spawn_in_job(
        &self,
        command: &SpawnCommand,
        job: std::os::windows::io::BorrowedHandle<'_>,
    ) -> io::Result<PtyChild> {
        self.0.spawn_in_job(command, job).map(PtyChild)
    }
}

/// Reads output from a pseudo-terminal; see [`PtyMaster::try_clone_reader`].
#[derive(Debug)]
pub struct PtyReader(native::Reader);

impl io::Read for PtyReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl PtyReader {
    /// Wait at most `timeout` for output to read (or end of file); whether there is.
    ///
    /// # Errors
    /// * [`io::Error`] when the terminal cannot be waited on.
    pub fn wait_readable(&self, timeout: Duration) -> io::Result<bool> {
        self.0.wait_readable(timeout)
    }
}

/// Writes input to a pseudo-terminal; see [`PtyMaster::take_writer`].
#[derive(Debug)]
pub struct PtyInput(native::Input);

impl io::Write for PtyInput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl PtyInput {
    /// Wait at most `timeout` for room to write; whether there is.
    ///
    /// # Errors
    /// * [`io::Error`] when the terminal cannot be waited on.
    pub fn wait_writable(&self, timeout: Duration) -> io::Result<bool> {
        self.0.wait_writable(timeout)
    }
}

/// Reads a spawned process's piped standard output; see [`PtyChild::take_stdout`].
#[derive(Debug)]
pub struct PtyPipeReader(native::PipeReader);

impl io::Read for PtyPipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

// Constant only on some backends.
#[allow(clippy::missing_const_for_fn)]
impl PtyPipeReader {
    /// Make reads return [`io::ErrorKind::WouldBlock`] instead of waiting.
    ///
    /// # Errors
    /// * [`io::Error`] when the mode cannot be changed.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }

    /// Wait at most `timeout` for output to read (or end of file); whether there is.
    ///
    /// # Errors
    /// * [`io::Error`] when the pipe cannot be waited on.
    pub fn wait_readable(&self, timeout: Duration) -> io::Result<bool> {
        self.0.wait_readable(timeout)
    }
}

/// Writes a spawned process's piped standard input; see [`PtyChild::take_stdin`]. Dropping it
/// closes the pipe, so the process reads end of file.
#[derive(Debug)]
pub struct PtyPipeWriter(native::PipeWriter);

impl io::Write for PtyPipeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// A process started on a pseudo-terminal. Dropping it neither kills nor waits for the process.
#[derive(Debug)]
pub struct PtyChild(native::Child);

impl PtyChild {
    /// Operating system process id.
    #[must_use]
    // Constant only on some backends.
    #[allow(clippy::missing_const_for_fn)]
    pub fn id(&self) -> u32 {
        self.0.id()
    }

    /// How the process ended, if it has, without waiting.
    ///
    /// # Errors
    /// * [`io::Error`] when the process cannot be queried.
    pub fn try_wait(&mut self) -> io::Result<Option<ChildExit>> {
        self.0.try_wait()
    }

    /// Wait for the process to end without reaping it: it stays a zombie, so its process id,
    /// and the process group it leads, cannot be reused by another process until
    /// [`Self::wait`] (or [`Self::try_wait`]) reaps it. A caller that may still signal the
    /// process's group after it exits waits this way, so the signal cannot reach a newer
    /// process that was given the same id.
    ///
    /// # Errors
    /// * [`io::Error`] when the process cannot be waited for.
    pub fn wait_unreaped(&mut self) -> io::Result<ChildExit> {
        self.0.wait_unreaped()
    }

    /// Wait for the process to end.
    ///
    /// # Errors
    /// * [`io::Error`] when the process cannot be waited for.
    pub fn wait(&mut self) -> io::Result<ChildExit> {
        self.0.wait()
    }

    /// End the process. On Unix it is first sent `SIGHUP`, as when a terminal closes, and given
    /// a moment to exit before `SIGKILL`. On Windows it is terminated. Succeeds when the process
    /// has already exited.
    ///
    /// # Errors
    /// * [`io::Error`] when the process cannot be signalled.
    pub fn kill(&mut self) -> io::Result<()> {
        self.0.kill()
    }

    /// The pipe to the process's standard input, when [`SpawnCommand::stdin`] was
    /// [`Stdio::Piped`]. Available once.
    pub fn take_stdin(&mut self) -> Option<PtyPipeWriter> {
        self.0.take_stdin().map(PtyPipeWriter)
    }

    /// The pipe from the process's standard output, when [`SpawnCommand::stdout`] was
    /// [`Stdio::Piped`]. Available once.
    pub fn take_stdout(&mut self) -> Option<PtyPipeReader> {
        self.0.take_stdout().map(PtyPipeReader)
    }

    /// A handle that ends the process from elsewhere; see [`PtyChildKiller`].
    ///
    /// # Errors
    /// * [`io::Error`] when the process handle cannot be duplicated (Windows).
    pub fn killer(&self) -> io::Result<PtyChildKiller> {
        self.0.killer().map(PtyChildKiller)
    }
}

/// Adopt a process started some other way, such as a helper given the terminal's
/// [`PtyMaster::tty_name`], so it is waited for and killed as a spawned one is.
#[cfg(all(unix, not(feature = "simulator")))]
impl From<std::process::Child> for PtyChild {
    fn from(child: std::process::Child) -> Self {
        Self(native::Child::from(child))
    }
}

#[cfg(all(windows, not(feature = "simulator")))]
impl std::os::windows::io::AsRawHandle for PtyChild {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        self.0.as_raw_handle()
    }
}

/// Ends a [`PtyChild`] from any thread, without waiting for it. On Unix it sends `SIGHUP` to the
/// process; on Windows it terminates it.
#[derive(Debug, Clone)]
pub struct PtyChildKiller(native::Killer);

impl PtyChildKiller {
    /// Signal or terminate the process. Succeeds when the process has already exited.
    ///
    /// # Errors
    /// * [`io::Error`] when the process cannot be signalled.
    pub fn kill(&self) -> io::Result<()> {
        self.0.kill()
    }
}

/// How a process started with [`PtySlave::spawn`] ended.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChildExit {
    code: u32,
    signal: Option<String>,
    signal_number: Option<i32>,
}

impl ChildExit {
    /// An exit with `code`.
    #[must_use]
    pub const fn with_exit_code(code: u32) -> Self {
        Self {
            code,
            signal: None,
            signal_number: None,
        }
    }

    /// An exit by the signal named `signal`; the exit code is then 1.
    #[must_use]
    pub fn with_signal(signal: &str) -> Self {
        Self {
            code: 1,
            signal: Some(signal.to_owned()),
            signal_number: None,
        }
    }

    /// An exit by signal number `number`, described as `name`.
    #[cfg_attr(not(feature = "simulator"), allow(dead_code))]
    pub(crate) fn signaled(number: i32, name: &str) -> Self {
        Self {
            signal_number: Some(number),
            ..Self::with_signal(name)
        }
    }

    /// Whether the process exited with code zero and no signal.
    #[must_use]
    pub const fn success(&self) -> bool {
        self.signal.is_none() && self.code == 0
    }

    /// The exit code; 1 when a signal ended the process. On Windows, the full 32-bit code.
    #[must_use]
    pub const fn exit_code(&self) -> u32 {
        self.code
    }

    /// The system's description of the signal that ended the process, such as `Killed`.
    #[must_use]
    pub fn signal(&self) -> Option<&str> {
        self.signal.as_deref()
    }

    /// The number of the signal that ended the process, when it is known (Unix).
    #[must_use]
    pub const fn signal_number(&self) -> Option<i32> {
        self.signal_number
    }
}

impl fmt::Display for ChildExit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.signal {
            Some(signal) => write!(formatter, "Terminated by {signal}"),
            None if self.code == 0 => formatter.write_str("Success"),
            None => write!(formatter, "Exited with code {}", self.code),
        }
    }
}

impl From<std::process::ExitStatus> for ChildExit {
    fn from(status: std::process::ExitStatus) -> Self {
        #[cfg(unix)]
        if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
            return Self {
                signal_number: Some(signal),
                ..Self::with_signal(&native::signal_name(signal))
            };
        }
        let code = status.code().map_or_else(
            || u32::from(!status.success()),
            |code| u32::from_ne_bytes(code.to_ne_bytes()),
        );
        Self::with_exit_code(code)
    }
}

/// One environment variable: its name as last given, and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Variable {
    name: OsString,
    value: OsString,
}

/// A program, its arguments, environment and working directory, to start with
/// [`PtySlave::spawn`].
///
/// The environment starts as this process's, captured by [`SpawnCommand::new`]. On Unix,
/// `SHELL` is added from the user database when unset. On Windows, the system and user
/// environment registered for new sessions is applied over it (as Explorer does for new
/// programs), with the user's `PATH` appended to the system's. Names are compared without case on
/// Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnCommand {
    argv: Vec<OsString>,
    env: BTreeMap<OsString, Variable>,
    cwd: Option<OsString>,
    exact: bool,
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
    controlling: bool,
}

impl SpawnCommand {
    /// A command running `program`, with the current environment.
    #[must_use]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        let mut env = BTreeMap::new();
        for (name, value) in native::base_environment() {
            env.insert(env_key(&name), Variable { name, value });
        }
        Self {
            argv: vec![program.as_ref().to_owned()],
            env,
            cwd: None,
            exact: false,
            stdin: Stdio::Terminal,
            stdout: Stdio::Terminal,
            stderr: Stdio::Terminal,
            controlling: true,
        }
    }

    /// Where the process's standard input comes from; the terminal by default.
    pub const fn stdin(&mut self, stdio: Stdio) -> &mut Self {
        self.stdin = stdio;
        self
    }

    /// Where the process's standard output goes; the terminal by default.
    pub const fn stdout(&mut self, stdio: Stdio) -> &mut Self {
        self.stdout = stdio;
        self
    }

    /// Where the process's standard error goes; the terminal by default.
    pub const fn stderr(&mut self, stdio: Stdio) -> &mut Self {
        self.stderr = stdio;
        self
    }

    /// Whether the terminal becomes the process's controlling terminal (Unix), so it can read
    /// `/dev/tty`, receives the terminal's signals and is hung up on when it closes. On by
    /// default.
    pub const fn controlling_terminal(&mut self, controlling: bool) -> &mut Self {
        self.controlling = controlling;
        self
    }

    /// Where each standard stream goes: input, output, error.
    #[must_use]
    pub const fn stdio(&self) -> (Stdio, Stdio, Stdio) {
        (self.stdin, self.stdout, self.stderr)
    }

    /// Whether the terminal becomes the process's controlling terminal.
    #[must_use]
    pub const fn is_controlling_terminal(&self) -> bool {
        self.controlling
    }

    /// Add an argument.
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.argv.push(arg.as_ref().to_owned());
        self
    }

    /// Add arguments.
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.argv
            .extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    /// Set an environment variable.
    pub fn env(&mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        let name = name.as_ref().to_owned();
        self.env.insert(
            env_key(&name),
            Variable {
                name,
                value: value.as_ref().to_owned(),
            },
        );
        self
    }

    /// Remove an environment variable.
    pub fn env_remove(&mut self, name: impl AsRef<OsStr>) -> &mut Self {
        self.env.remove(&env_key(name.as_ref()));
        self
    }

    /// Remove every environment variable.
    pub fn env_clear(&mut self) -> &mut Self {
        self.env.clear();
        self
    }

    /// Give the process exactly the environment set on this command: nothing is added to it
    /// (on Unix, `SHELL` is otherwise added when unset).
    pub const fn exact_environment(&mut self) -> &mut Self {
        self.exact = true;
        self
    }

    /// Whether [`Self::exact_environment`] was requested.
    #[must_use]
    pub const fn is_exact_environment(&self) -> bool {
        self.exact
    }

    /// Run in `dir`. A directory that does not exist when the process starts is replaced by the
    /// home directory.
    pub fn cwd(&mut self, dir: impl AsRef<OsStr>) -> &mut Self {
        self.cwd = Some(dir.as_ref().to_owned());
        self
    }

    /// The program followed by its arguments.
    #[must_use]
    pub fn argv(&self) -> &[OsString] {
        &self.argv
    }

    /// The value the process will see for environment variable `name`.
    #[must_use]
    pub fn get_env(&self, name: impl AsRef<OsStr>) -> Option<&OsStr> {
        self.env
            .get(&env_key(name.as_ref()))
            .map(|variable| variable.value.as_os_str())
    }

    /// The working directory set with [`Self::cwd`].
    #[must_use]
    pub fn get_cwd(&self) -> Option<&OsStr> {
        self.cwd.as_deref()
    }

    /// Every environment variable the process will see, ordered by name.
    pub fn environment(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.env
            .values()
            .map(|variable| (variable.name.as_os_str(), variable.value.as_os_str()))
    }

    /// The environment variables whose name and value are both valid UTF-8, ordered by name.
    pub fn utf8_environment(&self) -> impl Iterator<Item = (&str, &str)> {
        self.environment()
            .filter_map(|(name, value)| Some((name.to_str()?, value.to_str()?)))
    }
}

/// The key a variable is stored under: its name, without case on Windows.
fn env_key(name: &OsStr) -> OsString {
    if cfg!(windows) {
        name.to_str()
            .map_or_else(|| name.to_owned(), |name| name.to_lowercase().into())
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "simulator")]
    #[test]
    fn simulated_environment_is_fixed() {
        let command = SpawnCommand::new("program");
        assert_eq!(command.get_env("HOME"), Some(OsStr::new("/home/sim")));
        assert_eq!(command.get_env("SHELL"), Some(OsStr::new("/bin/sh")));
    }

    #[cfg(not(feature = "simulator"))]
    #[test]
    fn environment_starts_from_this_process_and_reads_back_changes() {
        let mut command = SpawnCommand::new("program");
        let (name, value) = std::env::vars_os()
            .next()
            .expect("the test process has an environment");
        assert!(command.get_env(&name).is_some());
        if cfg!(unix) {
            assert_eq!(command.get_env(&name), Some(value.as_os_str()));
        }
        command
            .env("SWITCHY_PTY_PAIR_TEST", "set")
            .arg("one")
            .args(["two"]);
        assert_eq!(
            command.get_env("SWITCHY_PTY_PAIR_TEST"),
            Some(OsStr::new("set"))
        );
        assert!(
            command
                .utf8_environment()
                .any(|entry| entry == ("SWITCHY_PTY_PAIR_TEST", "set"))
        );
        command.env_remove("SWITCHY_PTY_PAIR_TEST");
        assert_eq!(command.get_env("SWITCHY_PTY_PAIR_TEST"), None);
        assert_eq!(command.argv(), ["program", "one", "two"]);
        assert_eq!(command.get_cwd(), None);
        command.cwd("/somewhere");
        assert_eq!(command.get_cwd(), Some(OsStr::new("/somewhere")));
        command.env_clear();
        assert_eq!(command.environment().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn names_are_compared_without_case_on_windows() {
        let mut command = SpawnCommand::new("program");
        command.env("Switchy_Pty_Case", "value");
        assert_eq!(
            command.get_env("SWITCHY_PTY_CASE"),
            Some(OsStr::new("value"))
        );
        assert!(
            command
                .utf8_environment()
                .any(|entry| entry == ("Switchy_Pty_Case", "value"))
        );
    }

    #[test]
    fn exits_describe_codes_and_signals() {
        let exit = ChildExit::with_exit_code(0);
        assert!(exit.success());
        assert_eq!(exit.to_string(), "Success");
        let exit = ChildExit::with_exit_code(3);
        assert!(!exit.success());
        assert_eq!(exit.exit_code(), 3);
        assert_eq!(exit.to_string(), "Exited with code 3");
        let exit = ChildExit::with_signal("Hangup");
        assert!(!exit.success());
        assert_eq!(exit.exit_code(), 1);
        assert_eq!(exit.signal(), Some("Hangup"));
        assert_eq!(exit.to_string(), "Terminated by Hangup");
    }
}
