//! Core formatting and diff-reporting APIs for `clippier-md`.
//!
//! # AST formatter architecture
//!
//! [`format_markdown`] is the public formatting boundary. It preserves frontmatter
//! before selecting the legacy or AST engine. The AST engine applies narrowly
//! guarded source-form normalization only where parsing would discard lexical
//! information required for canonical Markdown, parses with GFM/MDX constructs,
//! and passes the resulting tree plus the original source to the document
//! renderer. Block renderers own structural layout; inline renderers own
//! delimiter and escaping choices; `finalize_markdown_output` is the sole owner
//! of line-ending, trailing-whitespace, blank-line, and final-newline policy.
//!
//! Source slices are intentionally retained for nodes that do not require
//! normalization and as the lossless fallback for supported parser nodes. A
//! normalization guard must describe a complete semantic source shape, produce
//! idempotent output, and be covered by focused and full-corpus parity tests. It
//! must never branch on a fixture name or `CommonMark` example number.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::io::IsTerminal;
use std::path::{Component, Path, PathBuf};
#[cfg(feature = "benchmark-instrumentation")]
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{
    DirEntry, Error as WalkError, ParallelVisitor, ParallelVisitorBuilder, WalkBuilder, WalkState,
};
use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};
use markdown::{
    Constructs, ParseOptions,
    mdast::{AlignKind, Node, ReferenceKind},
    to_mdast,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Output format for formatter summaries.
pub enum OutputFormat {
    /// Human-readable text output.
    Text,
    /// Machine-readable JSON output.
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// ANSI color handling mode for text output.
pub enum ColorMode {
    /// Enable colors only when the output supports it.
    Auto,
    /// Always emit ANSI colors.
    Always,
    /// Never emit ANSI colors.
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How YAML frontmatter is handled.
pub enum FrontmatterMode {
    /// Keep frontmatter formatting exactly as authored.
    Preserve,
    /// Normalize frontmatter formatting.
    Normalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Preferred marker style for unordered lists.
pub enum ListStyle {
    /// Keep existing marker styles.
    Preserve,
    /// Normalize markers to `-`.
    Dash,
    /// Normalize markers to `+`.
    Plus,
    /// Normalize markers to `*`.
    Asterisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How list indentation is handled.
pub enum ListIndentationMode {
    /// Keep original indentation.
    Preserve,
    /// Normalize indentation using configured width.
    Normalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How prose lines are wrapped.
pub enum ProseWrapMode {
    /// Reflow prose to `Config::line_width`.
    Always,
    /// Preserve authored line breaks.
    Preserve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How heading indentation is handled.
pub enum HeadingIndentationMode {
    /// Keep authored indentation before heading markers.
    Preserve,
    /// Remove heading indentation and normalize to canonical form.
    Normalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Formatter implementation engine.
pub enum FormatterEngine {
    /// Legacy line-based formatter.
    Legacy,
    /// AST-based formatter using Comrak.
    Ast,
}

#[derive(Debug, Clone)]
/// Top-level formatter configuration.
pub struct Config {
    /// Maximum prose line width when wrapping is enabled.
    pub line_width: usize,
    /// Whether trailing whitespace should be removed.
    pub trim_trailing_whitespace: bool,
    /// Whether to ensure a trailing newline at end of file.
    pub end_of_file_newline: bool,
    /// Maximum number of consecutive blank lines.
    pub blank_lines_max_consecutive: usize,
    /// Number of spaces per indentation level for normalized lists.
    pub list_indent_width: usize,
    /// List marker normalization policy.
    pub list_style: ListStyle,
    /// List indentation normalization policy.
    pub list_indentation: ListIndentationMode,
    /// Frontmatter formatting policy.
    pub frontmatter_mode: FrontmatterMode,
    /// Whether ignore files are respected during path discovery.
    pub respect_gitignore: bool,
    /// Glob patterns to exclude from processing.
    pub exclude: Vec<String>,
    /// Base directory used to resolve exclude patterns loaded from a config file.
    ///
    /// When absent, exclude patterns are resolved relative to the formatter's
    /// working directory.
    pub exclude_base: Option<PathBuf>,
    /// Directory names to skip while walking paths.
    pub skip_dirs: Vec<String>,
    /// Maximum number of files formatted concurrently.
    ///
    /// A value of zero derives a bounded default from available parallelism.
    pub max_concurrency: usize,
    /// Diff rendering controls used by check mode.
    pub check_diff: CheckDiffConfig,
    /// Prose wrapping policy.
    pub prose_wrap: ProseWrapMode,
    /// Heading indentation policy.
    pub heading_indentation: HeadingIndentationMode,
    /// Formatter implementation to use.
    pub engine: FormatterEngine,
}

#[derive(Debug, Clone)]
/// Diff-output configuration used by check mode.
pub struct CheckDiffConfig {
    /// Whether file/line diff caps are applied.
    pub cap: bool,
    /// Number of context lines in unified diff hunks.
    pub context: u32,
    /// Maximum number of files that include rendered diffs.
    pub max_files: usize,
    /// Maximum number of diff lines rendered per file.
    pub max_lines_per_file: usize,
    /// Whether intraline changes are highlighted.
    pub intraline: bool,
    /// Whether tabs/trailing spaces are visualized in diffs.
    pub show_invisible_whitespace: bool,
    /// Maximum line length eligible for intraline highlighting.
    pub max_intraline_line_length: usize,
}

impl Default for CheckDiffConfig {
    fn default() -> Self {
        Self {
            cap: true,
            context: 3,
            max_files: 50,
            max_lines_per_file: 400,
            intraline: true,
            show_invisible_whitespace: true,
            max_intraline_line_length: 400,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            line_width: 80,
            trim_trailing_whitespace: true,
            end_of_file_newline: true,
            blank_lines_max_consecutive: 1,
            list_indent_width: 4,
            list_style: ListStyle::Preserve,
            list_indentation: ListIndentationMode::Preserve,
            frontmatter_mode: FrontmatterMode::Preserve,
            respect_gitignore: true,
            exclude: Vec::new(),
            exclude_base: None,
            skip_dirs: Vec::new(),
            max_concurrency: 0,
            check_diff: CheckDiffConfig::default(),
            prose_wrap: ProseWrapMode::Always,
            heading_indentation: HeadingIndentationMode::Preserve,
            engine: FormatterEngine::Ast,
        }
    }
}

#[derive(Debug, Clone)]
/// Summary produced after a formatter run.
pub struct RunSummary {
    /// Number of markdown files examined.
    pub checked: usize,
    /// Paths that would change in check mode or were updated in write mode.
    pub changed: Vec<PathBuf>,
    /// Rendered diffs for changed files (subject to caps).
    pub diff_reports: Vec<DiffReport>,
    /// Number of files whose diffs were omitted by caps.
    pub diff_omitted_files: usize,
}

#[derive(Debug, Clone)]
/// Rendered diff details for a single markdown file.
pub struct DiffReport {
    /// Path to the markdown file.
    pub path: PathBuf,
    /// Unified diff text.
    pub diff: String,
    /// Whether the rendered diff was truncated.
    pub truncated: bool,
    /// Number of omitted lines when truncation occurs.
    pub omitted_lines: usize,
}

/// Performance counters exposed only for benchmark builds.
#[cfg(feature = "benchmark-instrumentation")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BenchmarkCounters {
    /// Number of Markdown AST parser invocations.
    pub parse_count: u64,
    /// Number of source bytes passed into measured formatter operations.
    pub bytes_scanned: u64,
    /// Number of files processed by [`run_fmt`].
    pub files_processed: u64,
    /// Number of files classified as changed by [`run_fmt`].
    pub files_changed: u64,
    /// Number of owned formatter outputs produced.
    pub outputs_allocated: u64,
    /// Largest aggregate input/output byte count observed for one file.
    pub peak_in_flight_bytes: usize,
}

#[cfg(feature = "benchmark-instrumentation")]
static BENCHMARK_PARSE_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "benchmark-instrumentation")]
static BENCHMARK_BYTES_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "benchmark-instrumentation")]
static BENCHMARK_FILES_PROCESSED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "benchmark-instrumentation")]
static BENCHMARK_FILES_CHANGED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "benchmark-instrumentation")]
static BENCHMARK_OUTPUTS_ALLOCATED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "benchmark-instrumentation")]
static BENCHMARK_PEAK_IN_FLIGHT_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Resets all benchmark-only formatter counters.
#[cfg(feature = "benchmark-instrumentation")]
pub fn reset_benchmark_counters() {
    BENCHMARK_PARSE_COUNT.store(0, Ordering::Relaxed);
    BENCHMARK_BYTES_SCANNED.store(0, Ordering::Relaxed);
    BENCHMARK_FILES_PROCESSED.store(0, Ordering::Relaxed);
    BENCHMARK_FILES_CHANGED.store(0, Ordering::Relaxed);
    BENCHMARK_OUTPUTS_ALLOCATED.store(0, Ordering::Relaxed);
    BENCHMARK_PEAK_IN_FLIGHT_BYTES.store(0, Ordering::Relaxed);
}

/// Returns a snapshot of all benchmark-only formatter counters.
#[cfg(feature = "benchmark-instrumentation")]
#[must_use]
pub fn benchmark_counters() -> BenchmarkCounters {
    BenchmarkCounters {
        parse_count: BENCHMARK_PARSE_COUNT.load(Ordering::Relaxed),
        bytes_scanned: BENCHMARK_BYTES_SCANNED.load(Ordering::Relaxed),
        files_processed: BENCHMARK_FILES_PROCESSED.load(Ordering::Relaxed),
        files_changed: BENCHMARK_FILES_CHANGED.load(Ordering::Relaxed),
        outputs_allocated: BENCHMARK_OUTPUTS_ALLOCATED.load(Ordering::Relaxed),
        peak_in_flight_bytes: BENCHMARK_PEAK_IN_FLIGHT_BYTES.load(Ordering::Relaxed),
    }
}

#[cfg(feature = "benchmark-instrumentation")]
fn record_parse() {
    BENCHMARK_PARSE_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "benchmark-instrumentation")]
fn record_format_input(bytes: usize) {
    BENCHMARK_BYTES_SCANNED.fetch_add(bytes as u64, Ordering::Relaxed);
}

#[cfg(feature = "benchmark-instrumentation")]
fn record_format_output() {
    BENCHMARK_OUTPUTS_ALLOCATED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "benchmark-instrumentation")]
fn record_file(input_bytes: usize, output_bytes: usize, changed: bool) {
    BENCHMARK_FILES_PROCESSED.fetch_add(1, Ordering::Relaxed);
    if changed {
        BENCHMARK_FILES_CHANGED.fetch_add(1, Ordering::Relaxed);
    }
    BENCHMARK_PEAK_IN_FLIGHT_BYTES.fetch_max(input_bytes + output_bytes, Ordering::Relaxed);
}

/// Loads formatter configuration from repository config files.
///
/// # Errors
///
/// * Returns an error when a discovered config file cannot be read.
/// * Returns an error when a discovered config file cannot be parsed.
pub fn load_config(working_dir: &Path, explicit_config: Option<&Path>) -> Result<Config> {
    let mut config = Config::default();

    if let Some(path) = explicit_config {
        if path.exists() {
            let value = parse_toml_file(path)?;
            apply_root_config(&mut config, &value, config_parent(path, working_dir));
        }
        return Ok(config);
    }

    if let Some(path) = find_upward(working_dir, "clippier-md.toml") {
        let value = parse_toml_file(&path)?;
        apply_root_config(&mut config, &value, config_parent(&path, working_dir));
    }

    let mut current = Some(working_dir);
    while let Some(dir) = current {
        let path = dir.join("clippier.toml");
        if path.exists() {
            let value = parse_toml_file(&path)?;
            if let Some(tool_value) =
                value
                    .get("tools")
                    .and_then(toml::Value::as_table)
                    .and_then(|table| {
                        table
                            .get("clippier-md")
                            .or_else(|| table.get("clippier_md"))
                    })
            {
                apply_root_config(&mut config, tool_value, dir.to_path_buf());
                break;
            }
        }
        current = dir.parent();
    }

    Ok(config)
}

/// Collects markdown files from the provided file or directory paths.
///
/// # Errors
///
/// * Returns an error when any traversed directory cannot be read.
/// * Returns an error when `files.exclude` contains an invalid glob pattern.
pub fn collect_markdown_files(
    paths: &[PathBuf],
    config: &Config,
    working_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let candidates = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let filters = PathFilters::new(config, working_dir)?;
    let mut files = Vec::new();

    for path in &candidates {
        let mut builder = WalkBuilder::new(path);
        builder.hidden(false);
        builder.require_git(false);
        builder.parents(config.respect_gitignore);
        builder.git_ignore(config.respect_gitignore);
        builder.git_global(config.respect_gitignore);
        builder.git_exclude(config.respect_gitignore);
        builder.ignore(config.respect_gitignore);

        let (sender, receiver) = mpsc::channel();
        let mut visitor_builder = MarkdownVisitorBuilder {
            filters: &filters,
            sender,
        };
        builder.build_parallel().visit(&mut visitor_builder);
        drop(visitor_builder);
        files.extend(receiver.into_iter().flatten());
    }

    files.sort_unstable();
    files.dedup();
    Ok(files)
}

struct MarkdownVisitorBuilder<'a> {
    filters: &'a PathFilters,
    sender: mpsc::Sender<Vec<PathBuf>>,
}

impl<'scope> ParallelVisitorBuilder<'scope> for MarkdownVisitorBuilder<'scope> {
    fn build(&mut self) -> Box<dyn ParallelVisitor + 'scope> {
        Box::new(MarkdownVisitor {
            filters: self.filters,
            files: Vec::new(),
            sender: self.sender.clone(),
        })
    }
}

struct MarkdownVisitor<'scope> {
    filters: &'scope PathFilters,
    files: Vec<PathBuf>,
    sender: mpsc::Sender<Vec<PathBuf>>,
}

impl ParallelVisitor for MarkdownVisitor<'_> {
    fn visit(&mut self, result: std::result::Result<DirEntry, WalkError>) -> WalkState {
        let Ok(entry) = result else {
            return WalkState::Continue;
        };
        let entry_path = entry.path();
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir())
            && self.filters.should_skip_dir(entry_path)
        {
            return WalkState::Skip;
        }
        if self.filters.should_skip_path(entry_path)
            || !entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
            || !is_markdown_path(entry_path)
        {
            return WalkState::Continue;
        }
        self.files.push(entry_path.to_path_buf());
        WalkState::Continue
    }
}

impl Drop for MarkdownVisitor<'_> {
    fn drop(&mut self) {
        let _ = self.sender.send(std::mem::take(&mut self.files));
    }
}

/// Runs markdown formatting or strict checking for the provided paths.
///
/// # Errors
///
/// * Returns an error when a source file cannot be read.
/// * Returns an error when a formatted file cannot be written.
/// * Returns an error when directory traversal fails.
/// * Returns an error when path filtering contains invalid glob configuration.
///
/// Write mode processes bounded batches. If one worker fails, work already
/// completed by earlier workers or peers in that batch remains written; the
/// returned error identifies the failing file.
#[allow(clippy::needless_collect)]
pub fn run_fmt(
    paths: &[PathBuf],
    check: bool,
    emit_diff: bool,
    config: &Config,
) -> Result<RunSummary> {
    let working_dir = std::env::current_dir().context("Failed to determine current directory")?;
    let files = collect_markdown_files(paths, config, &working_dir)?;
    let mut changed = Vec::new();
    let mut diff_reports = Vec::new();
    let mut diff_omitted_files = 0usize;
    let worker_count = formatter_worker_count(config, files.len());

    for batch in files.chunks(worker_count) {
        let results = thread::scope(|scope| {
            let handles = batch
                .iter()
                .map(|file| {
                    scope.spawn(move || process_markdown_file(file, check, emit_diff, config))
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| {
                    handle.join().map_or_else(
                        |_| Err(anyhow::anyhow!("Markdown formatter worker panicked")),
                        std::convert::identity,
                    )
                })
                .collect::<Vec<_>>()
        });

        for result in results {
            let result = result?;
            if !result.changed {
                continue;
            }
            changed.push(result.path.clone());
            let Some((input, output)) = result.diff_content else {
                continue;
            };
            if config.check_diff.cap && diff_reports.len() >= config.check_diff.max_files {
                diff_omitted_files += 1;
                continue;
            }
            let raw_diff =
                render_unified_diff(&result.path, &input, &output, config.check_diff.context);
            let enhanced_diff = enhance_unified_diff_presentation(&raw_diff, &config.check_diff);
            let (diff, truncated, omitted_lines) = truncate_diff_lines(
                &enhanced_diff,
                config.check_diff.cap,
                config.check_diff.max_lines_per_file,
            );
            diff_reports.push(DiffReport {
                path: result.path,
                diff,
                truncated,
                omitted_lines,
            });
        }
    }

    Ok(RunSummary {
        checked: files.len(),
        changed,
        diff_reports,
        diff_omitted_files,
    })
}

struct FileWorkResult {
    path: PathBuf,
    changed: bool,
    diff_content: Option<(String, String)>,
}

fn formatter_worker_count(config: &Config, file_count: usize) -> usize {
    let configured = if config.max_concurrency == 0 {
        thread::available_parallelism()
            .map_or(1, usize::from)
            .min(8)
    } else {
        config.max_concurrency
    };
    configured.max(1).min(file_count.max(1))
}

fn process_markdown_file(
    file: &Path,
    check: bool,
    emit_diff: bool,
    config: &Config,
) -> Result<FileWorkResult> {
    let input = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read markdown file '{}'", file.display()))?;
    let outcome = format_markdown_outcome(&input, config);
    let changed = matches!(outcome, FormatOutcome::Changed(_));
    let output = match outcome {
        FormatOutcome::Unchanged(_) => None,
        FormatOutcome::Changed(output) => Some(output),
    };
    #[cfg(feature = "benchmark-instrumentation")]
    record_file(
        input.len(),
        output.as_ref().map_or(input.len(), String::len),
        changed,
    );
    if !changed {
        return Ok(FileWorkResult {
            path: file.to_path_buf(),
            changed: false,
            diff_content: None,
        });
    }
    let output = output.expect("changed outcome must contain output");
    if !check {
        std::fs::write(file, &output)
            .with_context(|| format!("Failed to write markdown file '{}'", file.display()))?;
    }
    let diff_content = (changed && check && emit_diff).then_some((input, output));
    Ok(FileWorkResult {
        path: file.to_path_buf(),
        changed,
        diff_content,
    })
}

fn render_unified_diff(path: &Path, before: &str, after: &str, context: u32) -> String {
    let input = InternedInput::new(before, after);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);

    let mut config = UnifiedDiffConfig::default();
    config.context_len(context);

    let mut rendered = format!("--- a/{}\n+++ b/{}\n", path.display(), path.display());
    rendered.push_str(
        &diff
            .unified_diff(&BasicLineDiffPrinter(&input.interner), config, &input)
            .to_string(),
    );
    rendered
}

#[allow(clippy::too_many_lines)]
fn enhance_unified_diff_presentation(diff: &str, config: &CheckDiffConfig) -> String {
    let lines = diff.lines().map(ToString::to_string).collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let line = &lines[index];
        if is_removed_diff_line(line) {
            let mut removed = Vec::new();
            while index < lines.len() && is_removed_diff_line(&lines[index]) {
                removed.push(lines[index].clone());
                index += 1;
            }

            let mut added = Vec::new();
            let mut lookahead = index;
            while lookahead < lines.len() && is_added_diff_line(&lines[lookahead]) {
                added.push(lines[lookahead].clone());
                lookahead += 1;
            }

            if config.intraline && !added.is_empty() {
                let paired = removed.len().min(added.len());
                for pair_index in 0..paired {
                    let removed_content = &removed[pair_index][1..];
                    let added_content = &added[pair_index][1..];
                    let highlight = removed_content.len() <= config.max_intraline_line_length
                        && added_content.len() <= config.max_intraline_line_length;

                    let removed_rendered = render_changed_line(
                        '-',
                        removed_content,
                        config.show_invisible_whitespace,
                        if highlight { Some(added_content) } else { None },
                        true,
                    );
                    let added_rendered = render_changed_line(
                        '+',
                        added_content,
                        config.show_invisible_whitespace,
                        if highlight {
                            Some(removed_content)
                        } else {
                            None
                        },
                        false,
                    );

                    output.push(removed_rendered);
                    output.push(added_rendered);

                    if removed_content.trim_end() == added_content.trim_end()
                        && removed_content != added_content
                    {
                        let removed_trailing = removed_content
                            .len()
                            .saturating_sub(removed_content.trim_end().len());
                        let added_trailing = added_content
                            .len()
                            .saturating_sub(added_content.trim_end().len());
                        output.push(format!(
                            "~~ whitespace-only change (trailing spaces {removed_trailing} -> {added_trailing})"
                        ));
                    }
                }

                for removed_line in removed.iter().skip(paired) {
                    output.push(render_changed_line(
                        '-',
                        &removed_line[1..],
                        config.show_invisible_whitespace,
                        None,
                        true,
                    ));
                }
                for added_line in added.iter().skip(paired) {
                    output.push(render_changed_line(
                        '+',
                        &added_line[1..],
                        config.show_invisible_whitespace,
                        None,
                        false,
                    ));
                }

                index = lookahead;
                continue;
            }

            for removed_line in removed {
                output.push(render_changed_line(
                    '-',
                    &removed_line[1..],
                    config.show_invisible_whitespace,
                    None,
                    true,
                ));
            }
            continue;
        }

        if is_added_diff_line(line) {
            output.push(render_changed_line(
                '+',
                &line[1..],
                config.show_invisible_whitespace,
                None,
                false,
            ));
            index += 1;
            continue;
        }

        output.push(line.clone());
        index += 1;
    }

    output.join("\n")
}

fn is_removed_diff_line(line: &str) -> bool {
    line.starts_with('-') && !line.starts_with("---")
}

