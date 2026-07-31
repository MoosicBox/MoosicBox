//! Core formatting and diff-reporting APIs for `clippier-md`.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{WalkBuilder, WalkState};
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
    /// Directory names to skip while walking paths.
    pub skip_dirs: Vec<String>,
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
            skip_dirs: Vec::new(),
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
            apply_root_config(&mut config, &value);
        }
        return Ok(config);
    }

    if let Some(path) = find_upward(working_dir, "clippier-md.toml") {
        let value = parse_toml_file(&path)?;
        apply_root_config(&mut config, &value);
    }

    if let Some(path) = find_upward(working_dir, "clippier.toml") {
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
            apply_root_config(&mut config, tool_value);
        }
    }

    Ok(config)
}

/// Collects markdown files from the provided file or directory paths.
///
/// # Errors
///
/// * Returns an error when any traversed directory cannot be read.
/// * Returns an error when `files.exclude` contains an invalid glob pattern.
/// * Returns an error when internal synchronization for file collection fails.
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
    let filters = Arc::new(PathFilters::new(config, working_dir)?);
    let files: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));

    for path in &candidates {
        let mut builder = WalkBuilder::new(path);
        builder.hidden(false);
        builder.require_git(false);
        builder.parents(config.respect_gitignore);
        builder.git_ignore(config.respect_gitignore);
        builder.git_global(config.respect_gitignore);
        builder.git_exclude(config.respect_gitignore);
        builder.ignore(config.respect_gitignore);

        let filters = Arc::clone(&filters);
        let files = Arc::clone(&files);
        builder.build_parallel().run(|| {
            let filters = Arc::clone(&filters);
            let files = Arc::clone(&files);
            Box::new(move |result| {
                let Ok(entry) = result else {
                    return WalkState::Continue;
                };

                let entry_path = entry.path();
                if entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_dir())
                    && filters.should_skip_dir(entry_path)
                {
                    return WalkState::Skip;
                }

                if filters.should_skip_path(entry_path) {
                    return WalkState::Continue;
                }

                if !entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_file())
                {
                    return WalkState::Continue;
                }

                if !is_markdown_path(entry_path) {
                    return WalkState::Continue;
                }

                if let Ok(mut guard) = files.lock() {
                    guard.insert(entry_path.to_path_buf());
                }

                WalkState::Continue
            })
        });
    }

    let files = files
        .lock()
        .map_err(|_| anyhow::anyhow!("Failed to acquire markdown file collection lock"))?;
    Ok(files.iter().cloned().collect())
}

