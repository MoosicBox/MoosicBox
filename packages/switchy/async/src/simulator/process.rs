//! Simulated process spawning and management.
//!
//! Provides fully deterministic mock implementations for testing.
//! No real processes are spawned - all behavior is controlled via the [`ProcessRegistry`].
//!
//! # Example
//!
//! ```ignore
//! use switchy_async::process::{Command, MockResponse, ProcessRegistry, set_registry};
//!
//! // Set up mock responses
//! let registry = ProcessRegistry::new();
//! registry.register(
//!     MockResponse::success()
//!         .for_program("rustfmt")
//!         .with_stdout(b"Formatted successfully")
//! );
//! set_registry(registry);
//!
//! // Now Command will return the mocked response
//! let output = Command::new("rustfmt").output().await.unwrap();
//! assert!(output.status.success());
//! ```

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::{self, Cursor};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

/// Simulated exit status.
///
/// Unlike `std::process::ExitStatus`, this is fully controlled by tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus {
    code: Option<i32>,
}

impl ExitStatus {
    /// Creates a successful exit status (code 0).
    #[must_use]
    pub const fn from_success() -> Self {
        Self { code: Some(0) }
    }

    /// Creates an exit status with the given code.
    #[must_use]
    pub const fn from_code(code: i32) -> Self {
        Self { code: Some(code) }
    }

    /// Creates an exit status representing termination by signal (no code).
    #[must_use]
    pub const fn from_signal() -> Self {
        Self { code: None }
    }

    /// Returns true if the process exited successfully (code 0).
    #[must_use]
    pub const fn success(&self) -> bool {
        matches!(self.code, Some(0))
    }

    /// Returns the exit code, if any.
    #[must_use]
    pub const fn code(&self) -> Option<i32> {
        self.code
    }
}

impl Default for ExitStatus {
    fn default() -> Self {
        Self::from_success()
    }
}

/// Simulated process output.
///
/// Contains the exit status and captured stdout/stderr.
#[derive(Debug, Clone, Default)]
pub struct Output {
    /// The exit status of the process.
    pub status: ExitStatus,
    /// The data that the process wrote to stdout.
    pub stdout: Vec<u8>,
    /// The data that the process wrote to stderr.
    pub stderr: Vec<u8>,
}

/// Configuration for standard I/O streams.
///
/// Mirrors `std::process::Stdio` but is fully simulated.
#[derive(Debug, Clone, Copy, Default)]
pub enum Stdio {
    /// Inherit the parent's stdio.
    #[default]
    Inherit,
    /// Capture the stdio as a pipe.
    Piped,
    /// Discard the stdio (like /dev/null).
    Null,
}

impl From<std::process::Stdio> for Stdio {
    fn from(stdio: std::process::Stdio) -> Self {
        // We can't inspect std::process::Stdio, so default to Inherit
        // In practice, tests should use our Stdio directly
        let _ = stdio;
        Self::Inherit
    }
}

/// A mock response for a simulated command.
///
/// Use the builder methods to configure the response.
#[derive(Debug, Clone)]
pub struct MockResponse {
    /// Program name matcher (None = match any)
    pub program: Option<String>,
    /// Arguments matcher (None = match any)
    pub args: Option<Vec<String>>,
    /// Simulated stdout
    pub stdout: Vec<u8>,
    /// Simulated stderr
    pub stderr: Vec<u8>,
    /// Simulated exit code
    pub exit_code: i32,
    /// Optional delay before returning (for timing tests)
    #[cfg(feature = "time")]
    pub delay: Option<std::time::Duration>,
    /// If true, simulate spawn failure instead of returning output
    pub fail_to_spawn: bool,
    /// Error message if `fail_to_spawn` is true
    pub spawn_error: Option<String>,
}

impl Default for MockResponse {
    fn default() -> Self {
        Self::success()
    }
}