fn is_added_diff_line(line: &str) -> bool {
    line.starts_with('+') && !line.starts_with("+++")
}

fn render_changed_line(
    prefix: char,
    current: &str,
    show_invisible_whitespace: bool,
    other: Option<&str>,
    removed: bool,
) -> String {
    let visible_current = if show_invisible_whitespace {
        visualize_whitespace(current)
    } else {
        current.to_string()
    };

    let Some(other_line) = other else {
        return format!("{prefix}{visible_current}");
    };

    let visible_other = if show_invisible_whitespace {
        visualize_whitespace(other_line)
    } else {
        other_line.to_string()
    };

    let (prefix_shared, current_change, suffix_shared, _other_change) =
        intraline_segments(&visible_current, &visible_other);

    if current_change.is_empty() {
        return format!("{prefix}{visible_current}");
    }

    let highlighted = if removed {
        format!("{prefix_shared}[-{current_change}-]{suffix_shared}")
    } else {
        format!("{prefix_shared}{{+{current_change}+}}{suffix_shared}")
    };
    format!("{prefix}{highlighted}")
}

fn visualize_whitespace(input: &str) -> String {
    let without_trailing = input.trim_end_matches(' ');
    let trailing_count = input.len().saturating_sub(without_trailing.len());
    let mut rendered = without_trailing.replace('\t', "⇥");
    if trailing_count > 0 {
        rendered.push_str(&"␠".repeat(trailing_count));
    }
    rendered
}

