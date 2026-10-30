//! Native execution for trusted worker commands, not a security sandbox.
//!
//! Only explicit `bash`, `sh`, `pwsh`, and `cmd` shells are accepted; shell
//! arguments belong in `run`, not `shell`. No profiles are loaded. Each step gets
//! a fresh environment: on Unix the worker's PATH is inherited (including Nix
//! tool locations); on Windows SystemRoot, WINDIR, PATH, PATHEXT, TEMP and TMP
//! are inherited. Explicit step environment entries override these defaults
//! (and can therefore deliberately enable shell startup hooks). No HOME, credentials, or other
//! worker environment variables are implicitly forwarded.
//!
//! Unix children run in a new process group; Windows children start suspended,
//! join a kill-on-close Job Object, then resume. Descendants are terminated even
//! when the shell exits first. Unix commands that deliberately escape their
//! process group are outside this trusted-worker contract. Pipe polling never
//! blocks on such escaped descendants: final draining is limited to 250 ms.
//! Output memory is bounded per step/stream, not by total execution length.
//! This module performs no source checkout, containerization, or remote calls.

use std::{
    collections::VecDeque,
    io::{self, Read},
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::plan::CommandStep;

/// Maximum UTF-8 byte length of each returned stdout/stderr tail.
pub const LOG_TAIL_BYTES: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(250);

/// Terminal state of a command or execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionOutcome {
    /// All commands exited with status zero.
    Succeeded,
    /// A command exited nonzero or was terminated by a signal.
    Failed,
    /// The current command exceeded its deadline.
    TimedOut,
    /// Cancellation was observed before or during a command.
    Cancelled,
}

/// Bounded output from one stream. Invalid UTF-8 is replaced lossily.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogTail {
    /// Last at most `LOG_TAIL_BYTES` UTF-8 bytes.
    pub text: String,
    /// Whether earlier bytes were discarded, including during UTF-8 conversion.
    pub truncated: bool,
    /// Whether EOF was reached; false if a descendant kept the pipe open.
    pub complete: bool,
}

/// Result of one attempted step; later steps are omitted after early failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepResult {
    /// Zero-based position in the input plan.
    pub step_index: usize,
    /// True only for a normal zero exit.
    pub success: bool,
    /// Distinguishes nonzero exit, timeout and cancellation.
    pub outcome: ExecutionOutcome,
    /// Shell exit code; absent before launch or after Unix signal termination.
    pub exit_code: Option<i32>,
    /// Bounded stdout tail.
    pub stdout: LogTail,
    /// Bounded stderr tail.
    pub stderr: LogTail,
}

/// Aggregate result, including only attempted steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// True only if all requested steps succeeded (including an empty plan).
    pub success: bool,
    /// Overall terminal outcome.
    pub outcome: ExecutionOutcome,
    /// Last attempted shell's exit code, if available.
    pub exit_code: Option<i32>,
    /// Results in input order.
    pub steps: Vec<StepResult>,
}

/// Run commands sequentially in an existing local directory, stopping on failure.
///
/// A zero timeout is an immediate deadline: the step is not launched. A
/// pre-cancelled step is likewise recorded without launching. Cancellation wins
/// over timeout when both are observed on the same poll. Polling granularity is
/// 10 ms; spawn and OS process termination latency are additional. Successful
/// shell exit terminates remaining background descendants before final draining.
///
/// # Errors
///
/// * The directory is missing/not a directory, the shell is unsupported, or an
///   environment entry contains invalid NUL/`=` characters.
/// * A process or pipe cannot be created/configured, monitored, read or reaped.
/// * Process-tree termination fails. Best-effort cleanup also runs on errors.
///
/// Nonzero exits, timeout and cancellation are ordinary `Ok` results. An
/// infrastructure error returns `Err` rather than a partial execution result.
/// Unix integration requires the workspace `libc` dependency; Windows uses
/// system Job Object APIs and `NtResumeProcess` from ntdll, with no extra crate.
pub fn execute(
    steps: &[CommandStep],
    directory: &Path,
    cancelled: &AtomicBool,
) -> Result<ExecutionResult, String> {
    if !directory.is_dir() {
        return Err(format!(
            "execution directory is not a directory: {}",
            directory.display()
        ));
    }
    // Validate the whole plan before performing any side effects.
    for step in steps {
        let _ = shell_command(&step.shell)?;
        if step.run.contains('\0')
            || step.env.iter().any(|(key, value)| {
                key.is_empty() || key.contains(['\0', '=']) || value.contains('\0')
            })
        {
            return Err("invalid NUL in command or invalid environment entry".to_owned());
        }
    }
    let mut result = ExecutionResult {
        success: true,
        outcome: ExecutionOutcome::Succeeded,
        exit_code: None,
        steps: Vec::new(),
    };
    for (step_index, step) in steps.iter().enumerate() {
        let step_result = execute_step(step, step_index, directory, cancelled)?;
        result.success = step_result.success;
        result.outcome = step_result.outcome;
        result.exit_code = step_result.exit_code;
        result.steps.push(step_result);
        if !result.success {
            break;
        }
    }
    // Cancellation of an empty plan is observable as well.
    if steps.is_empty() && cancelled.load(Ordering::Acquire) {
        result.success = false;
        result.outcome = ExecutionOutcome::Cancelled;
    }
    Ok(result)
}

