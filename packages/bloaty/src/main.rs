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
    time::{SystemTime, UNIX_EPOCH},
};

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

    #[arg(short, long, value_parser = ["bloat", "llvm-lines", "size"], default_values = &["bloat", "size"], value_name = "TOOL")]
    tool: Vec<String>,

    #[arg(long, value_name = "REPORT_FILE")]
    report_file: Option<String>,

    #[arg(long, value_parser = ["text", "json", "jsonl", "all"], default_value = "all", value_name = "FORMAT")]
    output_format: Vec<String>,
}

struct ReportFiles {
    text: Option<fs::File>,
    jsonl: Option<fs::File>,
}

struct AnalysisContext {
    timestamp: u64,
    base_filename: String,
    report_files: ReportFiles,
    json_report: serde_json::Value,
}

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

fn setup_report_files(args: &Args) -> Result<AnalysisContext> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

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
                "diff_formatted": format!("{}{}", sign, ByteSize(diff.unsigned_abs())),
                "size_formatted": ByteSize(size).to_string(),
                "timestamp": ctx.timestamp
            }))?
        )?;
    }
    Ok(())
}

fn write_text_package_header(ctx: &mut AnalysisContext, package_name: &str) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        writeln!(report, "\nPackage: {package_name}")?;
        writeln!(report, "===================")?;
    }
    Ok(())
}

fn write_text_target_header(ctx: &mut AnalysisContext, target_name: &str) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        writeln!(report, "\nTarget: {target_name}")?;
        writeln!(report, "-------------------")?;
    }
    Ok(())
}

fn write_text_base_size(ctx: &mut AnalysisContext, base_size: u64) -> Result<()> {
    if let Some(report) = &mut ctx.report_files.text {
        writeln!(report, "Base size: {}", ByteSize(base_size))?;
    }
    Ok(())
}

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
        "features": []
    });

    write_text_target_header(ctx, &target.name)?;
    write_jsonl_target_start(ctx, &pkg.name, &target.name)?;

    let base_size = build_and_measure_rlib(
        &pkg.manifest_path,
        &metadata.target_directory,
        &pkg.name,
        None,
    )?;
    println!("  base: {}", ByteSize(base_size));
    write_text_base_size(ctx, base_size)?;
    write_jsonl_base_size(ctx, &pkg.name, &target.name, base_size)?;

    target_json["base_size"] = json!(base_size);

    for feat in available_features {
        if should_skip_feature(feat, args)? {
            continue;
        }

        let size = build_and_measure_rlib(
            &pkg.manifest_path,
            &metadata.target_directory,
            &pkg.name,
            Some(feat),
        )?;

        #[allow(clippy::cast_possible_wrap)]
        let diff = size as i64 - base_size as i64;

        println!(
            "  feature {:<15}: {} ({}{})",
            feat,
            ByteSize(size),
            if diff >= 0 { '+' } else { '-' },
            ByteSize(diff.unsigned_abs()),
        );

        write_text_feature(ctx, feat, size, diff)?;
        write_jsonl_feature(ctx, &pkg.name, &target.name, feat, size, diff)?;

        target_json["features"].as_array_mut().unwrap().push(json!({
            "name": feat,
            "size": size,
            "diff": diff,
            "diff_formatted": format!("{}{}", if diff >= 0 { '+' } else { '-' }, ByteSize(diff.unsigned_abs())),
            "size_formatted": ByteSize(size).to_string()
        }));
    }

    write_jsonl_target_end(ctx, &pkg.name, &target.name)?;
    Ok(target_json)
}

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

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn main() -> Result<()> {
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
        if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
            if fname.starts_with(&prefix) {
                return Ok(fs::metadata(&path)?.len());
            }
        }
    }
    Err(anyhow::anyhow!("rlib for {} not found", crate_name))
}

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