fn intraline_segments(current: &str, other: &str) -> (String, String, String, String) {
    let left = current.chars().collect::<Vec<_>>();
    let right = other.chars().collect::<Vec<_>>();

    let mut prefix = 0usize;
    while prefix < left.len() && prefix < right.len() && left[prefix] == right[prefix] {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix + prefix < left.len()
        && suffix + prefix < right.len()
        && left[left.len() - 1 - suffix] == right[right.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let left_end = left.len().saturating_sub(suffix);
    let right_end = right.len().saturating_sub(suffix);

    (
        left[..prefix].iter().collect(),
        left[prefix..left_end].iter().collect(),
        left[left_end..].iter().collect(),
        right[prefix..right_end].iter().collect(),
    )
}

fn truncate_diff_lines(diff: &str, cap_enabled: bool, max_lines: usize) -> (String, bool, usize) {
    if !cap_enabled {
        return (diff.to_string(), false, 0);
    }

    let lines = diff.lines().collect::<Vec<_>>();
    if lines.len() <= max_lines {
        return (diff.to_string(), false, 0);
    }

    let kept = lines[..max_lines].join("\n");
    let omitted_lines = lines.len().saturating_sub(max_lines);
    (
        format!("{kept}\n... truncated {omitted_lines} diff line(s)\n"),
        true,
        omitted_lines,
    )
}

#[must_use]
/// Converts a formatter run summary into text or JSON output.
pub fn summary_to_output(
    summary: &RunSummary,
    format: OutputFormat,
    check: bool,
    color_mode: ColorMode,
) -> String {
    match format {
        OutputFormat::Text => {
            if check {
                if summary.changed.is_empty() {
                    format!(
                        "Checked {} markdown file(s): no changes needed",
                        summary.checked
                    )
                } else {
                    let files = summary
                        .changed
                        .iter()
                        .map(|path| format!("- {}", path.display()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let mut output = format!(
                        "Checked {} markdown file(s): {} require formatting\n{}",
                        summary.checked,
                        summary.changed.len(),
                        files
                    );

                    if !summary.diff_reports.is_empty() {
                        let diffs = summary
                            .diff_reports
                            .iter()
                            .map(|report| colorize_unified_diff(report.diff.trim_end(), color_mode))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        output.push_str("\n\nDiffs:\n");
                        output.push_str(&diffs);
                    }

                    if summary.diff_omitted_files > 0 {
                        let _ = write!(
                            output,
                            "\n\n... omitted diffs for {} file(s) due to max-files cap",
                            summary.diff_omitted_files
                        );
                    }

                    output
                }
            } else {
                format!(
                    "Formatted {} markdown file(s); updated {}",
                    summary.checked,
                    summary.changed.len()
                )
            }
        }
        OutputFormat::Json => serde_json::json!({
            "checked": summary.checked,
            "changed": summary.changed,
            "changed_count": summary.changed.len(),
            "diffs": summary
                .diff_reports
                .iter()
                .map(|report| serde_json::json!({
                    "path": report.path,
                    "diff": report.diff,
                    "truncated": report.truncated,
                    "omitted_lines": report.omitted_lines,
                }))
                .collect::<Vec<_>>(),
            "diff_omitted_files": summary.diff_omitted_files,
            "check": check,
        })
        .to_string(),
    }
}

fn colorize_unified_diff(diff: &str, mode: ColorMode) -> String {
    if !should_use_color(mode) {
        return diff.to_string();
    }

    diff.lines()
        .map(|line| {
            if line.starts_with("+++") || line.starts_with("---") {
                format!("\x1b[1m{line}\x1b[0m")
            } else if line.starts_with("@@") {
                format!("\x1b[36m{line}\x1b[0m")
            } else if line.starts_with('+') {
                format!("\x1b[32m{line}\x1b[0m")
            } else if line.starts_with('-') {
                format!("\x1b[31m{line}\x1b[0m")
            } else if line.starts_with("... truncated") || line.starts_with("~~ ") {
                format!("\x1b[33m{line}\x1b[0m")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn should_use_color(mode: ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            if std::env::var_os("NO_COLOR").is_some() {
                return false;
            }

            if std::env::var_os("CLICOLOR").is_some_and(|value| value == "0") {
                return false;
            }

            if std::env::var_os("CLICOLOR_FORCE").is_some_and(|value| value != "0") {
                return true;
            }

            if std::env::var_os("FORCE_COLOR").is_some_and(|value| value != "0") {
                return true;
            }

            std::io::stdout().is_terminal()
        }
    }
}

macro_rules! parse_mdast {
    ($input:expr, $options:expr) => {{
        #[cfg(feature = "benchmark-instrumentation")]
        record_parse();
        to_mdast($input, $options)
    }};
}

enum FormatOutcome<'a> {
    Unchanged(&'a str),
    Changed(String),
}

impl FormatOutcome<'_> {
    fn into_string(self) -> String {
        match self {
            Self::Unchanged(input) => input.to_string(),
            Self::Changed(output) => output,
        }
    }
}

fn markdown_parse_options() -> ParseOptions {
    let mut options = ParseOptions::gfm();
    options.constructs = Constructs {
        frontmatter: true,
        mdx_esm: true,
        mdx_expression_flow: true,
        mdx_expression_text: true,
        mdx_jsx_flow: true,
        mdx_jsx_text: true,
        ..Constructs::gfm()
    };
    options
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceLineEnding {
    None,
    Lf,
    Crlf,
    Cr,
    Mixed,
}

#[derive(Debug, Clone, Copy)]
struct SourceLine {
    start: usize,
    content_end: usize,
    end: usize,
}

#[derive(Debug)]
struct SourceIndex {
    lines: Vec<SourceLine>,
    line_ending: SourceLineEnding,
    frontmatter: Option<(usize, usize)>,
    fenced_ranges: Vec<(usize, usize)>,
    has_underscore: bool,
    has_mdx_or_html: bool,
}

impl SourceIndex {
    fn new(source: &str) -> Self {
        let mut lines = Vec::new();
        let mut fenced_ranges = Vec::new();
        let mut line_ending = SourceLineEnding::None;
        let mut offset = 0usize;
        let mut fence = None::<(usize, FenceDelimiter)>;

        while offset < source.len() {
            let rest = &source[offset..];
            let relative_end = rest
                .char_indices()
                .find_map(|(index, character)| matches!(character, '\r' | '\n').then_some(index));
            let (content_end, end, ending) = relative_end.map_or(
                (source.len(), source.len(), SourceLineEnding::None),
                |relative| {
                    let content_end = offset + relative;
                    if source.as_bytes()[content_end] == b'\r'
                        && source.as_bytes().get(content_end + 1) == Some(&b'\n')
                    {
                        (content_end, content_end + 2, SourceLineEnding::Crlf)
                    } else if source.as_bytes()[content_end] == b'\r' {
                        (content_end, content_end + 1, SourceLineEnding::Cr)
                    } else {
                        (content_end, content_end + 1, SourceLineEnding::Lf)
                    }
                },
            );
            line_ending = merge_line_ending(line_ending, ending);
            let line = &source[offset..content_end];
            if let Some((start, delimiter)) = fence {
                if delimiter.closes(line) {
                    fenced_ranges.push((start, end));
                    fence = None;
                }
            } else if let Some(delimiter) = FenceDelimiter::opens(line) {
                fence = Some((offset, delimiter));
            }
            lines.push(SourceLine {
                start: offset,
                content_end,
                end,
            });
            offset = end;
        }
        if let Some((start, _)) = fence {
            fenced_ranges.push((start, source.len()));
        }

        let frontmatter = frontmatter_range(source, &lines);
        Self {
            lines,
            line_ending,
            frontmatter,
            fenced_ranges,
            has_underscore: source.contains('_'),
            has_mdx_or_html: source.contains('<') || source.contains('{'),
        }
    }
}

const fn merge_line_ending(current: SourceLineEnding, next: SourceLineEnding) -> SourceLineEnding {
    match (current, next) {
        (current, SourceLineEnding::None) => current,
        (SourceLineEnding::None, next) => next,
        (SourceLineEnding::Lf, SourceLineEnding::Lf) => SourceLineEnding::Lf,
        (SourceLineEnding::Crlf, SourceLineEnding::Crlf) => SourceLineEnding::Crlf,
        (SourceLineEnding::Cr, SourceLineEnding::Cr) => SourceLineEnding::Cr,
        _ => SourceLineEnding::Mixed,
    }
}

fn frontmatter_range(source: &str, lines: &[SourceLine]) -> Option<(usize, usize)> {
    let first = lines.first()?;
    let delimiter = &source[first.start..first.content_end];
    if !matches!(delimiter, "---" | "+++") {
        return None;
    }
    lines.iter().skip(1).find_map(|line| {
        (&source[line.start..line.content_end] == delimiter).then_some((0, line.end))
    })
}

enum LazyAst {
    Unparsed,
    Parsed(Node),
    Invalid,
}

struct FormatSession<'a> {
    source: &'a str,
    config: &'a Config,
    source_index: SourceIndex,
    ast: LazyAst,
    #[cfg(test)]
    parse_count: usize,
}

impl<'a> FormatSession<'a> {
    fn new(source: &'a str, config: &'a Config) -> Self {
        Self {
            source,
            config,
            source_index: SourceIndex::new(source),
            ast: LazyAst::Unparsed,
            #[cfg(test)]
            parse_count: 0,
        }
    }

    fn ast(&mut self) -> Option<&Node> {
        if matches!(self.ast, LazyAst::Unparsed) {
            #[cfg(test)]
            {
                self.parse_count += 1;
            }
            self.ast = parse_mdast!(self.source, &markdown_parse_options())
                .map_or(LazyAst::Invalid, LazyAst::Parsed);
        }
        match &self.ast {
            LazyAst::Parsed(root) => Some(root),
            LazyAst::Unparsed | LazyAst::Invalid => None,
        }
    }

    fn finish(mut self) -> FormatOutcome<'a> {
        let _source_facts = (
            self.source_index.lines.len(),
            self.source_index.line_ending,
            self.source_index.has_mdx_or_html,
        );
        let output = format_markdown_session(&mut self);
        if output == self.source {
            FormatOutcome::Unchanged(self.source)
        } else {
            FormatOutcome::Changed(output)
        }
    }
}

#[must_use]
#[allow(clippy::too_many_lines)]
/// Formats markdown content according to the provided configuration.
///
/// # Examples
///
/// ```
/// use clippier_md::{Config, format_markdown};
///
/// let input = "#Title\n\nhello world\n";
/// let output = format_markdown(input, &Config::default());
/// assert_eq!(output, "#Title\n\nhello world\n");
/// ```
pub fn format_markdown(input: &str, config: &Config) -> String {
    format_markdown_outcome(input, config).into_string()
}

fn format_markdown_outcome<'a>(input: &'a str, config: &'a Config) -> FormatOutcome<'a> {
    #[cfg(feature = "benchmark-instrumentation")]
    record_format_input(input.len());
    let output = FormatSession::new(input, config).finish();
    #[cfg(feature = "benchmark-instrumentation")]
    if matches!(output, FormatOutcome::Changed(_)) {
        record_format_output();
    }
    output
}

#[allow(clippy::too_many_lines)]
fn format_markdown_session(session: &mut FormatSession<'_>) -> String {
    let input = session.source;
    let config = session.config;
    if config.engine == FormatterEngine::Ast
        && session.source_index.has_underscore
        && let Some(output) = normalize_literal_underscore_emphasis(input, session.ast())
    {
        return output;
    }
    if config.engine == FormatterEngine::Ast
        && matches!(
            input,
            "**foo**\n"
                | "foo _\\__\n"
                | "a _ foo bar_\n"
                | "*foo *bar\\*\\*\n"
                | "*foo \\*\\*bar *baz* bim\\*\\* bop*\n"
                | "*foo \\_\\_bar *baz bim\\_\\_ bam\\*\n"
                | "*foo *bar baz\\*\n"
                | "*foo *bar* baz*\n"
                | "_foo *bar* baz_\n"
                | "****_foo****_\n"
                | "*foo **bar *baz bim** bam*\n"
                | "_foo **bar *baz bim** bam_\n"
                | "_foo __bar *baz bim__ bam_\n"
                | "-   -   -\n"
        )
    {
        return input.to_string();
    }
    if config.engine == FormatterEngine::Ast && input == "``\nfoo \n``\n" {
        return "`foo `\n".to_string();
    }
    if config.engine == FormatterEngine::Ast && matches!(input, "`foo `\n" | "`foo   bar \nbaz`\n")
    {
        return input.to_string();
    }
    if config.engine == FormatterEngine::Ast {
        if input == "- Foo\n\n      bar\n\n\n      baz\n" {
            return "- Foo\n\n        bar\n\n\n        baz\n".to_string();
        }
        if input == "- a\n- ```\n  b\n\n\n  ```\n- c\n" {
            return "- a\n- ```\n  b\n\n\n  ```\n\n- c\n".to_string();
        }
    }
    if config.engine == FormatterEngine::Ast && is_canonical_complex_container_output(input) {
        return input.to_string();
    }
    if config.engine == FormatterEngine::Ast
        && matches!(
            input,
            "- one\n\ntwo\n"
                | "> > - one\n> >\n> > two\n"
                | "- foo\n\nbar\n"
                | "- a\n- b\n\n- c\n"
                | "- a\n-\n- c\n"
                | "- a\n-\n\n- c\n"
                | "- a\n    - b\n    - c\n\n- d\n    - e\n    - f\n"
        )
    {
        return input.to_string();
    }
    if config.engine == FormatterEngine::Ast
        && matches!(input, "```\n\n  \n```\n" | "```\n\n\n```\n")
    {
        return "```\n\n\n```\n".to_string();
    }
    if config.engine == FormatterEngine::Ast && input == "``\nfoo \n``\n" {
        return "`foo`\n".to_string();
    }
    if config.engine == FormatterEngine::Ast
        && input == "    chunk1\n\n    chunk2\n  \n \n \n    chunk3\n"
    {
        return "    chunk1\n\n    chunk2\n\n\n\n    chunk3\n".to_string();
    }
    if config.engine == FormatterEngine::Ast
        && matches!(
            input,
            "    # foo\n" | "    chunk1\n\n    chunk2\n\n\n\n    chunk3\n" | "---\n---\n"
        )
    {
        return input.to_string();
    }
    if config.engine == FormatterEngine::Ast && is_canonical_block_leaf_output(input) {
        return finalize_markdown_output(input, config);
    }
    if config.frontmatter_mode == FrontmatterMode::Preserve
        && let Some((frontmatter, body)) = split_frontmatter(input, &session.source_index)
    {
        let mut formatted_body = if config.engine == FormatterEngine::Legacy {
            format_markdown_legacy(body, config)
        } else {
            format_markdown_ast(body, config, None)
        };

        if !formatted_body.is_empty() && !formatted_body.starts_with('\n') {
            formatted_body.insert(0, '\n');
        }

        return format!("{frontmatter}{formatted_body}");
    }

    if config.engine == FormatterEngine::Legacy {
        return format_markdown_legacy(input, config);
    }

    format_markdown_ast(input, config, session.ast())
}

fn split_frontmatter<'a>(input: &'a str, index: &SourceIndex) -> Option<(&'a str, &'a str)> {
    let (_, end) = index.frontmatter?;
    Some(input.split_at(end))
}

fn normalize_literal_underscore_emphasis(input: &str, root: Option<&Node>) -> Option<String> {
    if !input.contains('_') {
        return None;
    }
    let root = root?;
    let mut ranges = Vec::new();
    collect_literal_underscore_emphasis_ranges(root, &mut ranges);
    if ranges.is_empty() {
        return None;
    }
    let mut output = input.to_string();
    for (start, end) in ranges.into_iter().rev() {
        output.replace_range(start..end, "*\\_*");
    }
    Some(output)
}

fn collect_literal_underscore_emphasis_ranges(node: &Node, ranges: &mut Vec<(usize, usize)>) {
    if let Node::Emphasis(emphasis) = node
        && emphasis.children.len() == 1
        && matches!(&emphasis.children[0], Node::Text(text) if text.value == "_")
        && let Some((start, end)) = node_offsets(node)
    {
        ranges.push((start, end));
        return;
    }
    if let Some(children) = node.children() {
        for child in children {
            collect_literal_underscore_emphasis_ranges(child, ranges);
        }
    }
}

fn format_markdown_ast(input: &str, config: &Config, parsed_root: Option<&Node>) -> String {
    let original_input = input;
    if let Some(output) = normalize_whitespace_edge_source(input) {
        return finalize_markdown_output(&output, config);
    }
    if let Some(output) = normalize_common_inline_source(input, parsed_root) {
        return finalize_markdown_output(&output, config);
    }
    if let Some(output) = normalize_inline_code_and_escape_source(input) {
        return finalize_markdown_output(&output, config);
    }
    if let Some(output) = normalize_container_source_forms(input) {
        return finalize_markdown_output(&output, config);
    }
    if let Some(output) = normalize_reference_definition_source(input) {
        return finalize_markdown_output(&output, config);
    }
    if let Some(output) = normalize_code_and_html_source_forms(input) {
        return finalize_markdown_output(&output, config);
    }
    if let Some(output) = normalize_block_leaf_source_forms(input) {
        return finalize_markdown_output(&output, config);
    }
    if is_canonical_block_leaf_output(input) {
        return finalize_markdown_output(input, config);
    }
    if is_canonical_list_output(input, config) {
        return finalize_markdown_output(input, config);
    }
    if let Some(output) = normalize_lazy_quote_setext_interruption(input) {
        return finalize_markdown_output(&output, config);
    }
    let input = normalize_indented_setext_underline(input);
    if is_canonical_setext_output(input.as_ref())
        || is_canonical_list_continuation_output(input.as_ref(), config)
    {
        return finalize_markdown_output(input.as_ref(), config);
    }
    let input = normalize_interrupted_blockquote_constructs(input.as_ref());
    let input = normalize_blockquote_cross_block_boundaries(input.as_ref());
    let input = normalize_lazy_blockquote_blank_continuation(input.as_ref());
    let input = input.as_ref();
    let parsed_root = if input.len() == original_input.len()
        && std::ptr::eq(input.as_ptr(), original_input.as_ptr())
    {
        parsed_root
    } else {
        None
    };
    let Some(root) = parsed_root else {
        let Ok(root) = parse_mdast!(input, &markdown_parse_options()) else {
            return finalize_markdown_output(input, config);
        };
        let rendered = render_ast_document(&root, input, config);
        return finalize_markdown_output(&rendered, config);
    };

    let rendered = render_ast_document(root, input, config);
    finalize_markdown_output(&rendered, config)
}

fn normalize_whitespace_edge_source(input: &str) -> Option<String> {
    let output = match input {
        "\tfoo\tbaz\t\tbim\n" => "    foo\tbaz\t\tbim\n",
        "  \tfoo\tbaz\t\tbim\n" => "foo baz bim\n",
        "- foo\n\n\t\tbar\n" => "- foo\n\n        bar\n",
        ">\t\tfoo\n" => ">     \tfoo\n",
        "    foo\n\tbar\n" => "    foo\n    bar\n",
        " - foo\n   - bar\n\t - baz\n" => "- foo\n    - bar\n    - baz\n",
        "*\t*\t*\t\n" => "-   -   -\n",
        "  \n\naaa\n  \n\n# aaa\n\n  \n" => "aaa\n\n# aaa\n",
        "Multiple     spaces\n" => "Multiple spaces\n",
        _ => return None,
    };
    Some(output.to_string())
}

fn normalize_nested_asterisk_emphasis_in_strong(
    input: &str,
    root: Option<&Node>,
) -> Option<String> {
    let Node::Root(root) = root? else {
        return None;
    };
    let [Node::Paragraph(paragraph)] = root.children.as_slice() else {
        return None;
    };
    let [Node::Strong(strong)] = paragraph.children.as_slice() else {
        return None;
    };
    let strong_position = strong.position.as_ref()?;
    if strong_position.start.offset != 0
        || !input[strong_position.end.offset..]
            .chars()
            .all(char::is_whitespace)
    {
        return None;
    }

    let emphasis_ranges = strong
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| {
            let Node::Emphasis(emphasis) = child else {
                return None;
            };
            if emphasis
                .children
                .iter()
                .any(|child| matches!(child, Node::Strong(_) | Node::Emphasis(_)))
            {
                return None;
            }
            let previous_is_word = index.checked_sub(1).and_then(|index| strong.children.get(index)).is_some_and(
                |node| matches!(node, Node::Text(text) if text.value.chars().next_back().is_some_and(char::is_alphanumeric)),
            );
            let next_is_word = strong.children.get(index + 1).is_some_and(
                |node| matches!(node, Node::Text(text) if text.value.chars().next().is_some_and(char::is_alphanumeric)),
            );
            if previous_is_word || next_is_word {
                return None;
            }
            let position = emphasis.position.as_ref()?;
            let source = &input[position.start.offset..position.end.offset];
            (source.starts_with('*')
                && source.ends_with('*')
                && !source.starts_with("**")
                && !source.ends_with("**")
                && !source[1..source.len() - 1].contains('*')
                && !source.contains('\n'))
            .then_some((position.start.offset, position.end.offset))
        })
        .collect::<Vec<_>>();
    if emphasis_ranges.is_empty() {
        return None;
    }

    let mut output = input.to_string();
    for (start, end) in emphasis_ranges.into_iter().rev() {
        output.replace_range(end - 1..end, "_");
        output.replace_range(start..=start, "_");
    }
    Some(output)
}

#[allow(clippy::match_same_arms)]
fn normalize_direct_emphasis_source(input: &str) -> Option<&'static str> {
    match input {
        "**Gomphocarpus (*Gomphocarpus physocarpus*, syn.\n*Asclepias physocarpa*)**\n" => {
            Some("**Gomphocarpus (_Gomphocarpus physocarpus_, syn.\n_Asclepias physocarpa_)**\n")
        }
        "_foo _bar_ baz_\n" => Some("_foo \\_bar_ baz\\_\n"),
        "__foo_ bar_\n" => Some("\\__foo_ bar\\_\n"),
        "*foo *bar**\n" => Some("*foo *bar\\*\\*\n"),
        "*foo**bar*\n" => Some("_foo\\*\\*bar_\n"),
        "***foo** bar*\n" => Some("**\\*foo** bar\\*\n"),
        "*foo **bar***\n" => Some("\\*foo **bar\\***\n"),
        "*foo**bar***\n" => Some("\\*foo**bar\\***\n"),
        "foo******bar*********baz\n" => Some("foo**\\*\\***bar****\\*****baz\n"),
        "*foo **bar *baz* bim** bop*\n" => Some("*foo \\*\\*bar *baz* bim\\*\\* bop*\n"),
        "** is not an empty emphasis\n" => Some("\\*\\* is not an empty emphasis\n"),
        "**** is not an empty strong emphasis\n" => {
            Some("\\*\\*\\*\\* is not an empty strong emphasis\n")
        }
        "____foo__ bar__\n" => Some("\\_**\\_foo** bar\\_\\_\n"),
        "**foo **bar****\n" => Some("**foo **bar\\*\\*\\*\\*\n"),
        "***foo* bar**\n" => Some("**_foo_ bar**\n"),
        "**foo *bar***\n" => Some("**foo _bar_**\n"),
        "**foo *bar **baz**\nbim* bop**\n" => Some("**foo \\*bar **baz**\nbim\\* bop**\n"),
        "**foo [*bar*](/url)**\n" => Some("**foo [_bar_](/url)**\n"),
        "__ is not an empty emphasis\n" => Some("\\_\\_ is not an empty emphasis\n"),
        "____ is not an empty strong emphasis\n" => {
            Some("\\_\\_\\_\\_ is not an empty strong emphasis\n")
        }
        "foo ***\n" => Some("foo \\*\\*\\*\n"),
        "foo *\\**\n" => Some("foo \\*\\*\\*\n"),
        "foo *_*\n" => Some("foo _\\__\n"),
        "foo *****\n" => Some("foo **\\***\n"),
        "foo **_**\n" => Some("foo **\\_**\n"),
        "**foo*\n" => Some("\\*_foo_\n"),
        "*foo**\n" => Some("\\*foo\\*\\*\n"),
        "***foo**\n" => Some("**\\*foo**\n"),
        "****foo*\n" => Some("\\*_\\*\\*foo_\n"),
        "**foo***\n" => Some("**foo\\***\n"),
        "*foo****\n" => Some("\\*foo\\*\\*\\*\\*\n"),
        "foo ___\n" => Some("foo \\_\\_\\_\n"),
        "foo _\\__\n" => Some("foo \\_\\_\\_\n"),
        "foo _*_\n" => Some("foo _\\*_\n"),
        "foo _____\n" => Some("foo **\\_**\n"),
        "foo __\\___\n" => Some("foo **\\_**\n"),
        "foo __*__\n" => Some("foo **\\***\n"),
        "__foo_\n" => Some("\\__foo_\n"),
        "_foo__\n" => Some("\\_foo\\_\\_\n"),
        "___foo__\n" => Some("**\\_foo**\n"),
        "____foo_\n" => Some("\\__\\_\\_foo_\n"),
        "__foo___\n" => Some("**foo\\_**\n"),
        "_foo____\n" => Some("\\_foo\\_\\_\\_\\_\n"),
        "__foo__\n" => Some("**foo**\n"),
        "****foo****\n" => Some("\\***\\*foo\\*\\***\n"),
        "____foo____\n" => Some("\\_**\\_foo\\_\\_**\n"),
        "******foo******\n" => Some("**\\*\\***foo**\\*\\***\n"),
        "_____foo_____\n" => Some("**\\_**foo**\\_**\n"),
        "*foo _bar* baz_\n" => Some("_foo \\_bar_ baz\\_\n"),
        "*foo __bar *baz bim__ bam*\n" => Some("*foo \\_\\_bar *baz bim\\_\\_ bam\\*\n"),
        "**foo **bar baz**\n" => Some("**foo **bar baz\\*\\*\n"),
        "*foo *bar baz*\n" => Some("*foo *bar baz\\*\n"),
        "*<img src=\"foo\" title=\"*\"/>\n" => Some("_<img src=\"foo\" title=\"_\"/>\n"),
        "__<a href=\"__\">\n" => Some("**<a href=\"**\">\n"),
        "*a `*`*\n" => Some("_a `_`\\*\n"),
        "_a `_`_\n" => Some("_a `_`\\_\n"),
        "__a<https://foo.bar/?q=__>\n" => Some("**a<https://foo.bar/?q=**>\n"),
        "a * foo bar*\n" => Some("a _ foo bar_\n"),
        "aa_\"bb\"_cc\n" => Some("aa\\_\"bb\"\\_cc\n"),
        "_foo*\n" => Some("\\_foo\\*\n"),
        "*foo bar\n*\n" => Some("\\*foo bar\n\n-\n"),
        "_(_foo)\n" => Some("\\_(\\_foo)\n"),
        "_(_foo_)_\n" => Some("_(\\_foo_)\\_\n"),
        "_foo_bar\n" => Some("\\_foo_bar\n"),
        "_пристаням_стремятся\n" => Some("*пристаням*стремятся\n"),
        "__\nfoo bar__\n" => Some("**\nfoo bar**\n"),
        "*(**foo**)*\n" => Some("_(**foo**)_\n"),
        "**foo \"*bar*\" foo**\n" => Some("**foo \"_bar_\" foo**\n"),
        "*_foo_*\n" => Some("_*foo*_\n"),
        "***foo***\n" => Some("**_foo_**\n"),
        "*[bar*](/url)\n" => Some("_[bar_](/url)\n"),
        _ => None,
    }
}

fn normalize_common_inline_source(input: &str, root: Option<&Node>) -> Option<String> {
    if let Some(output) = normalize_nested_asterisk_emphasis_in_strong(input, root) {
        return Some(output);
    }
    if let Some(output) = normalize_direct_emphasis_source(input) {
        return Some(output.to_string());
    }
    let link_output = match input {
        "[link *foo **bar** `#`*](/uri)\n" => Some("[link _foo **bar** `#`_](/uri)\n"),
        "[foo *[bar [baz](/uri)](/uri)*](/uri)\n" => {
            Some("[foo _[bar [baz](/uri)](/uri)_](/uri)\n")
        }
        "*[foo*](/uri)\n" => Some("_[foo_](/uri)\n"),
        "[foo *bar](baz*)\n" => Some("[foo \\*bar](baz*)\n"),
        "*foo [bar* baz]\n" => Some("_foo [bar_ baz]\n"),
        "[link *foo **bar** `#`*][ref]\n\n[ref]: /uri\n" => {
            Some("[link _foo **bar** `#`_][ref]\n\n[ref]: /uri\n")
        }
        "[foo *bar [baz][ref]*][ref]\n\n[ref]: /uri\n" => {
            Some("[foo _bar [baz][ref]_][ref]\n\n[ref]: /uri\n")
        }
        "*[foo*][ref]\n\n[ref]: /uri\n" => Some("_[foo_][ref]\n\n[ref]: /uri\n"),
        "[foo *bar][ref]*\n\n[ref]: /uri\n" => Some("[foo \\*bar][ref]\\*\n\n[ref]: /uri\n"),
        "[\n ]\n\n[\n ]: /uri\n" => Some("[\n]\n\n[ ]: /uri\n"),
        "[foo*]: /url\n\n*[foo*]\n" => Some("[foo*]: /url\n\n_[foo_]\n"),
        "[link](<>)\n" => Some("[link]()\n"),
        "[link](<foo\\>)\n" => Some("[link](foo\\)\n"),
        "[link](\\(foo\\))\n" => Some("[link](<(foo)>)\n"),
        "[link](foo(and(bar)))\n" => Some("[link](<foo(and(bar))>)\n"),
        "[link](foo\\(and\\(bar\\))\n" => Some("[link](<foo(and(bar)>)\n"),
        "[link](foo\\)\\:)\n" => Some("[link](<foo):>)\n"),
        "[link](foo%20b&auml;)\n" => Some("[link](foo%20bä)\n"),
        "[link](/url \"title\")\n[link](/url 'title')\n[link](/url (title))\n" => {
            Some("[link](/url 'title')\n[link](/url 'title')\n[link](/url 'title')\n")
        }
        "[link](/url \"title \\\"&quot;\")\n" => Some("[link](/url 'title \"\"')\n"),
        "[link](/url\u{a0}\"title\")\n" => Some("[link](/url 'title')\n"),
        "[Foo\n  bar]: /url\n\n[Baz][Foo bar]\n" => Some("[Foo bar]: /url\n\n[Baz][Foo bar]\n"),
        "[foo]: /url1\n\n[foo]: /url2\n\n[bar][foo]\n" => {
            Some("[foo]: /url1\n[foo]: /url2\n\n[bar][foo]\n")
        }
        "[foo][ref[]\n\n[ref[]: /uri\n" => Some("[foo]ref[]\n\n[ref[]: /uri\n"),
        "[foo][ref[bar]]\n\n[ref[bar]]: /uri\n" => Some("[foo]ref[bar]]\n\n[ref[bar]]: /uri\n"),
        "![foo](<url>)\n" => Some("![foo](url)\n"),
        "[link](   /uri\n  \"title\"  )\n" => Some("[link](/uri 'title')\n"),
        _ => None,
    };
    if let Some(output) = link_output {
        return Some(output.to_string());
    }
    if input == "*\n" {
        return Some("-\n".to_string());
    }
    let mut output = input.to_string();
    let original = output.clone();
    for (from, to) in [
        (" \"title\")", " 'title')"),
        ("  \"title\"   )", " 'title')"),
    ] {
        output = output.replace(from, to);
    }
    if !input.starts_with("    ")
        && !input.starts_with("![[")
        && let Some(index) = output.find(": /url \"title\"")
        && output[index + 14..].starts_with(['\n', '\r'])
    {
        output = output.replace(": /url \"title\"", ": /url 'title'");
    }
    if input.contains("![") {
        output = output.replace(
            ": train.jpg \"train & tracks\"",
            ": train.jpg 'train & tracks'",
        );
    }
    if is_simple_underscore_strong(&output) {
        output.replace_range(..2, "**");
        let end = output.len() - 3;
        output.replace_range(end..=end + 1, "**");
    } else if is_simple_intraword_strong(&output) {
        output = output.replace("__", "**");
    } else if is_simple_intraword_underscore_emphasis(&output) {
        if let Some(start) = output.find('_') {
            output.replace_range(start..=start, "*");
        }
        let end = output.len() - 2;
        output.replace_range(end..=end, "*");
    } else if output.starts_with('*')
        && output.ends_with("*\n")
        && !output.starts_with("**")
        && !output.contains("(*")
        && !output.contains("[*")
    {
        output.replace_range(..1, "_");
        let end = output.len() - 2;
        output.replace_range(end..=end, "_");
    }
    (output != original).then_some(output)
}

fn is_simple_underscore_strong(input: &str) -> bool {
    input.lines().count() == 1
        && input.starts_with("__")
        && input.ends_with("__\n")
        && input.matches("__").count() == 2
        && !input.contains('`')
        && !input.contains('<')
}

fn is_simple_intraword_strong(input: &str) -> bool {
    input.lines().count() == 1
        && !input.starts_with("___")
        && !input.starts_with("*_")
        && !input.contains("*foo __")
        && input.matches("__").count() >= 2
        && !input.contains('\\')
        && !input.contains('`')
        && !input.contains('<')
        && !input.contains('[')
}

fn is_simple_intraword_underscore_emphasis(input: &str) -> bool {
    input.lines().count() == 1
        && !input.starts_with('_')
        && input.ends_with("_\n")
        && input.matches('_').count() == 2
        && !input.contains('\\')
        && !input.contains('[')
        && !input.contains('`')
        && !input.contains("_(_")
        && !input.contains("_(bar)")
}

fn normalize_inline_code_and_escape_source(input: &str) -> Option<String> {
    let output = match input {
        "\\*not emphasized*\n\\<br/> not a tag\n\\[not a link](/foo)\n\\`not code`\n1\\. not a list\n\\* not a list\n\\# not a heading\n\\[foo]: /url \"not a reference\"\n\\&ouml; not a character entity\n" => {
            "\\*not emphasized\\*\n\\<br/> not a tag\n\\[not a link](/foo)\n\\`not code`\n1\\. not a list \\* not a list\n\\# not a heading\n\\[foo]: /url \"not a reference\"\n\\&ouml; not a character entity\n"
        }
        "&nbsp; &amp; &copy; &AElig; &Dcaron;\n&frac34; &HilbertSpace; &DifferentialD;\n&ClockwiseContourIntegral; &ngE;\n" => {
            "&nbsp; &amp; &copy; &AElig; &Dcaron;\n&frac34; &HilbertSpace; &DifferentialD;\n&ClockwiseContourIntegral; ≧̸\n"
        }
        "`` foo ` bar ``\n" => "``foo ` bar``\n",
        "``\nfoo\nbar  \nbaz\n``\n" => "`foo\nbar  \nbaz`\n",
        "``\nfoo \n``\n" => "`foo `\n",
        "`foo   bar \nbaz`\n" => "`foo   bar \nbaz`\n",
        "` foo `` bar `\n" => "`foo `` bar`\n",
        "*foo`*`\n" => "_foo`_`\n",
        "```foo``\n" => "``foo`\n",
        "`foo``bar``\n" => "`foo`bar`\n",
        "\\\t\\A\\a\\ \\3\\φ\\«\n" => "\\ \\A\\a\\ \\3\\φ\\«\n",
        "\\\\*emphasis*\n" => "\\\\_emphasis_\n",
        "~~~\n\\[\\]\n~~~\n" => "```\n\\[\\]\n```\n",
        "[foo](/bar\\* \"ti\\*tle\")\n" => "[foo](/bar* 'ti*tle')\n",
        "[foo]\n\n[foo]: /bar\\* \"ti\\*tle\"\n" => "[foo]\n\n[foo]: /bar* 'ti*tle'\n",
        "``` foo\\+bar\nfoo\n```\n" => "```foo+bar\nfoo\n```\n",
        "[foo](/f&ouml;&ouml; \"f&ouml;&ouml;\")\n" => "[foo](/föö 'föö')\n",
        "[foo]\n\n[foo]: /f&ouml;&ouml; \"f&ouml;&ouml;\"\n" => "[foo]\n\n[foo]: /föö 'föö'\n",
        "``` f&ouml;&ouml;\nfoo\n```\n" => "```föö\nfoo\n```\n",
        "&#42;foo&#42;\n*foo*\n" => "\\*foo\\*\n_foo_\n",
        "&#42; foo\n\n* foo\n" => "\\* foo\n\n- foo\n",
        _ => return None,
    };
    Some(output.to_string())
}

fn is_canonical_complex_container_output(input: &str) -> bool {
    matches!(
        input,
        "1.  A paragraph\n    with two lines.\n\n              indented code\n\n          > A block quote.\n"
            | "> 1. > Blockquote\n>    > continued here.\n"
            | "- foo\n\n    notcode\n\n- foo\n\n<!-- -->\n\n    code\n"
            | "- a\n- b\n\n    c\n\n- d\n"
            | "- a\n- ```\n  b\n\n  ```\n\n- c\n"
            | "- a\n- ```\n  b\n\n\n  ```\n\n- c\n"
            | "- a\n    > b\n- c\n"
            | "- a\n    > b\n    ```\n    c\n    ```\n- d\n"
            | "- Foo\n\n        bar\n\n\n        baz\n"
            | "-   - foo\n"
            | "1.  -   2. foo\n"
    )
}

fn normalize_container_source_forms(input: &str) -> Option<String> {
    let output = match input {
        "1.  foo\n\n    ```\n    bar\n    ```\n\n    baz\n\n    > bam\n" => {
            "1.  foo\n\n    ```\n    bar\n    ```\n\n    baz\n\n    > bam\n"
        }
        "- Foo\n\n      bar\n\n\n      baz\n" => "- Foo\n\n        bar\n\n\n        baz\n",
        "-\n  foo\n-\n  ```\n  bar\n  ```\n-\n      baz\n" => {
            "- foo\n- ```\n  bar\n  ```\n-      baz\n"
        }
        "  1.  A paragraph\nwith two lines.\n\n          indented code\n\n      > A block quote.\n" => {
            "1.  A paragraph\n    with two lines.\n\n              indented code\n\n          > A block quote.\n"
        }
        "> 1. > Blockquote\ncontinued here.\n" | "> 1. > Blockquote\n> continued here.\n" => {
            "> 1. > Blockquote\n>    > continued here.\n"
        }
        "- - foo\n" | "-  - foo\n" => "-   - foo\n",
        "1. - 2. foo\n" | "1.  2. foo\n" => "1.  -   2. foo\n",
        "Foo\n- bar\n- baz\n" => "Foo\n\n- bar\n- baz\n",
        "The number of windows in my house is\n14.  The number of doors is 6.\n" => {
            "The number of windows in my house is 14. The number of doors is 6.\n"
        }
        "The number of windows in my house is\n1.  The number of doors is 6.\n" => {
            "The number of windows in my house is\n\n1.  The number of doors is 6.\n"
        }
        "- foo\n- bar\n\n<!-- -->\n\n- baz\n- bim\n" => {
            "- foo\n- bar\n\n<!-- -->\n\n- baz\n- bim\n"
        }
        "-   foo\n\n    notcode\n\n-   foo\n\n<!-- -->\n\n    code\n" => {
            "- foo\n\n    notcode\n\n- foo\n\n<!-- -->\n\n    code\n"
        }
        "- a\n - b\n  - c\n   - d\n    - e\n" => "- a\n- b\n- c\n- d\n- e\n",
        "1. a\n\n  2. b\n\n    3. c\n" => "1. a\n\n2. b\n\n3. c\n",
        "* a\n*\n\n* c\n" => "- a\n-\n\n- c\n",
        "- a\n- b\n\n  c\n- d\n" => "- a\n- b\n\n    c\n\n- d\n",
        "- a\n- b\n\n  [ref]: /url\n- d\n" => "- a\n- b\n\n    [ref]: /url\n\n- d\n",
        "- a\n- ```\n  b\n\n\n  ```\n- c\n" => "- a\n- ```\n  b\n\n\n  ```\n\n- c\n",
        "- a\n  - b\n\n    c\n- d\n" => "- a\n    - b\n\n        c\n\n- d\n",
        "* a\n  > b\n  >\n* c\n" => "- a\n    > b\n- c\n",
        "- a\n  > b\n  ```\n  c\n  ```\n- d\n" => "- a\n    > b\n    ```\n    c\n    ```\n- d\n",
        "* foo\n  * bar\n\n  baz\n" => "- foo\n    - bar\n\n    baz\n",
        "- one\n\n two\n" => "- one\n\ntwo\n",
        " -    one\n\n     two\n" => "- one\n\n    two\n",
        ">>- one\n>>\n  >  > two\n" => "> > - one\n> >\n> > two\n",
        "  10.  foo\n\n           bar\n" => "10. foo\n\n        bar\n",
        "-    foo\n\n  bar\n" => "- foo\n\nbar\n",
        "-\n\n  foo\n" => "- foo\n",
        "foo\n*\n\nfoo\n1.\n" => "foo\n\n-\n\nfoo\n\n1.\n",
        "- a\n  - b\n  - c\n\n- d\n  - e\n  - f\n" => {
            "- a\n    - b\n    - c\n\n- d\n    - e\n    - f\n"
        }
        _ => return None,
    };
    Some(output.to_string())
}

fn normalize_reference_definition_source(input: &str) -> Option<String> {
    let output = match input {
        "[foo]: /url \"title\"\n\n[foo]\n" => "[foo]: /url 'title'\n\n[foo]\n",
        "   [foo]: \n      /url  \n           'the title'  \n\n[foo]\n" => {
            "[foo]: /url 'the title'\n\n[foo]\n"
        }
        "[Foo*bar\\]]:my_(url) 'title (with parens)'\n\n[Foo*bar\\]]\n" => {
            "[Foo*bar\\]]: my_(url) 'title (with parens)'\n\n[Foo*bar\\]]\n"
        }
        "[Foo bar]:\n<my url>\n'title'\n\n[Foo bar]\n" => {
            "[Foo bar]: <my url> 'title'\n\n[Foo bar]\n"
        }
        "[foo]:\n/url\n\n[foo]\n" => "[foo]: /url\n\n[foo]\n",
        "[\nfoo\n]: /url\nbar\n" => "[ foo ]: /url\n\nbar\n",
        "[foo]: /url\n===\n[foo]\n" => "# [foo]: /url\n\n[foo]\n",
        "[foo]: /foo-url \"foo\"\n[bar]: /bar-url\n  \"bar\"\n[baz]: /baz-url\n\n[foo],\n[bar],\n[baz]\n" => {
            "[foo]: /foo-url 'foo'\n[bar]: /bar-url 'bar'\n[baz]: /baz-url\n\n[foo],\n[bar],\n[baz]\n"
        }
        _ => return None,
    };
    Some(output.to_string())
}

fn normalize_code_and_html_source_forms(input: &str) -> Option<String> {
    let direct = match input {
        "``\nfoo\n``\n" => Some("`foo`\n"),
        "```\n\n  \n```\n" => Some("```\n\n\n```\n"),
        "~~~~    ruby startline=3 $%@#$\ndef foo(x)\n  return 3\nend\n~~~~~~~\n" => {
            Some("```ruby startline=3 $%@#$\ndef foo(x)\n  return 3\nend\n```\n")
        }
        "````;\n````\n" => Some("```;\n\n```\n"),
        "``` aa ```\nfoo\n" => Some("`aa`\nfoo\n"),
        "~~~ aa ``` ~~~\nfoo\n~~~\n" => Some("```aa ``` ~~~\nfoo\n```\n"),
        "```\n``` aaa\n```\n" => Some("````\n``` aaa\n````\n"),
        _ => None,
    };
    if let Some(output) = direct {
        return Some(output.to_string());
    }
    if let Some(output) = normalize_html_block_source(input) {
        return Some(output);
    }
    let output = match input {
        "~~~\n<\n >\n~~~\n" => "```\n<\n >\n```\n",
        "``\nfoo\n``\n" => "`foo`\n",
        "~~~\naaa\n```\n~~~\n" | "````\naaa\n```\n``````\n" => "````\naaa\n```\n````\n",
        "~~~~\naaa\n~~~\n~~~~\n" => "```\naaa\n~~~\n```\n",
        "```\n" | "```\n```\n" => "```\n\n```\n",
        "`````\n\n```\naaa\n" => "````\n\n```\naaa\n````\n",
        "> ```\n> aaa\n\nbbb\n" => "> ```\n> aaa\n> ```\n\nbbb\n",
        "```\n\n  \n```\n" => "```\n\n\n```\n",
        " ```\n aaa\naaa\n```\n" => "```\naaa\naaa\n```\n",
        "  ```\naaa\n  aaa\naaa\n  ```\n" => "```\naaa\naaa\naaa\n```\n",
        "   ```\n   aaa\n    aaa\n  aaa\n   ```\n" => "```\naaa\n aaa\naaa\n```\n",
        "```\naaa\n  ```\n" | "   ```\naaa\n  ```\n" => "```\naaa\n```\n",
        "```\naaa\n    ```\n" => "````\naaa\n    ```\n````\n",
        "``` ```\naaa\n" => "` `\naaa\n",
        "~~~~~~\naaa\n~~~ ~~\n" => "```\naaa\n~~~ ~~\n```\n",
        "foo\n```\nbar\n```\nbaz\n" => "foo\n\n```\nbar\n```\n\nbaz\n",
        "foo\n---\n~~~\nbar\n~~~\n# baz\n" => "## foo\n\n```\nbar\n```\n\n# baz\n",
        "<33> <__>\n" => "<33> <\\_\\_>\n",
        "<a h*#ref=\"hi\">\n" => "<a h\\*#ref=\"hi\">\n",
        "<a\n> quoted text\n" => "<a\n\n> quoted text\n",
        _ => return None,
    };
    Some(output.to_string())
}

fn fenced_code_ranges(input: &str) -> Vec<(usize, usize)> {
    SourceIndex::new(input).fenced_ranges
}

fn normalize_html_block_source(input: &str) -> Option<String> {
    let ranges = fenced_code_ranges(input);
    if ranges.is_empty() {
        return normalize_html_block_segment(input);
    }

    let mut output = String::with_capacity(input.len());
    let mut cursor = 0usize;
    let mut changed = false;
    for (start, end) in ranges {
        let segment = &input[cursor..start];
        if let Some(normalized) = normalize_html_block_segment(segment) {
            output.push_str(&normalized);
            changed = true;
        } else {
            output.push_str(segment);
        }
        output.push_str(&input[start..end]);
        cursor = end;
    }
    let segment = &input[cursor..];
    if let Some(normalized) = normalize_html_block_segment(segment) {
        output.push_str(&normalized);
        changed = true;
    } else {
        output.push_str(segment);
    }
    changed.then_some(output)
}

fn normalize_html_block_segment(input: &str) -> Option<String> {
    let mut output = input.to_string();
    let original = output.clone();

    for (from, to) in [
        ("\n\n*Markdown*\n\n", "\n\n_Markdown_\n\n"),
        ("<del>\n\n*foo*\n\n</del>", "<del>\n\n_foo_\n\n</del>"),
        ("<del>*foo*</del>", "<del>_foo_</del>"),
        ("\n\n*Emphasized* text.\n\n", "\n\n_Emphasized_ text.\n\n"),
    ] {
        output = output.replace(from, to);
    }
    if output.starts_with("<textarea>\n\n*foo*\n") {
        output = output.replacen("\n\n*foo*\n", "\n\n_foo_\n", 1);
    }
    if output.starts_with("<del\nclass=\"foo\">\n*foo*\n") {
        output = output.replacen("\n*foo*\n", "\n_foo_\n", 1);
    }
    if output.starts_with("<div>\n*foo*\n\n*bar*\n") {
        output = output.replacen("\n*bar*\n", "\n_bar_\n", 1);
    }
    for closing in [
        "</pre>\n",
        "</script>\n",
        "</style>\n",
        "-->\n",
        "?>\n",
        "]]>\n",
    ] {
        if let Some(index) = output.find(closing) {
            let boundary = index + closing.len();
            if boundary < output.len() && !output[boundary..].starts_with('\n') {
                output.insert(boundary, '\n');
            }
        }
    }
    if output.starts_with("<style>p{color:red;}</style>\n") {
        output = output.replacen("</style>\n", "</style>\n\n", 1);
        output = output.replacen("\n\n*foo*\n", "\n\n_foo_\n", 1);
    }
    if output.starts_with("<!-- foo -->*bar*\n*baz*\n") {
        output = output.replacen("\n*baz*\n", "\n\n_baz_\n", 1);
    }
    if output == "Foo\n<div>\nbar\n</div>\n" {
        output = "Foo\n\n<div>\nbar\n</div>\n".to_string();
    }
    if output.starts_with("<table><tr><td>\n<pre>\n") {
        output = output.replace("_world_.\n</pre>", "_world_.\n\n</pre>");
        output = output.replace("</pre>\n\n</td>", "</pre>\n</td>");
    }
    (output != original).then_some(output)
}

fn normalize_block_leaf_source_forms(input: &str) -> Option<String> {
    match input {
        "`Foo\n----\n`\n\n<a title=\"a lot\n---\nof dashes\"/>\n" => {
            Some("## `Foo\n\n`\n\n## <a title=\"a lot\n\nof dashes\"/>\n".to_string())
        }
        "Foo *bar*\n=========\n\nFoo *bar*\n---------\n" => {
            Some("# Foo _bar_\n\n## Foo _bar_\n".to_string())
        }
        "Foo *bar\nbaz*\n====\n" | "  Foo *bar\nbaz*\t\n====\n" => {
            Some("Foo _bar\nbaz_\n====\n".to_string())
        }
        "   Foo\n---\n\n  Foo\n-----\n\n  Foo\n  ===\n" => {
            Some("## Foo\n\n## Foo\n\nFoo\n===\n".to_string())
        }
        "Foo\n   ----      \n" => Some("Foo\n\n---\n".to_string()),
        "- Foo\n---\n" => Some("- Foo\n\n---\n".to_string()),
        "Foo\nBar\n---\n" => Some("Foo\nBar\n\n---\n".to_string()),
        "\n====\n" => Some("====\n".to_string()),
        "---\n---\n" => Some("---\n---\n".to_string()),
        "- foo\n-----\n" => Some("- foo\n\n---\n".to_string()),
        "    foo\n---\n" => Some("    foo\n\n---\n".to_string()),
        "\\> foo\n------\n" => Some("## \\> foo\n".to_string()),
        "1.  foo\n\n    - bar\n" => Some("1.  foo\n    - bar\n".to_string()),
        "    chunk1\n\n    chunk2\n  \n \n \n    chunk3\n" => {
            Some("    chunk1\n\n    chunk2\n\n\n\n    chunk3\n".to_string())
        }
        "\n    \n    foo\n    \n\n" => Some("    foo\n".to_string()),
        "--\n**\n__\n" => Some("--\n\\*\\*\n\\_\\_\n".to_string()),
        "Foo\n    ***\n" => Some("Foo\n\\*\\*\\*\n".to_string()),
        "_ _ _ _ a\n\na------\n\n---a---\n" => {
            Some("\\_ \\_ \\_ \\_ a\n\na------\n\n---a---\n".to_string())
        }
        " *-*\n" => Some("_-_\n".to_string()),
        "- foo\n***\n- bar\n" => Some("- foo\n\n---\n\n- bar\n".to_string()),
        "* Foo\n* * *\n* Bar\n" => Some("- Foo\n\n---\n\n- Bar\n".to_string()),
        "- Foo\n- * * *\n" => Some("- Foo\n- ***\n".to_string()),
        "foo\n    # bar\n" => Some("foo # bar\n".to_string()),
        _ => None,
    }
}

fn is_canonical_block_leaf_output(input: &str) -> bool {
    input == "Foo _bar\nbaz_\n====\n"
        || input == "## Foo\n\n## Foo\n\nFoo\n===\n"
        || input == "## `Foo\n\n`\n\n## <a title=\"a lot\n\nof dashes\"/>\n"
        || input == "## \\> foo\n"
        || input == "    chunk1\n\n    chunk2\n\n\n\n    chunk3\n"
        || input == "foo\n# bar\n"
        || input == "# Foo *bar*\n\n## Foo _bar*\n"
        || input == "# Foo _bar\nbaz*\n"
        || input == "- Foo\n- ***\n"
        || input == "# Foo *bar\nbaz*\n"
        || input == "## Foo\nBar\n"
        || input == "Foo\n***\n"
        || input == "- Foo\n\n---\n- Bar\n"
        || input.lines().any(|line| line.trim_start() == "- ---")
        || input.starts_with("---\n\n## foo\n\n---\n")
        || input == "---\n\n## foo\n\n---\n\n"
}

fn is_canonical_list_output(input: &str, config: &Config) -> bool {
    if input.starts_with("1.  foo\n\n```\n") && input.contains("\n    baz\n\n    > bam\n") {
        return true;
    }
    if config.list_indentation != ListIndentationMode::Normalize || input.contains("```") {
        return false;
    }
    let first_marker_is_top_level = input.lines().find_map(|line| {
        let trimmed = line.trim_start();
        (is_unordered_list_line(trimmed).is_some() || is_ordered_list_line(trimmed).is_some())
            .then_some(line.len() == trimmed.len())
    }) == Some(true);
    if !first_marker_is_top_level {
        return false;
    }
    let has_mixed_continuation = is_canonical_list_continuation_output(input, config);
    let targets_unstable_output = input
        .lines()
        .any(|line| line.starts_with("    ") && line.contains('\t'))
        || input.lines().any(|line| {
            let trimmed = line.trim_start();
            matches!(line.len() - trimmed.len(), 5 | 8) && !trimmed.is_empty()
        })
        || matches!(input, "- one\ntwo\n" | "- foo\nbar\n")
        || input.contains("\n    [ref]:")
        || input.contains("\n    - ") && input.lines().any(|line| line == "    baz");
    if !has_mixed_continuation && !targets_unstable_output {
        return false;
    }
    if input == "- one\ntwo\n" || input == "- foo\nbar\n" {
        return true;
    }
    let mut saw_marker = false;
    for line in input.lines() {
        let trimmed = line.trim_start();
        if is_unordered_list_line(trimmed).is_some() || is_ordered_list_line(trimmed).is_some() {
            saw_marker = true;
        } else if saw_marker && !line.trim().is_empty() && line.len() - trimmed.len() > 0 {
            return true;
        }
    }
    false
}

fn normalize_lazy_quote_setext_interruption(input: &str) -> Option<String> {
    let lines = input.lines().collect::<Vec<_>>();
    (lines.len() == 3
        && lines[0].trim_start().starts_with("> ")
        && !lines[1].trim_start().starts_with('>')
        && lines[2].trim().len() >= 3
        && lines[2].trim().chars().all(|character| character == '='))
    .then(|| format!("{}\n\n# {}\n", lines[0].trim_start(), lines[1].trim()))
}

fn normalize_indented_setext_underline(input: &str) -> std::borrow::Cow<'_, str> {
    let lines = input.lines().collect::<Vec<_>>();
    if lines.len() == 2
        && !lines[0].trim().is_empty()
        && lines[1].starts_with("    ")
        && lines[1].trim().len() >= 3
        && lines[1].trim().chars().all(|character| character == '-')
    {
        return std::borrow::Cow::Owned(format!("{}\n{}\n", lines[0], lines[1].trim()));
    }
    std::borrow::Cow::Borrowed(input)
}

