//! Tool execution and result aggregation.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, IsTerminal};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(feature = "tools-tui")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "tools-tui")]
use std::sync::{Arc, Mutex, OnceLock, mpsc};
#[cfg(feature = "tools-tui")]
use std::thread;

use rayon::prelude::*;

use crate::ColorMode;
use crate::tools::registry::{ToolError, ToolRegistry};
#[cfg(feature = "tools-tui")]
use crate::tools::tui;
use crate::tools::types::{Tool, ToolKind};

/// Live tool execution events used by the TUI.
#[cfg(feature = "tools-tui")]
#[derive(Debug, Clone)]
pub enum ToolEvent {
    Started {
        tool_name: String,
        display_name: String,
    },
    StdoutLine {
        tool_name: String,
        line: String,
    },
    StderrLine {
        tool_name: String,
        line: String,
    },
    Finished {
        tool_name: String,
        success: bool,
    },
}

#[cfg(feature = "tools-tui")]
static POST_TUI_INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "tools-tui")]
static POST_TUI_INTERRUPT_ENABLED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "tools-tui")]
static POST_TUI_INTERRUPT_HANDLER_INIT: OnceLock<()> = OnceLock::new();

/// Result of running a single tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Name of the tool
    pub tool_name: String,
    /// Display name of the tool
    pub display_name: String,
    /// Whether the tool succeeded (exit code 0)
    pub success: bool,
    /// Exit code from the tool
    pub exit_code: Option<i32>,
    /// Standard output from the tool
    pub stdout: String,
    /// Standard error from the tool
    pub stderr: String,
    /// How long the tool took to run
    pub duration: Duration,
}

impl ToolResult {
    /// Creates a new successful result
    #[must_use]
    pub const fn success(tool_name: String, display_name: String, duration: Duration) -> Self {
        Self {
            tool_name,
            display_name,
            success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            duration,
        }
    }

    /// Creates a new failed result
    #[must_use]
    pub const fn failure(
        tool_name: String,
        display_name: String,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration: Duration,
    ) -> Self {
        Self {
            tool_name,
            display_name,
            success: false,
            exit_code,
            stdout,
            stderr,
            duration,
        }
    }
}

/// Aggregated results from running multiple tools
#[derive(Debug, Clone)]
pub struct AggregatedResults {
    /// Results from each tool
    pub results: Vec<ToolResult>,
    /// Total duration
    pub total_duration: Duration,
    /// Number of tools that succeeded
    pub success_count: usize,
    /// Number of tools that failed
    pub failure_count: usize,
}

impl AggregatedResults {
    /// Returns true if all tools succeeded
    #[must_use]
    pub const fn all_success(&self) -> bool {
        self.failure_count == 0
    }

    /// Returns the overall exit code (0 if all succeeded, 1 otherwise)
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        if self.failure_count == 0 { 0 } else { 1 }
    }
}

/// Runs tools and aggregates results
#[derive(Debug)]
pub struct ToolRunner<'a> {
    registry: &'a ToolRegistry,
    /// Working directory to run tools in
    working_dir: Option<&'a Path>,
    /// Whether to stream output in real-time (only used in sequential mode)
    stream_output: bool,
    /// Whether to run tools in parallel
    parallel: bool,
    /// Color mode for child tool output
    color_mode: ColorMode,
}

impl<'a> ToolRunner<'a> {
    /// Creates a new tool runner (parallel by default)
    #[must_use]
    pub const fn new(registry: &'a ToolRegistry) -> Self {
        Self {
            registry,
            working_dir: None,
            stream_output: true,
            parallel: true,
            color_mode: ColorMode::Auto,
        }
    }