impl MockResponse {
    /// Creates a successful response with no output.
    #[must_use]
    pub const fn success() -> Self {
        Self {
            program: None,
            args: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: 0,
            #[cfg(feature = "time")]
            delay: None,
            fail_to_spawn: false,
            spawn_error: None,
        }
    }

    /// Creates a failed response with the given exit code.
    #[must_use]
    pub const fn failure(exit_code: i32) -> Self {
        Self {
            program: None,
            args: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code,
            #[cfg(feature = "time")]
            delay: None,
            fail_to_spawn: false,
            spawn_error: None,
        }
    }

    /// Sets the program name to match.
    #[must_use]
    pub fn for_program(mut self, program: impl Into<String>) -> Self {
        self.program = Some(program.into());
        self
    }

    /// Sets the arguments to match.
    #[must_use]
    pub fn for_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = Some(args.into_iter().map(Into::into).collect());
        self
    }

    /// Sets stdout content.
    #[must_use]
    pub fn with_stdout(mut self, stdout: impl Into<Vec<u8>>) -> Self {
        self.stdout = stdout.into();
        self
    }

    /// Sets stderr content.
    #[must_use]
    pub fn with_stderr(mut self, stderr: impl Into<Vec<u8>>) -> Self {
        self.stderr = stderr.into();
        self
    }

    /// Sets the exit code.
    #[must_use]
    pub const fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    /// Sets a simulated delay before the command completes.
    ///
    /// Requires the `time` feature.
    #[cfg(feature = "time")]
    #[must_use]
    pub const fn with_delay(mut self, delay: std::time::Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// Makes the command fail to spawn with the given error message.
    #[must_use]
    pub fn fail_spawn(mut self, message: impl Into<String>) -> Self {
        self.fail_to_spawn = true;
        self.spawn_error = Some(message.into());
        self
    }

    /// Checks if this response matches the given program and args.
    fn matches(&self, program: &str, args: &[String]) -> bool {
        if let Some(ref expected_program) = self.program
            && expected_program != program
        {
            return false;
        }
        if let Some(ref expected_args) = self.args
            && expected_args != args
        {
            return false;
        }
        true
    }
}

/// Registry for mock process responses.
///
/// Tests register expected responses, and [`Command`] consumes them in order.
#[derive(Debug, Default, Clone)]
pub struct ProcessRegistry {
    responses: Arc<Mutex<VecDeque<MockResponse>>>,
    default_response: Arc<Mutex<Option<MockResponse>>>,
}

impl ProcessRegistry {
    /// Creates a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a mock response (FIFO order).
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn register(&self, response: MockResponse) {
        self.responses.lock().unwrap().push_back(response);
    }

    /// Registers multiple mock responses.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn register_all<I>(&self, responses: I)
    where
        I: IntoIterator<Item = MockResponse>,
    {
        let mut queue = self.responses.lock().unwrap();
        for response in responses {
            queue.push_back(response);
        }
    }

    /// Sets a default response for unmatched commands.
    ///
    /// This is returned when no registered response matches.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn set_default(&self, response: MockResponse) {
        *self.default_response.lock().unwrap() = Some(response);
    }

    /// Returns the number of registered responses remaining.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.responses.lock().unwrap().len()
    }

    /// Clears all registered responses.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn clear(&self) {
        self.responses.lock().unwrap().clear();
        *self.default_response.lock().unwrap() = None;
    }

    /// Takes the next matching response.
    fn take_response(&self, program: &str, args: &[String]) -> Option<MockResponse> {
        let mut responses = self.responses.lock().unwrap();

        // Find first matching response
        if let Some(idx) = responses.iter().position(|r| r.matches(program, args)) {
            return responses.remove(idx);
        }

        drop(responses);

        // Fall back to default
        self.default_response.lock().unwrap().clone()
    }
}

thread_local! {
    static PROCESS_REGISTRY: std::cell::RefCell<Option<ProcessRegistry>> =
        const { std::cell::RefCell::new(None) };
}