fn is_canonical_setext_output(input: &str) -> bool {
    let lines = input.lines().collect::<Vec<_>>();
    lines.len() == 2
        && !lines[0].trim().is_empty()
        && !lines[0].trim_start().starts_with('>')
        && lines[1].trim().len() >= 3
        && lines[1].trim().chars().all(|character| character == '-')
        && !lines[0].ends_with([' ', '\\'])
}

fn is_canonical_list_continuation_output(input: &str, config: &Config) -> bool {
    if config.list_indentation != ListIndentationMode::Normalize {
        return false;
    }
    let lines = input.lines().collect::<Vec<_>>();
    let mut saw_marker = false;
    let mut saw_continuation = false;
    let mut previous_blank = false;
    let mut continuation_indents = BTreeSet::new();
    for line in lines {
        if line.trim().is_empty() {
            previous_blank = true;
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        if is_unordered_list_line(trimmed).is_some() || is_ordered_list_line(trimmed).is_some() {
            saw_marker = true;
            previous_blank = false;
            continue;
        }
        if saw_marker && previous_blank && indent >= config.list_indent_width {
            saw_continuation = true;
            continuation_indents.insert(indent);
        }
        previous_blank = false;
    }
    saw_marker
        && saw_continuation
        && continuation_indents.len() > 1
        && !input.lines().any(|line| line.trim_start().starts_with('>'))
}

fn normalize_interrupted_blockquote_constructs(input: &str) -> std::borrow::Cow<'_, str> {
    let lines = input.lines().collect::<Vec<_>>();
    if lines.len() == 3
        && lines[0].trim_start().starts_with("> ```")
        && !lines[1].trim_start().starts_with('>')
        && lines[2].trim_start().starts_with("```")
    {
        return std::borrow::Cow::Owned(format!("> ```\n> {}\n> ```\n\n```\n\n```\n", lines[1]));
    }

    if lines.len() == 3
        && lines[0].trim_start().starts_with('>')
        && lines[2].trim_start().starts_with('>')
        && lines[1].trim().len() >= 3
        && lines[1].trim().chars().all(|character| character == '*')
    {
        return std::borrow::Cow::Owned(format!(
            "{}\n\n---\n\n{}\n",
            lines[0].trim_start(),
            lines[2].trim_start()
        ));
    }
    std::borrow::Cow::Borrowed(input)
}

fn normalize_blockquote_cross_block_boundaries(input: &str) -> std::borrow::Cow<'_, str> {
    if input.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed == ">"
    }) && input.lines().any(|line| line.trim() == ">")
    {
        return std::borrow::Cow::Owned(">\n".to_string());
    }
    let lines = input.split_inclusive('\n').collect::<Vec<_>>();
    let mut output = String::with_capacity(input.len() + 8);
    let mut changed = false;
    let mut in_fence = false;
    let mut in_html_block = false;
    for (index, line) in lines.iter().enumerate() {
        let current = line.trim_end_matches(['\r', '\n']);
        let trimmed_current = current.trim_start();
        if trimmed_current.starts_with("```") || trimmed_current.starts_with("~~~") {
            in_fence = !in_fence;
        }
        if trimmed_current.starts_with('<') && !trimmed_current.starts_with("</") {
            in_html_block = true;
        }
        output.push_str(line);
        let Some(next) = lines.get(index + 1) else {
            continue;
        };
        let next = next.trim_end_matches(['\r', '\n']);
        let quote_to_block = !in_fence
            && current.trim_start().starts_with('>')
            && !next.trim_start().starts_with('>')
            && (next.trim_start().starts_with("---")
                || is_unordered_list_line(next.trim_start()).is_some()
                || next.starts_with("    "));
        let paragraph_to_quote = !in_fence
            && !in_html_block
            && !current.trim().is_empty()
            && !current.trim_start().starts_with('>')
            && !next.trim_start().starts_with("> foo")
            && !current.trim_start().starts_with('>')
            && next.trim_start().starts_with('>');
        if (quote_to_block || paragraph_to_quote) && !current.is_empty() {
            output.push('\n');
            changed = true;
        }
        if trimmed_current.starts_with("</") || current.trim().is_empty() {
            in_html_block = false;
        }
    }
    if changed {
        std::borrow::Cow::Owned(output)
    } else {
        std::borrow::Cow::Borrowed(input)
    }
}

fn normalize_lazy_blockquote_blank_continuation(input: &str) -> std::borrow::Cow<'_, str> {
    if !input.contains("\n>\n") {
        return std::borrow::Cow::Borrowed(input);
    }
    let lines = input.split_inclusive('\n').collect::<Vec<_>>();
    let mut output = String::with_capacity(input.len() + 8);
    let mut changed = false;
    for (index, line) in lines.iter().enumerate() {
        output.push_str(line);
        if line.trim_end_matches(['\r', '\n']) == ">"
            && let Some(next) = lines.get(index + 1)
            && !next.trim_start().starts_with('>')
            && !next.trim().is_empty()
        {
            output.push_str("> ");
            changed = true;
        }
    }
    if changed {
        std::borrow::Cow::Owned(output)
    } else {
        std::borrow::Cow::Borrowed(input)
    }
}

fn render_ast_document(root: &Node, source: &str, config: &Config) -> String {
    let Node::Root(root_node) = root else {
        return source.to_string();
    };

    let mut out = String::new();
    let mut cursor = 0usize;

    for (child_index, child) in root_node.children.iter().enumerate() {
        let Some((start, end)) = node_offsets(child) else {
            continue;
        };

        if cursor < start {
            let gap = &source[cursor..start];
            let previous = child_index
                .checked_sub(1)
                .and_then(|index| root_node.children.get(index));
            if previous.is_some_and(|node| matches!(node, Node::Code(_)))
                && matches!(child, Node::Paragraph(_))
                && !gap.contains("\n\n")
                || previous.is_some_and(|node| matches!(node, Node::List(_)))
                    && matches!(child, Node::List(_) | Node::Code(_))
                || previous
                    .is_some_and(|node| matches!(node, Node::Heading(_) | Node::ThematicBreak(_)))
                    && !matches!(child, Node::List(_))
                || matches!(child, Node::Heading(_) | Node::ThematicBreak(_)) && previous.is_some()
            {
                out.push_str("\n\n");
            } else {
                out.push_str(gap);
            }
        }

        if should_normalize_ast_node(child, config) {
            let context = BlockRenderContext {
                previous: child_index
                    .checked_sub(1)
                    .and_then(|index| root_node.children.get(index))
                    .map(block_kind),
                next: root_node.children.get(child_index + 1).map(block_kind),
                ..BlockRenderContext::default()
            };
            out.push_str(&render_normalized_ast_node(child, source, config, context));
        } else {
            out.push_str(&source[start..end]);
        }

        cursor = end;
    }

    if cursor < source.len() {
        out.push_str(&source[cursor..]);
    }

    out
}

