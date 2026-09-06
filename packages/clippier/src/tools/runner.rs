//! Tool execution and result aggregation.

use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
#[cfg(feature = "format")]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(feature = "tools-tui")]
use std::io::Read;
#[cfg(feature = "tools-tui")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "tools-tui")]
use std::sync::{Arc, Mutex, OnceLock, mpsc};
#[cfg(feature = "tools-tui")]
use std::thread;

use rayon::prelude::*;

use crate::ColorMode;
use crate::tools::registry::{ToolError, ToolRegistry};
use crate::tools::scope::{EffectiveScope, ScopeMatcher};
#[cfg(feature = "tools-tui")]
use crate::tools::tui;
use crate::tools::types::{Tool, ToolKind};
use crate::tools::{
    FormatSelection, ToolAdapter, ToolPlan, default_extensions_for_tool, tool_catalog_entry,
};

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
    /// Number of files selected before per-tool extension filtering.
    pub selected_file_count: Option<usize>,
    /// Whether Git-aware selection fell back to all files.
    pub selection_fallback: bool,
    /// Effective execution mode by selected tool.
    pub execution_modes: BTreeMap<String, String>,
    /// Selection evidence by automatically planned tool.
    pub selection_evidence: BTreeMap<String, String>,
    /// Formatter ownership extensions by tool.
    pub formatter_ownership: BTreeMap<String, BTreeSet<String>>,
    /// Explicit formatter pipeline order by tool.
    pub format_order: BTreeMap<String, i32>,
    /// Number of planned files per selected tool.
    pub planned_file_counts: BTreeMap<String, usize>,
    /// Deterministic command-scoped inventory/planning diagnostics.
    pub inventory_diagnostics: crate::tools::InventoryDiagnostics,
    /// Effective automatic exclusion patterns.
    pub automatic_exclusions: Vec<String>,
    /// Relevant tools which were unavailable during automatic planning.
    pub unavailable_tools: Vec<String>,
    /// Formatter overlap decisions which still require user attention.
    pub overlap_warnings: Vec<String>,
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

#[derive(Debug, Clone, Default)]
struct ResultPlanMetadata {
    execution_modes: BTreeMap<String, String>,
    selection_evidence: BTreeMap<String, String>,
    formatter_ownership: BTreeMap<String, BTreeSet<String>>,
    format_order: BTreeMap<String, i32>,
    planned_file_counts: BTreeMap<String, usize>,
    inventory_diagnostics: crate::tools::InventoryDiagnostics,
    automatic_exclusions: Vec<String>,
    unavailable_tools: Vec<String>,
    overlap_warnings: Vec<String>,
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
    /// Global repository scope shared by file-oriented tools.
    scope: Option<ScopeMatcher>,
    /// Per-tool include and exclusion scopes.
    tool_scopes: BTreeMap<String, ScopeMatcher>,
    /// Resolved formatter file selection.
    format_selection: FormatSelection,
    /// Automatic formatter ownership by tool.
    formatter_ownership: Option<BTreeMap<String, BTreeSet<String>>>,
    /// Effective extensions resolved by planning for each selected tool.
    effective_extensions: BTreeMap<String, BTreeSet<String>>,
    /// Selection evidence by automatically planned tool.
    selection_evidence: BTreeMap<String, String>,
    /// Explicit formatter pipeline order by tool.
    format_order: BTreeMap<String, i32>,
    /// Actual repository-relative files planned per tool.
    planned_files: BTreeMap<String, BTreeSet<PathBuf>>,
    /// Deterministic command-scoped inventory/planning diagnostics.
    inventory_diagnostics: crate::tools::InventoryDiagnostics,
    /// Effective automatic exclusion patterns.
    automatic_exclusions: Vec<String>,
    /// Relevant tools unavailable during automatic planning.
    unavailable_tools: Vec<String>,
    /// Formatter overlap decisions emitted for the execution plan.
    overlap_warnings: Vec<String>,
    /// Whether Git-aware selection fell back to all files.
    selection_fallback: bool,
}

impl<'a> ToolRunner<'a> {
    /// Creates a new tool runner (parallel by default)
    #[must_use]
    pub fn new(registry: &'a ToolRegistry) -> Self {
        let scope_root = registry
            .config()
            .scope_base
            .as_deref()
            .unwrap_or_else(|| registry.working_dir());
        let effective_scope =
            EffectiveScope::resolve(scope_root, registry.config().effective_scope());
        let scope = if effective_scope.exclusions.is_empty() {
            None
        } else {
            effective_scope
                .matcher(scope_root)
                .map_err(|error| log::error!("failed to build runner scope: {error}"))
                .ok()
        };
        let tool_scopes = registry
            .config()
            .tools
            .iter()
            .filter(|(_, policy)| !policy.include.is_empty() || !policy.exclude.is_empty())
            .filter_map(|(name, policy)| {
                ScopeMatcher::with_patterns(scope_root, &policy.include, &policy.exclude)
                    .map(|matcher| (name.clone(), matcher))
                    .map_err(|error| {
                        log::error!("failed to build scope for tool '{name}': {error}");
                    })
                    .ok()
            })
            .collect();
        Self {
            registry,
            working_dir: None,
            stream_output: true,
            parallel: true,
            color_mode: ColorMode::Auto,
            scope,
            tool_scopes,
            format_selection: FormatSelection::All,
            formatter_ownership: None,
            effective_extensions: BTreeMap::new(),
            selection_evidence: BTreeMap::new(),
            format_order: BTreeMap::new(),
            planned_files: BTreeMap::new(),
            inventory_diagnostics: crate::tools::InventoryDiagnostics::default(),
            automatic_exclusions: effective_scope.exclusions,
            unavailable_tools: Vec::new(),
            overlap_warnings: Vec::new(),
            selection_fallback: false,
        }
    }