fn shell_command(shell: &str) -> Result<Command, String> {
    let mut command = match shell {
        "bash" => {
            let mut command = Command::new("bash");
            command.args(["--noprofile", "--norc", "-c"]);
            command
        }
        "sh" => {
            let mut command = Command::new("sh");
            command.arg("-c");
            command
        }
        "pwsh" => {
            let mut command = Command::new("pwsh");
            command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"]);
            command
        }
        #[cfg(windows)]
        "cmd" => {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/S", "/C"]);
            command
        }
        _ => return Err(format!("unsupported native shell: {shell:?}")),
    };
    command.env_clear();
    #[cfg(unix)]
    command.env(
        "PATH",
        std::env::var_os("PATH").unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into()),
    );
    #[cfg(windows)]
    for name in ["SystemRoot", "WINDIR", "PATH", "PATHEXT", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    Ok(command)
}

fn execute_step(
    step: &CommandStep,
    step_index: usize,
    directory: &Path,
    cancelled: &AtomicBool,
) -> Result<StepResult, String> {
    let mut stdout_tail = Tail::default();
    let mut stderr_tail = Tail::default();
    let initial_outcome = if cancelled.load(Ordering::Acquire) {
        Some(ExecutionOutcome::Cancelled)
    } else if step.timeout_secs == 0 {
        Some(ExecutionOutcome::TimedOut)
    } else {
        None
    };
    if let Some(outcome) = initial_outcome {
        return Ok(StepResult {
            step_index,
            success: false,
            outcome,
            exit_code: None,
            stdout: stdout_tail.finish(true),
            stderr: stderr_tail.finish(true),
        });
    }
    let mut command = shell_command(&step.shell)?;
    command
        .arg(&step.run)
        .envs(&step.env)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    platform::prepare(&mut command);
    let started = Instant::now();
    let child = command
        .spawn()
        .map_err(|error| format!("spawn {}: {error}", step.shell))?;
    let mut process = Process::new(child)?;
    let mut stdout = process
        .child
        .stdout
        .take()
        .ok_or("missing child stdout pipe")?;
    let mut stderr = process
        .child
        .stderr
        .take()
        .ok_or("missing child stderr pipe")?;
    platform::nonblocking(&stdout).map_err(|error| format!("configure stdout: {error}"))?;
    platform::nonblocking(&stderr).map_err(|error| format!("configure stderr: {error}"))?;
    let mut stdout_eof = false;
    let mut stderr_eof = false;
    let (outcome, status) = loop {
        // Bounded work on each stream prevents a continuous writer from starving
        // stderr, cancellation checks, or deadline checks.
        drain(&mut stdout, &mut stdout_tail, &mut stdout_eof)?;
        drain(&mut stderr, &mut stderr_tail, &mut stderr_eof)?;
        if cancelled.load(Ordering::Acquire) {
            break (ExecutionOutcome::Cancelled, None);
        }
        if started.elapsed() >= Duration::from_secs(step.timeout_secs) {
            break (ExecutionOutcome::TimedOut, None);
        }
        if let Some(status) = process
            .child
            .try_wait()
            .map_err(|error| format!("poll child: {error}"))?
        {
            let outcome = if status.success() {
                ExecutionOutcome::Succeeded
            } else {
                ExecutionOutcome::Failed
            };
            break (outcome, Some(status));
        }
        thread::sleep(POLL_INTERVAL);
    };
    // Kill descendants on EVERY exit, including successful shells that left
    // background jobs behind. Guard remains armed until all cleanup succeeds.
    process.terminate()?;
    let status = match status {
        Some(status) => status,
        None => process
            .child
            .wait()
            .map_err(|error| format!("reap child: {error}"))?,
    };
    process.terminated = true;
    let drain_started = Instant::now();
    while !(stdout_eof && stderr_eof) && drain_started.elapsed() < DRAIN_TIMEOUT {
        drain(&mut stdout, &mut stdout_tail, &mut stdout_eof)?;
        drain(&mut stderr, &mut stderr_tail, &mut stderr_eof)?;
        if !(stdout_eof && stderr_eof) {
            thread::sleep(POLL_INTERVAL);
        }
    }
    Ok(StepResult {
        step_index,
        success: outcome == ExecutionOutcome::Succeeded,
        outcome,
        exit_code: status.code(),
        stdout: stdout_tail.finish(stdout_eof),
        stderr: stderr_tail.finish(stderr_eof),
    })
}