/// Sets the process registry for the current thread/simulation.
///
/// # Example
///
/// ```ignore
/// let registry = ProcessRegistry::new();
/// registry.register(MockResponse::success().for_program("ls"));
/// set_registry(registry);
/// ```
pub fn set_registry(registry: ProcessRegistry) {
    PROCESS_REGISTRY.with(|r| {
        *r.borrow_mut() = Some(registry);
    });
}

/// Clears the process registry for the current thread.
pub fn clear_registry() {
    PROCESS_REGISTRY.with(|r| {
        *r.borrow_mut() = None;
    });
}

/// Gets the current process registry, if set.
#[must_use]
pub fn get_registry() -> Option<ProcessRegistry> {
    PROCESS_REGISTRY.with(|r| r.borrow().clone())
}

/// Simulated async command builder.
///
/// Mirrors `tokio::process::Command` but returns mocked responses
/// from the [`ProcessRegistry`].
#[derive(Debug)]
pub struct Command {
    program: String,
    args: Vec<String>,
    current_dir: Option<std::path::PathBuf>,
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
}

impl Command {
    /// Creates a new `Command` for the given program.
    #[must_use]
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            program: program.as_ref().to_string_lossy().to_string(),
            args: Vec::new(),
            current_dir: None,
            stdin: Stdio::default(),
            stdout: Stdio::default(),
            stderr: Stdio::default(),
        }
    }

    /// Adds an argument.
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.args.push(arg.as_ref().to_string_lossy().to_string());
        self
    }

    /// Adds multiple arguments.
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.arg(arg);
        }
        self
    }

    /// Sets the working directory.
    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.current_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Sets the stdin configuration.
    pub fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stdin = cfg.into();
        self
    }

    /// Sets the stdout configuration.
    pub fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stdout = cfg.into();
        self
    }

    /// Sets the stderr configuration.
    pub fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.stderr = cfg.into();
        self
    }

    /// Executes the command and collects output.
    ///
    /// Returns the mocked response from the registry, or a default success
    /// if no response is registered.
    ///
    /// # Errors
    ///
    /// Returns an error if the mock is configured to fail spawn.
    pub async fn output(&mut self) -> io::Result<Output> {
        let response = get_registry()
            .and_then(|r| r.take_response(&self.program, &self.args))
            .unwrap_or_default();

        if response.fail_to_spawn {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                response
                    .spawn_error
                    .unwrap_or_else(|| "command not found".to_string()),
            ));
        }

        // Simulate delay if configured
        #[cfg(feature = "time")]
        if let Some(delay) = response.delay {
            crate::time::sleep(delay).await;
        }

        Ok(Output {
            status: ExitStatus::from_code(response.exit_code),
            stdout: response.stdout,
            stderr: response.stderr,
        })
    }

    /// Spawns the command as a child process.
    ///
    /// # Errors
    ///
    /// Returns an error if the mock is configured to fail.
    pub fn spawn(&mut self) -> io::Result<Child> {
        let response = get_registry()
            .and_then(|r| r.take_response(&self.program, &self.args))
            .unwrap_or_default();

        if response.fail_to_spawn {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                response
                    .spawn_error
                    .unwrap_or_else(|| "command not found".to_string()),
            ));
        }

        // Create stdout/stderr handles based on Stdio configuration
        let stdout = if matches!(self.stdout, Stdio::Piped) {
            Some(ChildStdout::new(response.stdout.clone()))
        } else {
            None
        };

        let stderr = if matches!(self.stderr, Stdio::Piped) {
            Some(ChildStderr::new(response.stderr.clone()))
        } else {
            None
        };

        Ok(Child {
            response,
            stdout,
            stderr,
        })
    }
}

