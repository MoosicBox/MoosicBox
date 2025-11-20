#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use anyhow::{Context, Result};
use bytesize::ByteSize;
use cargo_metadata::{MetadataCommand, TargetKind, camino::Utf8Path};
use clap::Parser;
use glob::glob;
use regex::Regex;
use serde_json::json;
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    time::UNIX_EPOCH,
};

/// Command-line arguments for the bloaty binary size analysis tool.
#[derive(Parser)]
#[command(
    author,
    version,
    about = "Run cargo-bloat, cargo-llvm-lines, or cargo size across workspace members"
)]
struct Args {
    #[arg(short, long, value_name = "PACKAGE")]
    package: Vec<String>,

    #[arg(long, value_name = "PACKAGE_PATTERN")]
    package_pattern: Option<String>,

    #[arg(long, value_name = "SKIP_PACKAGES")]
    skip_packages: Vec<String>,

    #[arg(long, value_name = "SKIP_PACKAGE_PATTERN")]
    skip_package_pattern: Option<String>,

    #[arg(long, value_name = "SKIP_FEATURES")]
    skip_features: Vec<String>,

    #[arg(long, value_name = "SKIP_FEATURE_PATTERN")]
    skip_feature_pattern: Option<String>,

    #[arg(short, long, value_parser = ["bloat", "llvm-lines", "size"], value_name = "TOOL")]
    tool: Vec<String>,

    #[arg(long, value_name = "REPORT_FILE")]
    report_file: Option<String>,

    #[arg(long, value_parser = ["text", "json", "jsonl", "all"], default_value = "all", value_name = "FORMAT")]
    output_format: Vec<String>,
}

/// File handles for the different output report formats.
struct ReportFiles {
    /// Optional text format report file handle.
    text: Option<fs::File>,
    /// Optional JSONL format report file handle.
    jsonl: Option<fs::File>,
}

/// Context for tracking analysis state and output files across package analysis runs.
struct AnalysisContext {
    /// Unix timestamp when the analysis was started.
    timestamp: u64,
    /// Base filename for output reports (without extension).
    base_filename: String,
    /// File handles for active report outputs.
    report_files: ReportFiles,
    /// In-memory JSON report structure being built during analysis.
    json_report: serde_json::Value,
}

/// Parses and normalizes command-line arguments.
///
/// Expands comma-separated values in package, skip-packages, skip-features, tool, and
/// output-format arguments into individual items.
#[must_use]
fn parse_args() -> Args {
    let mut args = Args::parse();

    args.package = args
        .package
        .into_iter()
        .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
        .collect();

    args.skip_packages = args
        .skip_packages
        .into_iter()
        .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
        .collect();

    args.skip_features = args
        .skip_features
        .into_iter()
        .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
        .collect();

    args.tool = args
        .tool
        .into_iter()
        .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
        .collect();

    args.output_format = args
        .output_format
        .into_iter()
        .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
        .collect();

    args
}

/// Verifies that all requested cargo tools are installed.
///
/// # Panics
///
/// Exits the process with status code 1 if any required tools are not available.
fn check_tools_availability(tools: &[String]) {
    let mut any_unavailable = false;

    for tool in tools {
        if !tool_available(tool) {
            eprintln!("[error] cargo {tool} not found; install cargo-{tool}");
            any_unavailable = true;
        }
    }

    if any_unavailable {
        std::process::exit(1);
    }
}

/// Creates report output files based on command-line arguments.
///
/// # Errors
///
/// * File creation fails
/// * Writing initial report headers fails
fn setup_report_files(args: &Args) -> Result<AnalysisContext> {
    let timestamp = switchy_time::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let base_filename = args
        .report_file
        .clone()
        .map_or_else(|| format!("bloaty_report_{timestamp}"), |path| path);

    let should_output_text = args.output_format.contains(&"text".to_string())
        || args.output_format.contains(&"all".to_string());
    let should_output_jsonl = args.output_format.contains(&"jsonl".to_string())
        || args.output_format.contains(&"all".to_string());

    let mut text_report_file = if should_output_text {
        Some(fs::File::create(format!("{base_filename}.txt"))?)
    } else {
        None
    };

    let jsonl_report_file = if should_output_jsonl {
        Some(fs::File::create(format!("{base_filename}.jsonl"))?)
    } else {
        None
    };

    if let Some(report) = &mut text_report_file {
        writeln!(report, "Bloaty Analysis Report")?;
        writeln!(report, "===================\n")?;
    }

    let json_report = json!({
        "timestamp": timestamp,
        "packages": []
    });

    Ok(AnalysisContext {
        timestamp,
        base_filename,
        report_files: ReportFiles {
            text: text_report_file,
            jsonl: jsonl_report_file,
        },
        json_report,
    })
}

