//! Tool execution and result aggregation.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::tools::registry::{ToolError, ToolRegistry};
use crate::tools::types::{Tool, ToolKind};

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