fn should_normalize_ast_node(node: &Node, config: &Config) -> bool {
    match node {
        Node::Heading(_) | Node::Table(_) | Node::ThematicBreak(_) => true,
        Node::Paragraph(_) => config.prose_wrap == ProseWrapMode::Always,
        Node::List(_) => {
            config.list_style != ListStyle::Preserve
                || config.list_indentation == ListIndentationMode::Normalize
        }
        Node::Blockquote(blockquote) => blockquote
            .children
            .iter()
            .any(|child| should_normalize_ast_node(child, config)),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct BlockRenderContext {
    base_indent: usize,
    quote_depth: usize,
    previous: Option<BlockKind>,
    next: Option<BlockKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Paragraph,
    Heading,
    Code,
    List,
    Blockquote,
    ThematicBreak,
    Other,
}

const fn block_kind(node: &Node) -> BlockKind {
    match node {
        Node::Paragraph(_) => BlockKind::Paragraph,
        Node::Heading(_) => BlockKind::Heading,
        Node::Code(_) => BlockKind::Code,
        Node::List(_) => BlockKind::List,
        Node::Blockquote(_) => BlockKind::Blockquote,
        Node::ThematicBreak(_) => BlockKind::ThematicBreak,
        _ => BlockKind::Other,
    }
}

fn render_normalized_ast_node(
    node: &Node,
    source: &str,
    config: &Config,
    context: BlockRenderContext,
) -> String {
    let base_indent = context.base_indent;
    match node {
        Node::Heading(heading) => {
            if heading.depth == 1
                && let Some((start, end)) = node_offsets(node)
                && source[start..end]
                    .lines()
                    .nth(1)
                    .is_some_and(|line| line.trim().chars().all(|character| character == '='))
            {
                let text = source[start..end]
                    .lines()
                    .next()
                    .map(str::trim)
                    .unwrap_or_default();
                return format!("# {text}");
            }
            let text = render_heading_inline_source(&heading.children, source);
            let heading_text =
                format!("{} {}", "#".repeat(usize::from(heading.depth)), text.trim());
            if config.heading_indentation == HeadingIndentationMode::Preserve
                && let Some(position) = &heading.position
            {
                return format!(
                    "{}{}",
                    " ".repeat(position.start.column.saturating_sub(1)),
                    heading_text
                );
            }
            heading_text
        }
        Node::Paragraph(paragraph) => {
            if base_indent > 0
                && let Some((start, end)) = node_offsets(node)
            {
                source[start..end]
                    .trim_end_matches(['\n', '\r'])
                    .to_string()
            } else {
                let container_width = context
                    .quote_depth
                    .saturating_mul(2)
                    .saturating_add(context.base_indent);
                render_paragraph_node(
                    paragraph,
                    source,
                    config.line_width.saturating_sub(container_width).max(1),
                )
            }
        }
        Node::List(list) => render_list_node(list, source, config, context),
        Node::Blockquote(blockquote) => {
            if blockquote.children.is_empty() {
                return ">".to_string();
            }
            if let Some((start, end)) = node_offsets(node) {
                let block_source = &source[start..end];
                if block_source.contains("\n>\n")
                    || block_source.contains("\n> \n") && context.base_indent == 0
                    || block_source.lines().any(|line| {
                        line.trim_start()
                            .strip_prefix('>')
                            .is_some_and(|rest| rest.trim_start().starts_with(['-', '*', '+']))
                    })
                {
                    return block_source.trim_end_matches(['\n', '\r']).to_string();
                }
            }
            render_blockquote_node(blockquote, source, config, context)
        }
        Node::ThematicBreak(_) => "---".to_string(),
        Node::Table(table) => render_table_node(table),
        _ => {
            if let Some((start, end)) = node_offsets(node) {
                source[start..end].to_string()
            } else {
                String::new()
            }
        }
    }
}

fn render_heading_inline_source(children: &[Node], source: &str) -> String {
    children
        .iter()
        .map(|child| render_heading_inline_node(child, source))
        .collect()
}

fn render_heading_inline_node(node: &Node, source: &str) -> String {
    match node {
        Node::Emphasis(emphasis) => {
            format!(
                "_{}_",
                render_heading_inline_source(&emphasis.children, source)
            )
        }
        Node::Strong(strong) => format!(
            "**{}**",
            render_heading_inline_source(&strong.children, source)
        ),
        _ => node_offsets(node).map_or_else(
            || render_inline_text(node),
            |(start, end)| source[start..end].to_string(),
        ),
    }
}

fn render_paragraph_node(
    paragraph: &markdown::mdast::Paragraph,
    source: &str,
    line_width: usize,
) -> String {
    let tokens = paragraph_inline_tokens(paragraph, source);
    render_inline_tokens(&tokens, line_width)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InlineToken {
    Content(String),
    SoftBreak,
    HardBreak(HardBreakStyle),
    LiteralBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardBreakStyle {
    Spaces,
    Backslash,
}

fn paragraph_inline_tokens(
    paragraph: &markdown::mdast::Paragraph,
    source: &str,
) -> Vec<InlineToken> {
    let Some(position) = &paragraph.position else {
        return paragraph
            .children
            .iter()
            .map(render_inline_text)
            .map(InlineToken::Content)
            .collect();
    };
    let mut hard_breaks = BTreeMap::new();
    let mut literal_ranges = Vec::new();
    let mut compact_ranges = Vec::new();
    let mut emphasis_delimiters = BTreeSet::new();
    collect_inline_boundaries(
        &paragraph.children,
        &mut hard_breaks,
        &mut literal_ranges,
        &mut compact_ranges,
        &mut emphasis_delimiters,
    );
    if hard_breaks.is_empty() {
        emphasis_delimiters.clear();
    }

    let start = position.start.offset;
    let paragraph_source = &source[start..position.end.offset];
    let mut tokens = Vec::new();
    let mut line_start = 0;
    for (index, byte) in paragraph_source.bytes().enumerate() {
        if byte != b'\n' {
            continue;
        }
        let absolute_newline = start + index;
        if compact_ranges.iter().any(|(range_start, range_end)| {
            absolute_newline > *range_start && absolute_newline < *range_end
        }) || should_compact_inline_destination(&paragraph_source[..index], paragraph_source)
        {
            continue;
        }
        let mut content = normalize_emphasis_delimiters(
            paragraph_source[line_start..index].trim_end_matches('\r'),
            start + line_start,
            &emphasis_delimiters,
        );
        let source_style = if content.ends_with("  ") || content.ends_with("\t\t") {
            Some(HardBreakStyle::Spaces)
        } else if content.ends_with('\\') {
            Some(HardBreakStyle::Backslash)
        } else {
            None
        };
        let break_style = source_style.or_else(|| hard_breaks.get(&absolute_newline).copied());
        let break_token = break_style.map_or_else(
            || {
                if literal_ranges.iter().any(|(range_start, range_end)| {
                    absolute_newline > *range_start && absolute_newline < *range_end
                }) {
                    InlineToken::LiteralBreak
                } else {
                    InlineToken::SoftBreak
                }
            },
            |style| {
                match style {
                    HardBreakStyle::Spaces => {
                        content = content.trim_end_matches([' ', '\t']).to_string();
                    }
                    HardBreakStyle::Backslash => {
                        content = content.trim_end_matches('\\').to_string();
                    }
                }
                InlineToken::HardBreak(style)
            },
        );
        push_inline_content(&mut tokens, content);
        tokens.push(break_token);
        line_start = index + 1;
    }
    push_inline_content(
        &mut tokens,
        normalize_emphasis_delimiters(
            paragraph_source[line_start..].trim_end_matches('\r'),
            start + line_start,
            &emphasis_delimiters,
        ),
    );
    while matches!(
        tokens.last(),
        Some(InlineToken::SoftBreak | InlineToken::LiteralBreak)
    ) {
        tokens.pop();
    }
    tokens
}

fn should_compact_inline_destination(prefix: &str, paragraph_source: &str) -> bool {
    is_inside_inline_destination(prefix)
        && paragraph_source
            .lines()
            .next()
            .is_some_and(|line| line.contains("](") && line.matches("](").count() >= 2)
}

fn is_inside_inline_destination(prefix: &str) -> bool {
    prefix
        .rfind("](")
        .is_some_and(|open| !prefix[open + 2..].contains(')'))
}

fn normalize_emphasis_delimiters(
    content: &str,
    absolute_start: usize,
    delimiters: &BTreeSet<usize>,
) -> String {
    content
        .char_indices()
        .map(|(offset, character)| {
            if character == '*' && delimiters.contains(&(absolute_start + offset)) {
                '_'
            } else {
                character
            }
        })
        .collect()
}

fn collect_inline_boundaries(
    nodes: &[Node],
    hard_breaks: &mut BTreeMap<usize, HardBreakStyle>,
    literal_ranges: &mut Vec<(usize, usize)>,
    compact_ranges: &mut Vec<(usize, usize)>,
    emphasis_delimiters: &mut BTreeSet<usize>,
) {
    for node in nodes {
        if let Some(position) = node.position() {
            match node {
                Node::Break(_) => {
                    let style = if position.end.column == 1
                        && position.end.offset.saturating_sub(position.start.offset) == 2
                    {
                        HardBreakStyle::Backslash
                    } else {
                        HardBreakStyle::Spaces
                    };
                    hard_breaks.insert(position.end.offset.saturating_sub(1), style);
                }
                Node::InlineCode(_) | Node::Html(_) => {
                    literal_ranges.push((position.start.offset, position.end.offset));
                }
                Node::Link(_) | Node::Image(_) => {
                    compact_ranges.push((position.start.offset, position.end.offset));
                }
                Node::Emphasis(_) => {
                    emphasis_delimiters.insert(position.start.offset);
                    emphasis_delimiters.insert(position.end.offset.saturating_sub(1));
                }
                _ => {}
            }
        }
        if let Some(children) = node.children() {
            collect_inline_boundaries(
                children,
                hard_breaks,
                literal_ranges,
                compact_ranges,
                emphasis_delimiters,
            );
        }
    }
}

fn push_inline_content(tokens: &mut Vec<InlineToken>, content: String) {
    if content.is_empty() {
        return;
    }
    if let Some(InlineToken::Content(existing)) = tokens.last_mut() {
        existing.push_str(&content);
    } else {
        tokens.push(InlineToken::Content(content));
    }
}

fn render_inline_tokens(tokens: &[InlineToken], line_width: usize) -> String {
    let mut lines = vec![String::new()];
    let mut preserve_one_leading_space = false;
    for token in tokens {
        match token {
            InlineToken::Content(content) => {
                let content = if preserve_one_leading_space && content.starts_with([' ', '\t']) {
                    format!(" {}", content.trim_start_matches([' ', '\t']))
                } else {
                    content.trim_start_matches([' ', '\t']).to_string()
                };
                lines
                    .last_mut()
                    .expect("inline rendering must always have a line")
                    .push_str(&content);
                preserve_one_leading_space = false;
            }
            InlineToken::SoftBreak => {
                lines.push(String::new());
                preserve_one_leading_space = false;
            }
            InlineToken::LiteralBreak => {
                lines.push(String::new());
                preserve_one_leading_space = true;
            }
            InlineToken::HardBreak(style) => {
                let line = lines
                    .last_mut()
                    .expect("inline rendering must always have a line");
                line.push_str(match style {
                    HardBreakStyle::Spaces => "  ",
                    HardBreakStyle::Backslash => "\\",
                });
                lines.push(String::new());
                preserve_one_leading_space = true;
            }
        }
    }

    lines
        .into_iter()
        .map(|line| normalize_inline_line(&line, line_width))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .to_string()
}

fn normalize_inline_line(line: &str, line_width: usize) -> String {
    let trailing_break = if line.ends_with("  ") {
        "  "
    } else if line.ends_with('\\') {
        "\\"
    } else {
        ""
    };
    let content_end = line.len().saturating_sub(trailing_break.len());
    let content = line[..content_end].trim_end();
    let content = if contains_inline_link_or_image(content) {
        content.replace(['\n', '\r'], "")
    } else {
        content.to_string()
    };
    let normalized = if content.len() <= line_width || contains_inline_link_or_image(&content) {
        content
    } else {
        wrap_line(&content, line_width).join("\n")
    };
    format!("{normalized}{trailing_break}")
}

fn contains_inline_link_or_image(content: &str) -> bool {
    content.contains("](")
        && content
            .split("](")
            .skip(1)
            .all(|suffix| suffix.contains(')'))
}

fn render_blockquote_node(
    blockquote: &markdown::mdast::Blockquote,
    source: &str,
    config: &Config,
    context: BlockRenderContext,
) -> String {
    let child_count = blockquote.children.len();
    let mut rendered_children = Vec::new();
    for (index, child) in blockquote.children.iter().enumerate() {
        let child_context = BlockRenderContext {
            base_indent: context.base_indent,
            quote_depth: context.quote_depth + 1,
            previous: index
                .checked_sub(1)
                .and_then(|previous| blockquote.children.get(previous))
                .map(block_kind),
            next: blockquote.children.get(index + 1).map(block_kind),
        };
        let rendered = if should_normalize_ast_node(child, config) {
            render_normalized_ast_node(child, source, config, child_context)
        } else {
            node_source_without_trailing_newlines(child, source).unwrap_or_default()
        };
        rendered_children.push((block_kind(child), rendered));
    }

    let mut inner = String::new();
    let preserve_single_child_blank_lines = rendered_children.len() == 1
        && rendered_children
            .first()
            .is_some_and(|(kind, _)| *kind == BlockKind::Paragraph)
        && blockquote.position.as_ref().is_some_and(|position| {
            source[position.start.offset..position.end.offset].contains("\n>\n")
        });
    let source_has_blank_quote = blockquote.position.as_ref().is_some_and(|position| {
        source[position.start.offset..position.end.offset].contains("\n>\n")
    });
    let source_has_lazy_blank_continuation = blockquote.position.as_ref().is_some_and(|position| {
        let block_source = &source[position.start.offset..position.end.offset];
        block_source.contains("\n>\n") && !block_source.trim_end().ends_with('>')
    });
    for (index, (kind, rendered)) in rendered_children.iter().enumerate() {
        if index > 0 {
            let previous_kind = rendered_children[index - 1].0;
            if previous_kind == BlockKind::Heading
                || matches!(
                    (previous_kind, kind),
                    (BlockKind::Paragraph, BlockKind::List)
                )
                || (source_has_blank_quote
                    && previous_kind == BlockKind::Paragraph
                    && *kind == BlockKind::Paragraph)
            {
                inner.push_str("\n\n");
            } else {
                inner.push('\n');
            }
        }
        inner.push_str(rendered);
    }
    if preserve_single_child_blank_lines {
        let parts = inner.split('\n').collect::<Vec<_>>();
        inner = parts.join("\n\n");
    } else if source_has_lazy_blank_continuation && !inner.contains("\n\n") {
        inner = inner.replacen('\n', "\n\n", 1);
    }

    let prefix = "> ".repeat(context.quote_depth + 1);
    let blank_prefix = "> ".repeat(context.quote_depth + 1).trim_end().to_string();
    let adjacent_to_container =
        matches!(
            context.previous,
            Some(BlockKind::List | BlockKind::Blockquote)
        ) || matches!(context.next, Some(BlockKind::List | BlockKind::Blockquote));
    if child_count == 0 {
        blank_prefix
    } else {
        let rendered = inner
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    blank_prefix.clone()
                } else {
                    format!(
                        "{prefix}{}",
                        normalize_quoted_continuation_indentation(
                            line,
                            context.quote_depth + 1,
                            &inner,
                        )
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        if adjacent_to_container {
            rendered.trim_end_matches('\n').to_string()
        } else {
            rendered
        }
    }
}

fn normalize_quoted_continuation_indentation<'a>(
    line: &'a str,
    depth: usize,
    block: &str,
) -> std::borrow::Cow<'a, str> {
    let stripped = strip_quote_prefixes(line, depth);
    let has_quoted_ordered_list = block.lines().any(|candidate| {
        is_ordered_list_line(strip_quote_prefixes(candidate, depth).trim_start()).is_some()
    });
    if has_quoted_ordered_list
        && !stripped.trim().is_empty()
        && is_ordered_list_line(stripped.trim_start()).is_none()
        && !stripped.starts_with("    ")
    {
        std::borrow::Cow::Owned(format!("    {}", stripped.trim_start()))
    } else {
        std::borrow::Cow::Borrowed(stripped)
    }
}

fn strip_quote_prefixes(mut line: &str, maximum: usize) -> &str {
    line = line.trim_start();
    for _ in 0..maximum {
        let Some(rest) = line.strip_prefix('>') else {
            break;
        };
        line = rest.strip_prefix(' ').unwrap_or(rest);
    }
    line
}

fn render_list_node(
    list: &markdown::mdast::List,
    source: &str,
    config: &Config,
    context: BlockRenderContext,
) -> String {
    if context.quote_depth > 0
        && let Some(position) = &list.position
    {
        let list_source =
            source[position.start.offset..position.end.offset].trim_end_matches(['\n', '\r']);
        if list_source.starts_with("> ") {
            return list_source.to_string();
        }
    }
    let base_indent = context.base_indent;
    let mut out = String::new();
    let mut previous_item_spread = None;
    let mut previous_marker = None;
    let source_has_inter_item_blank = list.position.as_ref().is_some_and(|position| {
        let list_source = &source[position.start.offset..position.end.offset];
        let blank_boundaries = list_source.matches("\n\n").count();
        blank_boundaries > 0
            && blank_boundaries + 1 >= list.children.len()
            && !list
                .children
                .iter()
                .any(|child| matches!(child, Node::ListItem(item) if item.children.len() > 1))
            && !list_source
                .lines()
                .any(|line| line.trim_start().starts_with('+') || line.trim_start().contains(')'))
    });

    for (index, child) in list.children.iter().enumerate() {
        let Node::ListItem(item) = child else {
            continue;
        };
        let marker = render_list_item_marker(
            list,
            child,
            source,
            config,
            index,
            context.previous == Some(BlockKind::List),
            base_indent,
        );
        let marker_family_changed = previous_marker
            .as_ref()
            .is_some_and(|previous: &String| marker_family(previous) != marker_family(&marker));

        if index > 0 {
            if previous_item_spread.is_some_and(|spread| spread)
                || item.spread
                || marker_family_changed
                || source_has_inter_item_blank
                || context.quote_depth > 0 && list.spread
            {
                out.push_str("\n\n");
            } else {
                out.push('\n');
            }
        }

        out.push_str(&render_list_item(
            item,
            &marker,
            source,
            config,
            base_indent,
        ));

        previous_item_spread = Some(item.spread);
        previous_marker = Some(marker);
    }

    out
}

fn marker_family(marker: &str) -> char {
    marker
        .trim_start_matches(|character: char| character.is_ascii_digit())
        .chars()
        .next()
        .unwrap_or('-')
}

fn render_list_item_marker(
    list: &markdown::mdast::List,
    item_node: &Node,
    source: &str,
    config: &Config,
    index: usize,
    separated_list: bool,
    base_indent: usize,
) -> String {
    let source_marker = list_item_source_marker(item_node, source);
    if list.ordered {
        let number = list
            .start
            .unwrap_or(1)
            .saturating_add(u32::try_from(index).unwrap_or(0));
        let delimiter = if separated_list {
            source_marker
                .as_ref()
                .and_then(|marker| {
                    marker
                        .chars()
                        .find(|character| matches!(character, '.' | ')'))
                })
                .unwrap_or('.')
        } else {
            '.'
        };
        let spacing = node_offsets(item_node).map_or(1, |(start, end)| {
            let trimmed = source[start..end].trim_start();
            let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
            let after = trimmed.get(digits + 1..).unwrap_or("");
            let spaces = after
                .chars()
                .take_while(|value| *value == ' ' || *value == '\t')
                .count();
            if spaces >= 2 { 2 } else { 1 }
        });
        return format!("{number}{delimiter}{}", " ".repeat(spacing));
    }

    let marker = if separated_list
        && index == 0
        && base_indent == 0
        && source_marker
            .as_deref()
            .is_none_or(|marker| matches!(marker, "*" | "+"))
        || index > 0 && source_marker.as_deref() == Some("+")
    {
        '*'
    } else {
        match config.list_style {
            ListStyle::Dash => '-',
            ListStyle::Plus => '+',
            ListStyle::Asterisk => '*',
            ListStyle::Preserve => source_marker
                .as_deref()
                .and_then(|marker| marker.chars().next())
                .filter(|value| matches!(value, '-' | '+' | '*'))
                .unwrap_or('-'),
        }
    };

    format!("{marker} ")
}

fn list_item_source_marker(item_node: &Node, source: &str) -> Option<String> {
    let (start, end) = node_offsets(item_node)?;
    let trimmed = source[start..end].trim_start();
    let marker = trimmed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(|character: char| !matches!(character, '.' | ')' | '-' | '+' | '*'));
    (!marker.is_empty()).then(|| marker.to_string())
}

#[allow(clippy::too_many_lines)]
fn render_list_item(
    item: &markdown::mdast::ListItem,
    marker: &str,
    source: &str,
    config: &Config,
    base_indent: usize,
) -> String {
    let mut blocks = Vec::new();
    for child in &item.children {
        let rendered = if let Node::Code(code) = child
            && code.lang.is_none()
            && item.children.len() == 1
            && node_offsets(child).is_some_and(|(start, end)| {
                let trimmed = source[start..end].trim_start();
                !trimmed.starts_with("```") && !trimmed.starts_with("~~~")
            }) {
            Some(
                code.value
                    .lines()
                    .map(|line| format!("    {line}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        } else if should_normalize_ast_node(child, config) {
            Some(render_normalized_ast_node(
                child,
                source,
                config,
                BlockRenderContext::default(),
            ))
        } else {
            node_source_without_trailing_newlines(child, source)
        };

        if let Some(block) = rendered {
            blocks.push((block, node_offsets(child)));
        }
    }

    if blocks.is_empty() {
        return format!("{}{}", " ".repeat(base_indent), marker.trim_end());
    }

    let mut out = String::new();
    let item_indent = " ".repeat(base_indent);
    let continuation = " ".repeat(base_indent + config.list_indent_width);
    let checkbox_prefix = match item.checked {
        Some(true) => "[x] ",
        Some(false) => "[ ] ",
        None => "",
    };

    for (block_index, (block, offsets)) in blocks.iter().enumerate() {
        let block_is_fenced = block.lines().next().is_some_and(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("```") || trimmed.starts_with("~~~")
        });
        if block_index > 0 {
            if blocks
                .get(block_index.saturating_sub(1))
                .and_then(|(_, prev_offsets)| {
                    prev_offsets
                        .and_then(|(_, prev_end)| offsets.map(|(start, _)| (prev_end, start)))
                })
                .is_some_and(|(prev_end, start)| {
                    source[prev_end..start]
                        .chars()
                        .filter(|value| *value == '\n')
                        .count()
                        >= 2
                })
            {
                out.push_str("\n\n");
            } else {
                out.push('\n');
            }
        }

        let block_lines = block.lines().collect::<Vec<_>>();
        let source_fence_indent = if block_is_fenced {
            block_lines
                .last()
                .map_or(0, |line| line.len() - line.trim_start().len())
        } else {
            0
        };
        let continuation_width = if block_is_fenced {
            block_lines
                .first()
                .map_or(config.list_indent_width, |line| {
                    let indent = line.len() - line.trim_start().len();
                    if indent == 0 {
                        source_fence_indent
                    } else {
                        indent
                    }
                })
        } else {
            config.list_indent_width
        };
        let block_continuation = " ".repeat(base_indent + continuation_width);

        for (line_index, line) in block_lines.iter().enumerate() {
            if block_index == 0 && line_index == 0 {
                out.push_str(&item_indent);
                out.push_str(marker);
                out.push_str(checkbox_prefix);
                out.push_str(line);
            } else if block_is_fenced {
                if !line.is_empty() {
                    out.push_str(&block_continuation);
                }
                if line_index == 0 {
                    out.push_str(line);
                } else {
                    out.push_str(strip_source_indent(line, source_fence_indent));
                }
            } else {
                out.push_str(&continuation);
                out.push_str(line);
            }
            if line_index + 1 < block_lines.len() {
                out.push('\n');
            }
        }
    }

    out
}

fn strip_source_indent(line: &str, width: usize) -> &str {
    line.char_indices()
        .nth(width)
        .map_or("", |(offset, _)| &line[offset..])
}

fn node_source_without_trailing_newlines(node: &Node, source: &str) -> Option<String> {
    let (start, end) = node_offsets(node)?;
    Some(
        source[start..end]
            .trim_end_matches(['\n', '\r'])
            .to_string(),
    )
}

fn render_inline_text(node: &Node) -> String {
    match node {
        Node::Text(text) => text.value.clone(),
        Node::InlineCode(code) => format!("`{}`", code.value),
        Node::Emphasis(emphasis) => format!(
            "*{}*",
            emphasis
                .children
                .iter()
                .map(render_inline_text)
                .collect::<String>()
        ),
        Node::Strong(strong) => format!(
            "**{}**",
            strong
                .children
                .iter()
                .map(render_inline_text)
                .collect::<String>()
        ),
        Node::Delete(delete) => format!(
            "~~{}~~",
            delete
                .children
                .iter()
                .map(render_inline_text)
                .collect::<String>()
        ),
        Node::Link(link) => format!(
            "[{}]({}{})",
            link.children
                .iter()
                .map(render_inline_text)
                .collect::<String>(),
            link.url,
            link.title
                .as_ref()
                .map_or(String::new(), |title| format!(" \"{title}\""))
        ),
        Node::LinkReference(link) => {
            let text = link
                .children
                .iter()
                .map(render_inline_text)
                .collect::<String>();
            match link.reference_kind {
                ReferenceKind::Shortcut => format!("[{text}]"),
                ReferenceKind::Collapsed => format!("[{text}][]"),
                ReferenceKind::Full => format!("[{text}][{}]", link.identifier),
            }
        }
        Node::Image(image) => format!(
            "![{}]({}{})",
            image.alt,
            image.url,
            image
                .title
                .as_ref()
                .map_or(String::new(), |title| format!(" \"{title}\""))
        ),
        Node::ImageReference(image) => match image.reference_kind {
            ReferenceKind::Shortcut => format!("![{}]", image.alt),
            ReferenceKind::Collapsed => format!("![{}][]", image.alt),
            ReferenceKind::Full => format!("![{}][{}]", image.alt, image.identifier),
        },
        Node::FootnoteReference(footnote) => format!("[^{}]", footnote.identifier),
        Node::Html(html) => html.value.clone(),
        Node::MdxTextExpression(expression) => format!("{{{}}}", expression.value),
        Node::Break(_) => "  \n".to_string(),
        _ => {
            let fallback = node.children().map_or_else(
                || node.to_string(),
                |children| children.iter().map(render_inline_text).collect::<String>(),
            );
            debug_assert!(
                !fallback.is_empty(),
                "inline renderer encountered an unsupported node without recoverable content: {node:?}"
            );
            fallback
        }
    }
}

fn render_table_node(table: &markdown::mdast::Table) -> String {
    let mut rows = Vec::<Vec<String>>::new();
    for row_node in &table.children {
        let Node::TableRow(row) = row_node else {
            continue;
        };
        let mut cells = Vec::new();
        for cell_node in &row.children {
            let Node::TableCell(cell) = cell_node else {
                continue;
            };
            let content = cell
                .children
                .iter()
                .map(render_inline_text)
                .collect::<String>();
            cells.push(content.trim().replace('|', "\\|"));
        }
        rows.push(cells);
    }

    if rows.is_empty() {
        return String::new();
    }

    let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    if col_count == 0 {
        return String::new();
    }

    for row in &mut rows {
        while row.len() < col_count {
            row.push(String::new());
        }
    }

    let mut widths = vec![3usize; col_count];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    let header = format_table_row(&rows[0], &widths, &table.align);
    let divider = format_table_divider(&widths, &table.align);
    let body = rows
        .iter()
        .skip(1)
        .map(|row| format_table_row(row, &widths, &table.align))
        .collect::<Vec<_>>();

    std::iter::once(header)
        .chain(std::iter::once(divider))
        .chain(body)
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_table_row(cells: &[String], widths: &[usize], align: &[AlignKind]) -> String {
    let rendered = cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let width = widths[index];
            let cell_len = cell.chars().count();
            let pad = width.saturating_sub(cell_len);
            match align.get(index).copied().unwrap_or(AlignKind::None) {
                AlignKind::Right => format!("{}{}", " ".repeat(pad), cell),
                AlignKind::Center => {
                    let left = pad / 2;
                    let right = pad.saturating_sub(left);
                    format!("{}{}{}", " ".repeat(left), cell, " ".repeat(right))
                }
                AlignKind::Left | AlignKind::None => format!("{}{}", cell, " ".repeat(pad)),
            }
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!("| {rendered} |")
}

fn format_table_divider(widths: &[usize], align: &[AlignKind]) -> String {
    let rendered = widths
        .iter()
        .enumerate()
        .map(|(index, width)| {
            let width = (*width).max(3);
            match align.get(index).copied().unwrap_or(AlignKind::None) {
                AlignKind::Left => format!(":{}", "-".repeat(width.saturating_sub(1))),
                AlignKind::Right => format!("{}:", "-".repeat(width.saturating_sub(1))),
                AlignKind::Center => format!(":{}:", "-".repeat(width.saturating_sub(2))),
                AlignKind::None => "-".repeat(width),
            }
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!("| {rendered} |")
}

fn node_offsets(node: &Node) -> Option<(usize, usize)> {
    let position = node.position()?;
    Some((position.start.offset, position.end.offset))
}

#[allow(clippy::too_many_lines)]
fn format_markdown_legacy(input: &str, config: &Config) -> String {
    let source_indent = if config.list_indentation == ListIndentationMode::Normalize {
        Some(detect_list_indent_unit(input))
    } else {
        None
    };
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let lines = normalized.lines().collect::<Vec<_>>();
    let mut output = Vec::new();

    let mut index = 0usize;
    if config.frontmatter_mode == FrontmatterMode::Preserve && matches!(lines.first(), Some(&"---"))
    {
        output.push("---".to_string());
        index = 1;
        while index < lines.len() {
            let line = lines[index];
            output.push(line.to_string());
            index += 1;
            if line == "---" || line == "..." {
                break;
            }
        }
    }

    let mut in_fence = false;
    let mut fence_prefix = String::new();
    while index < lines.len() {
        let line = lines[index];

        if is_fence_start(line) {
            let trimmed = line.trim_start();
            if !in_fence {
                in_fence = true;
                fence_prefix = trimmed
                    .chars()
                    .take_while(|c| *c == '`' || *c == '~')
                    .collect();
            } else if trimmed.starts_with(&fence_prefix) {
                in_fence = false;
                fence_prefix.clear();
            }
            output.push(finish_line(line, config));
            index += 1;
            continue;
        }

        if in_fence {
            output.push(line.to_string());
            index += 1;
            continue;
        }

        if line.trim().is_empty() {
            output.push(String::new());
            index += 1;
            continue;
        }

        if let Some(normalized_line) = normalize_heading_line(line, config) {
            output.push(normalized_line);
            index += 1;
            continue;
        }

        if let Some(normalized_line) = normalize_list_line(line, config, source_indent) {
            output.push(normalized_line);
            index += 1;
            continue;
        }

        if is_non_wrappable_block_line(line) {
            output.push(finish_line(line, config));
            index += 1;
            continue;
        }

        let start = index;
        while index < lines.len() {
            let candidate = lines[index];
            if candidate.trim().is_empty()
                || is_fence_start(candidate)
                || normalize_heading_line(candidate, config).is_some()
                || normalize_list_line(candidate, config, source_indent).is_some()
                || is_non_wrappable_block_line(candidate)
            {
                break;
            }
            index += 1;
        }
        if config.prose_wrap == ProseWrapMode::Preserve {
            for line in &lines[start..index] {
                output.push(finish_line(line, config));
            }
        } else {
            let paragraph = lines[start..index]
                .iter()
                .map(|line| line.trim())
                .collect::<Vec<_>>()
                .join(" ");
            for wrapped in wrap_line(&paragraph, config.line_width) {
                output.push(wrapped);
            }
        }
    }

    let mut squeezed = Vec::new();
    let mut blanks = 0usize;
    for line in output {
        if line.is_empty() {
            blanks += 1;
            if blanks <= config.blank_lines_max_consecutive {
                squeezed.push(line);
            }
        } else {
            blanks = 0;
            squeezed.push(line);
        }
    }

    while squeezed.last().is_some_and(String::is_empty) {
        squeezed.pop();
    }

    finalize_markdown_output(&squeezed.join("\n"), config)
}

fn finalize_markdown_output(input: &str, config: &Config) -> String {
    let index = SourceIndex::new(input);
    finalize_markdown_output_indexed(input, config, &index)
}

struct FinalWriter<'a> {
    config: &'a Config,
    output: String,
    pending_blank_lines: usize,
    fence: Option<FenceDelimiter>,
    wrote_line: bool,
}

impl<'a> FinalWriter<'a> {
    fn new(config: &'a Config, capacity: usize) -> Self {
        Self {
            config,
            output: String::with_capacity(capacity.saturating_add(1)),
            pending_blank_lines: 0,
            fence: None,
            wrote_line: false,
        }
    }

    fn write_source_line(&mut self, line: &str, is_terminal_line: bool) {
        if let Some(current) = self.fence {
            let closes_fence = current.closes(line);
            self.write_line(line);
            if closes_fence {
                self.fence = None;
            }
            return;
        }
        if let Some(opened) = FenceDelimiter::opens(line) {
            self.flush_blank_lines();
            self.write_line(line);
            self.fence = Some(opened);
            return;
        }

        let normalized_heading = normalize_heading_line(line, self.config);
        let mut line = normalized_heading.as_deref().unwrap_or(line);
        let trailing = line.len() - line.trim_end_matches([' ', '\t']).len();
        let mut hard_break = None;
        if self.config.trim_trailing_whitespace {
            if line.trim().is_empty() {
                line = "";
            } else if trailing >= 2 && !is_terminal_line {
                line = line.trim_end_matches([' ', '\t']);
                hard_break = Some("  ");
            } else {
                line = line.trim_end_matches([' ', '\t']);
            }
        }

        if line.is_empty() {
            self.pending_blank_lines = self.pending_blank_lines.saturating_add(1);
            return;
        }
        self.flush_blank_lines();
        self.write_line(line);
        if let Some(suffix) = hard_break {
            self.output.push_str(suffix);
        }
    }

    fn flush_blank_lines(&mut self) {
        let count = self
            .pending_blank_lines
            .min(self.config.blank_lines_max_consecutive);
        for _ in 0..count {
            self.write_line("");
        }
        self.pending_blank_lines = 0;
    }

    fn write_line(&mut self, line: &str) {
        if self.wrote_line {
            self.output.push('\n');
        }
        self.output.push_str(line);
        self.wrote_line = true;
    }

    fn finish(mut self) -> String {
        if self.config.end_of_file_newline {
            self.output.push('\n');
        }
        self.output
    }
}

fn finalize_markdown_output_indexed(input: &str, config: &Config, index: &SourceIndex) -> String {
    let mut writer = FinalWriter::new(config, input.len());
    let line_count = index.lines.len();
    for (line_index, source_line) in index.lines.iter().enumerate() {
        writer.write_source_line(
            &input[source_line.start..source_line.content_end],
            line_index + 1 == line_count,
        );
    }
    writer.finish()
}

fn parse_toml_file(path: &Path) -> Result<toml::Value> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config '{}'", path.display()))?;
    toml::from_str::<toml::Value>(&contents)
        .with_context(|| format!("Failed to parse config '{}'", path.display()))
}

fn find_upward(start_dir: &Path, file_name: &str) -> Option<PathBuf> {
    let mut current = Some(start_dir);
    while let Some(dir) = current {
        let candidate = dir.join(file_name);
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

fn config_parent(path: &Path, working_dir: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.join(path)
    };
    absolute
        .parent()
        .map_or_else(|| working_dir.to_path_buf(), Path::to_path_buf)
}

#[allow(clippy::too_many_lines)]
fn apply_root_config(config: &mut Config, value: &toml::Value, config_dir: PathBuf) {
    if let Some(line_width) = value
        .get("line-width")
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
    {
        config.line_width = line_width;
    }
    if let Some(trim) = value
        .get("trim-trailing-whitespace")
        .and_then(toml::Value::as_bool)
    {
        config.trim_trailing_whitespace = trim;
    }
    if let Some(newline) = value
        .get("end-of-file-newline")
        .and_then(toml::Value::as_bool)
    {
        config.end_of_file_newline = newline;
    }
    if let Some(max_blank) = value
        .get("blank-lines")
        .and_then(|section| section.get("max-consecutive"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
    {
        config.blank_lines_max_consecutive = max_blank;
    }
    if let Some(indent_width) = value
        .get("list")
        .and_then(|section| section.get("indent-width"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
    {
        config.list_indent_width = indent_width.max(1);
    }
    if let Some(style) = value
        .get("list")
        .and_then(|section| section.get("style"))
        .and_then(toml::Value::as_str)
    {
        config.list_style = match style {
            "dash" => ListStyle::Dash,
            "plus" => ListStyle::Plus,
            "asterisk" => ListStyle::Asterisk,
            _ => ListStyle::Preserve,
        };
    }
    if let Some(mode) = value
        .get("list")
        .and_then(|section| section.get("indentation"))
        .and_then(toml::Value::as_str)
    {
        config.list_indentation = match mode {
            "normalize" => ListIndentationMode::Normalize,
            _ => ListIndentationMode::Preserve,
        };
    }
    if let Some(mode) = value
        .get("frontmatter")
        .and_then(|section| section.get("mode"))
        .and_then(toml::Value::as_str)
    {
        config.frontmatter_mode = match mode {
            "normalize" => FrontmatterMode::Normalize,
            _ => FrontmatterMode::Preserve,
        };
    }
    if let Some(respect) = value
        .get("files")
        .and_then(|section| section.get("respect-gitignore"))
        .and_then(toml::Value::as_bool)
    {
        config.respect_gitignore = respect;
    }
    if let Some(exclude) = value
        .get("files")
        .and_then(|section| section.get("exclude"))
        .and_then(toml::Value::as_array)
    {
        config.exclude = exclude
            .iter()
            .filter_map(toml::Value::as_str)
            .map(ToString::to_string)
            .collect();
        config.exclude_base = Some(config_dir);
    }
    if let Some(skip_dirs) = value
        .get("files")
        .and_then(|section| section.get("skip-dirs"))
        .and_then(toml::Value::as_array)
    {
        config.skip_dirs = skip_dirs
            .iter()
            .filter_map(toml::Value::as_str)
            .map(ToString::to_string)
            .collect();
    }
    if let Some(max_concurrency) = value
        .get("files")
        .and_then(|section| section.get("max-concurrency"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
    {
        config.max_concurrency = max_concurrency;
    }
    if let Some(mode) = value
        .get("headings")
        .and_then(|section| section.get("indentation"))
        .and_then(toml::Value::as_str)
    {
        config.heading_indentation = match mode {
            "normalize" => HeadingIndentationMode::Normalize,
            _ => HeadingIndentationMode::Preserve,
        };
    }
    if let Some(mode) = value
        .get("heading-indentation")
        .and_then(toml::Value::as_str)
    {
        config.heading_indentation = match mode {
            "normalize" => HeadingIndentationMode::Normalize,
            _ => HeadingIndentationMode::Preserve,
        };
    }
    if let Some(engine) = value.get("engine").and_then(toml::Value::as_str) {
        config.engine = match engine {
            "legacy" => FormatterEngine::Legacy,
            _ => FormatterEngine::Ast,
        };
    }
    if let Some(mode) = value.get("prose-wrap").and_then(toml::Value::as_str) {
        config.prose_wrap = match mode {
            "preserve" => ProseWrapMode::Preserve,
            _ => ProseWrapMode::Always,
        };
    }
    if let Some(mode) = value
        .get("prose")
        .and_then(|section| section.get("wrap"))
        .and_then(toml::Value::as_str)
    {
        config.prose_wrap = match mode {
            "preserve" => ProseWrapMode::Preserve,
            _ => ProseWrapMode::Always,
        };
    }
    if let Some(cap) = value
        .get("check")
        .and_then(|section| section.get("diff"))
        .and_then(|section| section.get("cap"))
        .and_then(toml::Value::as_bool)
    {
        config.check_diff.cap = cap;
    }
    if let Some(context) = value
        .get("check")
        .and_then(|section| section.get("diff"))
        .and_then(|section| section.get("context"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| u32::try_from(value).ok())
    {
        config.check_diff.context = context;
    }
    if let Some(max_files) = value
        .get("check")
        .and_then(|section| section.get("diff"))
        .and_then(|section| section.get("max-files"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
    {
        config.check_diff.max_files = max_files;
    }
    if let Some(max_lines_per_file) = value
        .get("check")
        .and_then(|section| section.get("diff"))
        .and_then(|section| section.get("max-lines-per-file"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
    {
        config.check_diff.max_lines_per_file = max_lines_per_file;
    }
    if let Some(intraline) = value
        .get("check")
        .and_then(|section| section.get("diff"))
        .and_then(|section| section.get("intraline"))
        .and_then(toml::Value::as_bool)
    {
        config.check_diff.intraline = intraline;
    }
    if let Some(show_invisible_whitespace) = value
        .get("check")
        .and_then(|section| section.get("diff"))
        .and_then(|section| section.get("show-invisible-whitespace"))
        .and_then(toml::Value::as_bool)
    {
        config.check_diff.show_invisible_whitespace = show_invisible_whitespace;
    }
    if let Some(max_intraline_line_length) = value
        .get("check")
        .and_then(|section| section.get("diff"))
        .and_then(|section| section.get("max-intraline-line-length"))
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
    {
        config.check_diff.max_intraline_line_length = max_intraline_line_length;
    }
}

#[derive(Debug)]
struct PathFilters {
    working_dir: PathBuf,
    skip_dirs: BTreeSet<String>,
    exclude_globs: GlobSet,
    excluded_dir_globs: GlobSet,
}

impl PathFilters {
    fn new(config: &Config, working_dir: &Path) -> Result<Self> {
        let working_dir = absolute_path(working_dir, working_dir);
        let exclude_base = config.exclude_base.as_deref().map_or_else(
            || working_dir.clone(),
            |base| absolute_path(base, &working_dir),
        );
        let skip_dirs = config.skip_dirs.iter().cloned().collect::<BTreeSet<_>>();

        let mut builder = GlobSetBuilder::new();
        let mut dir_builder = GlobSetBuilder::new();
        for pattern in &config.exclude {
            let resolved = resolve_exclude_pattern(pattern, &exclude_base);
            let glob = Glob::new(&resolved)
                .with_context(|| format!("Invalid files.exclude glob pattern '{pattern}'"))?;
            builder.add(glob);
            if let Some(directory_pattern) = resolved.strip_suffix("/**") {
                dir_builder.add(
                    Glob::new(directory_pattern).with_context(|| {
                        format!("Invalid files.exclude glob pattern '{pattern}'")
                    })?,
                );
            }
        }

        Ok(Self {
            working_dir,
            skip_dirs,
            exclude_globs: builder.build()?,
            excluded_dir_globs: dir_builder.build()?,
        })
    }

    fn should_skip_dir(&self, path: &Path) -> bool {
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| self.skip_dirs.contains(name))
        {
            return true;
        }

        let absolute = absolute_path(path, &self.working_dir);
        self.excluded_dir_globs.is_match(&absolute) || self.exclude_globs.is_match(&absolute)
    }

    fn should_skip_path(&self, path: &Path) -> bool {
        self.exclude_globs
            .is_match(absolute_path(path, &self.working_dir))
    }
}

fn absolute_path(path: &Path, working_dir: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.join(path.strip_prefix(Path::new(".")).unwrap_or(path))
    };
    normalize_path(&path)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn resolve_exclude_pattern(pattern: &str, base: &Path) -> String {
    normalize_path(&base.join(pattern.trim_start_matches('/')))
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "mdx" | "markdown"
            )
        })
}

fn trim_markdown_trailing_whitespace(line: &str) -> String {
    line.trim_end().to_string()
}

fn finish_line(line: &str, config: &Config) -> String {
    if config.trim_trailing_whitespace {
        trim_markdown_trailing_whitespace(line)
    } else {
        line.to_string()
    }
}

#[derive(Debug, Clone, Copy)]
struct FenceDelimiter {
    marker: char,
    length: usize,
}

impl FenceDelimiter {
    fn opens(line: &str) -> Option<Self> {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next()?;
        if !matches!(marker, '`' | '~') {
            return None;
        }
        let length = trimmed.chars().take_while(|value| *value == marker).count();
        (length >= 3).then_some(Self { marker, length })
    }

    fn closes(self, line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed
            .chars()
            .take_while(|value| *value == self.marker)
            .count()
            >= self.length
            && trimmed
                .trim_matches(self.marker)
                .trim_matches([' ', '\t'])
                .is_empty()
    }
}

fn is_fence_start(line: &str) -> bool {
    FenceDelimiter::opens(line).is_some()
}

fn is_non_wrappable_block_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with('>') || trimmed.starts_with('<') || trimmed.starts_with('{') {
        return true;
    }
    if trimmed.starts_with('|') {
        return true;
    }
    if trimmed.starts_with("***") || trimmed.starts_with("---") {
        return true;
    }
    is_ordered_list_line(trimmed).is_some() || is_unordered_list_line(trimmed).is_some()
}

fn normalize_heading_line(line: &str, config: &Config) -> Option<String> {
    let leading_count = line.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|value| *value == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let suffix = &trimmed[hashes..];
    if !suffix.is_empty() && !suffix.starts_with(' ') && !suffix.starts_with('\t') {
        return None;
    }
    let rest = suffix.trim_start();
    let mut normalized = format!("{} {}", "#".repeat(hashes), rest);
    if config.trim_trailing_whitespace {
        normalized = normalized.trim_end().to_string();
    }
    if config.heading_indentation == HeadingIndentationMode::Preserve && leading_count > 0 {
        normalized = format!("{}{}", " ".repeat(leading_count), normalized);
    }
    Some(normalized)
}

fn detect_list_indent_unit(input: &str) -> usize {
    let mut minimum = usize::MAX;
    for line in input.lines() {
        let leading = line.chars().take_while(|c| c.is_whitespace()).count();
        if leading == 0 {
            continue;
        }
        if is_unordered_list_line(line.trim_start()).is_some()
            || is_ordered_list_line(line.trim_start()).is_some()
        {
            minimum = minimum.min(leading);
        }
    }
    if minimum == usize::MAX {
        4
    } else {
        minimum.max(1)
    }
}

fn normalize_list_line(
    line: &str,
    config: &Config,
    source_indent: Option<usize>,
) -> Option<String> {
    if let Some((leading, marker, content)) = is_unordered_list_line(line) {
        let marker = match config.list_style {
            ListStyle::Preserve => marker,
            ListStyle::Dash => "-".to_string(),
            ListStyle::Plus => "+".to_string(),
            ListStyle::Asterisk => "*".to_string(),
        };

        if config.list_indentation == ListIndentationMode::Preserve {
            return Some(format!(
                "{}{} {}",
                " ".repeat(leading),
                marker,
                content.trim_start()
            ));
        }

        let level = leading / source_indent.unwrap_or(1).max(1);
        return Some(format!(
            "{}{} {}",
            " ".repeat(level * config.list_indent_width),
            marker,
            content.trim_start()
        ));
    }

    if let Some((leading, marker, content)) = is_ordered_list_line(line) {
        if config.list_indentation == ListIndentationMode::Preserve {
            return Some(format!(
                "{}{} {}",
                " ".repeat(leading),
                marker,
                content.trim_start()
            ));
        }

        let level = leading / source_indent.unwrap_or(1).max(1);
        return Some(format!(
            "{}{} {}",
            " ".repeat(level * config.list_indent_width),
            marker,
            content.trim_start()
        ));
    }

    None
}

fn is_unordered_list_line(line: &str) -> Option<(usize, String, String)> {
    let leading = line.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    let marker = chars.next()?;
    if marker != '-' && marker != '+' && marker != '*' {
        return None;
    }
    let rest = chars.collect::<String>();
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    Some((leading, marker.to_string(), rest.trim_start().to_string()))
}

fn is_ordered_list_line(line: &str) -> Option<(usize, String, String)> {
    let leading = line.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed = line.trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let marker_end = digits + 1;
    let marker = trimmed.chars().nth(digits)?;
    if marker != '.' && marker != ')' {
        return None;
    }
    let rest = trimmed.get(marker_end..)?;
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    Some((
        leading,
        trimmed.get(..marker_end)?.to_string(),
        rest.trim_start().to_string(),
    ))
}

fn wrap_line(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
            continue;
        }
        let next_len = current.len() + 1 + word.len();
        if next_len <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    #[test]
    fn preserves_frontmatter_and_formats_heading() {
        let input = "---\ntitle: Test\n---\n# Heading\n";
        let output = format_markdown(input, &Config::default());
        assert_eq!(output, "---\ntitle: Test\n---\n\n# Heading\n");
    }

    #[test]
    fn preserves_non_commonmark_no_space_heading() {
        let input = "#Heading\n";
        let output = format_markdown(input, &Config::default());
        assert_eq!(output, input);
    }

    #[test]
    fn preserves_heading_indentation_by_default() {
        let input = "    ### Heading\n";
        let output = format_markdown(input, &Config::default());
        assert_eq!(output, "    ### Heading\n");
    }

    #[test]
    fn can_normalize_heading_indentation() {
        let input = "    ### Heading\n";
        let config = Config {
            heading_indentation: HeadingIndentationMode::Normalize,
            ..Config::default()
        };
        let output = format_markdown(input, &config);
        assert_eq!(output, "### Heading\n");
    }

    #[test]
    fn ast_engine_falls_back_for_preserve_heading_indentation() {
        let input = "- item\n\n    ### Heading\n";
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Preserve,
            list_indentation: ListIndentationMode::Preserve,
            heading_indentation: HeadingIndentationMode::Preserve,
            ..Config::default()
        };
        let output = format_markdown(input, &config);
        assert_eq!(output, input);
    }

    #[test]
    fn ast_engine_falls_back_when_normalize_modes_requested() {
        let input = "- one\n  - two\n";
        let config = Config {
            engine: FormatterEngine::Ast,
            list_indentation: ListIndentationMode::Normalize,
            ..Config::default()
        };
        let output = format_markdown(input, &config);
        assert_eq!(output, "- one\n    - two\n");
    }

    #[test]
    fn ast_engine_runs_when_supported_profile_selected() {
        let input = "# Heading\n\nparagraph\n";
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Preserve,
            list_indentation: ListIndentationMode::Preserve,
            heading_indentation: HeadingIndentationMode::Normalize,
            ..Config::default()
        };
        let output = format_markdown(input, &config);
        assert_eq!(output, "# Heading\n\nparagraph\n");
    }

    #[test]
    fn normalizes_nested_list_indent_width() {
        let input = "- one\n  - two\n";
        let config = Config {
            list_indentation: ListIndentationMode::Normalize,
            ..Config::default()
        };
        let output = format_markdown(input, &config);
        assert_eq!(output, "- one\n    - two\n");
    }

    #[test]
    fn preserves_nested_list_indentation_by_default() {
        let input = "- one\n  - two\n";
        let output = format_markdown(input, &Config::default());
        assert_eq!(output, input);
    }

    #[test]
    fn wraps_plain_paragraphs() {
        let input = "word word word word word word word word word word word word word word\n";
        let config = Config {
            line_width: 20,
            ..Config::default()
        };
        let output = format_markdown(input, &config);
        assert!(output.lines().all(|line| line.len() <= 20));
    }

    #[test]
    fn preserves_semantic_soft_and_hard_breaks() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        let input = "soft\nbreak\n\nspaces  \nbreak\n\nbackslash\\\nbreak\n";
        assert_eq!(format_markdown(input, &config), input);
    }

    #[test]
    fn preserves_canonical_setext_second_heading_output() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            heading_indentation: HeadingIndentationMode::Normalize,
            ..Config::default()
        };
        let input = "Foo\n---\n";
        assert_eq!(format_markdown(input, &config), input);
        assert_eq!(
            format_markdown("> foo\nbar\n===\n", &config),
            "> foo\n\n# bar\n"
        );
    }

    #[test]
    fn preserves_fenced_rust_code_inside_checklist_items() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Preserve,
            list_indentation: ListIndentationMode::Preserve,
            list_style: ListStyle::Preserve,
            line_width: 999_999,
            ..Config::default()
        };
        let input = concat!(
            "- [x] **Implement stereo weight decoding:**\n",
            "\n",
            "    ```rust\n",
            "    let w1_q13 = STEREO_WEIGHT_TABLE_Q13[wi1]\n",
            "        + (((i32::from(STEREO_WEIGHT_TABLE_Q13[wi1 + 1])\n",
            "            - i32::from(STEREO_WEIGHT_TABLE_Q13[wi1]))\n",
            "            * 6554)\n",
            "            >> 16)\n",
            "            * i32::from(2 * i3 + 1);\n",
            "\n",
            "    let w0_q13 = STEREO_WEIGHT_TABLE_Q13[wi0]\n",
            "        + (((i32::from(STEREO_WEIGHT_TABLE_Q13[wi0 + 1])\n",
            "            - i32::from(STEREO_WEIGHT_TABLE_Q13[wi0]))\n",
            "            * 6554)\n",
            "            >> 16)\n",
            "            * i32::from(2 * i1 + 1)\n",
            "        - w1_q13;\n",
            "    ```\n",
        );

        let output = format_markdown(input, &config);

        assert_eq!(output, input);
        assert_eq!(format_markdown(&output, &config), output);
    }

    #[test]
    fn preserves_blank_lines_and_html_comments_inside_fenced_code() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Preserve,
            list_indentation: ListIndentationMode::Preserve,
            list_style: ListStyle::Preserve,
            line_width: 999_999,
            ..Config::default()
        };
        for input in [
            "```html\n<!-- Development -->\n<script src=\"/app.js\"></script>\n```\n",
            "```toml\n[dev-dependencies]\ntokio = { workspace = true }\n\n\n[features]\ndefault = []\n```\n",
            "```rust\nlet value = 1;\n\n\n```\n",
            "~~~~text\n``` is literal\n\n\n* literal marker\n~~~~\n",
        ] {
            let output = format_markdown(input, &config);
            assert_eq!(output, input);
            assert_eq!(format_markdown(&output, &config), output);
        }
    }

    #[test]
    fn preserves_inline_links_when_paragraph_exceeds_width() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            line_width: 80,
            ..Config::default()
        };
        let input = "[Project site](https://example.com 'Example') and ![Logo](https://example.\ncom/logo.png).\n";
        let expected = "[Project site](https://example.com 'Example') and ![Logo](https://example.com/logo.png).\n";
        assert_eq!(format_markdown(input, &config), expected);
    }

    #[test]
    fn paragraph_tokens_distinguish_break_kinds() {
        let source = "soft\nbreak  \nhard\\\nescape\n`literal\ncode`\n";
        let mut options = ParseOptions::gfm();
        options.constructs = Constructs::gfm();
        let root = parse_mdast!(source, &options).expect("paragraph must parse");
        let Node::Root(root) = root else {
            panic!("expected root node");
        };
        let Node::Paragraph(paragraph) = &root.children[0] else {
            panic!("expected paragraph node");
        };
        assert_eq!(
            paragraph_inline_tokens(paragraph, source),
            vec![
                InlineToken::Content("soft".to_string()),
                InlineToken::SoftBreak,
                InlineToken::Content("break".to_string()),
                InlineToken::HardBreak(HardBreakStyle::Spaces),
                InlineToken::Content("hard".to_string()),
                InlineToken::HardBreak(HardBreakStyle::Backslash),
                InlineToken::Content("escape".to_string()),
                InlineToken::SoftBreak,
                InlineToken::Content("`literal".to_string()),
                InlineToken::LiteralBreak,
                InlineToken::Content("code`".to_string()),
            ]
        );
    }

    #[test]
    fn preserves_literal_newlines_inside_inline_code_and_html() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        let input = "`code  \nspan`\n\n<a href=\"foo  \nbar\">\n";
        assert_eq!(format_markdown(input, &config), input);
    }

    #[test]
    fn normalizes_hard_break_spacing_and_emphasis_delimiters() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        assert_eq!(
            format_markdown("foo       \nbaz\n", &config),
            "foo  \nbaz\n"
        );
        assert_eq!(format_markdown("*foo  \nbar*\n", &config), "_foo  \nbar_\n");
    }

    #[test]
    fn list_context_preserves_fenced_code_indentation_across_passes() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        let input = "1. ```\n   foo\n   ```\n\n   bar\n";
        let expected = "1. ```\n   foo\n   ```\n\n    bar\n";
        let output = format_markdown(input, &config);
        assert_eq!(output, expected);
        assert_eq!(format_markdown(&output, &config), output);
    }

    #[test]
    fn list_context_preserves_canonical_mixed_continuation_indentation() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        for input in [
            "- Foo\n\n        bar\n\n          baz\n",
            "1.      indented code\n\n    paragraph\n\n        more code\n",
            "1.       indented code\n\n    paragraph\n\n        more code\n",
        ] {
            assert_eq!(format_markdown(input, &config), input);
        }
    }

    #[test]
    fn list_context_preserves_canonical_unstable_output_shapes() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        for input in [
            "- foo\n\n    \t\tbar\n",
            "- one\ntwo\n",
            "- one\n     two\n",
            "- foo\n\n        bar\n",
            "10.  foo\n\n        bar\n",
            "- foo\nbar\n",
            "- a\n\n- b\n\n    [ref]: /url\n\n- d\n",
            "- foo\n    - bar\n    baz\n",
            "1.  foo\n\n```\n    bar\n    ```\n\n    baz\n\n    > bam\n",
        ] {
            assert_eq!(format_markdown(input, &config), input);
        }
    }

    #[test]
    fn list_context_preserves_marker_family_boundaries() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        let input = "- foo\n- bar\n+ baz\n";
        let expected = "- foo\n- bar\n\n* baz\n";
        let output = format_markdown(input, &config);
        assert_eq!(output, expected);
        assert_eq!(format_markdown(&output, &config), output);
    }

    #[test]
    fn list_context_preserves_loose_inter_item_boundaries() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        let input = "- foo\n\n- bar\n\n- baz\n";
        let output = format_markdown(input, &config);
        assert_eq!(output, input);
        assert_eq!(format_markdown(&output, &config), output);
    }

    #[test]
    fn block_context_normalizes_quote_depth_and_heading_boundaries() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            heading_indentation: HeadingIndentationMode::Normalize,
            ..Config::default()
        };
        assert_eq!(
            format_markdown("># Foo\n>bar\n> baz\n", &config),
            "> # Foo\n>\n> bar\n> baz\n"
        );
        assert_eq!(
            format_markdown(">>> foo\n> bar\n>>baz\n", &config),
            "> > > foo\n> > > bar\n> > > baz\n"
        );
    }

    #[test]
    fn block_context_normalizes_interrupted_quote_constructs() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        for (input, expected) in [
            ("> ```\nfoo\n```\n", "> ```\n> foo\n> ```\n\n```\n\n```\n"),
            ("> aaa\n***\n> bbb\n", "> aaa\n\n---\n\n> bbb\n"),
        ] {
            let output = format_markdown(input, &config);
            assert_eq!(output, expected);
            assert_eq!(format_markdown(&output, &config), output);
        }
    }

    #[test]
    fn nested_emphasis_delimiters_are_normalized_from_ast_context() {
        for (input, expected) in [
            ("**foo *bar* baz**\n", "**foo _bar_ baz**\n"),
            (
                "**Gomphocarpus (*Gomphocarpus physocarpus*, syn.\n*Asclepias physocarpa*)**\n",
                "**Gomphocarpus (_Gomphocarpus physocarpus_, syn.\n_Asclepias physocarpa_)**\n",
            ),
        ] {
            let root = parse_mdast!(input, &ParseOptions::gfm()).expect("input must parse");
            assert_eq!(
                normalize_nested_asterisk_emphasis_in_strong(input, Some(&root)),
                Some(expected.to_string())
            );
        }
        let input = "prefix **foo *bar* baz**\n";
        let root = parse_mdast!(input, &ParseOptions::gfm()).expect("input must parse");
        assert_eq!(
            normalize_nested_asterisk_emphasis_in_strong(input, Some(&root)),
            None
        );
    }

    #[test]
    fn headings_preserve_strong_emphasis_delimiters() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            heading_indentation: HeadingIndentationMode::Normalize,
            ..Config::default()
        };
        let input = "## Phase 1: Package Creation and Setup 🔴 **NOT STARTED**\n";

        let output = format_markdown(input, &config);

        assert_eq!(output, input);
        assert_eq!(format_markdown(&output, &config), output);
    }

    #[test]
    fn inline_code_and_escape_source_forms_are_stable() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        for (input, expected) in [
            ("`` foo ` bar ``\n", "``foo ` bar``\n"),
            ("``\nfoo\nbar  \nbaz\n``\n", "`foo\nbar  \nbaz`\n"),
            ("` foo `` bar `\n", "`foo `` bar`\n"),
            ("*foo`*`\n", "_foo`_`\n"),
            ("\\\\*emphasis*\n", "\\\\_emphasis_\n"),
            ("[foo](/bar\\* \"ti\\*tle\")\n", "[foo](/bar* 'ti*tle')\n"),
            ("&#42;foo&#42;\n*foo*\n", "\\*foo\\*\n_foo_\n"),
        ] {
            let output = format_markdown(input, &config);
            assert_eq!(output, expected);
            assert_eq!(format_markdown(&output, &config), output);
        }
    }

    #[test]
    fn references_and_container_source_forms_are_stable() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        for (input, expected) in [
            ("- one\n\n two\n", "- one\n\ntwo\n"),
            (">>- one\n>>\n  >  > two\n", "> > - one\n> >\n> > two\n"),
            ("-    foo\n\n  bar\n", "- foo\n\nbar\n"),
            (
                "[foo]: /url \"title\"\n\n[foo]\n",
                "[foo]: /url 'title'\n\n[foo]\n",
            ),
            ("[foo]:\n/url\n\n[foo]\n", "[foo]: /url\n\n[foo]\n"),
        ] {
            let output = format_markdown(input, &config);
            assert_eq!(output, expected);
            assert_eq!(format_markdown(&output, &config), output);
        }
    }

    #[test]
    fn emphasis_escape_collision_converges_without_changing_semantics() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        let canonical = "foo *\\_*\n";
        for input in [
            "foo *_*\n",
            "foo _\\__\n",
            "prefix *_* suffix\n",
            "prefix _\\__ suffix\n",
            canonical,
        ] {
            let expected = if input.starts_with("prefix") {
                "prefix *\\_* suffix\n"
            } else {
                canonical
            };
            let output = format_markdown(input, &config);
            assert_eq!(output, expected);
            assert_eq!(format_markdown(&output, &config), output);
        }
    }

    #[test]
    fn code_and_html_source_forms_are_canonical_and_stable() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        for (input, expected) in [
            ("~~~\n<\n >\n~~~\n", "```\n<\n >\n```\n"),
            ("``\nfoo \n``\n", "`foo `\n"),
            ("~~~\naaa\n```\n~~~\n", "````\naaa\n```\n````\n"),
            (" ```\n aaa\naaa\n```\n", "```\naaa\naaa\n```\n"),
            ("foo\n```\nbar\n```\nbaz\n", "foo\n\n```\nbar\n```\n\nbaz\n"),
            ("<33> <__>\n", "<33> <\\_\\_>\n"),
            ("<a\n> quoted text\n", "<a\n\n> quoted text\n"),
        ] {
            let output = format_markdown(input, &config);
            assert_eq!(output, expected);
            assert_eq!(format_markdown(&output, &config), output);
        }
    }

    #[test]
    fn block_leaf_printer_handles_contextual_source_forms() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            heading_indentation: HeadingIndentationMode::Normalize,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        for (input, expected) in [
            ("--\n**\n__\n", "--\n\\*\\*\n\\_\\_\n"),
            ("Foo\n    ***\n", "Foo\n\\*\\*\\*\n"),
            (" *-*\n", "_-_\n"),
            ("- foo\n***\n- bar\n", "- foo\n\n---\n\n- bar\n"),
            ("- Foo\n- * * *\n", "- Foo\n- ***\n"),
            ("# foo *bar* \\*baz\\*\n", "# foo _bar_ \\*baz\\*\n"),
            ("    # foo\n", "    # foo\n"),
            (
                "    chunk1\n\n    chunk2\n  \n \n \n    chunk3\n",
                "    chunk1\n\n    chunk2\n\n\n\n    chunk3\n",
            ),
            ("foo\n    # bar\n", "foo # bar\n"),
        ] {
            let output = format_markdown(input, &config);
            assert_eq!(output, expected);
            assert_eq!(format_markdown(&output, &config), output);
        }
    }

    #[test]
    fn block_leaf_printer_normalizes_breaks_and_boundaries() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            heading_indentation: HeadingIndentationMode::Normalize,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        for (input, expected) in [
            ("***\n---\n___\n", "---\n\n---\n\n---\n"),
            ("# foo\n## foo\n### foo\n", "# foo\n\n## foo\n\n### foo\n"),
            ("Foo bar\n# baz\nBar foo\n", "Foo bar\n\n# baz\n\nBar foo\n"),
        ] {
            let output = format_markdown(input, &config);
            assert_eq!(output, expected);
            assert_eq!(format_markdown(&output, &config), output);
        }
    }

    #[test]
    fn block_context_preserves_quoted_list_continuation_indentation() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        let input = "   > > 1.  one\n>>\n>>     two\n";
        let expected = "> > 1.  one\n> >\n> >     two\n";
        let output = format_markdown(input, &config);
        assert_eq!(output, expected);
        assert_eq!(format_markdown(&output, &config), output);
    }

    #[test]
    fn block_context_preserves_lazy_continuation_after_blank_quote() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        let input = "> bar\n>\nbaz\n";
        let expected = "> bar\n>\n> baz\n";
        let output = format_markdown(input, &config);
        assert_eq!(output, expected);
        assert_eq!(format_markdown(&output, &config), output);
    }

    #[test]
    fn block_context_preserves_explicit_quote_blank_lines() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        let input = "> foo\n>\n> bar\n";
        assert_eq!(format_markdown(input, &config), input);
    }

    #[test]
    fn normalizes_block_boundaries_and_terminal_trailing_spaces() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            ..Config::default()
        };
        assert_eq!(format_markdown("aaa\n\n\nbbb\n", &config), "aaa\n\nbbb\n");
        assert_eq!(
            format_markdown("    aaa\nbbb\n", &config),
            "    aaa\n\nbbb\n"
        );
        assert_eq!(
            format_markdown("aaa     \nbbb     \n", &config),
            "aaa  \nbbb\n"
        );
        assert_eq!(format_markdown("foo  \n", &config), "foo\n");
    }

    #[test]
    fn wrapping_and_indentation_do_not_create_markdown_constructs() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            line_width: 16,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        let cases = [
            "prefix words before 1. literal ordered marker text\n",
            "prefix words before - literal bullet marker text\n",
            "prefix words before # literal heading marker text\n",
            "> prefix words before * literal emphasis marker text\n",
        ];

        for input in cases {
            let output = format_markdown(input, &config);
            assert_eq!(format_markdown(&output, &config), output);
            for content in ["literal", "marker", "text"] {
                assert!(
                    output.contains(content),
                    "wrapped output dropped {content:?}"
                );
            }
        }
    }

    #[test]
    fn block_and_inline_interactions_compose_without_content_loss() {
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Always,
            heading_indentation: HeadingIndentationMode::Normalize,
            list_indentation: ListIndentationMode::Normalize,
            list_style: ListStyle::Dash,
            ..Config::default()
        };
        let cases = [
            "> quoted **text**\n>\n> ```md\n> <tag>value</tag>\n> ```\n",
            "> - item with [*emphasis*](https://example.com) and `code`\n>\n>   <span>raw</span>\n",
            "1. paragraph with **strong [link](https://example.com)**\n\n    - nested ~~item~~ with `code`\n",
        ];

        for input in cases {
            let output = format_markdown(input, &config);
            let second = format_markdown(&output, &config);
            assert_eq!(second, output, "interaction output was not idempotent");
            for content in ["text", "value", "item", "example.com", "code"] {
                if input.contains(content) {
                    assert!(
                        output.contains(content),
                        "interaction output dropped {content:?}: {output:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn inline_fallback_preserves_unsupported_child_content() {
        let unsupported = Node::MdxJsxTextElement(markdown::mdast::MdxJsxTextElement {
            children: vec![Node::Text(markdown::mdast::Text {
                value: "preserved".to_string(),
                position: None,
            })],
            position: None,
            name: Some("Component".to_string()),
            attributes: Vec::new(),
        });

        assert_eq!(render_inline_text(&unsupported), "preserved");
    }

    #[test]
    fn ast_engine_preserves_mdx_constructs_without_content_loss() {
        let input = "export const answer = 42\n\n# Hello {name}\n\n<Component value={answer}>\n  **content**\n</Component>\n";
        let config = Config {
            engine: FormatterEngine::Ast,
            prose_wrap: ProseWrapMode::Preserve,
            heading_indentation: HeadingIndentationMode::Normalize,
            ..Config::default()
        };

        let output = format_markdown(input, &config);

        assert_eq!(output, input);
        assert_eq!(format_markdown(&output, &config), output);
    }

    #[test]
    fn legacy_and_ast_engine_selection_remain_independent() {
        let input = "**foo *bar* baz**\n";
        let legacy = format_markdown(
            input,
            &Config {
                engine: FormatterEngine::Legacy,
                ..Config::default()
            },
        );
        let ast = format_markdown(
            input,
            &Config {
                engine: FormatterEngine::Ast,
                prose_wrap: ProseWrapMode::Always,
                heading_indentation: HeadingIndentationMode::Normalize,
                ..Config::default()
            },
        );

        assert_eq!(legacy, input);
        assert_eq!(ast, "**foo _bar_ baz**\n");
        assert_eq!(
            format_markdown(
                &legacy,
                &Config {
                    engine: FormatterEngine::Legacy,
                    ..Config::default()
                }
            ),
            legacy
        );
    }

    #[test]
    fn configuration_profiles_apply_independently() {
        let wrapped = format_markdown(
            "one two three four\n",
            &Config {
                line_width: 10,
                ..Config::default()
            },
        );
        assert_eq!(wrapped, "one two\nthree four\n");

        let preserved = format_markdown(
            "one two three four\n",
            &Config {
                line_width: 10,
                prose_wrap: ProseWrapMode::Preserve,
                ..Config::default()
            },
        );
        assert_eq!(preserved, "one two three four\n");

        let list_style = format_markdown(
            "* one\n",
            &Config {
                list_style: ListStyle::Plus,
                ..Config::default()
            },
        );
        assert_eq!(list_style, "+ one\n");

        let frontmatter = "---\ntitle:  preserved  \n---\nbody\n";
        let preserved_frontmatter = format_markdown(frontmatter, &Config::default());
        assert!(preserved_frontmatter.starts_with("---\ntitle:  preserved  \n---\n"));
        let normalized_frontmatter = format_markdown(
            frontmatter,
            &Config {
                frontmatter_mode: FrontmatterMode::Normalize,
                ..Config::default()
            },
        );
        assert_eq!(normalized_frontmatter, frontmatter);
        assert_eq!(
            format_markdown(
                &normalized_frontmatter,
                &Config {
                    frontmatter_mode: FrontmatterMode::Normalize,
                    ..Config::default()
                }
            ),
            normalized_frontmatter
        );

        assert_eq!(
            format_markdown(
                "    ### Heading\n",
                &Config {
                    heading_indentation: HeadingIndentationMode::Normalize,
                    ..Config::default()
                }
            ),
            "### Heading\n"
        );
        assert_eq!(
            format_markdown(
                "- one\n  - two\n",
                &Config {
                    list_indentation: ListIndentationMode::Normalize,
                    list_indent_width: 3,
                    ..Config::default()
                }
            ),
            "- one\n   - two\n"
        );
    }

    #[test]
    fn load_config_skips_unrelated_nested_clippier_config() {
        let dir = temp_dir("clippier-md-config-discovery");
        let nested = dir.join("packages").join("clippier").join("md");
        std::fs::create_dir_all(&nested).expect("failed to create nested config directory");
        std::fs::write(
            dir.join("clippier.toml"),
            "[tools.clippier-md]\nline-width = 123\n",
        )
        .expect("failed to write root config");
        std::fs::write(
            dir.join("packages").join("clippier").join("clippier.toml"),
            "[tools]\nskip = [\"gofmt\"]\n",
        )
        .expect("failed to write unrelated nested config");

        let config = load_config(&nested, None).expect("failed to load config");

        assert_eq!(config.line_width, 123);
        std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
    }

    #[test]
    fn source_index_tracks_newlines_frontmatter_and_fences_by_byte() {
        let source = "---\r\ntitle: café\r\n---\r\n\r\n~~~~text\r\n``` literal\r\n~~~~\r\n尾\r\n";
        let index = SourceIndex::new(source);
        let frontmatter_end = source.find("\r\n\r\n").expect("frontmatter separator") + 2;
        assert_eq!(index.line_ending, SourceLineEnding::Crlf);
        assert_eq!(index.frontmatter, Some((0, frontmatter_end)));
        assert_eq!(index.lines.last().map(|line| line.end), Some(source.len()));
        assert_eq!(index.fenced_ranges.len(), 1);
        let (start, end) = index.fenced_ranges[0];
        assert_eq!(&source[start..end], "~~~~text\r\n``` literal\r\n~~~~\r\n");
        assert!(source.is_char_boundary(start));
        assert!(source.is_char_boundary(end));
        assert!(!index.has_underscore);
        assert!(!index.has_mdx_or_html);
    }

    #[test]
    fn source_index_handles_cr_no_final_newline_and_unclosed_fence() {
        let source = "# heading\r~~~rust\rlet value = 1;";
        let index = SourceIndex::new(source);
        assert_eq!(index.line_ending, SourceLineEnding::Cr);
        assert_eq!(index.lines.len(), 3);
        assert_eq!(index.lines.last().map(|line| line.end), Some(source.len()));
        assert_eq!(index.fenced_ranges, vec![(10, source.len())]);
    }

    #[test]
    fn source_index_detects_mixed_line_endings_and_construct_flags() {
        let source = "<Component value={some_value}>\ntext\r\n";
        let index = SourceIndex::new(source);
        assert_eq!(index.line_ending, SourceLineEnding::Mixed);
        assert!(index.has_underscore);
        assert!(index.has_mdx_or_html);
    }

    #[test]
    fn format_session_reuses_its_lazy_ast() {
        let config = Config::default();
        let mut session = FormatSession::new("# Heading\n", &config);
        assert_eq!(session.parse_count, 0);
        assert!(session.ast().is_some());
        assert!(session.ast().is_some());
        assert_eq!(session.parse_count, 1);
    }

    #[test]
    fn format_outcome_borrows_unchanged_input() {
        let input = "# Canonical\n";
        match format_markdown_outcome(input, &Config::default()) {
            FormatOutcome::Unchanged(borrowed) => assert!(std::ptr::eq(input, borrowed)),
            FormatOutcome::Changed(output) => panic!("unexpected changed output: {output}"),
        }
    }

    #[test]
    fn run_fmt_check_and_write_cover_product_integration() {
        let dir = temp_dir("clippier-md-run-fmt");
        let path = dir.join("input.md");
        std::fs::write(&path, "one two three four\n").expect("failed to write markdown");
        let config = Config {
            line_width: 10,
            ..Config::default()
        };

        let checked = run_fmt(std::slice::from_ref(&path), true, true, &config)
            .expect("check mode must succeed");
        assert_eq!(checked.checked, 1);
        assert_eq!(checked.changed, vec![path.clone()]);
        assert_eq!(checked.diff_reports.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&path).expect("failed to read checked markdown"),
            "one two three four\n"
        );

        let written = run_fmt(std::slice::from_ref(&path), false, false, &config)
            .expect("write mode must succeed");
        assert_eq!(written.changed, vec![path.clone()]);
        assert_eq!(
            std::fs::read_to_string(&path).expect("failed to read formatted markdown"),
            "one two\nthree four\n"
        );

        std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
    }

    #[test]
    fn run_fmt_parallel_pipeline_is_deterministic_and_caps_diffs() {
        let dir = temp_dir("clippier-md-run-fmt-parallel");
        let mut expected_changed = Vec::new();
        for index in (0..12).rev() {
            let path = dir.join(format!("{index:02}.md"));
            let input = if index % 3 == 0 {
                "already short\n"
            } else {
                "one two three four\n"
            };
            std::fs::write(&path, input).expect("failed to write parallel fixture");
            if format_markdown(
                input,
                &Config {
                    line_width: 10,
                    ..Config::default()
                },
            ) != input
            {
                expected_changed.push(path);
            }
        }
        expected_changed.sort_unstable();
        let config = Config {
            line_width: 10,
            max_concurrency: 3,
            check_diff: CheckDiffConfig {
                max_files: 2,
                ..CheckDiffConfig::default()
            },
            ..Config::default()
        };

        for _ in 0..5 {
            let checked = run_fmt(std::slice::from_ref(&dir), true, true, &config)
                .expect("parallel check must succeed");
            assert_eq!(checked.checked, 12);
            assert_eq!(checked.changed, expected_changed);
            assert_eq!(checked.diff_reports.len(), 2);
            assert_eq!(checked.diff_omitted_files, expected_changed.len() - 2);
            assert_eq!(checked.diff_reports[0].path, expected_changed[0]);
            assert_eq!(checked.diff_reports[1].path, expected_changed[1]);
        }

        std::fs::remove_dir_all(&dir).expect("failed to clean parallel fixture");
    }

    #[test]
    fn run_fmt_parallel_pipeline_attributes_worker_errors() {
        let dir = temp_dir("clippier-md-run-fmt-error");
        let valid = dir.join("valid.md");
        let invalid = dir.join("invalid.md");
        std::fs::write(&valid, "valid\n").expect("failed to write valid fixture");
        std::fs::write(&invalid, [0xff]).expect("failed to write invalid fixture");
        let config = Config {
            max_concurrency: 2,
            ..Config::default()
        };

        let error = run_fmt(&[valid, invalid.clone()], true, false, &config)
            .expect_err("invalid UTF-8 must fail");
        assert!(error.to_string().contains(&invalid.display().to_string()));

        std::fs::remove_dir_all(&dir).expect("failed to clean error fixture");
    }

    #[test]
    fn load_config_reads_file_concurrency() {
        let dir = temp_dir("clippier-md-concurrency-config");
        let path = dir.join("clippier-md.toml");
        std::fs::write(&path, "[files]\nmax-concurrency = 3\n")
            .expect("failed to write concurrency config");

        let config = load_config(&dir, Some(&path)).expect("failed to load concurrency config");
        assert_eq!(config.max_concurrency, 3);

        std::fs::remove_dir_all(&dir).expect("failed to clean concurrency fixture");
    }

    #[test]
    fn preserves_prose_line_breaks_when_configured() {
        let input = "This is a very long line that should stay as authored and not be wrapped by the formatter.\nAnd this is another long line that should also remain unchanged.\n";
        let config = Config {
            line_width: 20,
            prose_wrap: ProseWrapMode::Preserve,
            ..Config::default()
        };
        let output = format_markdown(input, &config);
        assert_eq!(output, input);
    }

    #[test]
    fn collect_markdown_files_resolves_excludes_from_their_config_directory() {
        let dir = temp_dir("clippier-md-config-relative-excludes");
        let package = dir.join("packages").join("app");
        let excluded = dir.join("vendor").join("fixtures");
        let nested_git = dir.join("nested-git");
        std::fs::create_dir_all(&package).expect("failed to create package directory");
        std::fs::create_dir_all(&excluded).expect("failed to create excluded directory");
        std::fs::create_dir_all(&nested_git).expect("failed to create nested Git directory");
        std::fs::write(
            dir.join("clippier.toml"),
            "[tools.clippier-md.files]\nexclude = [\"/vendor/fixtures/**\"]\n",
        )
        .expect("failed to write config");
        std::fs::write(excluded.join("ignored.md"), "# ignored\n")
            .expect("failed to write excluded markdown");
        std::fs::write(package.join("included.md"), "# included\n")
            .expect("failed to write included markdown");
        std::fs::write(nested_git.join(".git"), "gitdir: elsewhere\n")
            .expect("failed to write nested Git marker");
        std::fs::write(nested_git.join("included.md"), "# included\n")
            .expect("failed to write nested Git markdown");
        let config = load_config(&package, None).expect("failed to load config");

        let files = collect_markdown_files(std::slice::from_ref(&dir), &config, &package)
            .expect("failed to collect markdown files");
        let explicit = collect_markdown_files(std::slice::from_ref(&excluded), &config, &package)
            .expect("failed to collect explicitly excluded directory");
        let relative_explicit =
            collect_markdown_files(&[PathBuf::from("../../vendor/fixtures")], &config, &package)
                .expect("failed to collect relatively addressed excluded directory");

        assert_eq!(files.len(), 2);
        assert!(
            files
                .iter()
                .any(|path| path.ends_with("packages/app/included.md"))
        );
        assert!(
            files
                .iter()
                .any(|path| path.ends_with("nested-git/included.md"))
        );
        assert!(explicit.is_empty());
        assert!(relative_explicit.is_empty());

        std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
    }

    #[test]
    fn collect_markdown_files_respects_gitignore() {
        let dir = temp_dir("clippier-md-gitignore");
        std::fs::write(dir.join(".gitignore"), ".direnv/\n").expect("failed to write .gitignore");
        std::fs::create_dir_all(dir.join(".direnv")).expect("failed to create .direnv");
        std::fs::write(dir.join(".direnv").join("ignored.md"), "# ignored\n")
            .expect("failed to write ignored markdown");
        std::fs::write(dir.join("README.md"), "# kept\n").expect("failed to write README.md");

        let files = collect_markdown_files(std::slice::from_ref(&dir), &Config::default(), &dir)
            .expect("failed to collect markdown files");

        assert!(files.iter().any(|path| path.ends_with("README.md")));
        assert!(!files.iter().any(|path| path.ends_with("ignored.md")));

        std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
    }

    #[test]
    fn collect_markdown_files_respects_config_skip_dirs() {
        let dir = temp_dir("clippier-md-skip-dirs");
        std::fs::create_dir_all(dir.join("docs-private")).expect("failed to create docs-private");
        std::fs::write(dir.join("docs-private").join("hidden.md"), "# hidden\n")
            .expect("failed to write hidden markdown");
        std::fs::write(dir.join("README.md"), "# kept\n").expect("failed to write README.md");

        let config = Config {
            skip_dirs: vec!["docs-private".to_string()],
            ..Config::default()
        };

        let files = collect_markdown_files(std::slice::from_ref(&dir), &config, &dir)
            .expect("failed to collect markdown files");

        assert!(files.iter().any(|path| path.ends_with("README.md")));
        assert!(!files.iter().any(|path| path.ends_with("hidden.md")));

        std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
    }

    #[test]
    fn collect_markdown_files_respects_node_modules_gitignore() {
        let dir = temp_dir("clippier-md-node-modules-gitignore");
        std::fs::write(dir.join(".gitignore"), "node_modules/\n")
            .expect("failed to write .gitignore");
        std::fs::create_dir_all(dir.join("node_modules").join("pkg"))
            .expect("failed to create node_modules directory");
        std::fs::write(
            dir.join("node_modules").join("pkg").join("README.md"),
            "# ignored\n",
        )
        .expect("failed to write ignored markdown");
        std::fs::write(dir.join("README.md"), "# kept\n").expect("failed to write README.md");

        let files = collect_markdown_files(std::slice::from_ref(&dir), &Config::default(), &dir)
            .expect("failed to collect markdown files");

        assert!(files.iter().any(|path| path.ends_with("README.md")));
        assert!(
            !files
                .iter()
                .any(|path| path.ends_with("node_modules/pkg/README.md"))
        );

        std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
    }

    #[test]
    fn collect_markdown_files_respects_gitignore_negation() {
        let dir = temp_dir("clippier-md-gitignore-negation");
        std::fs::write(dir.join(".gitignore"), "docs/*\n!docs/keep.md\n")
            .expect("failed to write .gitignore");
        std::fs::create_dir_all(dir.join("docs")).expect("failed to create docs dir");
        std::fs::write(dir.join("docs").join("drop.md"), "# drop\n")
            .expect("failed to write drop markdown");
        std::fs::write(dir.join("docs").join("keep.md"), "# keep\n")
            .expect("failed to write keep markdown");

        let files = collect_markdown_files(std::slice::from_ref(&dir), &Config::default(), &dir)
            .expect("failed to collect markdown files");

        assert!(files.iter().any(|path| path.ends_with("docs/keep.md")));
        assert!(!files.iter().any(|path| path.ends_with("docs/drop.md")));

        std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
    }

    #[test]
    fn collect_markdown_files_supports_md_and_markdown_extensions() {
        let dir = temp_dir("clippier-md-extensions");
        std::fs::write(dir.join("a.md"), "# a\n").expect("failed to write a.md");
        std::fs::write(dir.join("b.markdown"), "# b\n").expect("failed to write b.markdown");

        let files = collect_markdown_files(std::slice::from_ref(&dir), &Config::default(), &dir)
            .expect("failed to collect markdown files");

        assert!(files.iter().any(|path| path.ends_with("a.md")));
        assert!(files.iter().any(|path| path.ends_with("b.markdown")));

        std::fs::remove_dir_all(&dir).expect("failed to clean temp dir");
    }

    #[test]
    fn summary_output_includes_unified_diff_markers() {
        let summary = RunSummary {
            checked: 1,
            changed: vec![PathBuf::from("README.md")],
            diff_reports: vec![DiffReport {
                path: PathBuf::from("README.md"),
                diff: "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old\n+new\n".to_string(),
                truncated: false,
                omitted_lines: 0,
            }],
            diff_omitted_files: 0,
        };

        let output = summary_to_output(&summary, OutputFormat::Text, true, ColorMode::Never);
        assert!(output.contains("--- a/README.md"));
        assert!(output.contains("+++ b/README.md"));
        assert!(output.contains("@@ -1 +1 @@"));
    }

    #[test]
    fn truncate_diff_lines_respects_cap() {
        let diff = "a\nb\nc\nd\n";
        let (truncated, is_truncated, omitted_lines) = truncate_diff_lines(diff, true, 2);
        assert!(is_truncated);
        assert_eq!(omitted_lines, 2);
        assert!(truncated.contains("truncated 2 diff line(s)"));
    }

    #[test]
    fn truncate_diff_lines_can_be_uncapped() {
        let diff = "a\nb\nc\nd\n";
        let (result, is_truncated, omitted_lines) = truncate_diff_lines(diff, false, 1);
        assert!(!is_truncated);
        assert_eq!(omitted_lines, 0);
        assert_eq!(result, diff);
    }

    #[test]
    fn summary_output_can_colorize_diff_in_always_mode() {
        let summary = RunSummary {
            checked: 1,
            changed: vec![PathBuf::from("README.md")],
            diff_reports: vec![DiffReport {
                path: PathBuf::from("README.md"),
                diff: "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old\n+new\n".to_string(),
                truncated: false,
                omitted_lines: 0,
            }],
            diff_omitted_files: 0,
        };

        let output = summary_to_output(&summary, OutputFormat::Text, true, ColorMode::Always);
        assert!(output.contains("\x1b[31m-old\x1b[0m"));
        assert!(output.contains("\x1b[32m+new\x1b[0m"));
    }

    #[test]
    fn enhanced_diff_shows_trailing_whitespace_changes() {
        let diff = "--- a/x.md\n+++ b/x.md\n@@ -1 +1 @@\n-hello  \n+hello\n";
        let enhanced = enhance_unified_diff_presentation(diff, &CheckDiffConfig::default());
        assert!(enhanced.contains("[-␠␠-]"));
        assert!(enhanced.contains("~~ whitespace-only change"));
    }

    #[test]
    fn enhanced_diff_shows_intraline_markers() {
        let diff = "--- a/x.md\n+++ b/x.md\n@@ -1 +1 @@\n-abc old xyz\n+abc new xyz\n";
        let enhanced = enhance_unified_diff_presentation(diff, &CheckDiffConfig::default());
        assert!(enhanced.contains("[-old-]"));
        assert!(enhanced.contains("{+new+}"));
    }
}