    /// Sets the working directory for tool execution
    #[must_use]
    pub const fn with_working_dir(mut self, dir: &'a Path) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// Sets the resolved formatter file selection.
    #[must_use]
    pub fn with_format_selection(mut self, selection: FormatSelection) -> Self {
        self.format_selection = selection;
        self
    }

    /// Sets formatter ownership from an automatic tool plan.
    #[must_use]
    pub fn with_tool_plan(mut self, plan: &ToolPlan) -> Self {
        self.formatter_ownership = Some(
            plan.tools
                .iter()
                .map(|tool| (tool.name.clone(), tool.format_extensions.clone()))
                .collect(),
        );
        self.effective_extensions
            .clone_from(&plan.effective_extensions);
        self.selection_evidence = plan
            .tools
            .iter()
            .map(|tool| {
                (
                    tool.name.clone(),
                    format!("{:?}:{}", tool.evidence.kind, tool.evidence.path.display()),
                )
            })
            .collect();
        self.format_order = plan
            .tools
            .iter()
            .filter_map(|tool| tool.format_order.map(|order| (tool.name.clone(), order)))
            .collect();
        self.planned_files = plan
            .tools
            .iter()
            .map(|tool| (tool.name.clone(), tool.files.clone()))
            .collect();
        self.inventory_diagnostics = plan.diagnostics.clone();
        self.automatic_exclusions
            .clone_from(&plan.automatic_exclusions);
        self.unavailable_tools.clone_from(&plan.unavailable);
        self
    }

    /// Sets actual planned files independently of automatic selection metadata.
    #[must_use]
    pub fn with_planned_files(mut self, files: BTreeMap<String, BTreeSet<PathBuf>>) -> Self {
        self.planned_files = files;
        self
    }

    /// Sets overlap decisions for machine-readable execution reporting.
    #[must_use]
    pub fn with_overlap_warnings(mut self, warnings: Vec<String>) -> Self {
        self.overlap_warnings = warnings;
        self
    }

    /// Records that Git-aware formatting fell back to all-files selection.
    #[must_use]
    pub const fn with_selection_fallback(mut self, fallback: bool) -> Self {
        self.selection_fallback = fallback;
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

    fn selected_file_count(&self) -> Option<usize> {
        match &self.format_selection {
            FormatSelection::Files(files) => Some(files.len()),
            FormatSelection::All | FormatSelection::NoRepository => None,
        }
    }

    fn result_metadata(&self, tools: &[&Tool]) -> ResultPlanMetadata {
        let execution_modes = tools
            .iter()
            .filter_map(|tool| {
                self.registry
                    .execution_mode(&tool.name)
                    .map(|mode| (tool.name.clone(), mode))
            })
            .collect();
        ResultPlanMetadata {
            execution_modes,
            selection_evidence: self.selection_evidence.clone(),
            formatter_ownership: self.formatter_ownership.clone().unwrap_or_default(),
            format_order: self.format_order.clone(),
            planned_file_counts: self
                .planned_files
                .iter()
                .map(|(name, files)| (name.clone(), files.len()))
                .collect(),
            inventory_diagnostics: self.inventory_diagnostics.clone(),
            automatic_exclusions: self.automatic_exclusions.clone(),
            unavailable_tools: self.unavailable_tools.clone(),
            overlap_warnings: self.overlap_warnings.clone(),
        }
    }

    fn aggregate_results(
        &self,
        tools: &[&Tool],
        results: Vec<ToolResult>,
        total_duration: Duration,
    ) -> AggregatedResults {
        let success_count = results.iter().filter(|result| result.success).count();
        let failure_count = results.len() - success_count;
        let metadata = self.result_metadata(tools);
        AggregatedResults {
            results,
            total_duration,
            success_count,
            failure_count,
            selected_file_count: self.selected_file_count(),
            selection_fallback: self.selection_fallback,
            execution_modes: metadata.execution_modes,
            selection_evidence: metadata.selection_evidence,
            formatter_ownership: metadata.formatter_ownership,
            format_order: metadata.format_order,
            planned_file_counts: metadata.planned_file_counts,
            inventory_diagnostics: metadata.inventory_diagnostics,
            automatic_exclusions: metadata.automatic_exclusions,
            unavailable_tools: metadata.unavailable_tools,
            overlap_warnings: metadata.overlap_warnings,
        }
    }

    fn empty_results(&self) -> AggregatedResults {
        self.aggregate_results(&[], Vec::new(), Duration::ZERO)
    }

    fn working_dir_path(&self) -> PathBuf {
        self.working_dir.map_or_else(
            || self.registry.working_dir().to_path_buf(),
            Path::to_path_buf,
        )
    }

    /// Produces successful empty results for an automatic plan with no installed
    /// selected tools.
    #[must_use]
    pub fn automatic_noop_results(&self) -> AggregatedResults {
        self.empty_results()
    }

    /// Returns the scoped file arguments that would be passed to a tool.
    #[must_use]
    pub fn scoped_files_for(&self, tool: &Tool) -> Option<Vec<String>> {
        self.scoped_file_args(tool)
    }

    fn scoped_file_args(&self, tool: &Tool) -> Option<Vec<String>> {
        if let Some(planned) = self.planned_files.get(&tool.name) {
            let working_dir = self.working_dir_path();
            let tool_scope = self.tool_scopes.get(&tool.name);
            return Some(
                planned
                    .iter()
                    .filter(|path| match &self.format_selection {
                        FormatSelection::Files(selected) => selected.contains(*path),
                        FormatSelection::All | FormatSelection::NoRepository => true,
                    })
                    .filter(|path| {
                        let absolute = working_dir.join(path);
                        self.scope
                            .as_ref()
                            .is_none_or(|scope| !scope.is_excluded(&absolute))
                            && tool_scope.is_none_or(|scope| !scope.is_excluded(&absolute))
                    })
                    .map(|path| path.to_string_lossy().to_string())
                    .collect(),
            );
        }
        if matches!(tool.kind, ToolKind::Cargo)
            && !matches!(
                tool_catalog_entry(&tool.name).map(|entry| entry.adapter()),
                Some(ToolAdapter::ClippierMarkdown | ToolAdapter::Rustfmt)
            )
        {
            return None;
        }
        let mut extensions = self
            .effective_extensions
            .get(&tool.name)
            .cloned()
            .or_else(|| {
                self.formatter_ownership
                    .as_ref()
                    .and_then(|ownership| ownership.get(&tool.name))
                    .cloned()
            })
            .unwrap_or_default();
        for capability in &tool.capabilities {
            if *capability == crate::tools::ToolCapability::Format
                && self.formatter_ownership.is_some()
            {
                continue;
            }
            extensions.extend(default_extensions_for_tool(&tool.name, *capability));
        }
        if extensions.is_empty() {
            return None;
        }
        let working_dir = self.working_dir_path();
        let tool_scope = self.tool_scopes.get(&tool.name);
        let is_excluded = |path: &Path| {
            self.scope
                .as_ref()
                .is_some_and(|scope| scope.is_excluded(path))
                || tool_scope.is_some_and(|scope| scope.is_excluded(path))
        };
        let paths = match &self.format_selection {
            FormatSelection::Files(files) => files
                .iter()
                .filter(|path| {
                    extensions
                        .iter()
                        .any(|key| crate::tools::catalog::matches_format(path, key))
                })
                .filter(|path| !is_excluded(&working_dir.join(path)))
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            FormatSelection::All | FormatSelection::NoRepository => {
                let default_scope;
                let collection_scope = if let Some(tool_scope) = tool_scope {
                    tool_scope
                } else if let Some(scope) = self.scope.as_ref() {
                    scope
                } else {
                    default_scope = ScopeMatcher::new(&working_dir, &[]).ok()?;
                    &default_scope
                };
                collection_scope
                    .collect_files(std::slice::from_ref(&working_dir), &extensions)
                    .into_iter()
                    .filter(|path| !is_excluded(path))
                    .filter_map(|path| {
                        path.strip_prefix(&working_dir)
                            .ok()
                            .map(|relative| relative.to_string_lossy().to_string())
                    })
                    .collect()
            }
        };
        Some(paths)
    }

    fn replace_default_path_args(tool: &Tool, args: &mut Vec<String>, files: &[String]) {
        if files.is_empty()
            || !tool_catalog_entry(&tool.name)
                .is_some_and(|entry| entry.uses_scoped_file_arguments())
        {
            return;
        }

        if tool_catalog_entry(&tool.name)
            .is_some_and(|entry| entry.adapter() == ToolAdapter::DotnetFormat)
        {
            args.push("--include".to_owned());
            args.extend(files.iter().cloned());
            return;
        }
        if tool_catalog_entry(&tool.name).is_some_and(|entry| entry.adapter() == ToolAdapter::Buf) {
            for file in files {
                args.push("--path".to_owned());
                args.push(file.clone());
            }
            return;
        }
        let entry = tool_catalog_entry(&tool.name).expect("catalog scoped tool");
        args.retain(|arg| !entry.replaced_path_args.contains(&arg.as_str()));
        args.extend(files.iter().cloned());
    }

    /// Captures both pipes concurrently and always reaps a cancelled child.
    fn capture_process(
        command: &mut Command,
        cancelled: &dyn Fn() -> bool,
    ) -> std::io::Result<std::process::Output> {
        Self::execute_process(command, cancelled, &|_, _| {})
    }

    fn execute_process(
        command: &mut Command,
        cancelled: &dyn Fn() -> bool,
        output: &(dyn Fn(bool, &[u8]) + Sync),
    ) -> std::io::Result<std::process::Output> {
        if cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Execution cancelled",
            ));
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        std::thread::scope(|scope| {
            let stdout = child.stdout.take().expect("piped stdout");
            let stderr = child.stderr.take().expect("piped stderr");
            let out = scope.spawn(move || Self::drain_process_pipe(stdout, false, output));
            let err = scope.spawn(move || Self::drain_process_pipe(stderr, true, output));
            let status = loop {
                if cancelled() {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Execution cancelled",
                    ));
                }
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(error);
                    }
                }
            };
            let stdout = out
                .join()
                .map_err(|_| std::io::Error::other("stdout reader panicked"))??;
            let stderr = err
                .join()
                .map_err(|_| std::io::Error::other("stderr reader panicked"))??;
            Ok(std::process::Output {
                status: status?,
                stdout,
                stderr,
            })
        })
    }

    fn drain_process_pipe(
        mut pipe: impl std::io::Read,
        stderr: bool,
        output: &(dyn Fn(bool, &[u8]) + Sync),
    ) -> std::io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let count = match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            output(stderr, &chunk[..count]);
            data.extend_from_slice(&chunk[..count]);
        }
        Ok(data)
    }

    fn directory_groups(files: &[String], filename_filter: &str) -> BTreeMap<PathBuf, Vec<String>> {
        let mut modules = BTreeMap::<PathBuf, Vec<String>>::new();
        for file in files {
            let path = Path::new(file);
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                modules
                    .entry(path.parent().unwrap_or_else(|| Path::new("")).to_path_buf())
                    .or_default()
                    .push(format!("{filename_filter}{name}"));
            }
        }
        modules
    }

    fn run_directory_scoped(
        &self,
        tool: &Tool,
        filename_filter: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> ToolResult {
        use std::fmt::Write as _;
        let start = Instant::now();
        let files = self.scoped_file_args(tool).unwrap_or_default();
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut success = true;
        let mut exit_code = Some(0);
        let entry = tool_catalog_entry(&tool.name).expect("catalog directory-scoped tool");
        for (directory, filters) in Self::directory_groups(&files, filename_filter) {
            if cancelled() {
                success = false;
                exit_code = None;
                stderr.push_str("Execution cancelled\n");
                break;
            }
            let mut command = Command::new(
                tool.detected_path
                    .as_deref()
                    .unwrap_or_else(|| Path::new(&tool.binary)),
            );
            command.current_dir(self.working_dir_path().join(&directory));
            command
                .args(
                    tool.check_args
                        .iter()
                        .filter(|arg| !entry.replaced_path_args.contains(&arg.as_str())),
                )
                .args(filters);
            match Self::capture_process(&mut command, cancelled) {
                Ok(output) => {
                    let _ = write!(
                        stdout,
                        "Module: {}\n{}",
                        directory.display(),
                        String::from_utf8_lossy(&output.stdout)
                    );
                    stderr.push_str(&String::from_utf8_lossy(&output.stderr));
                    if !output.status.success() {
                        success = false;
                        exit_code = output.status.code();
                    }
                }
                Err(error) => {
                    success = false;
                    exit_code = None;
                    let _ = writeln!(stderr, "Module {}: {error}", directory.display());
                }
            }
        }
        ToolResult {
            tool_name: tool.name.clone(),
            display_name: tool.display_name.clone(),
            success,
            exit_code,
            stdout,
            stderr,
            duration: start.elapsed(),
        }
    }

    fn command_succeeded(tool: &Tool, check_mode: bool, status_ok: bool, stdout: &str) -> bool {
        status_ok
            && !(check_mode
                && tool_catalog_entry(&tool.name).is_some_and(|entry| entry.check_uses_stdout())
                && !stdout.trim().is_empty())
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
        if tools.iter().any(|tool| {
            tool_catalog_entry(&tool.name).is_some_and(|entry| entry.uses_scoped_file_arguments())
                && !self.planned_files.contains_key(&tool.name)
        }) {
            return Err(ToolError::DetectionFailed(
                "inventory".to_string(),
                "file-oriented tool execution requires a resolved repository plan".to_string(),
            ));
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
        if tools.iter().any(|tool| {
            tool_catalog_entry(&tool.name).is_some_and(|entry| entry.uses_scoped_file_arguments())
                && !self.planned_files.contains_key(&tool.name)
        }) {
            return Err(ToolError::DetectionFailed(
                "inventory".to_string(),
                "file-oriented tool execution requires a resolved repository plan".to_string(),
            ));
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
        if matches!(&self.format_selection, FormatSelection::Files(files) if files.is_empty()) {
            return self.empty_results();
        }
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

        self.aggregate_results(tools, results, start_time.elapsed())
    }

    #[cfg(feature = "tools-tui")]
    fn run_tools_with_tui(
        &self,
        tools: &[&Tool],
        _paths: &[&str],
        check_mode: bool,
    ) -> AggregatedResults {
        if matches!(&self.format_selection, FormatSelection::Files(files) if files.is_empty()) {
            return self.empty_results();
        }
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

        self.aggregate_results(tools, results, start_time.elapsed())
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
        &self,
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

        if let Some(files) = self.scoped_file_args(tool) {
            if files.is_empty() {
                return None;
            }
            Self::replace_default_path_args(tool, &mut parts.1, &files);
        }
        Self::append_prettier_ignore_path_arg(tool, &mut parts.1, working_dir, args_start_index);
        let warnings =
            Self::append_mdformat_extension_args(tool, &mut parts.1, working_dir, args_start_index);

        Some((parts.0, parts.1, warnings))
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
        _working_dir: Option<&Path>,
        args_start_index: usize,
    ) -> Vec<String> {
        if tool_catalog_entry(&tool.name).map(|entry| entry.adapter())
            != Some(ToolAdapter::Mdformat)
        {
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

        let requested_extensions = &tool.native_requested_extensions;
        if requested_extensions.is_empty() {
            return Vec::new();
        }

        let mut extension_args = Vec::new();
        let mut missing_extensions = Vec::new();
        for extension in requested_extensions {
            if tool.native_supported_extensions.contains(extension) {
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

    fn append_prettier_ignore_path_arg(
        tool: &Tool,
        args: &mut Vec<String>,
        _working_dir: Option<&Path>,
        args_start_index: usize,
    ) {
        if tool_catalog_entry(&tool.name).map(|entry| entry.adapter())
            != Some(ToolAdapter::Prettier)
            || args.iter().any(|arg| arg == "--ignore-path")
        {
            return;
        }

        if let Some(ignore_path) = &tool.native_ignore_path {
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

            let Some(extension) = crate::tools::catalog::file_format(&path) else {
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

        if let Some(files) = self.scoped_file_args(tool) {
            if files.is_empty() {
                let _ = std::fs::remove_dir_all(&output_dir);
                return ToolResult::success(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    start_time.elapsed(),
                );
            }
            Self::replace_default_path_args(tool, &mut final_args, &files);
        }
        Self::append_or_replace_remark_output_arg(&mut final_args, &output_dir);

        let mut command = Command::new(&program);
        command.args(&final_args);
        command.current_dir(&working_dir);
        Self::apply_color_env(&mut command, self.effective_color_mode());

        let output = match Self::capture_process(&mut command, &|| false) {
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
        if check_mode
            && let Some(crate::tools::catalog::ExecutionScope::Directory { filename_filter }) =
                tool_catalog_entry(&tool.name).map(|entry| entry.execution_scope)
        {
            let _ = tx.send(ToolEvent::Started {
                tool_name: tool.name.clone(),
                display_name: tool.display_name.clone(),
            });
            let result = self.run_directory_scoped(tool, filename_filter, &|| {
                cancel_requested.load(Ordering::SeqCst)
            });
            for line in result.stdout.lines() {
                let _ = tx.send(ToolEvent::StdoutLine {
                    tool_name: tool.name.clone(),
                    line: line.to_owned(),
                    overwrite: false,
                });
            }
            for line in result.stderr.lines() {
                let _ = tx.send(ToolEvent::StderrLine {
                    tool_name: tool.name.clone(),
                    line: line.to_owned(),
                    overwrite: false,
                });
            }
            let _ = tx.send(ToolEvent::Finished {
                tool_name: tool.name.clone(),
                success: result.success,
            });
            return result;
        }
        let start_time = Instant::now();

        #[cfg(feature = "format")]
        if let Some(files) = self.selected_rust_files(tool) {
            let _ = tx.send(ToolEvent::Started {
                tool_name: tool.name.clone(),
                display_name: tool.display_name.clone(),
            });
            let result = self.run_selected_rustfmt(tool, &files, check_mode, start_time);
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

        if check_mode
            && tool_catalog_entry(&tool.name).map(|entry| entry.adapter())
                == Some(ToolAdapter::Remark)
        {
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
            self.build_command_parts(tool, check_mode, self.working_dir)
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
                let result = if Self::command_succeeded(tool, check_mode, status.success(), &stdout)
                {
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

    #[cfg(feature = "format")]
    fn resolve_rustfmt() -> Result<PathBuf, String> {
        if let Some(value) = std::env::var_os("RUSTFMT") {
            let configured = PathBuf::from(value);
            return if configured.is_file() {
                Ok(configured)
            } else {
                which::which(&configured).map_err(|error| {
                    format!(
                        "RUSTFMT points to '{}', but it could not be executed: {error}",
                        configured.display()
                    )
                })
            };
        }

        which::which("rustfmt").map_err(|error| {
            format!("rustfmt was not found; install the rustfmt component or set RUSTFMT: {error}")
        })
    }

    #[cfg(feature = "format")]
    fn write_formatted_source_atomically(path: &Path, source: &[u8]) -> std::io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let permissions = std::fs::metadata(path)?.permissions();
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(source)?;
        temporary.as_file().sync_all()?;
        temporary.as_file().set_permissions(permissions)?;
        temporary
            .persist(path)
            .map_err(|error| error.error)
            .map(|_| ())
    }

    #[cfg(feature = "format")]
    #[allow(clippy::too_many_lines)]
    fn run_selected_rustfmt(
        &self,
        tool: &Tool,
        files: &[String],
        check_mode: bool,
        start_time: Instant,
    ) -> ToolResult {
        let working_dir = self.working_dir_path();
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(&working_dir)
            .no_deps()
            .exec();
        let metadata = match metadata {
            Ok(metadata) => metadata,
            Err(error) => {
                return ToolResult::failure(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    None,
                    String::new(),
                    format!("Failed to resolve Cargo metadata for selected Rust files: {error}"),
                    start_time.elapsed(),
                );
            }
        };

        let rustfmt = match Self::resolve_rustfmt() {
            Ok(rustfmt) => rustfmt,
            Err(error) => {
                return ToolResult::failure(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    None,
                    String::new(),
                    error,
                    start_time.elapsed(),
                );
            }
        };
        let mut mismatched = Vec::new();
        let mut errors = Vec::new();

        for relative in files {
            let path = working_dir.join(relative);
            let source = match std::fs::read(&path) {
                Ok(source) => source,
                Err(error) => {
                    errors.push(format!("{relative}: failed to read file: {error}"));
                    continue;
                }
            };
            let absolute = path.canonicalize().unwrap_or_else(|_| path.clone());
            let edition = metadata
                .packages
                .iter()
                .filter(|package| {
                    package
                        .manifest_path
                        .parent()
                        .is_some_and(|root| absolute.starts_with(root.as_std_path()))
                })
                .max_by_key(|package| package.manifest_path.as_str().len())
                .map_or_else(|| "2015".to_string(), |package| package.edition.to_string());

            let mut command = Command::new(&rustfmt);
            command
                .arg("--emit")
                .arg("stdout")
                .arg("--edition")
                .arg(&edition);
            if let Some(config_path) = Self::find_file_in_ancestors(
                path.parent().unwrap_or(&working_dir),
                &["rustfmt.toml", ".rustfmt.toml"],
            ) {
                command.arg("--config-path").arg(config_path);
            }
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    errors.push(format!("{relative}: failed to spawn rustfmt: {error}"));
                    continue;
                }
            };
            if let Some(mut stdin) = child.stdin.take()
                && let Err(error) = stdin.write_all(&source)
            {
                errors.push(format!(
                    "{relative}: failed to send source to rustfmt: {error}"
                ));
                continue;
            }
            let output = match child.wait_with_output() {
                Ok(output) => output,
                Err(error) => {
                    errors.push(format!("{relative}: failed to wait for rustfmt: {error}"));
                    continue;
                }
            };
            if !output.status.success() {
                errors.push(format!(
                    "{relative}: rustfmt failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
                continue;
            }
            if output.stdout != source {
                if check_mode {
                    mismatched.push(relative.clone());
                } else if let Err(error) =
                    Self::write_formatted_source_atomically(&path, &output.stdout)
                {
                    errors.push(format!(
                        "{relative}: failed to atomically replace formatted source: {error}"
                    ));
                }
            }
        }

        if errors.is_empty() && mismatched.is_empty() {
            ToolResult::success(
                tool.name.clone(),
                tool.display_name.clone(),
                start_time.elapsed(),
            )
        } else {
            if !mismatched.is_empty() {
                errors.push(format!(
                    "Rust formatting required for:\n  - {}",
                    mismatched.join("\n  - ")
                ));
            }
            ToolResult::failure(
                tool.name.clone(),
                tool.display_name.clone(),
                Some(1),
                String::new(),
                errors.join("\n"),
                start_time.elapsed(),
            )
        }
    }

    #[cfg(feature = "format")]
    fn selected_rust_files(&self, tool: &Tool) -> Option<Vec<String>> {
        if tool_catalog_entry(&tool.name).map(|entry| entry.adapter()) != Some(ToolAdapter::Rustfmt)
        {
            return None;
        }
        self.scoped_file_args(tool)
    }

    /// Runs a single tool.
    #[allow(clippy::too_many_lines)]
    fn run_single_tool(&self, tool: &Tool, check_mode: bool) -> ToolResult {
        if check_mode
            && let Some(crate::tools::catalog::ExecutionScope::Directory { filename_filter }) =
                tool_catalog_entry(&tool.name).map(|entry| entry.execution_scope)
        {
            return self.run_directory_scoped(tool, filename_filter, &|| false);
        }
        let start_time = Instant::now();

        #[cfg(feature = "format")]
        if let Some(files) = self.selected_rust_files(tool) {
            return self.run_selected_rustfmt(tool, &files, check_mode, start_time);
        }

        if check_mode
            && tool_catalog_entry(&tool.name).map(|entry| entry.adapter())
                == Some(ToolAdapter::Remark)
        {
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
        if let Some(files) = self.scoped_file_args(tool) {
            if files.is_empty() {
                return ToolResult::success(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    start_time.elapsed(),
                );
            }
            Self::replace_default_path_args(tool, &mut final_args, &files);
        }
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

        let sink = |stderr: bool, bytes: &[u8]| {
            use std::io::Write as _;
            if self.stream_output {
                if stderr {
                    let mut output = std::io::stderr().lock();
                    let _ = output.write_all(bytes);
                    let _ = output.flush();
                } else {
                    let mut output = std::io::stdout().lock();
                    let _ = output.write_all(bytes);
                    let _ = output.flush();
                }
            }
        };
        // Capture all output at once
        match Self::execute_process(&mut command, &|| false, &sink) {
            Ok(output) => {
                let duration = start_time.elapsed();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = format!("{warning_text}{}", String::from_utf8_lossy(&output.stderr));
                let exit_code = output.status.code();

                if Self::command_succeeded(tool, check_mode, output.status.success(), &stdout) {
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

    /// Runs a single tool with buffered output (for parallel execution)
    #[allow(clippy::too_many_lines)]
    fn run_single_tool_buffered(&self, tool: &Tool, check_mode: bool) -> ToolResult {
        if check_mode
            && let Some(crate::tools::catalog::ExecutionScope::Directory { filename_filter }) =
                tool_catalog_entry(&tool.name).map(|entry| entry.execution_scope)
        {
            return self.run_directory_scoped(tool, filename_filter, &|| false);
        }
        let start_time = Instant::now();

        #[cfg(feature = "format")]
        if let Some(files) = self.selected_rust_files(tool) {
            return self.run_selected_rustfmt(tool, &files, check_mode, start_time);
        }

        if check_mode
            && tool_catalog_entry(&tool.name).map(|entry| entry.adapter())
                == Some(ToolAdapter::Remark)
        {
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
        if let Some(files) = self.scoped_file_args(tool) {
            if files.is_empty() {
                return ToolResult::success(
                    tool.name.clone(),
                    tool.display_name.clone(),
                    start_time.elapsed(),
                );
            }
            Self::replace_default_path_args(tool, &mut final_args, &files);
        }
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
        match Self::capture_process(&mut command, &|| false) {
            Ok(output) => {
                let duration = start_time.elapsed();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = format!("{warning_text}{}", String::from_utf8_lossy(&output.stderr));
                let exit_code = output.status.code();

                if Self::command_succeeded(tool, check_mode, output.status.success(), &stdout) {
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
    if let Some(count) = results.selected_file_count {
        println!("Selected files: {count}");
    } else {
        println!("Selection: all files");
    }
    if results.selection_fallback {
        println!("Fallback: no Git repository; formatted all files");
    }
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
        "selection": {
            "mode": if results.selected_file_count.is_some() { "files" } else { "all" },
            "file_count": results.selected_file_count,
            "fallback": results.selection_fallback,
        },
        "plan": {
            "execution_modes": results.execution_modes,
            "selection_evidence": results.selection_evidence,
            "formatter_ownership": results.formatter_ownership,
            "format_order": results.format_order,
            "planned_file_counts": results.planned_file_counts,
            "inventory": {
                "mode": if results.selected_file_count.is_some() { "git-candidates" } else { "repository-walk" },
                "recursive_walks": results.inventory_diagnostics.recursive_walks,
                "directories_visited": results.inventory_diagnostics.directories_visited,
                "files_indexed": results.inventory_diagnostics.files_indexed,
                "native_configs_parsed": results.inventory_diagnostics.native_configs_parsed,
                "executables_resolved": results.inventory_diagnostics.executables_resolved,
                "probes_executed": results.inventory_diagnostics.probes_executed,
                "files_assigned": results.inventory_diagnostics.files_assigned,
            },
            "automatic_exclusions": results.automatic_exclusions,
            "unavailable_tools": results.unavailable_tools,
            "overlap_warnings": results.overlap_warnings,
        },
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

    #[cfg(all(feature = "format", feature = "tools-tui"))]
    #[test]
    fn tui_non_tui_and_json_paths_report_the_same_plan_metadata() {
        let dir = temp_dir("clippier-tui-selection-parity");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"selection-parity\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn selected() {}\n").unwrap();
        std::fs::write(dir.join("src/unselected.rs"), "pub fn unselected() {}\n").unwrap();
        let registry = ToolRegistry::new(ToolsConfig::default(), Some(&dir)).unwrap();
        let plan = ToolPlan {
            tools: vec![crate::tools::PlannedTool {
                name: "rustfmt".to_string(),
                evidence: crate::tools::SelectionEvidence {
                    kind: crate::tools::SelectionEvidenceKind::Manifest,
                    path: PathBuf::from("Cargo.toml"),
                },
                files: BTreeSet::from([
                    PathBuf::from("src/lib.rs"),
                    PathBuf::from("src/unselected.rs"),
                ]),
                format_extensions: BTreeSet::from(["rs".to_string()]),
                format_order: Some(10),
            }],
            unavailable: vec!["taplo".to_string()],
            effective_extensions: std::collections::BTreeMap::new(),
            automatic_exclusions: Vec::new(),
            diagnostics: crate::tools::InventoryDiagnostics::default(),
        };
        let warnings = vec!["WARNING: configured formatter overlap".to_string()];
        let runner = ToolRunner::new(&registry)
            .with_working_dir(&dir)
            .with_tool_plan(&plan)
            .with_overlap_warnings(warnings)
            .with_format_selection(FormatSelection::Files(BTreeSet::from([PathBuf::from(
                "src/lib.rs",
            )])));

        let normal = runner.run_specific(&["rustfmt"], &[], true).unwrap();
        assert_eq!(
            runner.scoped_files_for(registry.get("rustfmt").unwrap()),
            Some(vec!["src/lib.rs".to_string()])
        );
        let tui = runner
            .run_specific_with_tui(&["rustfmt"], &[], true)
            .unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&results_to_json(&normal).unwrap()).unwrap();

        assert_eq!(normal.selected_file_count, Some(1));
        assert_eq!(tui.selected_file_count, normal.selected_file_count);
        assert_eq!(tui.selection_fallback, normal.selection_fallback);
        assert_eq!(tui.execution_modes, normal.execution_modes);
        assert_eq!(tui.selection_evidence, normal.selection_evidence);
        assert_eq!(tui.formatter_ownership, normal.formatter_ownership);
        assert_eq!(tui.format_order, normal.format_order);
        assert_eq!(tui.automatic_exclusions, normal.automatic_exclusions);
        assert_eq!(tui.unavailable_tools, normal.unavailable_tools);
        assert_eq!(tui.overlap_warnings, normal.overlap_warnings);
        assert_eq!(json["selection"]["file_count"], 1);
        assert_eq!(json["plan"]["execution_modes"]["rustfmt"], "cargo");
        assert_eq!(
            json["plan"]["selection_evidence"]["rustfmt"],
            "Manifest:Cargo.toml"
        );
        assert_eq!(json["plan"]["formatter_ownership"]["rustfmt"][0], "rs");
        assert_eq!(json["plan"]["format_order"]["rustfmt"], 10);
        assert_eq!(json["plan"]["unavailable_tools"][0], "taplo");
        assert_eq!(
            json["plan"]["overlap_warnings"][0],
            "WARNING: configured formatter overlap"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn json_results_include_bounded_selection_metadata() {
        let results = AggregatedResults {
            results: Vec::new(),
            total_duration: Duration::ZERO,
            success_count: 0,
            failure_count: 0,
            selected_file_count: Some(3),
            selection_fallback: false,
            execution_modes: BTreeMap::from([("prettier".to_string(), "binary".to_string())]),
            selection_evidence: BTreeMap::from([(
                "prettier".to_string(),
                "NativeConfig:.prettierrc".to_string(),
            )]),
            formatter_ownership: BTreeMap::from([(
                "prettier".to_string(),
                BTreeSet::from(["md".to_string()]),
            )]),
            format_order: BTreeMap::from([("prettier".to_string(), 10)]),
            planned_file_counts: BTreeMap::from([("prettier".to_string(), 3)]),
            inventory_diagnostics: crate::tools::InventoryDiagnostics {
                recursive_walks: 1,
                files_indexed: 3,
                files_assigned: 3,
                ..Default::default()
            },
            automatic_exclusions: vec!["node_modules/**".to_string()],
            unavailable_tools: vec!["dprint".to_string()],
            overlap_warnings: vec!["WARNING: formatter overlap".to_string()],
        };
        let output = results_to_json(&results).unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(value["selection"]["mode"], "files");
        assert_eq!(value["selection"]["file_count"], 3);
        assert!(value.get("files").is_none());
        assert_eq!(value["plan"]["execution_modes"]["prettier"], "binary");
        assert_eq!(
            value["plan"]["selection_evidence"]["prettier"],
            "NativeConfig:.prettierrc"
        );
        assert_eq!(value["plan"]["formatter_ownership"]["prettier"][0], "md");
        assert_eq!(value["plan"]["format_order"]["prettier"], 10);
        assert_eq!(value["plan"]["planned_file_counts"]["prettier"], 3);
        assert_eq!(value["plan"]["inventory"]["recursive_walks"], 1);
        assert_eq!(value["plan"]["inventory"]["files_indexed"], 3);
        assert_eq!(value["plan"]["inventory"]["files_assigned"], 3);
        assert_eq!(value["plan"]["automatic_exclusions"][0], "node_modules/**");
        assert_eq!(value["plan"]["unavailable_tools"][0], "dprint");
        assert_eq!(
            value["plan"]["overlap_warnings"][0],
            "WARNING: formatter overlap"
        );
    }

    #[cfg(all(feature = "format", unix))]
    #[test]
    fn atomic_source_replacement_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("clippier-atomic-rustfmt");
        let path = dir.join("source.rs");
        std::fs::write(&path, "fn before() {}\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();

        ToolRunner::write_formatted_source_atomically(&path, b"fn after() {}\n").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"fn after() {}\n");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    fn write_named_argument_capture_tool(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let executable = dir.join(format!("capture-args-{name}"));
        std::fs::write(
            &executable,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/captured-{name}\"\n"),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        executable
    }

    #[cfg(unix)]
    #[test]
    fn native_recursive_formatter_cannot_reintroduce_automatic_exclusions() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".terraform.lock.hcl"), "\n").unwrap();
        std::fs::write(root.path().join("main.tf"), "resource {}\n").unwrap();
        std::fs::create_dir(root.path().join(".terraform")).unwrap();
        std::fs::write(root.path().join(".terraform/generated.tf"), "generated\n").unwrap();
        let executable = write_named_argument_capture_tool(root.path(), "terraform");
        let mut config = ToolsConfig::default();
        config.executables.insert(
            "terraform".to_string(),
            executable.to_string_lossy().to_string(),
        );
        let registry = ToolRegistry::new(config, Some(root.path())).unwrap();
        let plan = crate::tools::plan_tools(&registry, &[ToolCapability::Format]).unwrap();
        assert_eq!(plan.names(), vec!["terraform"]);

        let results = ToolRunner::new(&registry)
            .with_working_dir(root.path())
            .with_tool_plan(&plan)
            .run_specific(&["terraform"], &[], false)
            .unwrap();
        assert!(results.all_success());
        let args = std::fs::read_to_string(root.path().join("captured-terraform")).unwrap();
        assert!(args.lines().any(|arg| arg == "fmt"));
        assert!(args.lines().any(|arg| arg == "main.tf"));
        assert!(!args.lines().any(|arg| arg == "-recursive"));
        assert!(
            !args
                .lines()
                .any(|arg| arg.contains(".terraform/generated.tf"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn automatic_ownership_prevents_competing_formatter_arguments() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("biome.json"), "{}\n").unwrap();
        std::fs::write(root.path().join(".prettierrc"), "{}\n").unwrap();
        std::fs::write(root.path().join("application.js"), "const value=1;\n").unwrap();
        std::fs::write(root.path().join("README.md"), "# Test\n").unwrap();

        let mut config = ToolsConfig::default();
        for name in ["biome", "prettier"] {
            let executable = write_named_argument_capture_tool(root.path(), name);
            config
                .executables
                .insert(name.to_string(), executable.to_string_lossy().to_string());
        }
        let registry = ToolRegistry::new(config, Some(root.path())).unwrap();
        let plan = crate::tools::plan_tools(&registry, &[ToolCapability::Format]).unwrap();
        let names = plan.names();
        assert_eq!(names, vec!["biome", "prettier"]);
        let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();

        let results = ToolRunner::new(&registry)
            .with_working_dir(root.path())
            .with_tool_plan(&plan)
            .run_specific(&name_refs, &[], false)
            .unwrap();
        assert!(results.all_success());

        let biome_args = std::fs::read_to_string(root.path().join("captured-biome")).unwrap();
        let prettier_args = std::fs::read_to_string(root.path().join("captured-prettier")).unwrap();
        assert!(biome_args.lines().any(|arg| arg == "application.js"));
        assert!(!prettier_args.lines().any(|arg| arg == "application.js"));
        assert!(prettier_args.lines().any(|arg| arg == "README.md"));
    }

    #[cfg(unix)]
    fn write_order_capture_tool(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let executable = dir.join(format!("capture-{name}"));
        std::fs::write(
            &executable,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--support-info\" ]; then printf '%s\\n' '{{\"languages\":[]}}'; exit 0; fi\nprintf '%s\\n' '{name}' >> \"$(dirname \"$0\")/execution-order\"\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        executable
    }

    #[cfg(unix)]
    fn ordered_markdown_pipeline(
        root: &Path,
        suppress_overlap: bool,
    ) -> (Vec<String>, Vec<String>) {
        let mut config = ToolsConfig::default();
        for (name, order) in [("dprint", 10), ("prettier", 20)] {
            let executable = write_order_capture_tool(root, name);
            config.tools.insert(
                name.to_string(),
                crate::tools::ToolPolicy {
                    mode: crate::tools::ToolSelectionMode::Enabled,
                    executable: Some(executable.to_string_lossy().to_string()),
                    format_extensions: BTreeSet::from(["md".to_string()]),
                    format_order: Some(order),
                    ..Default::default()
                },
            );
        }
        if suppress_overlap {
            config.overlap_warning_suppress = vec![crate::tools::OverlapWarningSuppressRule {
                capability: crate::tools::OverlapWarningCapability::Format,
                tools: vec!["dprint".to_string(), "prettier".to_string()],
                extensions: vec!["md".to_string()],
            }];
        }
        let registry = ToolRegistry::new(config, Some(root)).unwrap();
        let plan = crate::tools::plan_tools(&registry, &[ToolCapability::Format]).unwrap();
        let warnings = crate::tools::overlap_warnings_for_plan(
            &registry,
            &plan,
            &registry.config().overlap_warning_suppress,
        );
        let names = plan.names();
        let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        let results = ToolRunner::new(&registry)
            .with_working_dir(root)
            .with_tool_plan(&plan)
            .with_parallel(!plan.has_ordered_formatters())
            .run_specific(&name_refs, &[], false)
            .unwrap();
        assert!(results.all_success());
        let order = std::fs::read_to_string(root.join("execution-order"))
            .unwrap()
            .lines()
            .map(ToString::to_string)
            .collect();
        (order, warnings)
    }

    #[cfg(unix)]
    #[test]
    fn configured_pipeline_executes_in_order_and_suppression_changes_only_warnings() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("document.md"), "# test\n").unwrap();

        let (order, warnings) = ordered_markdown_pipeline(root.path(), false);
        assert_eq!(order, vec!["dprint", "prettier"]);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("dprint"));
        assert!(warnings[0].contains("prettier"));

        std::fs::remove_file(root.path().join("execution-order")).unwrap();
        let (suppressed_order, suppressed_warnings) = ordered_markdown_pipeline(root.path(), true);
        assert_eq!(suppressed_order, order);
        assert!(suppressed_warnings.is_empty());
    }

    #[cfg(unix)]
    fn write_argument_capture_tool(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let executable = dir.join("capture-tool");
        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/captured-args\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        executable
    }

    #[cfg(unix)]
    fn run_dprint_argument_capture(root: &Path, include: Vec<String>) -> Vec<String> {
        let executable = write_argument_capture_tool(root);
        let mut config = ToolsConfig::default();
        config.tools.insert(
            "dprint".to_string(),
            crate::tools::ToolPolicy {
                executable: Some(executable.to_string_lossy().to_string()),
                include,
                ..Default::default()
            },
        );
        let registry = ToolRegistry::new(config, Some(root)).unwrap();
        let plan = crate::tools::plan_tools(&registry, &[ToolCapability::Format]).unwrap();
        assert_eq!(plan.names(), vec!["dprint"]);
        let results = ToolRunner::new(&registry)
            .with_working_dir(root)
            .with_tool_plan(&plan)
            .run_specific(&["dprint"], &[], false)
            .unwrap();
        assert!(results.all_success());
        std::fs::read_to_string(root.join("captured-args"))
            .unwrap()
            .lines()
            .map(ToString::to_string)
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn automatic_exclusion_and_explicit_reinclusion_reach_actual_tool_arguments() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("package.json"), "{}\n").unwrap();
        std::fs::write(root.path().join("dprint.json"), "{}\n").unwrap();
        std::fs::write(root.path().join("source.json"), "{}\n").unwrap();
        std::fs::create_dir(root.path().join("node_modules")).unwrap();
        std::fs::write(root.path().join("node_modules/dependency.json"), "{}\n").unwrap();

        let excluded_args = run_dprint_argument_capture(root.path(), Vec::new());
        assert!(excluded_args.contains(&"source.json".to_string()));
        assert!(!excluded_args.iter().any(|arg| arg.contains("node_modules")));

        let reincluded_args =
            run_dprint_argument_capture(root.path(), vec!["node_modules/**".to_string()]);
        assert!(
            reincluded_args.contains(&"node_modules/dependency.json".to_string()),
            "explicit profile re-inclusion did not reach execution: {reincluded_args:?}"
        );
    }

    #[test]
    fn file_oriented_low_level_execution_requires_a_resolved_plan() {
        let dir = temp_dir("clippier-runner-requires-plan");
        let executable = dir.join("dprint");
        std::fs::write(&executable, "").unwrap();
        let mut config = ToolsConfig::default();
        config.executables.insert(
            "dprint".to_string(),
            executable.to_string_lossy().to_string(),
        );
        let registry = ToolRegistry::new(config, Some(&dir)).unwrap();
        let error = ToolRunner::new(&registry)
            .with_working_dir(&dir)
            .run_specific(&["dprint"], &[], false)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires a resolved repository plan")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn selected_files_are_filtered_by_extension_and_global_exclusions() {
        let dir = temp_dir("clippier-selected-scope");
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        std::fs::write(dir.join("included.json"), "{}\n").unwrap();
        std::fs::write(dir.join("other.rs"), "fn other() {}\n").unwrap();
        std::fs::write(dir.join("generated/excluded.json"), "{}\n").unwrap();
        let mut config = ToolsConfig::default();
        config.scope.exclude = vec!["/generated/**".to_string()];
        config.scope_base = Some(dir.clone());
        let registry = ToolRegistry::new(config, Some(&dir)).unwrap();
        let runner = ToolRunner::new(&registry)
            .with_working_dir(&dir)
            .with_format_selection(FormatSelection::Files(BTreeSet::from([
                PathBuf::from("included.json"),
                PathBuf::from("other.rs"),
                PathBuf::from("generated/excluded.json"),
            ])));
        let tool = Tool::new(
            "dprint",
            "Dprint",
            "dprint",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec!["check".to_string()],
            vec!["fmt".to_string()],
        );

        assert_eq!(
            runner.scoped_file_args(&tool),
            Some(vec!["included.json".to_string()])
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn new_file_scoped_integrations_exclude_unplanned_files() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ToolRegistry::new(ToolsConfig::default(), Some(dir.path())).unwrap();
        for (name, extension) in [
            ("flake8", "py"),
            ("ty", "py"),
            ("yapf", "py"),
            ("autopep8", "py"),
            ("phpstan", "php"),
            ("phpcs", "php"),
            ("google-java-format", "java"),
            ("vale", "rst"),
            ("codespell", "md"),
            ("cspell", "ts"),
        ] {
            let included = format!("included.{extension}");
            std::fs::write(dir.path().join(&included), "").unwrap();
            let runner = ToolRunner::new(&registry)
                .with_working_dir(dir.path())
                .with_format_selection(FormatSelection::Files(BTreeSet::from([PathBuf::from(
                    &included,
                )])));
            let tool = tool_catalog_entry(name).unwrap().tool();
            assert_eq!(
                runner.scoped_file_args(&tool),
                Some(vec![included]),
                "{name}"
            );
        }
    }

    #[test]
    #[cfg(feature = "tools-tui")]
    fn language_commands_preserve_mode_and_replace_repository_argument() {
        let dir = tempfile::tempdir().unwrap();
        let registry = ToolRegistry::new(ToolsConfig::default(), Some(dir.path())).unwrap();
        for (name, extension, check_prefix, format_prefix) in [
            ("zig", "zon", vec!["fmt", "--check"], vec!["fmt"]),
            (
                "ormolu",
                "hs",
                vec!["--mode", "check"],
                vec!["--mode", "inplace"],
            ),
            (
                "fourmolu",
                "hs",
                vec!["--mode", "check"],
                vec!["--mode", "inplace"],
            ),
            (
                "scalafmt",
                "scala",
                vec!["--test", "--non-interactive"],
                vec!["--non-interactive"],
            ),
            ("hlint", "lhs", vec![], vec![]),
        ] {
            let file = format!("input.{extension}");
            std::fs::write(dir.path().join(&file), "").unwrap();
            let runner = ToolRunner::new(&registry)
                .with_working_dir(dir.path())
                .with_format_selection(FormatSelection::Files(BTreeSet::from([PathBuf::from(
                    &file,
                )])));
            let tool = tool_catalog_entry(name).unwrap().tool();
            for (check, prefix) in [(true, check_prefix), (false, format_prefix)] {
                if !check && name == "hlint" {
                    assert!(
                        runner
                            .build_command_parts(&tool, false, Some(dir.path()))
                            .is_none()
                    );
                    continue;
                }
                let (binary, args, warnings) = runner
                    .build_command_parts(&tool, check, Some(dir.path()))
                    .unwrap();
                let mut expected = prefix.into_iter().map(str::to_owned).collect::<Vec<_>>();
                expected.push(file.clone());
                assert_eq!(binary, name);
                assert_eq!(args, expected, "{name} check={check}");
                assert!(warnings.is_empty());
            }
        }
    }

    #[test]
    fn buf_adapter_uses_local_source_and_explicit_path_filters() {
        let tool = tool_catalog_entry("buf").unwrap().tool();
        let files = vec![
            "api/a.proto".to_owned(),
            "api/path with spaces.proto".to_owned(),
        ];
        for mut args in [tool.check_args.clone(), tool.format_args.clone()] {
            let prefix = args.clone();
            ToolRunner::replace_default_path_args(&tool, &mut args, &files);
            let mut expected = prefix;
            for file in &files {
                expected.extend(["--path".to_owned(), file.clone()]);
            }
            assert_eq!(args, expected);
            assert_eq!(args.iter().filter(|arg| *arg == ".").count(), 1);
        }
    }

    #[test]
    fn additional_formatter_commands_have_nonwriting_checks_and_scoped_writes() {
        for (name, check, write) in [
            ("fish_indent", "--check", "--write"),
            ("jsonnetfmt", "--test", "--in-place"),
            ("typstyle", "--check", "--inplace"),
        ] {
            let tool = tool_catalog_entry(name).unwrap().tool();
            let files = vec!["selected file".to_owned()];
            let mut args = tool.check_args.clone();
            ToolRunner::replace_default_path_args(&tool, &mut args, &files);
            assert_eq!(args, vec![check.to_owned(), files[0].clone()]);
            let mut args = tool.format_args.clone();
            ToolRunner::replace_default_path_args(&tool, &mut args, &files);
            assert_eq!(args, vec![write.to_owned(), files[0].clone()]);
        }
    }

    #[test]
    fn buf_lint_and_cppcheck_commands_preserve_failure_and_scope_semantics() {
        let files = vec!["api/selected.proto".to_owned()];
        let tool = tool_catalog_entry("buf-lint").unwrap().tool();
        let mut args = tool.check_args.clone();
        ToolRunner::replace_default_path_args(&tool, &mut args, &files);
        assert_eq!(args, ["lint", ".", "--path", "api/selected.proto"]);
        assert!(tool.format_args.is_empty());
        let tool = tool_catalog_entry("cppcheck").unwrap().tool();
        let mut args = tool.check_args.clone();
        ToolRunner::replace_default_path_args(&tool, &mut args, &["src/selected.cpp".to_owned()]);
        assert_eq!(args, ["--error-exitcode=1", "src/selected.cpp"]);
        assert!(tool.format_args.is_empty());
    }

    #[test]
    fn elm_standard_and_rumdl_command_modes_are_scoped() {
        for (name, check, write) in [
            ("elm-format", "--validate", Some("--yes")),
            ("standard", ".", None),
            ("rumdl", "check", Some("fmt")),
        ] {
            let tool = tool_catalog_entry(name).unwrap().tool();
            let files = vec!["selected file".to_owned()];
            let mut args = tool.check_args.clone();
            ToolRunner::replace_default_path_args(&tool, &mut args, &files);
            let mut expected = if check == "." {
                vec![]
            } else {
                vec![check.to_owned()]
            };
            expected.extend(files.clone());
            assert_eq!(args, expected);
            if let Some(write) = write {
                let mut args = tool.format_args.clone();
                ToolRunner::replace_default_path_args(&tool, &mut args, &files);
                assert_eq!(args, vec![write.to_owned(), files[0].clone()]);
            } else {
                assert!(tool.format_args.is_empty());
            }
        }
    }

    #[test]
    fn r_lua_and_sql_commands_pass_only_selected_files() {
        for (name, prefix) in [
            ("air", vec!["format", "--check"]),
            ("selene", vec![]),
            ("squawk", vec![]),
        ] {
            let tool = tool_catalog_entry(name).unwrap().tool();
            let mut args = tool.check_args.clone();
            ToolRunner::replace_default_path_args(&tool, &mut args, &["selected file".to_owned()]);
            let mut expected = prefix.into_iter().map(str::to_owned).collect::<Vec<_>>();
            expected.push("selected file".to_owned());
            assert_eq!(args, expected);
            if name == "air" {
                let mut args = tool.format_args.clone();
                ToolRunner::replace_default_path_args(&tool, &mut args, &["analysis.R".to_owned()]);
                assert_eq!(args, ["format", "analysis.R"]);
            } else {
                assert!(tool.format_args.is_empty());
            }
        }
    }

    #[test]
    fn go_format_checks_fail_on_listed_files_but_writes_do_not() {
        for name in ["gofmt", "gofumpt", "goimports"] {
            let tool = tool_catalog_entry(name).unwrap().tool();
            assert!(!ToolRunner::command_succeeded(
                &tool,
                true,
                true,
                "src/main.go\n"
            ));
            assert!(ToolRunner::command_succeeded(&tool, true, true, ""));
            assert!(ToolRunner::command_succeeded(
                &tool,
                false,
                true,
                "src/main.go\n"
            ));
            assert!(!ToolRunner::command_succeeded(&tool, true, false, ""));
            let mut args = tool.check_args.clone();
            ToolRunner::replace_default_path_args(&tool, &mut args, &["src/main.go".to_owned()]);
            assert_eq!(args, ["-l", "src/main.go"]);
        }
        let tool = tool_catalog_entry("eslint").unwrap().tool();
        assert!(ToolRunner::command_succeeded(
            &tool,
            true,
            true,
            "informational output"
        ));
    }

    #[test]
    fn dotnet_formatter_uses_workspace_includes_and_disables_restore() {
        let tool = tool_catalog_entry("dotnet-format").unwrap().tool();
        let files = vec!["src/One.cs".to_owned(), "src/Two words.vb".to_owned()];
        for (check, mut args) in [
            (true, tool.check_args.clone()),
            (false, tool.format_args.clone()),
        ] {
            ToolRunner::replace_default_path_args(&tool, &mut args, &files);
            let mut expected = vec!["format", "whitespace", "--no-restore"];
            if check {
                expected.push("--verify-no-changes");
            }
            expected.extend(["--include", "src/One.cs", "src/Two words.vb"]);
            assert_eq!(args, expected);
        }
    }

    #[test]
    fn zizmor_command_is_offline_strict_and_explicitly_scoped() {
        let tool = tool_catalog_entry("zizmor").unwrap().tool();
        let mut args = tool.check_args.clone();
        ToolRunner::replace_default_path_args(
            &tool,
            &mut args,
            &[
                ".github/workflows/ci.yml".to_owned(),
                "action.yaml".to_owned(),
            ],
        );
        assert_eq!(
            args,
            [
                "--offline",
                "--strict-collection",
                ".github/workflows/ci.yml",
                "action.yaml"
            ]
        );
        assert!(tool.format_args.is_empty());
    }

    #[test]
    fn directory_grouping_uses_catalog_filter_syntax() {
        let alternate = ToolRunner::directory_groups(&["src/input.ext".to_owned()], "--selected=");
        assert_eq!(alternate[Path::new("src")], ["--selected=input.ext"]);
        let modules = ToolRunner::directory_groups(
            &[
                "main.tf".to_owned(),
                "modules/a/main.tf".to_owned(),
                "modules/a/outputs.tf".to_owned(),
                "modules/b/main.tf".to_owned(),
            ],
            "--filter=",
        );
        assert_eq!(modules.len(), 3);
        assert_eq!(
            modules[Path::new("modules/a")],
            ["--filter=main.tf", "--filter=outputs.tf"]
        );
        assert_eq!(modules[Path::new("")], ["--filter=main.tf"]);
        assert!(ToolRunner::directory_groups(&[], "--filter=").is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn captured_process_drains_both_pipes_and_cancels_running_child() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "i=0; while [ $i -lt 10000 ]; do echo stdout; echo stderr >&2; i=$((i+1)); done",
        ]);
        let output = ToolRunner::capture_process(&mut command, &|| false).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 70000);
        assert_eq!(output.stderr.len(), 70000);
        let start = Instant::now();
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 30"]);
        let error = ToolRunner::capture_process(&mut command, &|| {
            start.elapsed() >= Duration::from_millis(100)
        })
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert!(start.elapsed() < Duration::from_secs(5));
        let mut command = Command::new("does-not-exist-clippier-test");
        assert_eq!(
            ToolRunner::capture_process(&mut command, &|| true)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Interrupted
        );
    }

    #[test]
    #[cfg(unix)]
    fn streaming_sink_receives_both_pipes_before_process_exits() {
        let stdout = std::sync::Mutex::new(Vec::new());
        let stderr = std::sync::Mutex::new(Vec::new());
        let started = Instant::now();
        let mut command = Command::new("sh");
        command.args(["-c", "printf out; printf err >&2; exec sleep 30"]);
        let result = ToolRunner::execute_process(
            &mut command,
            &|| {
                started.elapsed() > Duration::from_secs(3)
                    || (!stdout.lock().unwrap().is_empty() && !stderr.lock().unwrap().is_empty())
            },
            &|is_stderr, bytes| {
                if is_stderr {
                    stderr.lock().unwrap().extend_from_slice(bytes);
                } else {
                    stdout.lock().unwrap().extend_from_slice(bytes);
                }
            },
        );
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(*stdout.lock().unwrap(), b"out");
        assert_eq!(*stderr.lock().unwrap(), b"err");
    }

    #[test]
    fn selected_files_apply_per_tool_include_and_exclude_policy() {
        let dir = temp_dir("clippier-per-tool-scope");
        std::fs::create_dir_all(dir.join("src/generated")).unwrap();
        std::fs::write(dir.join("src/included.json"), "{}\n").unwrap();
        std::fs::write(dir.join("outside.json"), "{}\n").unwrap();
        std::fs::write(dir.join("src/generated/excluded.json"), "{}\n").unwrap();
        let mut config = ToolsConfig {
            scope_base: Some(dir.clone()),
            ..Default::default()
        };
        config.tools.insert(
            "dprint".to_string(),
            crate::tools::ToolPolicy {
                include: vec!["src/**".to_string()],
                exclude: vec!["src/generated/**".to_string()],
                ..Default::default()
            },
        );
        let registry = ToolRegistry::new(config, Some(&dir)).unwrap();
        let runner = ToolRunner::new(&registry)
            .with_working_dir(&dir)
            .with_format_selection(FormatSelection::Files(BTreeSet::from([
                PathBuf::from("src/included.json"),
                PathBuf::from("outside.json"),
                PathBuf::from("src/generated/excluded.json"),
            ])));
        let tool = Tool::new(
            "dprint",
            "Dprint",
            "dprint",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec!["check".to_string()],
            vec!["fmt".to_string()],
        );

        assert_eq!(
            runner.scoped_file_args(&tool),
            Some(vec!["src/included.json".to_string()])
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clippier_md_scope_uses_runner_working_directory_and_exclusions() {
        let dir = temp_dir("clippier-md-delegation-scope");
        let docs = dir.join("docs");
        let excluded = docs.join("generated");
        std::fs::create_dir_all(&excluded).expect("failed to create delegation fixtures");
        std::fs::write(docs.join("guide.md"), "# Guide\n").expect("failed to write guide");
        std::fs::write(excluded.join("ignored.md"), "# Ignored\n")
            .expect("failed to write excluded file");
        let mut config = ToolsConfig::default();
        config.scope.exclude = vec!["/docs/generated/**".to_string()];
        config.scope_base = Some(dir.clone());
        let registry = ToolRegistry::new(config, Some(&dir)).expect("failed to create registry");
        let runner = ToolRunner::new(&registry).with_working_dir(&dir);
        let tool = Tool::new(
            "clippier_md",
            "clippier-md",
            "cargo",
            ToolKind::Cargo,
            vec![ToolCapability::Format],
            vec![
                "fmt".to_string(),
                "--".to_string(),
                "fmt".to_string(),
                ".".to_string(),
            ],
            vec![],
        );

        assert_eq!(runner.working_dir_path(), dir);
        assert_eq!(
            runner.scoped_file_args(&tool),
            Some(vec!["docs/guide.md".to_string()])
        );

        std::fs::remove_dir_all(runner.working_dir_path())
            .expect("failed to clean delegation fixtures");
    }

    #[test]
    fn every_file_oriented_formatter_receives_only_selected_files() {
        let files = vec!["src/input.ext".to_string(), "docs/guide.ext".to_string()];
        for name in [
            "taplo",
            "biome",
            "dprint",
            "clippier_md",
            "prettier",
            "remark",
            "mdformat",
            "yamlfmt",
            "ruff",
            "black",
            "gofmt",
            "shfmt",
        ] {
            let mut args = if matches!(name, "biome" | "dprint") {
                vec!["fmt".to_string()]
            } else {
                vec!["fmt".to_string(), ".".to_string()]
            };
            let tool = Tool::new(
                name,
                name,
                name,
                ToolKind::Binary,
                vec![ToolCapability::Format],
                Vec::new(),
                args.clone(),
            );

            ToolRunner::replace_default_path_args(&tool, &mut args, &files);

            assert!(
                !args.iter().any(|arg| arg == "."),
                "{name} retained recursive path"
            );
            assert!(
                args.ends_with(&files),
                "{name} did not receive selected files"
            );
        }
    }

    #[test]
    fn clippier_md_delegation_replaces_default_path_with_scoped_files() {
        let tool = Tool::new(
            "clippier_md",
            "clippier-md",
            "cargo",
            ToolKind::Cargo,
            vec![ToolCapability::Format],
            vec![
                "fmt".to_string(),
                "--".to_string(),
                "fmt".to_string(),
                ".".to_string(),
            ],
            vec![],
        );
        let mut args = tool.check_args.clone();
        let files = vec!["README.md".to_string(), "docs/guide.md".to_string()];

        ToolRunner::replace_default_path_args(&tool, &mut args, &files);

        assert_eq!(
            args,
            vec!["fmt", "--", "fmt", "README.md", "docs/guide.md",]
        );
    }

    #[test]
    fn scope_excludes_apply_to_collected_files_without_skipping_nested_repositories() {
        let dir = temp_dir("clippier-runner-scope");
        let excluded = dir.join("vendor").join("checkout");
        let nested = dir.join("nested");
        std::fs::create_dir_all(&excluded).expect("failed to create excluded directory");
        std::fs::create_dir_all(&nested).expect("failed to create nested repository");
        std::fs::write(excluded.join("ignored.json"), "{}\n")
            .expect("failed to write excluded file");
        std::fs::write(nested.join(".git"), "gitdir: elsewhere\n")
            .expect("failed to write nested Git marker");
        std::fs::write(nested.join("included.json"), "{}\n")
            .expect("failed to write included file");

        let matcher = ScopeMatcher::new(&dir, &["/vendor/checkout/**".to_string()])
            .expect("failed to build scope matcher");
        let files = matcher.collect_files(
            std::slice::from_ref(&dir),
            &std::iter::once("json".to_string()).collect(),
        );

        assert_eq!(files, vec![nested.join("included.json")]);
        std::fs::remove_dir_all(&dir).expect("failed to clean up temp dir");
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

        let mut tool = Tool::new(
            "prettier",
            "Prettier",
            "prettier",
            ToolKind::Binary,
            vec![ToolCapability::Format],
            vec![],
            vec![],
        );
        tool.native_ignore_path = Some(dir.join(".prettierignore"));

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
            "#!/bin/sh\nout=\"\"\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    --output|-o)\n      shift\n      out=\"$1\"\n      ;;\n  esac\n  shift\ndone\nif [ -z \"$out\" ]; then\n  exit 2\nfi\nmkdir -p \"$out\"\nsed 's/bad/good/g' README.md > \"$out/README.md\"\n",
        )
        .expect("failed to write remark formatter script");

        let registry = ToolRegistry::new(ToolsConfig::default(), Some(&dir))
            .expect("failed to create registry");
        let runner = ToolRunner::new(&registry).with_working_dir(&dir);

        let tool = Tool::new(
            "remark",
            "remark",
            "sh",
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
            "#!/bin/sh\nout=\"\"\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    --output|-o)\n      shift\n      out=\"$1\"\n      ;;\n  esac\n  shift\ndone\nif [ -z \"$out\" ]; then\n  exit 2\nfi\nmkdir -p \"$out\"\ncat README.md > \"$out/README.md\"\n",
        )
        .expect("failed to write remark formatter script");

        let registry = ToolRegistry::new(ToolsConfig::default(), Some(&dir))
            .expect("failed to create registry");
        let runner = ToolRunner::new(&registry).with_working_dir(&dir);

        let tool = Tool::new(
            "remark",
            "remark",
            "sh",
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