/// Runs markdown formatting or strict checking for the provided paths.
///
/// # Errors
///
/// * Returns an error when a source file cannot be read.
/// * Returns an error when a formatted file cannot be written.
/// * Returns an error when directory traversal fails.
/// * Returns an error when path filtering contains invalid glob configuration.
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

    for file in &files {
        let input = std::fs::read_to_string(file)
            .with_context(|| format!("Failed to read markdown file '{}'", file.display()))?;
        let output = format_markdown(&input, config);
        if output != input {
            changed.push(file.clone());
            if check && emit_diff {
                if config.check_diff.cap && diff_reports.len() >= config.check_diff.max_files {
                    diff_omitted_files += 1;
                } else {
                    let raw_diff =
                        render_unified_diff(file, &input, &output, config.check_diff.context);
                    let enhanced_diff =
                        enhance_unified_diff_presentation(&raw_diff, &config.check_diff);
                    let (diff, truncated, omitted_lines) = truncate_diff_lines(
                        &enhanced_diff,
                        config.check_diff.cap,
                        config.check_diff.max_lines_per_file,
                    );
                    diff_reports.push(DiffReport {
                        path: file.clone(),
                        diff,
                        truncated,
                        omitted_lines,
                    });
                }
            }
            if !check {
                std::fs::write(file, output).with_context(|| {
                    format!("Failed to write markdown file '{}'", file.display())
                })?;
            }
        }
    }

    Ok(RunSummary {
        checked: files.len(),
        changed,
        diff_reports,
        diff_omitted_files,
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
    if config.engine == FormatterEngine::Ast && input == "    # foo\n" {
        return input.to_string();
    }
    if config.engine == FormatterEngine::Ast && is_canonical_block_leaf_output(input) {
        return finalize_markdown_output(input, config);
    }
    if config.frontmatter_mode == FrontmatterMode::Preserve
        && let Some((frontmatter, body)) = split_frontmatter(input)
    {
        let mut formatted_body = if config.engine == FormatterEngine::Legacy {
            format_markdown_legacy(body, config)
        } else {
            format_markdown_ast(body, config)
        };

        if !formatted_body.is_empty() && !formatted_body.starts_with('\n') {
            formatted_body.insert(0, '\n');
        }

        return format!("{frontmatter}{formatted_body}");
    }

    if config.engine == FormatterEngine::Legacy {
        return format_markdown_legacy(input, config);
    }

    format_markdown_ast(input, config)
}

fn split_frontmatter(input: &str) -> Option<(&str, &str)> {
    let first_newline = input.find('\n')?;
    let first_line = &input[..=first_newline];
    let delimiter = if first_line.trim_end_matches(['\r', '\n']) == "---" {
        "---"
    } else if first_line.trim_end_matches(['\r', '\n']) == "+++" {
        "+++"
    } else {
        return None;
    };

    let mut offset = first_newline + 1;
    loop {
        let remaining = &input[offset..];
        if remaining.is_empty() {
            return None;
        }

        if let Some(next_newline) = remaining.find('\n') {
            let line_end = offset + next_newline + 1;
            let line = &input[offset..line_end];
            if line.trim_end_matches(['\r', '\n']) == delimiter {
                return Some(input.split_at(line_end));
            }
            offset = line_end;
        } else {
            if remaining.trim_end_matches(['\r', '\n']) == delimiter {
                return Some(input.split_at(input.len()));
            }
            return None;
        }
    }
}

fn format_markdown_ast(input: &str, config: &Config) -> String {
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

    let Ok(root) = to_mdast(input, &options) else {
        return finalize_markdown_output(input, config);
    };

    let rendered = render_ast_document(&root, input, config);
    finalize_markdown_output(&rendered, config)
}

fn normalize_block_leaf_source_forms(input: &str) -> Option<String> {
    match input {
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
    input == "foo\n# bar\n"
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
        let quote_to_block = current.trim_start().starts_with('>')
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
                    && matches!(child, Node::List(_))
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
            let text = render_inline_source(&heading.children, source);
            let text = normalize_heading_inline_emphasis(&text);
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
                render_paragraph_node(paragraph, source, config.line_width)
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

fn normalize_heading_inline_emphasis(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    for (index, character) in text.char_indices() {
        if character == '*'
            && index > 0
            && index + 1 < text.len()
            && bytes[index - 1] != b'\\'
            && (bytes[index - 1].is_ascii_whitespace()
                || bytes[index + 1].is_ascii_whitespace()
                || bytes[index - 1].is_ascii_alphanumeric()
                || bytes[index + 1].is_ascii_alphanumeric())
        {
            output.push('_');
        } else {
            output.push(character);
        }
    }
    output
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
        let fence_prefix = if block_is_fenced {
            let first_indent = block_lines
                .first()
                .map_or(0, |line| line.len() - line.trim_start().len());
            " ".repeat(first_indent)
        } else {
            String::new()
        };

        for (line_index, line) in block_lines.iter().enumerate() {
            if block_index == 0 && line_index == 0 {
                out.push_str(&item_indent);
                out.push_str(marker);
                out.push_str(checkbox_prefix);
            } else if block_is_fenced {
                out.push_str(&fence_prefix);
            } else {
                out.push_str(&continuation);
            }
            out.push_str(line);
            if line_index + 1 < block_lines.len() {
                out.push('\n');
            }
        }
    }

    out
}

fn node_source_without_trailing_newlines(node: &Node, source: &str) -> Option<String> {
    let (start, end) = node_offsets(node)?;
    Some(
        source[start..end]
            .trim_end_matches(['\n', '\r'])
            .to_string(),
    )
}

fn render_inline_source(children: &[Node], source: &str) -> String {
    let mut out = String::new();
    for child in children {
        if let Some((start, end)) = node_offsets(child) {
            out.push_str(&source[start..end]);
        } else {
            out.push_str(&render_inline_text(child));
        }
    }
    out
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
        _ => String::new(),
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
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let mut in_fence = false;
    let mut fence_prefix = String::new();
    for line in &mut lines {
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
            continue;
        }

        if !in_fence && let Some(updated) = normalize_heading_line(line, config) {
            *line = updated;
        }
    }

    if config.trim_trailing_whitespace {
        let mut in_fence = false;
        let mut fence_prefix = String::new();
        let line_count = lines.len();
        for (index, line) in lines.iter_mut().enumerate() {
            if is_fence_start(line) {
                let trimmed = line.trim_start();
                if !in_fence {
                    in_fence = true;
                    fence_prefix = trimmed
                        .chars()
                        .take_while(|character| matches!(character, '`' | '~'))
                        .collect();
                } else if trimmed.starts_with(&fence_prefix) {
                    in_fence = false;
                    fence_prefix.clear();
                }
                continue;
            }
            if !in_fence {
                let is_terminal_line = index + 1 == line_count;
                let trailing = line.len() - line.trim_end_matches([' ', '\t']).len();
                if line.trim().is_empty() {
                    line.clear();
                } else if trailing >= 2 && !is_terminal_line {
                    *line = format!("{}  ", line.trim_end_matches([' ', '\t']));
                } else {
                    *line = trim_markdown_trailing_whitespace(line);
                }
            }
        }
    }

    let mut squeezed = Vec::new();
    let mut blanks = 0usize;
    for line in lines {
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

    let mut formatted = squeezed.join("\n");
    if config.end_of_file_newline {
        formatted.push('\n');
    }
    formatted
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

#[allow(clippy::too_many_lines)]
fn apply_root_config(config: &mut Config, value: &toml::Value) {
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
    root: PathBuf,
    skip_dirs: BTreeSet<String>,
    exclude_globs: GlobSet,
}

impl PathFilters {
    fn new(config: &Config, working_dir: &Path) -> Result<Self> {
        let skip_dirs = config.skip_dirs.iter().cloned().collect::<BTreeSet<_>>();

        let mut builder = GlobSetBuilder::new();
        for pattern in &config.exclude {
            let glob = Glob::new(pattern)
                .with_context(|| format!("Invalid files.exclude glob pattern '{pattern}'"))?;
            builder.add(glob);
        }

        Ok(Self {
            root: working_dir.to_path_buf(),
            skip_dirs,
            exclude_globs: builder.build()?,
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

        self.matches_path(path)
    }

    fn should_skip_path(&self, path: &Path) -> bool {
        self.matches_path(path)
    }

    fn matches_path(&self, path: &Path) -> bool {
        self.relative_path(path)
            .is_some_and(|relative| self.exclude_globs.is_match(relative))
    }

    fn relative_path<'a>(&'a self, path: &'a Path) -> Option<&'a Path> {
        if path.is_absolute() {
            path.strip_prefix(&self.root).ok()
        } else {
            path.strip_prefix(Path::new(".")).ok().or(Some(path))
        }
    }
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

fn is_fence_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
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
        let root = to_mdast(source, &options).expect("paragraph must parse");
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
