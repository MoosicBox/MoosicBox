//! Tool execution and result aggregation.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(feature = "tools-tui")]
use std::io::Read;
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
        overwrite: bool,
    },
    StderrLine {
        tool_name: String,
        line: String,
        overwrite: bool,
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
                }
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
    fn wait_for_tool_threads(
        mut handles: Vec<std::thread::ScopedJoinHandle<'_, ToolResult>>,
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
    fn build_command_parts(
        tool: &Tool,
        check_mode: bool,
        working_dir: Option<&Path>,
    ) -> Option<(String, Vec<String>, Vec<String>)> {
        let args = if check_mode {
            &tool.check_args
        } else {
            &tool.format_args
        };

        if args.is_empty() {
            return None;
        }

        let (mut parts, args_start_index) = match &tool.kind {
            ToolKind::Cargo => (("cargo".to_string(), args.clone()), 0),
            ToolKind::Binary => {
                let binary = tool
                    .detected_path
                    .as_ref()
                    .map_or_else(|| tool.binary.clone(), |p| p.display().to_string());
                ((binary, args.clone()), 0)
            }
            ToolKind::Runner {
                runner,
                runner_args,
            } => {
                let mut all_args = runner_args.clone();
                all_args.push(tool.binary.clone());
                all_args.extend(args.clone());
                ((runner.clone(), all_args), runner_args.len() + 1)
            }
        };

        Self::append_prettier_ignore_path_arg(tool, &mut parts.1, working_dir, args_start_index);
        let warnings =
            Self::append_mdformat_extension_args(tool, &mut parts.1, working_dir, args_start_index);

        Some((parts.0, parts.1, warnings))
    }

    fn working_dir_absolute(working_dir: Option<&Path>) -> PathBuf {
        let base_dir = working_dir.map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf()),
            Path::to_path_buf,
        );
        if base_dir.is_absolute() {
            base_dir
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| Path::new(".").to_path_buf())
                .join(base_dir)
        }
    }

    fn mdformat_supports_extension(
        tool: &Tool,
        working_dir: Option<&Path>,
        extension: &str,
    ) -> bool {
        let mut args = match &tool.kind {
            ToolKind::Cargo => return false,
            ToolKind::Binary => vec![],
            ToolKind::Runner { runner_args, .. } => {
                let mut values = runner_args.clone();
                values.push(tool.binary.clone());
                values
            }
        };

        args.push("--check".to_string());
        args.push("--extensions".to_string());
        args.push(extension.to_string());
        args.push("-".to_string());

        let program = match &tool.kind {
            ToolKind::Cargo => return false,
            ToolKind::Binary => tool
                .detected_path
                .as_ref()
                .map_or_else(|| tool.binary.clone(), |p| p.display().to_string()),
            ToolKind::Runner { runner, .. } => runner.clone(),
        };

        let mut command = Command::new(program);
        command.args(args);
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        command.stdin(Stdio::piped());
        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }

        let Ok(mut child) = command.spawn() else {
            return false;
        };

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write as _;
            let _ = stdin.write_all(b"# mdformat extension probe\n");
        }

        child.wait().is_ok_and(|status| status.success())
    }

    fn find_file_in_ancestors(base_dir: &Path, names: &[&str]) -> Option<PathBuf> {
        let mut current = Some(base_dir);
        while let Some(dir) = current {
            for name in names {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
            current = dir.parent();
        }
        None
    }

    fn mdformat_requested_extensions(
        working_dir: Option<&Path>,
    ) -> std::collections::BTreeSet<String> {
        fn parse_extensions(value: &toml::Value) -> std::collections::BTreeSet<String> {
            value
                .as_array()
                .into_iter()
                .flat_map(|values| values.iter())
                .filter_map(toml::Value::as_str)
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| value == "gfm" || value == "mdx" || value == "frontmatter")
                .collect()
        }

        let mut requested = std::collections::BTreeSet::new();
        let base_dir = Self::working_dir_absolute(working_dir);

        if let Some(path) = Self::find_file_in_ancestors(&base_dir, &[".mdformat.toml"])
            && let Ok(contents) = std::fs::read_to_string(path)
            && let Ok(parsed) = toml::from_str::<toml::Value>(&contents)
            && let Some(extensions) = parsed.get("extensions")
        {
            requested.extend(parse_extensions(extensions));
        }

        if let Some(path) = Self::find_file_in_ancestors(&base_dir, &["pyproject.toml"])
            && let Ok(contents) = std::fs::read_to_string(path)
            && let Ok(parsed) = toml::from_str::<toml::Value>(&contents)
            && let Some(extensions) = parsed
                .get("tool")
                .and_then(|tool| tool.get("mdformat"))
                .and_then(|mdformat| mdformat.get("extensions"))
        {
            requested.extend(parse_extensions(extensions));
        }

        requested
    }

    fn mdformat_runtime_label(tool: &Tool) -> String {
        match &tool.kind {
            ToolKind::Binary => tool
                .detected_path
                .as_ref()
                .map_or_else(|| tool.binary.clone(), |path| path.display().to_string()),
            ToolKind::Runner {
                runner,
                runner_args,
            } => {
                let mut parts = vec![runner.clone()];
                parts.extend(runner_args.clone());
                parts.push(tool.binary.clone());
                parts.join(" ")
            }
            ToolKind::Cargo => "cargo".to_string(),
        }
    }

    fn append_mdformat_extension_args(
        tool: &Tool,
        args: &mut Vec<String>,
        working_dir: Option<&Path>,
        args_start_index: usize,
    ) -> Vec<String> {
        if tool.name != "mdformat" {
            return Vec::new();
        }

        let insert_index = args
            .iter()
            .enumerate()
            .skip(args_start_index)
            .find_map(|(index, arg)| {
                if arg.starts_with('-') {
                    None
                } else {
                    Some(index)
                }
            })
            .unwrap_or(args.len());

        let requested_extensions = Self::mdformat_requested_extensions(working_dir);
        if requested_extensions.is_empty() {
            return Vec::new();
        }

        let mut extension_args = Vec::new();
        let mut missing_extensions = Vec::new();
        for extension in &requested_extensions {
            if Self::mdformat_supports_extension(tool, working_dir, extension) {
                extension_args.push("--extensions".to_string());
                extension_args.push(extension.clone());
            } else {
                missing_extensions.push(extension.clone());
            }
        }

        if !extension_args.is_empty() {
            args.splice(insert_index..insert_index, extension_args);
        }

        if missing_extensions.is_empty() {
            return Vec::new();
        }

        let enabled_extensions = requested_extensions
            .iter()
            .filter(|extension| !missing_extensions.contains(extension))
            .cloned()
            .collect::<Vec<_>>();

        vec![format!(
            "WARNING: mdformat requested extensions unavailable in resolved runtime ({runtime}): {missing}. Continuing with available extensions: {enabled}.",
            runtime = Self::mdformat_runtime_label(tool),
            missing = missing_extensions.join(", "),
            enabled = if enabled_extensions.is_empty() {
                "none".to_string()
            } else {
                enabled_extensions.join(", ")
            }
        )]
    }

    fn find_prettier_ignore_path(base_dir: &Path) -> Option<std::path::PathBuf> {
        let mut current = Some(base_dir);
        while let Some(dir) = current {
            let candidate = dir.join(".prettierignore");
            if candidate.exists() {
                return Some(candidate);
            }
            current = dir.parent();
        }
        None
    }

    fn append_prettier_ignore_path_arg(
        tool: &Tool,
        args: &mut Vec<String>,
        working_dir: Option<&Path>,
        args_start_index: usize,
    ) {
        if tool.name != "prettier" || args.iter().any(|arg| arg == "--ignore-path") {
            return;
        }

        let base_dir = working_dir.map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf()),
            Path::to_path_buf,
        );
        let base_dir = if base_dir.is_absolute() {
            base_dir
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| Path::new(".").to_path_buf())
                .join(base_dir)
        };

        if let Some(ignore_path) = Self::find_prettier_ignore_path(&base_dir) {
            let insert_index = args
                .iter()
                .enumerate()
                .skip(args_start_index)
                .position(|(_, arg)| !arg.starts_with('-') && arg != "--")
                .map_or(args.len(), |idx| idx + args_start_index);
            args.insert(insert_index, "--ignore-path".to_string());
            args.insert(insert_index + 1, ignore_path.display().to_string());
        }
    }

    fn remark_check_output_dir() -> Result<PathBuf, std::io::Error> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "clippier-remark-check-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn collect_remark_markdown_outputs(
        root: &Path,
        current: &Path,
        outputs: &mut Vec<PathBuf>,
    ) -> Result<(), std::io::Error> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::collect_remark_markdown_outputs(root, &path, outputs)?;
                continue;
            }

            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if extension != "md" && extension != "mdx" {
                continue;
            }

            if let Ok(relative) = path.strip_prefix(root) {
                outputs.push(relative.to_path_buf());
            }
        }

        Ok(())
    }

    fn append_or_replace_remark_output_arg(args: &mut Vec<String>, output_dir: &Path) {
        let output_value = output_dir.display().to_string();
        if let Some(index) = args.iter().position(|arg| arg == "--output" || arg == "-o") {
            if args.get(index + 1).is_none_or(|next| next.starts_with('-')) {
                args.insert(index + 1, output_value);
            } else {
                args[index + 1] = output_value;
            }
            return;
        }

        args.push("--output".to_string());
        args.push(output_value);
    }

    #[allow(clippy::too_many_lines)]
    fn run_remark_strict_check(&self, tool: &Tool, start_time: Instant) -> ToolResult {
        let args = &tool.format_args;
        if args.is_empty() {
            return ToolResult::success(
                tool.name.clone(),
                tool.display_name.clone(),
                Duration::ZERO,
            );
        }

        let working_dir = self.working_dir.map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf()),
            Path::to_path_buf,
        );

        let output_dir = match Self::remark_check_output_dir() {
            Ok(path) => path,
            Err(error) => {
                return ToolResult::failure(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    None,
                    String::new(),
                    format!("Failed to create temporary directory for remark check: {error}"),
                    start_time.elapsed(),
                );
            }
        };

        let (program, mut final_args) = match &tool.kind {
            ToolKind::Cargo => ("cargo".to_string(), args.clone()),
            ToolKind::Binary => {
                let binary = tool
                    .detected_path
                    .as_ref()
                    .map_or_else(|| tool.binary.clone(), |p| p.display().to_string());
                (binary, args.clone())
            }
            ToolKind::Runner {
                runner,
                runner_args,
            } => {
                let mut all_args = runner_args.clone();
                all_args.push(tool.binary.clone());
                all_args.extend(args.clone());
                (runner.clone(), all_args)
            }
        };

        Self::append_or_replace_remark_output_arg(&mut final_args, &output_dir);

        let mut command = Command::new(&program);
        command.args(&final_args);
        command.current_dir(&working_dir);
        Self::apply_color_env(&mut command, self.effective_color_mode());

        let output = match command.output() {
            Ok(output) => output,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&output_dir);
                return ToolResult::failure(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    None,
                    String::new(),
                    format!("Failed to execute strict remark check: {error}"),
                    start_time.elapsed(),
                );
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() {
            let _ = std::fs::remove_dir_all(&output_dir);
            return ToolResult::failure(
                tool.name.clone(),
                tool.display_name.clone(),
                output.status.code(),
                stdout,
                stderr,
                start_time.elapsed(),
            );
        }

        let mut generated = Vec::new();
        if let Err(error) =
            Self::collect_remark_markdown_outputs(&output_dir, &output_dir, &mut generated)
        {
            let _ = std::fs::remove_dir_all(&output_dir);
            return ToolResult::failure(
                tool.name.clone(),
                tool.display_name.clone(),
                None,
                stdout,
                format!("Failed to inspect strict remark output: {error}"),
                start_time.elapsed(),
            );
        }

        generated.sort();

        let mut changed = Vec::new();
        for relative in &generated {
            let formatted_path = output_dir.join(relative);
            let source_path = working_dir.join(relative);

            let formatted = std::fs::read(&formatted_path).ok();
            let source = std::fs::read(&source_path).ok();
            if formatted.as_deref() != source.as_deref() {
                changed.push(relative.display().to_string());
            }
        }

        let _ = std::fs::remove_dir_all(&output_dir);

        if changed.is_empty() {
            return ToolResult::success(
                tool.name.clone(),
                tool.display_name.clone(),
                start_time.elapsed(),
            );
        }

        let sample = changed
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  - ");
        let extra = if changed.len() > 10 {
            format!("\n  ... and {} more", changed.len() - 10)
        } else {
            String::new()
        };

        ToolResult::failure(
            tool.name.clone(),
            tool.display_name.clone(),
            Some(1),
            stdout,
            format!(
                "remark strict check found {} file(s) requiring formatting:\n  - {sample}{extra}",
                changed.len()
            ),
            start_time.elapsed(),
        )
    }

    #[cfg(feature = "tools-tui")]
    fn emit_tool_line_event(
        tx: &mpsc::Sender<ToolEvent>,
        tool_name: &str,
        is_stderr: bool,
        bytes: &[u8],
        overwrite: bool,
    ) {
        let line = String::from_utf8_lossy(bytes).to_string();
        let event = if is_stderr {
            ToolEvent::StderrLine {
                tool_name: tool_name.to_string(),
                line,
                overwrite,
            }
        } else {
            ToolEvent::StdoutLine {
                tool_name: tool_name.to_string(),
                line,
                overwrite,
            }
        };
        let _ = tx.send(event);
    }

    #[cfg(feature = "tools-tui")]
    fn pump_stream_events<R: Read>(
        mut reader: R,
        tx: &mpsc::Sender<ToolEvent>,
        tool_name: &str,
        is_stderr: bool,
        output: &Arc<Mutex<Vec<u8>>>,
    ) {
        let mut buffer = [0_u8; 4096];
        let mut line = Vec::new();
        let mut overwrite_next = false;
        let mut pending_cr = false;

        loop {
            let read_count = match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };

            if let Ok(mut captured) = output.lock() {
                captured.extend_from_slice(&buffer[..read_count]);
            }

            for byte in &buffer[..read_count] {
                if pending_cr {
                    if *byte == b'\n' {
                        Self::emit_tool_line_event(tx, tool_name, is_stderr, &line, false);
                        line.clear();
                        overwrite_next = false;
                        pending_cr = false;
                        continue;
                    }

                    Self::emit_tool_line_event(tx, tool_name, is_stderr, &line, true);
                    line.clear();
                    overwrite_next = true;
                    pending_cr = false;
                }

                match *byte {
                    b'\r' => {
                        pending_cr = true;
                    }
                    b'\n' => {
                        Self::emit_tool_line_event(tx, tool_name, is_stderr, &line, overwrite_next);
                        line.clear();
                        overwrite_next = false;
                    }
                    value => {
                        line.push(value);
                    }
                }
            }
        }

        if pending_cr {
            Self::emit_tool_line_event(tx, tool_name, is_stderr, &line, true);
            line.clear();
            overwrite_next = true;
        }

        if !line.is_empty() {
            Self::emit_tool_line_event(tx, tool_name, is_stderr, &line, overwrite_next);
        }
    }

    #[cfg(feature = "tools-tui")]
    #[allow(clippy::too_many_lines)]
    fn run_single_tool_with_events(
        &self,
        tool: &Tool,
        check_mode: bool,
        tx: &mpsc::Sender<ToolEvent>,
        cancel_requested: &Arc<AtomicBool>,
    ) -> ToolResult {
        let start_time = Instant::now();

        if check_mode && tool.name == "remark" {
            let _ = tx.send(ToolEvent::Started {
                tool_name: tool.name.clone(),
                display_name: tool.display_name.clone(),
            });
            let result = self.run_remark_strict_check(tool, start_time);
            for line in result.stdout.lines() {
                let _ = tx.send(ToolEvent::StdoutLine {
                    tool_name: tool.name.clone(),
                    line: line.to_string(),
                    overwrite: false,
                });
            }
            for line in result.stderr.lines() {
                let _ = tx.send(ToolEvent::StderrLine {
                    tool_name: tool.name.clone(),
                    line: line.to_string(),
                    overwrite: false,
                });
            }
            let _ = tx.send(ToolEvent::Finished {
                tool_name: tool.name.clone(),
                success: result.success,
            });
            return result;
        }

        let Some((program, final_args, warnings)) =
            Self::build_command_parts(tool, check_mode, self.working_dir)
        else {
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
        for warning in &warnings {
            let _ = tx.send(ToolEvent::StderrLine {
                tool_name: tool.name.clone(),
                line: warning.clone(),
                overwrite: false,
            });
        }

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
                let stdout_content = Arc::new(Mutex::new(Vec::<u8>::new()));
                let stderr_content = Arc::new(Mutex::new(Vec::<u8>::new()));

                let stdout_handle = child.stdout.take().map(|stdout| {
                    let tool_name = tool.name.clone();
                    let output = Arc::clone(&stdout_content);
                    let tx = tx.clone();
                    thread::spawn(move || {
                        Self::pump_stream_events(stdout, &tx, &tool_name, false, &output);
                    })
                });

                let stderr_handle = child.stderr.take().map(|stderr| {
                    let tool_name = tool.name.clone();
                    let output = Arc::clone(&stderr_content);
                    let tx = tx.clone();
                    thread::spawn(move || {
                        Self::pump_stream_events(stderr, &tx, &tool_name, true, &output);
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

                let stdout = stdout_content.lock().map_or_else(
                    |_| String::new(),
                    |buf| String::from_utf8_lossy(buf.as_slice()).to_string(),
                );
                let stderr = stderr_content.lock().map_or_else(
                    |_| String::new(),
                    |buf| String::from_utf8_lossy(buf.as_slice()).to_string(),
                );
                let warning_text = if warnings.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", warnings.join("\n"))
                };

                let duration = start_time.elapsed();
                let result = if status.success() {
                    ToolResult::success(tool.name.clone(), tool.display_name.clone(), duration)
                } else {
                    ToolResult::failure(
                        tool.name.clone(),
                        tool.display_name.clone(),
                        status.code(),
                        stdout,
                        format!("{warning_text}{stderr}"),
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

        if check_mode && tool.name == "remark" {
            return self.run_remark_strict_check(tool, start_time);
        }

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

        let (program, mut final_args, args_start_index) = match &tool.kind {
            ToolKind::Cargo => ("cargo".to_string(), args.clone(), 0),
            ToolKind::Binary => {
                let binary = tool
                    .detected_path
                    .as_ref()
                    .map_or_else(|| tool.binary.clone(), |p| p.display().to_string());
                (binary, args.clone(), 0)
            }
            ToolKind::Runner {
                runner,
                runner_args,
            } => {
                let mut all_args = runner_args.clone();
                all_args.push(tool.binary.clone());
                all_args.extend(args.clone());
                (runner.clone(), all_args, runner_args.len() + 1)
            }
        };
        Self::append_prettier_ignore_path_arg(
            tool,
            &mut final_args,
            self.working_dir,
            args_start_index,
        );
        let warnings = Self::append_mdformat_extension_args(
            tool,
            &mut final_args,
            self.working_dir,
            args_start_index,
        );
        let warning_text = if warnings.is_empty() {
            String::new()
        } else {
            format!("{}\n", warnings.join("\n"))
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
                    let mut stderr_content = warning_text;

                    for warning in &warnings {
                        eprintln!("{warning}");
                    }

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
                    let stderr =
                        format!("{warning_text}{}", String::from_utf8_lossy(&output.stderr));
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

        if check_mode && tool.name == "remark" {
            return self.run_remark_strict_check(tool, start_time);
        }

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

        let (program, mut final_args, args_start_index) = match &tool.kind {
            ToolKind::Cargo => ("cargo".to_string(), args.clone(), 0),
            ToolKind::Binary => {
                let binary = tool
                    .detected_path
                    .as_ref()
                    .map_or_else(|| tool.binary.clone(), |p| p.display().to_string());
                (binary, args.clone(), 0)
            }
            ToolKind::Runner {
                runner,
                runner_args,
            } => {
                let mut all_args = runner_args.clone();
                all_args.push(tool.binary.clone());
                all_args.extend(args.clone());
                (runner.clone(), all_args, runner_args.len() + 1)
            }
        };
        Self::append_prettier_ignore_path_arg(
            tool,
            &mut final_args,
            self.working_dir,
            args_start_index,
        );
        let warnings = Self::append_mdformat_extension_args(
            tool,
            &mut final_args,
            self.working_dir,
            args_start_index,
        );
        let warning_text = if warnings.is_empty() {
            String::new()
        } else {
            format!("{}\n", warnings.join("\n"))
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
                let stderr = format!("{warning_text}{}", String::from_utf8_lossy(&output.stderr));
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
    use crate::tools::{ToolCapability, ToolsConfig};
    #[cfg(feature = "tools-tui")]
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
        std::fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }

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

    #[test]
    fn prettier_ignore_path_is_inserted_before_positional_patterns() {
        let dir = temp_dir("clippier-prettier-arg-order");
        std::fs::write(dir.join(".prettierignore"), "target/\n")
            .expect("failed to write prettier ignore");

        let tool = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );

        let mut args = vec![
            "--check".to_string(),
            "--ignore-unknown".to_string(),
            ".".to_string(),
        ];
        ToolRunner::append_prettier_ignore_path_arg(&tool, &mut args, Some(&dir), 0);

        let pattern_index = args
            .iter()
            .position(|arg| arg == ".")
            .expect("missing positional pattern");
        let ignore_flag_index = args
            .iter()
            .position(|arg| arg == "--ignore-path")
            .expect("missing ignore-path flag");

        assert!(ignore_flag_index < pattern_index);
        assert!(
            args.get(ignore_flag_index + 1)
                .is_some_and(|v| v.ends_with(".prettierignore"))
        );

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn remark_strict_check_fails_when_formatted_output_differs() {
        let dir = temp_dir("clippier-remark-strict-check-fail");
        std::fs::write(dir.join("README.md"), "- bad\n").expect("failed to write README.md");

        let formatter_script = dir.join("remark_formatter.py");
        std::fs::write(
            &formatter_script,
            "import pathlib\nimport sys\nargs = sys.argv[1:]\nout = None\nfor i, arg in enumerate(args):\n    if arg in ('--output', '-o') and i + 1 < len(args):\n        out = args[i + 1]\n        break\nif out is None:\n    sys.exit(2)\nroot = pathlib.Path('.')\nfor path in root.rglob('*.md'):\n    rel = path.relative_to(root)\n    target = pathlib.Path(out) / rel\n    target.parent.mkdir(parents=True, exist_ok=True)\n    target.write_text(path.read_text().replace('bad', 'good'))\n",
        )
        .expect("failed to write remark formatter script");

        let registry = ToolRegistry::new(ToolsConfig::default(), Some(&dir))
            .expect("failed to create registry");
        let runner = ToolRunner::new(&registry).with_working_dir(&dir);

        let tool = Tool::new(
            "remark",
            "remark",
            "python3",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![".".to_string()],
            vec![
                formatter_script.display().to_string(),
                ".".to_string(),
                "--output".to_string(),
                "--ext".to_string(),
                "md,mdx".to_string(),
            ],
        );

        let result = runner.run_remark_strict_check(&tool, Instant::now());
        assert!(!result.success);
        assert!(result.stderr.contains("requiring formatting"));
        assert!(result.stderr.contains("README.md"));

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[test]
    fn remark_strict_check_passes_when_formatted_output_matches() {
        let dir = temp_dir("clippier-remark-strict-check-pass");
        std::fs::write(dir.join("README.md"), "- stable\n").expect("failed to write README.md");

        let formatter_script = dir.join("remark_formatter.py");
        std::fs::write(
            &formatter_script,
            "import pathlib\nimport sys\nargs = sys.argv[1:]\nout = None\nfor i, arg in enumerate(args):\n    if arg in ('--output', '-o') and i + 1 < len(args):\n        out = args[i + 1]\n        break\nif out is None:\n    sys.exit(2)\nroot = pathlib.Path('.')\nfor path in root.rglob('*.md'):\n    rel = path.relative_to(root)\n    target = pathlib.Path(out) / rel\n    target.parent.mkdir(parents=True, exist_ok=True)\n    target.write_text(path.read_text())\n",
        )
        .expect("failed to write remark formatter script");

        let registry = ToolRegistry::new(ToolsConfig::default(), Some(&dir))
            .expect("failed to create registry");
        let runner = ToolRunner::new(&registry).with_working_dir(&dir);

        let tool = Tool::new(
            "remark",
            "remark",
            "python3",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![".".to_string()],
            vec![
                formatter_script.display().to_string(),
                ".".to_string(),
                "--output".to_string(),
                "--ext".to_string(),
                "md,mdx".to_string(),
            ],
        );

        let result = runner.run_remark_strict_check(&tool, Instant::now());
        assert!(result.success);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
    }

    #[cfg(feature = "tools-tui")]
    #[test]
    fn pump_stream_events_treats_crlf_as_single_newline() {
        let (tx, rx) = mpsc::channel();
        let output = Arc::new(Mutex::new(Vec::new()));

        ToolRunner::pump_stream_events(
            Cursor::new(b"hello\r\nworld\n".to_vec()),
            &tx,
            "tool",
            false,
            &output,
        );

        let events: Vec<ToolEvent> = rx.try_iter().collect();
        assert_eq!(events.len(), 2);

        match &events[0] {
            ToolEvent::StdoutLine {
                line, overwrite, ..
            } => {
                assert_eq!(line, "hello");
                assert!(!overwrite);
            }
            _ => panic!("unexpected event kind"),
        }

        match &events[1] {
            ToolEvent::StdoutLine {
                line, overwrite, ..
            } => {
                assert_eq!(line, "world");
                assert!(!overwrite);
            }
            _ => panic!("unexpected event kind"),
        }
    }

    #[cfg(feature = "tools-tui")]
    #[test]
    fn pump_stream_events_marks_overwrite_on_carriage_return_updates() {
        let (tx, rx) = mpsc::channel();
        let output = Arc::new(Mutex::new(Vec::new()));

        ToolRunner::pump_stream_events(
            Cursor::new(b"a\rb\n".to_vec()),
            &tx,
            "tool",
            false,
            &output,
        );

        let events: Vec<ToolEvent> = rx.try_iter().collect();
        assert_eq!(events.len(), 2);

        match &events[0] {
            ToolEvent::StdoutLine {
                line, overwrite, ..
            } => {
                assert_eq!(line, "a");
                assert!(*overwrite);
            }
            _ => panic!("unexpected event kind"),
        }

        match &events[1] {
            ToolEvent::StdoutLine {
                line, overwrite, ..
            } => {
                assert_eq!(line, "b");
                assert!(*overwrite);
            }
            _ => panic!("unexpected event kind"),
        }
    }
}