/// Simulated child process handle.
///
/// Created by [`Command::spawn`].
#[derive(Debug)]
pub struct Child {
    response: MockResponse,
    /// Handle to the child's standard output.
    ///
    /// This is `Some` if stdout was set to `Stdio::piped()` on the command.
    /// Use `.take()` to take ownership of the handle.
    pub stdout: Option<ChildStdout>,
    /// Handle to the child's standard error.
    ///
    /// This is `Some` if stderr was set to `Stdio::piped()` on the command.
    /// Use `.take()` to take ownership of the handle.
    pub stderr: Option<ChildStderr>,
}

impl Child {
    /// Waits for the child to exit.
    ///
    /// # Errors
    ///
    /// This simulated version never fails.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        #[cfg(feature = "time")]
        if let Some(delay) = self.response.delay {
            crate::time::sleep(delay).await;
        }
        Ok(ExitStatus::from_code(self.response.exit_code))
    }

    /// Waits for the child to exit and collects output.
    ///
    /// # Errors
    ///
    /// This simulated version never fails.
    pub async fn wait_with_output(self) -> io::Result<Output> {
        #[cfg(feature = "time")]
        if let Some(delay) = self.response.delay {
            crate::time::sleep(delay).await;
        }
        Ok(Output {
            status: ExitStatus::from_code(self.response.exit_code),
            stdout: self.response.stdout.clone(),
            stderr: self.response.stderr.clone(),
        })
    }

    /// Forces the child process to exit.
    ///
    /// In the simulator, this is a no-op since there's no real process to kill.
    /// The method exists for API compatibility with `tokio::process::Child`.
    ///
    /// # Errors
    ///
    /// This simulated version never fails.
    #[allow(clippy::unused_async)] // Keep async for API compatibility with tokio::process::Child
    pub async fn kill(&mut self) -> io::Result<()> {
        // No-op in simulator - there's no real process to kill
        Ok(())
    }
}

/// A handle to a child process's standard input.
///
/// In the simulator, this is a no-op sink that discards all written data.
#[derive(Debug)]
pub struct ChildStdin;

/// A handle to a child process's standard output.
///
/// In the simulator, this reads from the mocked stdout data.
#[derive(Debug)]
pub struct ChildStdout {
    data: Cursor<Vec<u8>>,
}

impl ChildStdout {
    /// Creates a new `ChildStdout` with the given data.
    pub(crate) const fn new(data: Vec<u8>) -> Self {
        Self {
            data: Cursor::new(data),
        }
    }
}

impl AsyncRead for ChildStdout {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let data = self.data.get_ref();
        #[allow(clippy::cast_possible_truncation)]
        // Position can never exceed Vec length (which is usize)
        let pos = self.data.position() as usize;
        let remaining = &data[pos..];

        let to_read = std::cmp::min(remaining.len(), buf.remaining());
        buf.put_slice(&remaining[..to_read]);
        self.data.set_position((pos + to_read) as u64);

        Poll::Ready(Ok(()))
    }
}

/// A handle to a child process's standard error.
///
/// In the simulator, this reads from the mocked stderr data.
#[derive(Debug)]
pub struct ChildStderr {
    data: Cursor<Vec<u8>>,
}

impl ChildStderr {
    /// Creates a new `ChildStderr` with the given data.
    pub(crate) const fn new(data: Vec<u8>) -> Self {
        Self {
            data: Cursor::new(data),
        }
    }
}

impl AsyncRead for ChildStderr {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let data = self.data.get_ref();
        #[allow(clippy::cast_possible_truncation)]
        // Position can never exceed Vec length (which is usize)
        let pos = self.data.position() as usize;
        let remaining = &data[pos..];

        let to_read = std::cmp::min(remaining.len(), buf.remaining());
        buf.put_slice(&remaining[..to_read]);
        self.data.set_position((pos + to_read) as u64);

        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn exit_status_success() {
        let status = ExitStatus::from_success();
        assert!(status.success());
        assert_eq!(status.code(), Some(0));
    }

    #[test_log::test]
    fn exit_status_failure() {
        let status = ExitStatus::from_code(1);
        assert!(!status.success());
        assert_eq!(status.code(), Some(1));
    }

