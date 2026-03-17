#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{WalkBuilder, WalkState};
use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontmatterMode {
    Preserve,
    Normalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyle {
    Preserve,
    Dash,
    Plus,
    Asterisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListIndentationMode {
    Preserve,
    Normalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProseWrapMode {
    Always,
    Preserve,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub line_width: usize,
    pub trim_trailing_whitespace: bool,
    pub end_of_file_newline: bool,
    pub blank_lines_max_consecutive: usize,
    pub list_indent_width: usize,
    pub list_style: ListStyle,
    pub list_indentation: ListIndentationMode,
    pub frontmatter_mode: FrontmatterMode,
    pub respect_gitignore: bool,
    pub exclude: Vec<String>,
    pub skip_dirs: Vec<String>,
    pub check_diff: CheckDiffConfig,
    pub prose_wrap: ProseWrapMode,
}

#[derive(Debug, Clone)]
pub struct CheckDiffConfig {
    pub cap: bool,
    pub context: u32,
    pub max_files: usize,
    pub max_lines_per_file: usize,
    pub intraline: bool,
    pub show_invisible_whitespace: bool,
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
            blank_lines_max_consecutive: 2,
            list_indent_width: 4,
            list_style: ListStyle::Preserve,
            list_indentation: ListIndentationMode::Preserve,
            frontmatter_mode: FrontmatterMode::Preserve,
            respect_gitignore: true,
            exclude: Vec::new(),
            skip_dirs: Vec::new(),
            check_diff: CheckDiffConfig::default(),
            prose_wrap: ProseWrapMode::Always,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub checked: usize,
    pub changed: Vec<PathBuf>,
    pub diff_reports: Vec<DiffReport>,
    pub diff_omitted_files: usize,
}

#[derive(Debug, Clone)]
pub struct DiffReport {
    pub path: PathBuf,
    pub diff: String,
    pub truncated: bool,
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
pub fn format_markdown(input: &str, config: &Config) -> String {
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

fn finish_line(line: &str, config: &Config) -> String {
    if config.trim_trailing_whitespace {
        line.trim_end().to_string()
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
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|value| *value == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = trimmed[hashes..].trim_start();
    let mut normalized = format!("{} {}", "#".repeat(hashes), rest);
    if config.trim_trailing_whitespace {
        normalized = normalized.trim_end().to_string();
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
        let input = "---\ntitle: Test\n---\n#Heading\n";
        let output = format_markdown(input, &Config::default());
        assert_eq!(output, "---\ntitle: Test\n---\n# Heading\n");
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