    /// Sets the working directory for tool execution
    #[must_use]
    pub const fn with_working_dir(mut self, dir: &'a Path) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// Sets whether to stream output in real-time (only applies in sequential mode)
    #[must_use]
    pub const fn with_stream_output(mut self, stream: bool) -> Self {
        self.stream_output = stream;
        self
    }

    /// Sets whether to run tools in parallel (default: true)
    #[must_use]
    pub const fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    /// Sets color mode for child tool execution
    #[must_use]
    pub const fn with_color_mode(mut self, color_mode: ColorMode) -> Self {
        self.color_mode = color_mode;
        self
    }

    fn should_use_color_auto() -> bool {
        std::io::stdout().is_terminal() || std::io::stderr().is_terminal()
    }

    const fn effective_color_mode_for_terminals(
        requested: ColorMode,
        stdout_is_terminal: bool,
        stderr_is_terminal: bool,
    ) -> ColorMode {
        match requested {
            ColorMode::Auto => {
                if stdout_is_terminal || stderr_is_terminal {
                    ColorMode::Always
                } else {
                    ColorMode::Never
                }
            }
            ColorMode::Always => ColorMode::Always,
            ColorMode::Never => ColorMode::Never,
        }
    }

    fn effective_color_mode(&self) -> ColorMode {
        Self::effective_color_mode_for_terminals(
            self.color_mode,
            std::io::stdout().is_terminal(),
            std::io::stderr().is_terminal(),
        )
    }

    fn apply_color_env(command: &mut Command, mode: ColorMode) {
        match mode {
            ColorMode::Always => {
                command.env("CLICOLOR_FORCE", "1");
                command.env("FORCE_COLOR", "1");
                command.env("CARGO_TERM_COLOR", "always");
                command.env("PY_COLORS", "1");
                command.env_remove("NO_COLOR");
                command.env_remove("CLICOLOR");
            }
            ColorMode::Never => {
                command.env("NO_COLOR", "1");
                command.env("CLICOLOR", "0");
                command.env("CARGO_TERM_COLOR", "never");
                command.env_remove("CLICOLOR_FORCE");
                command.env_remove("FORCE_COLOR");
                command.env_remove("PY_COLORS");
            }
            ColorMode::Auto => {
                if Self::should_use_color_auto() {
                    Self::apply_color_env(command, ColorMode::Always);
                } else {
                    Self::apply_color_env(command, ColorMode::Never);
                };
            }
        }
    }

    /// Runs all available formatters
    ///
    /// # Errors
    ///
    /// Returns an error if no formatters are available.
    pub fn run_formatters(&self, paths: &[&str]) -> Result<AggregatedResults, ToolError> {
        let formatters = self.registry.formatters();
        if formatters.is_empty() {
            return Err(ToolError::NoToolsAvailable);
        }

        Ok(self.run_tools(&formatters, paths, false))
    }

    /// Runs all available linters/checkers
    ///
    /// # Errors
    ///
    /// Returns an error if no linters are available.
    pub fn run_linters(&self, paths: &[&str]) -> Result<AggregatedResults, ToolError> {
        let linters = self.registry.linters();
        if linters.is_empty() {
            return Err(ToolError::NoToolsAvailable);
        }

        Ok(self.run_tools(&linters, paths, true))
    }

    /// Runs format check (--check mode) for all formatters
    ///
    /// # Errors
    ///
    /// Returns an error if no formatters are available.
    pub fn run_format_check(&self, paths: &[&str]) -> Result<AggregatedResults, ToolError> {
        let formatters = self.registry.formatters();
        if formatters.is_empty() {
            return Err(ToolError::NoToolsAvailable);
        }

        Ok(self.run_tools(&formatters, paths, true))
    }

    /// Runs specific tools by name
    ///
    /// # Errors
    ///
    /// Returns an error if no matching tools are found.
    pub fn run_specific(
        &self,
        tool_names: &[&str],
        paths: &[&str],
        check_mode: bool,
    ) -> Result<AggregatedResults, ToolError> {
        let tools: Vec<&Tool> = tool_names
            .iter()
            .filter_map(|name| self.registry.get(name))
            .collect();

        if tools.is_empty() {
            return Err(ToolError::NoToolsAvailable);
        }

        Ok(self.run_tools(&tools, paths, check_mode))
    }

    /// Runs specific tools by name and renders live pane output in a TUI.
    ///
    /// # Errors
    ///
    /// Returns an error if no matching tools are found.
    pub fn run_specific_with_tui(
        &self,
        tool_names: &[&str],
        paths: &[&str],
        check_mode: bool,
    ) -> Result<AggregatedResults, ToolError> {
        let tools: Vec<&Tool> = tool_names
            .iter()
            .filter_map(|name| self.registry.get(name))
            .collect();

        if tools.is_empty() {
            return Err(ToolError::NoToolsAvailable);
        }

        #[cfg(feature = "tools-tui")]
        {
            Ok(self.run_tools_with_tui(&tools, paths, check_mode))
        }

        #[cfg(not(feature = "tools-tui"))]
        {
            Ok(self.run_tools(&tools, paths, check_mode))
        }
    }

    /// Runs a collection of tools and aggregates results
    fn run_tools(&self, tools: &[&Tool], _paths: &[&str], check_mode: bool) -> AggregatedResults {
        let start_time = Instant::now();

        let results: Vec<ToolResult> = if self.parallel {
            // Run tools in parallel with buffered output
            tools
                .par_iter()
                .map(|tool| self.run_single_tool_buffered(tool, check_mode))
                .collect()
        } else {
            // Run tools sequentially (can stream output)
            tools
                .iter()
                .map(|tool| self.run_single_tool(tool, check_mode))
                .collect()
        };

        let total_duration = start_time.elapsed();
        let success_count = results.iter().filter(|r| r.success).count();
        let failure_count = results.len() - success_count;

        AggregatedResults {
            results,
            total_duration,
            success_count,
            failure_count,
        }
    }

    #[cfg(feature = "tools-tui")]
    fn run_tools_with_tui(
        &self,
        tools: &[&Tool],
        _paths: &[&str],
        check_mode: bool,
    ) -> AggregatedResults {
        let start_time = Instant::now();
        let (tx, rx) = mpsc::channel::<ToolEvent>();
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let tool_meta: Vec<(String, String)> = tools
            .iter()
            .map(|tool| (tool.name.clone(), tool.display_name.clone()))
            .collect();

        let results: Vec<ToolResult> = thread::scope(|scope| {
            let mut handles = Vec::new();
            for tool in tools {
                let tx = tx.clone();
                let cancel = Arc::clone(&cancel_requested);
                handles.push(scope.spawn(move || {
                    self.run_single_tool_with_events(tool, check_mode, &tx, &cancel)
                }));
            }
            drop(tx);

            let tui_exit = match tui::run_live_tui(&tool_meta, rx, start_time) {
                Ok(exit) => exit,
                Err(e) => {
                    log::warn!("failed to start tool TUI, continuing without live panes: {e}");
                    tui::TuiExit::Completed
                }
            };

            if tui_exit == tui::TuiExit::UserClosed {
                if let Err(e) = Self::install_post_tui_interrupt_handler() {
                    log::warn!("failed to install Ctrl-C handler for post-TUI mode: {e}");
                }
                POST_TUI_INTERRUPT_REQUESTED.store(false, Ordering::SeqCst);
                POST_TUI_INTERRUPT_ENABLED.store(true, Ordering::SeqCst);
            } else {
                POST_TUI_INTERRUPT_ENABLED.store(false, Ordering::SeqCst);
            }

            let results = Self::wait_for_tool_threads(handles, &cancel_requested, tui_exit);
            POST_TUI_INTERRUPT_ENABLED.store(false, Ordering::SeqCst);
            results
        });

        let total_duration = start_time.elapsed();
        let success_count = results.iter().filter(|r| r.success).count();
        let failure_count = results.len() - success_count;

        AggregatedResults {
            results,
            total_duration,
            success_count,
            failure_count,
        }
    }

    #[cfg(feature = "tools-tui")]
    fn install_post_tui_interrupt_handler() -> Result<(), ctrlc::Error> {
        if POST_TUI_INTERRUPT_HANDLER_INIT.get().is_some() {
            return Ok(());
        }

        ctrlc::set_handler(|| {
            if POST_TUI_INTERRUPT_ENABLED.load(Ordering::SeqCst) {
                POST_TUI_INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
            }
        })?;

        let _ = POST_TUI_INTERRUPT_HANDLER_INIT.set(());
        Ok(())
    }

    #[cfg(feature = "tools-tui")]
    fn wait_for_tool_threads<'scope>(
        mut handles: Vec<std::thread::ScopedJoinHandle<'scope, ToolResult>>,
        cancel_requested: &Arc<AtomicBool>,
        tui_exit: tui::TuiExit,
    ) -> Vec<ToolResult> {
        let mut results = Vec::with_capacity(handles.len());

        while !handles.is_empty() {
            if tui_exit == tui::TuiExit::UserClosed
                && POST_TUI_INTERRUPT_REQUESTED.load(Ordering::SeqCst)
            {
                cancel_requested.store(true, Ordering::SeqCst);
            }

            let mut index = 0_usize;
            while index < handles.len() {
                if handles[index].is_finished() {
                    let handle = handles.swap_remove(index);
                    results.push(handle.join().unwrap_or_else(|_panic| {
                        ToolResult::failure(
                            "unknown".to_string(),
                            "unknown".to_string(),
                            None,
                            String::new(),
                            "Tool execution thread panicked".to_string(),
                            Duration::ZERO,
                        )
                    }));
                } else {
                    index += 1;
                }
            }

            if !handles.is_empty() {
                thread::sleep(Duration::from_millis(50));
            }
        }

        if tui_exit == tui::TuiExit::UserClosed
            && POST_TUI_INTERRUPT_REQUESTED.load(Ordering::SeqCst)
        {
            std::process::exit(130);
        }

        results
    }