    #[test_log::test]
    fn exit_status_signal() {
        let status = ExitStatus::from_signal();
        assert!(!status.success());
        assert_eq!(status.code(), None);
    }

    #[test_log::test]
    fn mock_response_builder() {
        let response = MockResponse::success()
            .for_program("test")
            .with_stdout(b"hello".to_vec())
            .with_stderr(b"error".to_vec())
            .with_exit_code(42);

        assert_eq!(response.program, Some("test".to_string()));
        assert_eq!(response.stdout, b"hello");
        assert_eq!(response.stderr, b"error");
        assert_eq!(response.exit_code, 42);
    }

    #[test_log::test]
    fn mock_response_matches() {
        let response = MockResponse::success().for_program("cargo");

        assert!(response.matches("cargo", &[]));
        assert!(!response.matches("rustc", &[]));

        let response_any = MockResponse::success();
        assert!(response_any.matches("anything", &[]));
    }

    #[test_log::test]
    fn registry_fifo_order() {
        let registry = ProcessRegistry::new();
        registry.register(MockResponse::success().for_program("first"));
        registry.register(MockResponse::success().for_program("second"));

        let first = registry.take_response("first", &[]);
        assert!(first.is_some());
        assert_eq!(first.unwrap().program, Some("first".to_string()));

        let second = registry.take_response("second", &[]);
        assert!(second.is_some());
        assert_eq!(second.unwrap().program, Some("second".to_string()));

        assert!(registry.take_response("third", &[]).is_none());
    }

    #[test_log::test]
    fn registry_default_response() {
        let registry = ProcessRegistry::new();
        registry.set_default(MockResponse::failure(99));

        let response = registry.take_response("unknown", &[]);
        assert!(response.is_some());
        assert_eq!(response.unwrap().exit_code, 99);
    }

    #[test_log::test]
    fn mock_response_matches_with_args() {
        let response = MockResponse::success()
            .for_program("cargo")
            .for_args(["build", "--release"]);

        // Matches exact args
        assert!(response.matches("cargo", &["build".to_string(), "--release".to_string()]));

        // Doesn't match wrong args
        assert!(!response.matches("cargo", &["test".to_string()]));

        // Doesn't match extra args
        assert!(!response.matches(
            "cargo",
            &[
                "build".to_string(),
                "--release".to_string(),
                "--verbose".to_string()
            ]
        ));
    }

    #[test_log::test]
    fn mock_response_matches_program_only() {
        // Match program, any args
        let response = MockResponse::success().for_program("cargo");
        assert!(response.matches("cargo", &[]));
        assert!(response.matches("cargo", &["build".to_string()]));
        assert!(!response.matches("rustc", &[]));
    }

    #[test_log::test]
    fn registry_remaining_tracks_count() {
        let registry = ProcessRegistry::new();
        assert_eq!(registry.remaining(), 0);

        registry.register(MockResponse::success());
        assert_eq!(registry.remaining(), 1);

        registry.register(MockResponse::success());
        assert_eq!(registry.remaining(), 2);

        registry.take_response("any", &[]);
        assert_eq!(registry.remaining(), 1);

        registry.take_response("any", &[]);
        assert_eq!(registry.remaining(), 0);
    }

    #[test_log::test]
    fn registry_clear_removes_all_responses() {
        let registry = ProcessRegistry::new();
        registry.register(MockResponse::success().for_program("first"));
        registry.register(MockResponse::success().for_program("second"));
        registry.set_default(MockResponse::failure(1));

        assert_eq!(registry.remaining(), 2);

        registry.clear();

        assert_eq!(registry.remaining(), 0);
        // Default should also be cleared
        assert!(registry.take_response("any", &[]).is_none());
    }

