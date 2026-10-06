//! Shared command-line interface for standalone and bundled Markdown formatting.

use std::path::PathBuf;

use crate::{ColorMode, Config, OutputFormat, run_fmt, summary_to_output};
use anyhow::Result;
use clap::{Subcommand, ValueEnum};

/// Markdown-only commands, shared by both executables.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Format markdown files
    Fmt(FmtArgs),
}

/// Markdown formatter command-line options.
#[derive(Debug, clap::Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct FmtArgs {
    /// Validate formatting without writing files
    #[arg(long)]
    check: bool,
    /// Alias for --check
    #[arg(long)]
    dry_run: bool,
    /// Disable diff output in check mode
    #[arg(long)]
    no_diff: bool,
    /// Disable diff caps in check mode
    #[arg(long)]
    no_diff_cap: bool,
    /// Override diff context lines in check mode
    #[arg(long)]
    diff_context: Option<u32>,
    /// Override max number of files with shown diffs
    #[arg(long)]
    diff_max_files: Option<usize>,
    /// Override max diff lines per file
    #[arg(long)]
    diff_max_lines_per_file: Option<usize>,
    /// Output mode
    #[arg(long, value_enum, default_value_t = OutputArg::Text)]
    output: OutputArg,
    /// Color mode for check diff output
    #[arg(long, value_enum, default_value_t = ColorArg::Auto)]
    color: ColorArg,
    /// Optional config file path
    #[arg(long)]
    config: Option<PathBuf>,
    /// Override line width
    #[arg(long)]
    line_width: Option<usize>,
    /// Override list indentation width
    #[arg(long)]
    list_indent_width: Option<usize>,
    /// Files or directories to process
    paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputArg {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ColorArg {
    Auto,
    Always,
    Never,
}

/// Execute a Markdown command and print its output.
///
/// Returns 1 for unformatted files in check mode, otherwise 0.
///
/// # Errors
///
/// * Returns an error if the working directory or configuration cannot be read.
/// * Returns an error if traversal, reading, or writing Markdown files fails.
pub fn run(command: &Commands) -> Result<i32> {
    match command {
        Commands::Fmt(args) => run_fmt_command(args),
    }
}

fn run_fmt_command(args: &FmtArgs) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let mut config = crate::load_config(&cwd, args.config.as_deref())?;
    apply_cli_overrides(&mut config, args);
    let check = args.check || args.dry_run;
    let summary = run_fmt(&args.paths, check, !args.no_diff, &config)?;
    let output = summary_to_output(
        &summary,
        to_output_format(args.output),
        check,
        to_color_mode(args.color),
    );
    println!("{output}");
    Ok(i32::from(check && !summary.changed.is_empty()))
}

fn apply_cli_overrides(config: &mut Config, args: &FmtArgs) {
    if let Some(width) = args.line_width {
        config.line_width = width;
    }
    if let Some(indent) = args.list_indent_width {
        config.list_indent_width = indent.max(1);
    }
    if args.no_diff_cap {
        config.check_diff.cap = false;
    }
    if let Some(context) = args.diff_context {
        config.check_diff.context = context;
    }
    if let Some(max_files) = args.diff_max_files {
        config.check_diff.max_files = max_files;
    }
    if let Some(max_lines_per_file) = args.diff_max_lines_per_file {
        config.check_diff.max_lines_per_file = max_lines_per_file;
    }
}

const fn to_output_format(output: OutputArg) -> OutputFormat {
    match output {
        OutputArg::Text => OutputFormat::Text,
        OutputArg::Json => OutputFormat::Json,
    }
}

const fn to_color_mode(color: ColorArg) -> ColorMode {
    match color {
        ColorArg::Auto => ColorMode::Auto,
        ColorArg::Always => ColorMode::Always,
        ColorArg::Never => ColorMode::Never,
    }
}