    #[cfg(feature = "tools-tui")]
    fn build_command_parts(tool: &Tool, check_mode: bool) -> Option<(String, Vec<String>)> {
        let args = if check_mode {
            &tool.check_args
        } else {
            &tool.format_args
        };

        if args.is_empty() {
            return None;
        }

        let parts = match &tool.kind {
            ToolKind::Cargo => ("cargo".to_string(), args.clone()),
            ToolKind::Binary => {
                let binary = tool
                    .detected_path
                    .as_ref()
                    .map_or_else(|| tool.binary.clone(), |p| p.display().to_string());
                (binary, args.clone())
            }
            ToolKind::Runner { runner } => {
                let mut all_args = vec![tool.binary.clone()];
                all_args.extend(args.clone());
                (runner.clone(), all_args)
            }
        };

        Some(parts)
    }

    #[cfg(feature = "tools-tui")]
    fn run_single_tool_with_events(
        &self,
        tool: &Tool,
        check_mode: bool,
        tx: &mpsc::Sender<ToolEvent>,
        cancel_requested: &Arc<AtomicBool>,
    ) -> ToolResult {
        let start_time = Instant::now();

        let Some((program, final_args)) = Self::build_command_parts(tool, check_mode) else {
            let result =
                ToolResult::success(tool.name.clone(), tool.display_name.clone(), Duration::ZERO);
            let _ = tx.send(ToolEvent::Finished {
                tool_name: tool.name.clone(),
                success: true,
            });
            return result;
        };

        let _ = tx.send(ToolEvent::Started {
            tool_name: tool.name.clone(),
            display_name: tool.display_name.clone(),
        });

        let mut command = Command::new(&program);
        command.args(&final_args);
        Self::apply_color_env(&mut command, self.effective_color_mode());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        if let Some(dir) = self.working_dir {
            command.current_dir(dir);
        }

        match command.spawn() {
            Ok(mut child) => {
                let stdout_content = Arc::new(Mutex::new(String::new()));
                let stderr_content = Arc::new(Mutex::new(String::new()));

                let stdout_handle = child.stdout.take().map(|stdout| {
                    let tx = tx.clone();
                    let tool_name = tool.name.clone();
                    let output = Arc::clone(&stdout_content);
                    thread::spawn(move || {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines().map_while(Result::ok) {
                            let _ = tx.send(ToolEvent::StdoutLine {
                                tool_name: tool_name.clone(),
                                line: line.clone(),
                            });
                            if let Ok(mut buf) = output.lock() {
                                buf.push_str(&line);
                                buf.push('\n');
                            }
                        }
                    })
                });

                let stderr_handle = child.stderr.take().map(|stderr| {
                    let tx = tx.clone();
                    let tool_name = tool.name.clone();
                    let output = Arc::clone(&stderr_content);
                    thread::spawn(move || {
                        let reader = BufReader::new(stderr);
                        for line in reader.lines().map_while(Result::ok) {
                            let _ = tx.send(ToolEvent::StderrLine {
                                tool_name: tool_name.clone(),
                                line: line.clone(),
                            });
                            if let Ok(mut buf) = output.lock() {
                                buf.push_str(&line);
                                buf.push('\n');
                            }
                        }
                    })
                });

                let status = loop {
                    if cancel_requested.load(Ordering::SeqCst)
                        && let Err(e) = child.kill()
                        && e.kind() != std::io::ErrorKind::InvalidInput
                    {
                        log::debug!("failed to kill tool process '{}': {e}", tool.name);
                    }

                    match child.try_wait() {
                        Ok(Some(exit)) => {
                            break exit;
                        }
                        Ok(None) => thread::sleep(Duration::from_millis(25)),
                        Err(e) => {
                            let result = ToolResult::failure(
                                tool.name.clone(),
                                tool.display_name.clone(),
                                None,
                                String::new(),
                                format!("Failed to wait for process: {e}"),
                                start_time.elapsed(),
                            );

                            let _ = tx.send(ToolEvent::Finished {
                                tool_name: tool.name.clone(),
                                success: false,
                            });

                            return result;
                        }
                    }
                };

                if let Some(handle) = stdout_handle {
                    let _ = handle.join();
                }
                if let Some(handle) = stderr_handle {
                    let _ = handle.join();
                }

                let stdout = stdout_content
                    .lock()
                    .map_or_else(|_| String::new(), |buf| buf.clone());
                let stderr = stderr_content
                    .lock()
                    .map_or_else(|_| String::new(), |buf| buf.clone());

                let duration = start_time.elapsed();
                let result = if status.success() {
                    ToolResult::success(tool.name.clone(), tool.display_name.clone(), duration)
                } else {
                    ToolResult::failure(
                        tool.name.clone(),
                        tool.display_name.clone(),
                        status.code(),
                        stdout,
                        stderr,
                        duration,
                    )
                };

                let _ = tx.send(ToolEvent::Finished {
                    tool_name: tool.name.clone(),
                    success: result.success,
                });

                result
            }
            Err(e) => {
                let result = ToolResult::failure(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    None,
                    String::new(),
                    format!("Failed to spawn process: {e}"),
                    start_time.elapsed(),
                );
                let _ = tx.send(ToolEvent::Finished {
                    tool_name: tool.name.clone(),
                    success: false,
                });
                result
            }
        }
    }