    #[test_log::test]
    fn registry_register_all() {
        let registry = ProcessRegistry::new();
        registry.register_all([
            MockResponse::success().for_program("first"),
            MockResponse::success().for_program("second"),
            MockResponse::success().for_program("third"),
        ]);

        assert_eq!(registry.remaining(), 3);
    }

    #[test_log::test]
    fn thread_local_registry_operations() {
        // Clear any existing registry
        clear_registry();
        assert!(get_registry().is_none());

        // Set a registry
        let registry = ProcessRegistry::new();
        registry.register(MockResponse::success().for_program("test_cmd"));
        set_registry(registry);

        // Get registry should return it
        let retrieved = get_registry();
        assert!(retrieved.is_some());

        // Clear registry
        clear_registry();
        assert!(get_registry().is_none());
    }

    #[test_log::test]
    fn mock_response_fail_spawn() {
        let response = MockResponse::success().fail_spawn("command not in PATH");

        assert!(response.fail_to_spawn);
        assert_eq!(
            response.spawn_error,
            Some("command not in PATH".to_string())
        );
    }

    #[test_log::test]
    fn exit_status_default() {
        let status = ExitStatus::default();
        assert!(status.success());
        assert_eq!(status.code(), Some(0));
    }

    #[test_log::test]
    fn output_default() {
        let output = Output::default();
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    #[test_log::test]
    fn mock_response_default() {
        let response = MockResponse::default();
        assert_eq!(response.exit_code, 0);
        assert!(response.stdout.is_empty());
        assert!(response.stderr.is_empty());
        assert!(response.program.is_none());
        assert!(response.args.is_none());
    }

    #[test_log::test]
    fn child_stdout_async_read() {
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use tokio::io::ReadBuf;

        let data = b"hello world";
        let mut stdout = ChildStdout::new(data.to_vec());

        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        // Read into a buffer
        let mut buf = [0u8; 5];
        let mut read_buf = ReadBuf::new(&mut buf);

        let result = Pin::new(&mut stdout).poll_read(&mut cx, &mut read_buf);
        assert!(matches!(result, Poll::Ready(Ok(()))));
        assert_eq!(read_buf.filled(), b"hello");

        // Read remaining
        let mut buf2 = [0u8; 20];
        let mut read_buf2 = ReadBuf::new(&mut buf2);
        let result2 = Pin::new(&mut stdout).poll_read(&mut cx, &mut read_buf2);
        assert!(matches!(result2, Poll::Ready(Ok(()))));
        assert_eq!(read_buf2.filled(), b" world");

        // Read when exhausted returns empty
        let mut buf3 = [0u8; 5];
        let mut read_buf3 = ReadBuf::new(&mut buf3);
        let result3 = Pin::new(&mut stdout).poll_read(&mut cx, &mut read_buf3);
        assert!(matches!(result3, Poll::Ready(Ok(()))));
        assert_eq!(read_buf3.filled().len(), 0);
    }

    #[test_log::test]
    fn child_stderr_async_read() {
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use tokio::io::ReadBuf;

        let data = b"error message";
        let mut stderr = ChildStderr::new(data.to_vec());

        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut buf = [0u8; 20];
        let mut read_buf = ReadBuf::new(&mut buf);

        let result = Pin::new(&mut stderr).poll_read(&mut cx, &mut read_buf);
        assert!(matches!(result, Poll::Ready(Ok(()))));
        assert_eq!(read_buf.filled(), b"error message");
    }

    #[test_log::test]
    fn command_builder_args() {
        let mut cmd = Command::new("git");
        cmd.arg("commit").arg("-m").args(["Initial commit"]);

        assert_eq!(cmd.program, "git");
        assert_eq!(
            cmd.args,
            vec!["commit", "-m", "Initial commit"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test_log::test]
    fn command_builder_current_dir() {
        let mut cmd = Command::new("ls");
        cmd.current_dir("/tmp");

        assert_eq!(cmd.current_dir, Some(std::path::PathBuf::from("/tmp")));
    }

    #[test_log::test]
    fn command_builder_stdio() {
        let mut cmd = Command::new("cat");
        cmd.stdin(Stdio::Piped)
            .stdout(Stdio::Null)
            .stderr(Stdio::Inherit);

        assert!(matches!(cmd.stdin, Stdio::Piped));
        assert!(matches!(cmd.stdout, Stdio::Null));
        assert!(matches!(cmd.stderr, Stdio::Inherit));
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn command_spawn_creates_child() {
        clear_registry();
        let registry = ProcessRegistry::new();
        registry.register(
            MockResponse::success()
                .for_program("echo")
                .with_stdout(b"hello".to_vec()),
        );
        set_registry(registry);

        let mut cmd = Command::new("echo");
        cmd.stdout(Stdio::Piped);
        let child = cmd.spawn();

        assert!(child.is_ok());
        let child = child.unwrap();
        assert!(child.stdout.is_some());

        clear_registry();
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn command_spawn_fails_when_configured() {
        clear_registry();
        let registry = ProcessRegistry::new();
        registry.register(MockResponse::success().fail_spawn("not found"));
        set_registry(registry);

        let mut cmd = Command::new("nonexistent");
        let result = cmd.spawn();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains("not found"));

        clear_registry();
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn command_output_returns_mocked_response() {
        clear_registry();
        let registry = ProcessRegistry::new();
        registry.register(
            MockResponse::success()
                .for_program("echo")
                .with_stdout(b"hello\n".to_vec())
                .with_stderr(b"warning".to_vec())
                .with_exit_code(0),
        );
        set_registry(registry);

        let mut cmd = Command::new("echo");
        let output = cmd.output().await.unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, b"hello\n");
        assert_eq!(output.stderr, b"warning");

        clear_registry();
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn command_output_fails_when_configured() {
        clear_registry();
        let registry = ProcessRegistry::new();
        registry.register(MockResponse::success().fail_spawn("permission denied"));
        set_registry(registry);

        let mut cmd = Command::new("restricted");
        let result = cmd.output().await;

        assert!(result.is_err());

        clear_registry();
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn command_uses_default_when_no_match() {
        clear_registry();

        let mut cmd = Command::new("any_command");
        let output = cmd.output().await.unwrap();

        // Default is success with empty output
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());

        clear_registry();
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn child_wait_returns_exit_status() {
        clear_registry();
        let registry = ProcessRegistry::new();
        registry.register(MockResponse::failure(42).for_program("failing_cmd"));
        set_registry(registry);

        let mut cmd = Command::new("failing_cmd");
        let mut child = cmd.spawn().unwrap();
        let status = child.wait().await.unwrap();

        assert!(!status.success());
        assert_eq!(status.code(), Some(42));

        clear_registry();
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn child_wait_with_output_returns_full_output() {
        clear_registry();
        let registry = ProcessRegistry::new();
        registry.register(
            MockResponse::success()
                .for_program("cmd")
                .with_stdout(b"out".to_vec())
                .with_stderr(b"err".to_vec()),
        );
        set_registry(registry);

        let mut cmd = Command::new("cmd");
        let child = cmd.spawn().unwrap();
        let output = child.wait_with_output().await.unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");

        clear_registry();
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn child_kill_is_noop() {
        clear_registry();
        let registry = ProcessRegistry::new();
        registry.register(MockResponse::success());
        set_registry(registry);

        let mut cmd = Command::new("long_running");
        let mut child = cmd.spawn().unwrap();

        // Kill should succeed (no-op in simulator)
        let result = child.kill().await;
        assert!(result.is_ok());

        clear_registry();
    }

    #[test_log::test]
    fn stdio_from_std_process_stdio() {
        // Converting from std::process::Stdio defaults to Inherit
        let std_piped = std::process::Stdio::piped();
        let switchy_stdio: Stdio = std_piped.into();
        assert!(matches!(switchy_stdio, Stdio::Inherit));
    }
}