#[derive(Default)]
struct Tail {
    bytes: VecDeque<u8>,
    truncated: bool,
}

impl Tail {
    fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.bytes.len() == LOG_TAIL_BYTES {
                self.bytes.pop_front();
                self.truncated = true;
            }
            self.bytes.push_back(byte);
        }
    }

    fn finish(mut self, complete: bool) -> LogTail {
        let mut text = String::from_utf8_lossy(self.bytes.make_contiguous()).into_owned();
        // Replacement characters can expand invalid bytes by a factor of three.
        if text.len() > LOG_TAIL_BYTES {
            let mut start = text.len() - LOG_TAIL_BYTES;
            while !text.is_char_boundary(start) {
                start += 1;
            }
            text = text[start..].to_owned();
            self.truncated = true;
        }
        LogTail {
            text,
            truncated: self.truncated,
            complete,
        }
    }
}

fn drain<R: Read + platform::Pipe>(
    pipe: &mut R,
    tail: &mut Tail,
    eof: &mut bool,
) -> Result<(), String> {
    if *eof {
        return Ok(());
    }
    let mut buffer = [0_u8; 8192];
    for _ in 0..8 {
        if !platform::readable(pipe).map_err(|error| format!("poll output: {error}"))? {
            break;
        }
        match pipe.read(&mut buffer) {
            Ok(0) => {
                *eof = true;
                break;
            }
            Ok(count) => tail.push(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("read child output: {error}")),
        }
    }
    Ok(())
}

struct Process {
    child: Child,
    tree: platform::Tree,
    terminated: bool,
}