    /// Runs a single tool
    #[allow(clippy::too_many_lines)]
    fn run_single_tool(&self, tool: &Tool, check_mode: bool) -> ToolResult {
        let start_time = Instant::now();

        let args = if check_mode {
            &tool.check_args
        } else {
            &tool.format_args
        };

        // Skip if no args (tool doesn't support this mode)
        if args.is_empty() {
            return ToolResult::success(
                tool.name.clone(),
                tool.display_name.clone(),
                Duration::ZERO,
            );
        }

        let (program, final_args) = match &tool.kind {
            ToolKind::Cargo => ("cargo".to_string(), args.clone()),
            ToolKind::Binary => {
                let binary = tool
                    .detected_path
                    .as_ref()
                    .map_or_else(|| tool.binary.clone(), |p| p.display().to_string());
                (binary, args.clone())
            }
            ToolKind::Runner { runner } => {
                let mut all_args = vec![tool.binary.clone()];
                all_args.extend(args.clone());
                (runner.clone(), all_args)
            }
        };

        log::info!("Running {} ({})...", tool.display_name, tool.name);
        log::debug!("Command: {program} {final_args:?}");

        let mut command = Command::new(&program);
        command.args(&final_args);
        Self::apply_color_env(&mut command, self.effective_color_mode());

        if let Some(dir) = self.working_dir {
            command.current_dir(dir);
        }

        if self.stream_output {
            // Stream output in real-time
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());

            match command.spawn() {
                Ok(mut child) => {
                    let mut stdout_content = String::new();
                    let mut stderr_content = String::new();

                    // Read stdout
                    if let Some(stdout) = child.stdout.take() {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines().map_while(Result::ok) {
                            println!("{line}");
                            stdout_content.push_str(&line);
                            stdout_content.push('\n');
                        }
                    }

                    // Read stderr
                    if let Some(stderr) = child.stderr.take() {
                        let reader = BufReader::new(stderr);
                        for line in reader.lines().map_while(Result::ok) {
                            eprintln!("{line}");
                            stderr_content.push_str(&line);
                            stderr_content.push('\n');
                        }
                    }

                    match child.wait() {
                        Ok(status) => {
                            let duration = start_time.elapsed();
                            let exit_code = status.code();

                            if status.success() {
                                ToolResult::success(
                                    tool.name.clone(),
                                    tool.display_name.clone(),
                                    duration,
                                )
                            } else {
                                ToolResult::failure(
                                    tool.name.clone(),
                                    tool.display_name.clone(),
                                    exit_code,
                                    stdout_content,
                                    stderr_content,
                                    duration,
                                )
                            }
                        }
                        Err(e) => ToolResult::failure(
                            tool.name.clone(),
                            tool.display_name.clone(),
                            None,
                            String::new(),
                            format!("Failed to wait for process: {e}"),
                            start_time.elapsed(),
                        ),
                    }
                }
                Err(e) => ToolResult::failure(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    None,
                    String::new(),
                    format!("Failed to spawn process: {e}"),
                    start_time.elapsed(),
                ),
            }
        } else {
            // Capture all output at once
            match command.output() {
                Ok(output) => {
                    let duration = start_time.elapsed();
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let exit_code = output.status.code();

                    if output.status.success() {
                        ToolResult::success(tool.name.clone(), tool.display_name.clone(), duration)
                    } else {
                        ToolResult::failure(
                            tool.name.clone(),
                            tool.display_name.clone(),
                            exit_code,
                            stdout,
                            stderr,
                            duration,
                        )
                    }
                }
                Err(e) => ToolResult::failure(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    None,
                    String::new(),
                    format!("Failed to execute: {e}"),
                    start_time.elapsed(),
                ),
            }
        }
    }

    /// Runs a single tool with buffered output (for parallel execution)
    fn run_single_tool_buffered(&self, tool: &Tool, check_mode: bool) -> ToolResult {
        let start_time = Instant::now();

        let args = if check_mode {
            &tool.check_args
        } else {
            &tool.format_args
        };

        // Skip if no args (tool doesn't support this mode)
        if args.is_empty() {
            return ToolResult::success(
                tool.name.clone(),
                tool.display_name.clone(),
                Duration::ZERO,
            );
        }

        let (program, final_args) = match &tool.kind {
            ToolKind::Cargo => ("cargo".to_string(), args.clone()),
            ToolKind::Binary => {
                let binary = tool
                    .detected_path
                    .as_ref()
                    .map_or_else(|| tool.binary.clone(), |p| p.display().to_string());
                (binary, args.clone())
            }
            ToolKind::Runner { runner } => {
                let mut all_args = vec![tool.binary.clone()];
                all_args.extend(args.clone());
                (runner.clone(), all_args)
            }
        };

        log::info!("Running {} ({})...", tool.display_name, tool.name);
        log::debug!("Command: {program} {final_args:?}");

        let mut command = Command::new(&program);
        command.args(&final_args);
        Self::apply_color_env(&mut command, self.effective_color_mode());

        if let Some(dir) = self.working_dir {
            command.current_dir(dir);
        }

        // Capture all output at once (buffered for parallel execution)
        match command.output() {
            Ok(output) => {
                let duration = start_time.elapsed();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code();

                if output.status.success() {
                    ToolResult {
                        tool_name: tool.name.clone(),
                        display_name: tool.display_name.clone(),
                        success: true,
                        exit_code,
                        stdout,
                        stderr,
                        duration,
                    }
                } else {
                    ToolResult::failure(
                        tool.name.clone(),
                        tool.display_name.clone(),
                        exit_code,
                        stdout,
                        stderr,
                        duration,
                    )
                }
            }
            Err(e) => ToolResult::failure(
                tool.name.clone(),
                tool.display_name.clone(),
                None,
                String::new(),
                format!("Failed to execute: {e}"),
                start_time.elapsed(),
            ),
        }
    }
}