/// Writes a JSONL package start event to the report.
///
/// # Errors
///
/// * JSON serialization fails
/// * Writing to the JSONL report file fails
fn write_jsonl_package_start(ctx: &mut AnalysisContext, package_name: &str) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.jsonl {
        writeln!(
            report,
            "{}",
            serde_json::to_string(&json!({
                "type": "package_start",
                "name": package_name,
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

/// Writes a JSONL package end event to the report.
///
/// # Errors
///
/// * JSON serialization fails
/// * Writing to the JSONL report file fails
fn write_jsonl_package_end(ctx: &mut AnalysisContext, package_name: &str) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.jsonl {
        writeln!(
            report,
            "{}",
            serde_json::to_string(&json!({
                "type": "package_end",
                "name": package_name,
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

/// Writes a JSONL target start event to the report.
///
/// # Errors
///
/// * JSON serialization fails
/// * Writing to the JSONL report file fails
fn write_jsonl_target_start(
    ctx: &mut AnalysisContext,
    package_name: &str,
    target_name: &str,
) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.jsonl {
        writeln!(
            report,
            "{}",
            serde_json::to_string(&json!({
                "type": "target_start",
                "package": package_name,
                "target": target_name,
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

/// Writes a JSONL target end event to the report.
///
/// # Errors
///
/// * JSON serialization fails
/// * Writing to the JSONL report file fails
fn write_jsonl_target_end(
    ctx: &mut AnalysisContext,
    package_name: &str,
    target_name: &str,
) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.jsonl {
        writeln!(
            report,
            "{}",
            serde_json::to_string(&json!({
                "type": "target_end",
                "package": package_name,
                "target": target_name,
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

/// Writes a JSONL base rlib size record to the report.
///
/// # Errors
///
/// * JSON serialization fails
/// * Writing to the JSONL report file fails
fn write_jsonl_base_size(
    ctx: &mut AnalysisContext,
    package_name: &str,
    target_name: &str,
    base_size: u64,
) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.jsonl {
        writeln!(
            report,
            "{}",
            serde_json::to_string(&json!({
                "type": "base_size",
                "package": package_name,
                "target": target_name,
                "size": base_size,
                "size_formatted": ByteSize(base_size).to_string(),
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

/// Writes a JSONL feature rlib size record to the report.
///
/// # Errors
///
/// * JSON serialization fails
/// * Writing to the JSONL report file fails
fn write_jsonl_feature(
    ctx: &mut AnalysisContext,
    package_name: &str,
    target_name: &str,
    feature: &str,
    size: u64,
    diff: i64,
) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.jsonl {
        let sign = if diff >= 0 { '+' } else { '-' };
        writeln!(
            report,
            "{}",
            serde_json::to_string(&json!({
                "type": "feature",
                "package": package_name,
                "target": target_name,
                "feature": feature,
                "size": size,
                "diff": diff,
                "diff_formatted": format!("{sign}{}", ByteSize(diff.unsigned_abs())),
                "size_formatted": ByteSize(size).to_string(),
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

/// Writes a package header to the text report.
///
/// # Errors
///
/// * Writing to the text report file fails
fn write_text_package_header(ctx: &mut AnalysisContext, package_name: &str) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        writeln!(report, "\nPackage: {package_name}")?;
        writeln!(report, "===================")?;
    }
    Ok(())
}

/// Writes a target header to the text report.
///
/// # Errors
///
/// * Writing to the text report file fails
fn write_text_target_header(ctx: &mut AnalysisContext, target_name: &str) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        writeln!(report, "\nTarget: {target_name}")?;
        writeln!(report, "-------------------")?;
    }
    Ok(())
}

/// Writes a base rlib size to the text report.
///
/// # Errors
///
/// * Writing to the text report file fails
fn write_text_base_size(ctx: &mut AnalysisContext, base_size: u64) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        writeln!(report, "Base size: {}", ByteSize(base_size))?;
    }
    Ok(())
}

/// Writes a feature rlib size to the text report.
///
/// # Errors
///
/// * Writing to the text report file fails
fn write_text_feature(
    ctx: &mut AnalysisContext,
    feature: &str,
    size: u64,
    diff: i64,
) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        let sign = if diff >= 0 { '+' } else { '-' };
        writeln!(
            report,
            "Feature: {:<15} | Size: {} | Diff: {}{}",
            feature,
            ByteSize(size),
            sign,
            ByteSize(diff.unsigned_abs())
        )?;
    }
    Ok(())
}

/// Determines whether a feature should be skipped based on filter patterns.
///
/// # Errors
///
/// * Invalid regex pattern in `skip_feature_pattern`
fn should_skip_feature(feature: &str, args: &Args) -> Result<bool> {
    if args.skip_features.contains(&feature.to_string()) {
        return Ok(true);
    }

    if let Some(pattern) = &args.skip_feature_pattern {
        let re = Regex::new(pattern).context(format!("invalid regex pattern: {pattern}"))?;
        if re.is_match(feature) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Determines whether a package should be analyzed based on include/exclude filters.
///
/// # Errors
///
/// * Invalid regex pattern in `package_pattern` or `skip_package_pattern`
fn should_analyze_package(pkg_name: &str, args: &Args) -> Result<bool> {
    // If specific packages are specified, check if this package is in the list
    if !args.package.is_empty() && !args.package.contains(&pkg_name.to_string()) {
        return Ok(false);
    }

    // Check package pattern if specified
    if let Some(pattern) = &args.package_pattern {
        let re = Regex::new(pattern).context(format!("invalid package pattern: {pattern}"))?;
        if !re.is_match(pkg_name) {
            return Ok(false);
        }
    }

    // Check skip packages list
    if args.skip_packages.contains(&pkg_name.to_string()) {
        return Ok(false);
    }

    // Check skip package pattern if specified
    if let Some(pattern) = &args.skip_package_pattern {
        let re = Regex::new(pattern).context(format!("invalid skip package pattern: {pattern}"))?;
        if re.is_match(pkg_name) {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Analyzes a single target, measuring size impact of all features.
///
/// Builds the target with no features (base size), then with each feature individually,
/// recording both rlib and binary sizes (if applicable).
///
/// # Errors
///
/// * Building the target fails
/// * Measuring the built artifact fails
/// * Writing to report files fails
fn analyze_target(
    ctx: &mut AnalysisContext,
    pkg: &cargo_metadata::Package,
    target: &cargo_metadata::Target,
    available_features: &[String],
    args: &Args,
    metadata: &cargo_metadata::Metadata,
) -> Result<serde_json::Value> {
    let mut target_json = json!({
        "name": target.name,
        "base_size": 0,
        "base_binary_size": 0,
        "features": []
    });

    write_text_target_header(ctx, &target.name)?;
    write_jsonl_target_start(ctx, &pkg.name, &target.name)?;

    // Build and measure base rlib
    let base_size = build_and_measure_rlib(
        &pkg.manifest_path,
        &metadata.target_directory,
        &pkg.name,
        None,
    )?;
    println!("  base rlib: {}", ByteSize(base_size));
    write_text_base_size(ctx, base_size)?;
    write_jsonl_base_size(ctx, &pkg.name, &target.name, base_size)?;

    // Build and measure base binary if it's a binary target
    let base_binary_size = if target.kind.iter().any(|k| k == &TargetKind::Bin) {
        let size = build_and_measure_binary(
            &pkg.manifest_path,
            &metadata.target_directory,
            &target.name,
            None,
        )?;
        println!("  base binary: {}", ByteSize(size));
        write_text_base_binary_size(ctx, size)?;
        write_jsonl_base_binary_size(ctx, &pkg.name, &target.name, size)?;
        size
    } else {
        0
    };

    target_json["base_size"] = json!(base_size);
    target_json["base_binary_size"] = json!(base_binary_size);

    for feat in available_features {
        if should_skip_feature(feat, args)? {
            continue;
        }

        // Build and measure rlib with feature
        let size = build_and_measure_rlib(
            &pkg.manifest_path,
            &metadata.target_directory,
            &pkg.name,
            Some(feat),
        )?;

        #[allow(clippy::cast_possible_wrap)]
        let diff = size as i64 - base_size as i64;

        println!(
            "  feature {feat:<15} rlib: {} ({}{})",
            ByteSize(size),
            if diff >= 0 { '+' } else { '-' },
            ByteSize(diff.unsigned_abs()),
        );

        write_text_feature(ctx, feat, size, diff)?;
        write_jsonl_feature(ctx, &pkg.name, &target.name, feat, size, diff)?;

        // Build and measure binary with feature if it's a binary target
        let binary_size = if target.kind.iter().any(|k| k == &TargetKind::Bin) {
            let size = build_and_measure_binary(
                &pkg.manifest_path,
                &metadata.target_directory,
                &target.name,
                Some(feat),
            )?;
            #[allow(clippy::cast_possible_wrap)]
            let binary_diff = size as i64 - base_binary_size as i64;
            println!(
                "  feature {:<15} binary: {} ({}{})",
                feat,
                ByteSize(size),
                if binary_diff >= 0 { '+' } else { '-' },
                ByteSize(binary_diff.unsigned_abs()),
            );
            write_text_binary_feature(ctx, feat, size, binary_diff)?;
            write_jsonl_binary_feature(ctx, &pkg.name, &target.name, feat, size, binary_diff)?;
            size
        } else {
            0
        };

        #[allow(clippy::cast_possible_wrap)]
        target_json["features"].as_array_mut().unwrap().push(json!({
            "name": feat,
            "size": size,
            "diff": diff,
            "diff_formatted": format!("{}{}", if diff >= 0 { '+' } else { '-' }, ByteSize(diff.unsigned_abs())),
            "size_formatted": ByteSize(size).to_string(),
            "binary_size": binary_size,
            "binary_diff": binary_size as i64 - base_binary_size as i64,
            "binary_diff_formatted": format!("{}{}", if binary_size >= base_binary_size { '+' } else { '-' }, ByteSize((binary_size as i64 - base_binary_size as i64).unsigned_abs())),
            "binary_size_formatted": ByteSize(binary_size).to_string()
        }));
    }

    write_jsonl_target_end(ctx, &pkg.name, &target.name)?;
    Ok(target_json)
}

/// Writes a base binary size to the text report.
///
/// # Errors
///
/// * Writing to the text report file fails
fn write_text_base_binary_size(ctx: &mut AnalysisContext, base_size: u64) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        writeln!(report, "Base binary size: {}", ByteSize(base_size))?;
    }
    Ok(())
}

/// Writes a feature binary size to the text report.
///
/// # Errors
///
/// * Writing to the text report file fails
fn write_text_binary_feature(
    ctx: &mut AnalysisContext,
    feature: &str,
    size: u64,
    diff: i64,
) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        let sign = if diff >= 0 { '+' } else { '-' };
        writeln!(
            report,
            "Feature: {:<15} | Binary Size: {} | Binary Diff: {}{}",
            feature,
            ByteSize(size),
            sign,
            ByteSize(diff.unsigned_abs())
        )?;
    }
    Ok(())
}

/// Writes a JSONL base binary size record to the report.
///
/// # Errors
///
/// * JSON serialization fails
/// * Writing to the JSONL report file fails
fn write_jsonl_base_binary_size(
    ctx: &mut AnalysisContext,
    package_name: &str,
    target_name: &str,
    base_size: u64,
) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.jsonl {
        writeln!(
            report,
            "{}",
            serde_json::to_string(&json!({
                "type": "base_binary_size",
                "package": package_name,
                "target": target_name,
                "size": base_size,
                "size_formatted": ByteSize(base_size).to_string(),
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

/// Writes a JSONL feature binary size record to the report.
///
/// # Errors
///
/// * JSON serialization fails
/// * Writing to the JSONL report file fails
fn write_jsonl_binary_feature(
    ctx: &mut AnalysisContext,
    package_name: &str,
    target_name: &str,
    feature: &str,
    size: u64,
    diff: i64,
) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.jsonl {
        let sign = if diff >= 0 { '+' } else { '-' };
        writeln!(
            report,
            "{}",
            serde_json::to_string(&json!({
                "type": "binary_feature",
                "package": package_name,
                "target": target_name,
                "feature": feature,
                "size": size,
                "diff": diff,
                "diff_formatted": format!("{}{}", sign, ByteSize(diff.unsigned_abs())),
                "size_formatted": ByteSize(size).to_string(),
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

/// Analyzes a workspace package, running requested tools and measuring feature sizes.
///
/// # Errors
///
/// * Package filtering fails
/// * Running analysis tools fails
/// * Writing to report files fails
fn analyze_package(
    ctx: &mut AnalysisContext,
    pkg: &cargo_metadata::Package,
    args: &Args,
    metadata: &cargo_metadata::Metadata,
) -> Result<()> {
    if !should_analyze_package(&pkg.name, args)? {
        return Ok(());
    }

    println!("\n=== Analyzing package: {} ===", pkg.name);
    write_text_package_header(ctx, &pkg.name)?;
    write_jsonl_package_start(ctx, &pkg.name)?;

    let mut package_json = json!({
        "name": pkg.name,
        "targets": []
    });

    let available_features: Vec<String> = pkg.features.keys().cloned().collect();

    for target in &pkg.targets {
        if target
            .kind
            .iter()
            .any(|k| matches!(k, TargetKind::Bin | TargetKind::CDyLib | TargetKind::DyLib))
        {
            for tool in &args.tool {
                let mut cmd = Command::new("cargo");

                cmd.current_dir(pkg.manifest_path.parent().unwrap())
                    .arg(tool)
                    .arg("--release");

                if target.kind.iter().any(|k| k == &TargetKind::Bin) {
                    cmd.arg("--bin").arg(&target.name);
                } else {
                    cmd.arg("--lib");
                }

                println!("$ {cmd:?}");
                let status = cmd.status().context("running tool")?;
                if !status.success() {
                    eprintln!("[error] {} failed for {} ({})", tool, pkg.name, target.name);
                }
            }
        }
    }

    let rlib_targets: Vec<_> = pkg
        .targets
        .iter()
        .filter(|t| {
            t.kind.iter().any(|k| k == &TargetKind::Lib)
                && !t
                    .kind
                    .iter()
                    .any(|k| matches!(k, TargetKind::CDyLib | TargetKind::DyLib))
        })
        .collect();

    if !rlib_targets.is_empty() {
        for target in rlib_targets {
            let target_json =
                analyze_target(ctx, pkg, target, &available_features, args, metadata)?;
            package_json["targets"]
                .as_array_mut()
                .unwrap()
                .push(target_json);
        }
    }

    write_jsonl_package_end(ctx, &pkg.name)?;
    ctx.json_report["packages"]
        .as_array_mut()
        .unwrap()
        .push(package_json);

    Ok(())
}

/// Writes the final consolidated JSON report if JSON output is enabled.
///
/// # Errors
///
/// * Creating the JSON report file fails
/// * Serializing the JSON report fails
/// * Writing to the JSON report file fails
fn write_final_json_report(ctx: &AnalysisContext, args: &Args) -> Result<()> {
    let should_output_json = args.output_format.contains(&"json".to_string())
        || args.output_format.contains(&"all".to_string());
    if should_output_json {
        let mut json_file = fs::File::create(format!("{}.json", ctx.base_filename))?;
        writeln!(
            json_file,
            "{}",
            serde_json::to_string_pretty(&ctx.json_report)?
        )?;
    }
    Ok(())
}

/// Executes bloaty binary size analysis across workspace packages.
///
/// # Errors
///
/// * Loading workspace metadata fails
/// * Setting up report files fails
/// * Analyzing packages fails
/// * Writing final reports fails
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn main() -> Result<()> {
    pretty_env_logger::init();

    let args = parse_args();
    check_tools_availability(&args.tool);
    let mut ctx = setup_report_files(&args)?;
    let metadata = MetadataCommand::new().no_deps().exec()?;

    for pkg in metadata
        .packages
        .iter()
        .filter(|p| metadata.workspace_members.contains(&p.id))
    {
        analyze_package(&mut ctx, pkg, &args, &metadata)?;
    }

    write_final_json_report(&ctx, &args)?;
    Ok(())
}

/// Builds an rlib with optional features and measures its size.
///
/// # Errors
///
/// * Cargo clean fails
/// * Cargo build fails
/// * Finding the built rlib fails
/// * Reading rlib metadata fails
fn build_and_measure_rlib(
    manifest: &Utf8Path,
    target_dir: &Utf8Path,
    crate_name: &str,
    feat: Option<&String>,
) -> Result<u64> {
    let _ = Command::new("cargo")
        .current_dir(manifest.parent().unwrap())
        .arg("clean")
        .arg("--release")
        .status();

    let mut cmd = Command::new("cargo");

    cmd.current_dir(manifest.parent().unwrap())
        .arg("build")
        .arg("--release")
        .arg("--no-default-features");

    if let Some(f) = feat {
        cmd.arg("--features").arg(f);
    }

    println!("$ {cmd:?}\n");
    cmd.status().context("building rlib")?;

    let deps = target_dir.join("release").join("deps");
    let prefix = format!("lib{}-", crate_name.replace('-', "_"));
    for entry in glob(&format!("{deps}/*.rlib"))? {
        let path = entry?;
        if let Some(fname) = path.file_name().and_then(|f| f.to_str())
            && fname.starts_with(&prefix)
        {
            return Ok(fs::metadata(&path)?.len());
        }
    }
    Err(anyhow::anyhow!("rlib for {crate_name} not found"))
}

/// Builds a binary with optional features and measures its size.
///
/// # Errors
///
/// * Cargo clean fails
/// * Cargo build fails
/// * Reading binary metadata fails
fn build_and_measure_binary(
    manifest: &Utf8Path,
    target_dir: &Utf8Path,
    binary_name: &str,
    feat: Option<&String>,
) -> Result<u64> {
    let _ = Command::new("cargo")
        .current_dir(manifest.parent().unwrap())
        .arg("clean")
        .arg("--release")
        .status();

    let mut cmd = Command::new("cargo");

    cmd.current_dir(manifest.parent().unwrap())
        .arg("build")
        .arg("--release")
        .arg("--no-default-features")
        .arg("--bin")
        .arg(binary_name);

    if let Some(f) = feat {
        cmd.arg("--features").arg(f);
    }

    println!("$ {cmd:?}\n");
    cmd.status().context("building binary")?;

    let binary_path = target_dir.join("release").join(binary_name);
    Ok(fs::metadata(&binary_path)?.len())
}

/// Checks if a cargo tool is installed and available.
#[must_use]
fn tool_available(tool: &str) -> bool {
    Command::new("cargo")
        .arg(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_splits_comma_separated_packages() {
        // Mock the parse to test the comma splitting logic directly
        let mut args = Args {
            package: vec!["pkg1,pkg2,pkg3".to_string()],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        // Apply the comma-splitting logic
        args.package = args
            .package
            .into_iter()
            .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
            .collect();

        assert_eq!(args.package.len(), 3);
        assert_eq!(args.package[0], "pkg1");
        assert_eq!(args.package[1], "pkg2");
        assert_eq!(args.package[2], "pkg3");
    }

    #[test]
    fn test_parse_args_splits_comma_separated_skip_packages() {
        let mut args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec!["skip1,skip2".to_string()],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        args.skip_packages = args
            .skip_packages
            .into_iter()
            .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
            .collect();

        assert_eq!(args.skip_packages.len(), 2);
        assert_eq!(args.skip_packages[0], "skip1");
        assert_eq!(args.skip_packages[1], "skip2");
    }

    #[test]
    fn test_parse_args_splits_comma_separated_skip_features() {
        let mut args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec!["feature1,feature2,feature3".to_string()],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        args.skip_features = args
            .skip_features
            .into_iter()
            .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
            .collect();

        assert_eq!(args.skip_features.len(), 3);
        assert_eq!(args.skip_features[0], "feature1");
        assert_eq!(args.skip_features[1], "feature2");
        assert_eq!(args.skip_features[2], "feature3");
    }

    #[test]
    fn test_parse_args_splits_comma_separated_tools() {
        let mut args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec!["bloat,llvm-lines,size".to_string()],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        args.tool = args
            .tool
            .into_iter()
            .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
            .collect();

        assert_eq!(args.tool.len(), 3);
        assert_eq!(args.tool[0], "bloat");
        assert_eq!(args.tool[1], "llvm-lines");
        assert_eq!(args.tool[2], "size");
    }

    #[test]
    fn test_parse_args_splits_comma_separated_output_formats() {
        let mut args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["text,json,jsonl".to_string()],
        };

        args.output_format = args
            .output_format
            .into_iter()
            .flat_map(|x| x.split(',').map(ToString::to_string).collect::<Vec<_>>())
            .collect();

        assert_eq!(args.output_format.len(), 3);
        assert_eq!(args.output_format[0], "text");
        assert_eq!(args.output_format[1], "json");
        assert_eq!(args.output_format[2], "jsonl");
    }

    #[test]
    fn test_should_skip_feature_returns_true_for_exact_match() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec!["fail-on-warnings".to_string()],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_skip_feature("fail-on-warnings", &args).unwrap();
        assert!(result);
    }

    #[test]
    fn test_should_skip_feature_returns_false_for_no_match() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec!["fail-on-warnings".to_string()],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_skip_feature("some-feature", &args).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_should_skip_feature_with_pattern_match() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: Some("^test-.*".to_string()),
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_skip_feature("test-feature", &args).unwrap();
        assert!(result);

        let result = should_skip_feature("prod-feature", &args).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_should_skip_feature_pattern_takes_priority() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec!["exact-feature".to_string()],
            skip_feature_pattern: Some("pattern-.*".to_string()),
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        // Exact match should be caught first
        let result = should_skip_feature("exact-feature", &args).unwrap();
        assert!(result);

        // Pattern match should also work
        let result = should_skip_feature("pattern-feature", &args).unwrap();
        assert!(result);
    }

    #[test]
    fn test_should_skip_feature_invalid_regex_returns_error() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: Some("[invalid regex".to_string()),
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_skip_feature("any-feature", &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_should_analyze_package_returns_true_when_no_filters() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_analyze_package("any-package", &args).unwrap();
        assert!(result);
    }

    #[test]
    fn test_should_analyze_package_with_specific_packages() {
        let args = Args {
            package: vec!["pkg1".to_string(), "pkg2".to_string()],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_analyze_package("pkg1", &args).unwrap();
        assert!(result);

        let result = should_analyze_package("pkg3", &args).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_should_analyze_package_with_package_pattern() {
        let args = Args {
            package: vec![],
            package_pattern: Some("^moosicbox_.*".to_string()),
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_analyze_package("moosicbox_core", &args).unwrap();
        assert!(result);

        let result = should_analyze_package("other_package", &args).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_should_analyze_package_skip_packages() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec!["skip_this".to_string()],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_analyze_package("skip_this", &args).unwrap();
        assert!(!result);

        let result = should_analyze_package("analyze_this", &args).unwrap();
        assert!(result);
    }

    #[test]
    fn test_should_analyze_package_skip_package_pattern() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: Some("^test_.*".to_string()),
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_analyze_package("test_package", &args).unwrap();
        assert!(!result);

        let result = should_analyze_package("prod_package", &args).unwrap();
        assert!(result);
    }

    #[test]
    fn test_should_analyze_package_combined_filters() {
        let args = Args {
            package: vec!["moosicbox_core".to_string()],
            package_pattern: None,
            skip_packages: vec!["moosicbox_test".to_string()],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        // In the list
        let result = should_analyze_package("moosicbox_core", &args).unwrap();
        assert!(result);

        // Not in the list
        let result = should_analyze_package("moosicbox_other", &args).unwrap();
        assert!(!result);

        // In skip list (even if hypothetically in package list)
        let result = should_analyze_package("moosicbox_test", &args).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_should_analyze_package_invalid_package_pattern_returns_error() {
        let args = Args {
            package: vec![],
            package_pattern: Some("[invalid regex".to_string()),
            skip_packages: vec![],
            skip_package_pattern: None,
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_analyze_package("any-package", &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_should_analyze_package_invalid_skip_pattern_returns_error() {
        let args = Args {
            package: vec![],
            package_pattern: None,
            skip_packages: vec![],
            skip_package_pattern: Some("[invalid regex".to_string()),
            skip_features: vec![],
            skip_feature_pattern: None,
            tool: vec![],
            report_file: None,
            output_format: vec!["all".to_string()],
        };

        let result = should_analyze_package("any-package", &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_available_returns_false_for_nonexistent_tool() {
        // Test that the function correctly identifies when a tool is not available
        let result = tool_available("nonexistent-tool-xyz-123");
        assert!(!result);
    }

    #[test]
    fn test_tool_available_handles_various_invalid_inputs() {
        // Test various invalid tool names to ensure the function handles them gracefully
        assert!(!tool_available("definitely-not-a-real-cargo-subcommand-xyz"));
        assert!(!tool_available("invalid-tool-name-with-many-hyphens-123456"));
        assert!(!tool_available(""));
    }
}