impl Process {
    fn new(mut child: Child) -> Result<Self, String> {
        match platform::Tree::attach(&child) {
            Ok(tree) => Ok(Self {
                child,
                tree,
                terminated: false,
            }),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(format!("establish process tree: {error}"))
            }
        }
    }

    fn terminate(&self) -> Result<(), String> {
        self.tree
            .kill(&self.child)
            .map_err(|error| format!("terminate process tree: {error}"))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if !self.terminated {
            let _ = self.tree.kill(&self.child);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::{Child, Command, io};
    use std::os::{fd::AsRawFd, unix::process::CommandExt};

    pub trait Pipe: AsRawFd {}
    impl<T: AsRawFd> Pipe for T {}

    pub fn prepare(command: &mut Command) {
        command.process_group(0);
    }

    pub fn nonblocking(pipe: &impl Pipe) -> io::Result<()> {
        // SAFETY: pipe owns a valid descriptor; fcntl does not retain it.
        let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
        if flags == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: valid descriptor and flags from F_GETFL.
        if unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    #[allow(clippy::unnecessary_wraps)]
    pub const fn readable(_: &impl Pipe) -> io::Result<bool> {
        Ok(true)
    }

    pub struct Tree(libc::pid_t);

    impl Tree {
        pub fn attach(child: &Child) -> io::Result<Self> {
            let pid = libc::pid_t::try_from(child.id()).map_err(io::Error::other)?;
            Ok(Self(pid))
        }

        pub fn kill(&self, _: &Child) -> io::Result<()> {
            // SAFETY: the positive child PID is its dedicated process group ID;
            // negative PID targets only that group, never the worker's group.
            if unsafe { libc::kill(-self.0, libc::SIGKILL) } == -1 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
            Ok(())
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::c_void,
        mem::size_of,
        os::windows::{io::AsRawHandle, process::CommandExt},
        ptr,
    };

    type Handle = *mut c_void;
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimit {
        process_time: i64,
        job_time: i64,
        flags: u32,
        minimum_working_set: usize,
        maximum_working_set: usize,
        active_process_limit: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io_counters: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: i32,
            information: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn PeekNamedPipe(
            pipe: Handle,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            remaining: *mut u32,
        ) -> i32;
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtResumeProcess(process: Handle) -> i32;
    }

    pub trait Pipe: AsRawHandle {}
    impl<T: AsRawHandle> Pipe for T {}

    pub fn prepare(command: &mut Command) {
        command.creation_flags(CREATE_SUSPENDED);
    }
    #[allow(clippy::unnecessary_wraps)]
    pub const fn nonblocking(_: &impl Pipe) -> io::Result<()> {
        Ok(())
    }

    pub fn readable(pipe: &impl Pipe) -> io::Result<bool> {
        let mut available = 0;
        // SAFETY: valid owned pipe; only available points to writable storage.
        if unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                &mut available,
                ptr::null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(109) {
                return Ok(true);
            } // EOF (ERROR_BROKEN_PIPE)
            return Err(error);
        }
        Ok(available > 0)
    }

    pub struct Tree(Handle);
    impl Tree {
        pub fn attach(child: &Child) -> io::Result<Self> {
            // SAFETY: unnamed job with default security; handle is owned below.
            let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let tree = Self(handle);
            let mut limits = ExtendedLimit::default();
            limits.basic.flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let length = u32::try_from(size_of::<ExtendedLimit>()).map_err(io::Error::other)?;
            // SAFETY: structures have Windows ABI layout, pointers/length valid.
            if unsafe { SetInformationJobObject(handle, 9, ptr::from_ref(&limits).cast(), length) }
                == 0
            {
                return Err(io::Error::last_os_error());
            }
            // Child is suspended, so no descendants can escape job assignment.
            // SAFETY: child and job handles remain valid throughout these calls.
            if unsafe { AssignProcessToJobObject(handle, child.as_raw_handle()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: this child was created with CREATE_SUSPENDED above.
            let status = unsafe { NtResumeProcess(child.as_raw_handle()) };
            if status < 0 {
                return Err(io::Error::other(format!(
                    "NtResumeProcess failed: {status:#x}"
                )));
            }
            Ok(tree)
        }
        pub fn kill(&self, _: &Child) -> io::Result<()> {
            // SAFETY: self owns this valid job handle.
            if unsafe { TerminateJobObject(self.0, 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            // SAFETY: exactly one owned job handle, closed once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
compile_error!("native gpipe execution requires Unix or Windows process-tree support");

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn step(run: &str) -> CommandStep {
        CommandStep {
            shell: if cfg!(windows) { "cmd" } else { "sh" }.to_owned(),
            run: run.to_owned(),
            env: BTreeMap::new(),
            timeout_secs: 5,
        }
    }

    fn run(steps: &[CommandStep]) -> ExecutionResult {
        execute(
            steps,
            &std::env::current_dir().unwrap(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    #[test]
    fn success_and_sequential_failure() {
        let result = run(&[step("echo first"), step("exit 7"), step("echo never")]);
        assert!(!result.success);
        assert_eq!(result.outcome, ExecutionOutcome::Failed);
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(result.steps.len(), 2);
        assert!(result.steps[0].success);
        assert!(result.steps[0].stdout.text.contains("first"));
        assert!(result.steps[0].stdout.complete);
    }

    #[test]
    fn successful_execution_captures_both_streams() {
        let result = run(&[step(if cfg!(windows) {
            "echo stdout & echo stderr 1>&2"
        } else {
            "printf stdout; printf stderr >&2"
        })]);
        assert!(result.success);
        assert_eq!(result.outcome, ExecutionOutcome::Succeeded);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.steps[0].stdout.text.trim(), "stdout");
        assert_eq!(result.steps[0].stderr.text.trim(), "stderr");
    }

    #[test]
    fn empty_plan_and_zero_deadline() {
        assert!(run(&[]).success);
        let mut command = step("echo must-not-run");
        command.timeout_secs = 0;
        let result = run(&[command]);
        assert_eq!(result.outcome, ExecutionOutcome::TimedOut);
        assert_eq!(result.exit_code, None);
        assert_eq!(result.steps[0].stdout.text, "");
    }

    #[test]
    fn precancelled_does_not_spawn() {
        let result = execute(
            &[step("echo must-not-run")],
            &std::env::current_dir().unwrap(),
            &AtomicBool::new(true),
        )
        .unwrap();
        assert_eq!(result.outcome, ExecutionOutcome::Cancelled);
        assert_eq!(result.exit_code, None);
        assert_eq!(result.steps[0].stdout.text, "");
    }

    #[test]
    fn tail_is_bounded_including_invalid_utf8() {
        let mut tail = Tail::default();
        tail.push(&vec![b'a'; LOG_TAIL_BYTES + 17]);
        tail.push(b"last");
        let output = tail.finish(true);
        assert!(output.truncated);
        assert_eq!(output.text.len(), LOG_TAIL_BYTES);
        assert!(output.text.ends_with("last"));
        let mut tail = Tail::default();
        tail.push(&vec![0xff; LOG_TAIL_BYTES]);
        let output = tail.finish(true);
        assert!(output.truncated);
        assert!(output.text.len() <= LOG_TAIL_BYTES);
    }

    #[test]
    fn rejects_shell_arguments_and_invalid_environment() {
        let mut command = step("echo hi");
        command.shell = "sh -c".to_owned();
        assert!(execute(&[command], Path::new("."), &AtomicBool::new(false)).is_err());
        let mut command = step("echo hi");
        command.env.insert("bad=key".to_owned(), "value".to_owned());
        assert!(execute(&[command], Path::new("."), &AtomicBool::new(false)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn environment_is_explicit_and_stdin_is_closed() {
        let mut command = step(
            "printf '%s:%s:%s' \"$GPIPE_TEST_VALUE\" \"${HOME-unset}\" \"${BASH_ENV-unset}\"; read value; exit 0",
        );
        command
            .env
            .insert("GPIPE_TEST_VALUE".to_owned(), "explicit".to_owned());
        let result = run(&[command]);
        assert!(result.success);
        assert_eq!(result.steps[0].stdout.text, "explicit:unset:unset");
    }

    #[cfg(unix)]
    #[test]
    fn floods_both_pipes_without_deadlocking() {
        let result = run(&[step(
            "i=0; while [ $i -lt 12000 ]; do printf 'stdout-0123456789\n'; printf 'stderr-0123456789\n' >&2; i=$((i+1)); done; printf last-out; printf last-err >&2",
        )]);
        assert!(result.success);
        let output = &result.steps[0];
        assert!(output.stdout.truncated && output.stderr.truncated);
        assert!(output.stdout.complete && output.stderr.complete);
        assert!(output.stdout.text.len() <= LOG_TAIL_BYTES);
        assert!(output.stderr.text.len() <= LOG_TAIL_BYTES);
        assert!(output.stdout.text.ends_with("last-out"));
        assert!(output.stderr.text.ends_with("last-err"));
    }

    fn long_command() -> CommandStep {
        step(if cfg!(windows) {
            "ping -n 30 127.0.0.1 >nul"
        } else {
            "sleep 30 & wait"
        })
    }

    #[test]
    fn timeout_kills_descendants_holding_pipes() {
        let mut command = long_command();
        command.timeout_secs = 1;
        let start = Instant::now();
        let result = run(&[command, step("echo never")]);
        assert_eq!(result.outcome, ExecutionOutcome::TimedOut);
        assert_eq!(result.steps.len(), 1);
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(result.steps[0].stdout.complete && result.steps[0].stderr.complete);
    }

    #[test]
    fn cancellation_interrupts_running_process_tree() {
        let cancelled = AtomicBool::new(false);
        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(150));
                cancelled.store(true, Ordering::Release);
            });
            let start = Instant::now();
            let result = execute(&[long_command()], Path::new("."), &cancelled).unwrap();
            assert_eq!(result.outcome, ExecutionOutcome::Cancelled);
            assert!(start.elapsed() < Duration::from_secs(5));
            assert!(result.steps[0].stdout.complete && result.steps[0].stderr.complete);
        });
    }

    #[cfg(unix)]
    #[test]
    fn successful_shell_cleans_up_background_processes() {
        let start = Instant::now();
        let result = run(&[step("sleep 30 & echo done")]);
        assert!(result.success);
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(result.steps[0].stdout.complete && result.steps[0].stderr.complete);
    }
}