/// Prints a summary of results, including buffered output from each tool
pub fn print_summary(results: &AggregatedResults) {
    // First, print the buffered output from each tool sequentially
    for result in &results.results {
        // Print a header for each tool's output
        let has_output = !result.stdout.is_empty() || !result.stderr.is_empty();
        if has_output {
            println!();
            println!("--- {} ---", result.display_name);
        }

        if !result.stdout.is_empty() {
            print!("{}", result.stdout);
            // Ensure output ends with newline
            if !result.stdout.ends_with('\n') {
                println!();
            }
        }

        if !result.stderr.is_empty() {
            eprint!("{}", result.stderr);
            // Ensure output ends with newline
            if !result.stderr.ends_with('\n') {
                eprintln!();
            }
        }
    }

    // Then print the summary
    println!();
    println!("=== Summary ===");
    println!(
        "Total: {} tools, {} passed, {} failed",
        results.results.len(),
        results.success_count,
        results.failure_count
    );
    println!("Duration: {:.2?}", results.total_duration);
    println!();

    for result in &results.results {
        let status = if result.success { "PASS" } else { "FAIL" };
        let exit_info = result
            .exit_code
            .map_or(String::new(), |c| format!(" (exit code: {c})"));
        println!(
            "  [{status}] {} ({:.2?}){exit_info}",
            result.display_name, result.duration
        );
    }
}

/// Formats results as JSON
///
/// # Errors
///
/// Returns an error if JSON serialization fails.
pub fn results_to_json(
    results: &AggregatedResults,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let json_results: Vec<BTreeMap<String, serde_json::Value>> = results
        .results
        .iter()
        .map(|r| {
            let mut map = BTreeMap::new();
            map.insert("name".to_string(), serde_json::json!(r.tool_name));
            map.insert(
                "display_name".to_string(),
                serde_json::json!(r.display_name),
            );
            map.insert("success".to_string(), serde_json::json!(r.success));
            map.insert("exit_code".to_string(), serde_json::json!(r.exit_code));
            map.insert(
                "duration_ms".to_string(),
                serde_json::json!(r.duration.as_millis()),
            );
            if !r.stdout.is_empty() {
                map.insert("stdout".to_string(), serde_json::json!(r.stdout));
            }
            if !r.stderr.is_empty() {
                map.insert("stderr".to_string(), serde_json::json!(r.stderr));
            }
            map
        })
        .collect();

    let output = serde_json::json!({
        "success": results.all_success(),
        "total": results.results.len(),
        "passed": results.success_count,
        "failed": results.failure_count,
        "duration_ms": results.total_duration.as_millis(),
        "results": json_results,
    });

    Ok(serde_json::to_string_pretty(&output)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_color_mode_auto_uses_terminal_presence() {
        assert_eq!(
            ToolRunner::effective_color_mode_for_terminals(ColorMode::Auto, true, false),
            ColorMode::Always
        );
        assert_eq!(
            ToolRunner::effective_color_mode_for_terminals(ColorMode::Auto, false, true),
            ColorMode::Always
        );
        assert_eq!(
            ToolRunner::effective_color_mode_for_terminals(ColorMode::Auto, false, false),
            ColorMode::Never
        );
    }

    #[test]
    fn apply_color_env_sets_expected_vars_for_always() {
        let mut command = Command::new("true");
        ToolRunner::apply_color_env(&mut command, ColorMode::Always);

        let envs: BTreeMap<String, Option<String>> = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();

        assert_eq!(envs.get("CLICOLOR_FORCE"), Some(&Some("1".to_string())));
        assert_eq!(envs.get("FORCE_COLOR"), Some(&Some("1".to_string())));
        assert_eq!(
            envs.get("CARGO_TERM_COLOR"),
            Some(&Some("always".to_string()))
        );
        assert_eq!(envs.get("PY_COLORS"), Some(&Some("1".to_string())));
    }

    #[test]
    fn apply_color_env_sets_expected_vars_for_never() {
        let mut command = Command::new("true");
        ToolRunner::apply_color_env(&mut command, ColorMode::Never);

        let envs: BTreeMap<String, Option<String>> = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();

        assert_eq!(envs.get("NO_COLOR"), Some(&Some("1".to_string())));
        assert_eq!(envs.get("CLICOLOR"), Some(&Some("0".to_string())));
        assert_eq!(
            envs.get("CARGO_TERM_COLOR"),
            Some(&Some("never".to_string()))
        );
    }
}
