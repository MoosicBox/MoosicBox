#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use anyhow::{Context, Result};
use bytesize::ByteSize;
use cargo_metadata::{MetadataCommand, TargetKind, camino::Utf8Path};
use clap::Parser;
use glob::glob;
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

    #[arg(long, value_name = "SKIP_PACKAGES")]
    skip_packages: Vec<String>,

    #[arg(long, value_name = "SKIP_FEATURES")]
    skip_features: Vec<String>,

    #[arg(short, long, value_parser = ["bloat", "llvm-lines", "size"], default_values = &["bloat", "size"], value_name = "TOOL")]
    tool: Vec<String>,

    #[arg(long, value_name = "REPORT_FILE")]
    report_file: Option<String>,

    #[arg(long, value_parser = ["text", "json", "both"], default_value = "both", value_name = "FORMAT")]
    output_format: String,
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
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

    let args = args;

    let mut any_unavailable = false;

    for tool in &args.tool {
        if !tool_available(tool) {
            eprintln!("[error] cargo {tool} not found; install cargo-{tool}");
            any_unavailable = true;
        }
    }

    if any_unavailable {
        std::process::exit(1);
    }

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let base_filename = args
        .report_file
        .clone()
        .map_or_else(|| format!("bloaty_report_{timestamp}"), |path| path);

    let mut text_report_file = if args.output_format == "text" || args.output_format == "both" {
        Some(fs::File::create(format!("{base_filename}.txt"))?)
    } else {
        None
    };

    if let Some(report) = &mut text_report_file {
        writeln!(report, "Bloaty Analysis Report")?;
        writeln!(report, "===================\n")?;
    }

    let mut json_report = json!({
        "timestamp": timestamp,
        "packages": []
    });

    let metadata = MetadataCommand::new().no_deps().exec()?;

    for pkg in metadata
        .packages
        .into_iter()
        .filter(|p| metadata.workspace_members.contains(&p.id))
    {
        if !args.package.is_empty() && !args.package.contains(&pkg.name)
            || args.skip_packages.contains(&pkg.name)
        {
            continue;
        }

        println!("\n=== Analyzing package: {} ===", pkg.name);
        if let Some(report) = &mut text_report_file {
            writeln!(report, "\nPackage: {}", pkg.name)?;
            writeln!(report, "===================")?;
        }

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
                println!("\nrlib target: {}", target.name);
                if let Some(report) = &mut text_report_file {
                    writeln!(report, "\nTarget: {}", target.name)?;
                    writeln!(report, "-------------------")?;
                }

                let mut target_json = json!({
                    "name": target.name,
                    "base_size": 0,
                    "features": []
                });

                let base_size = build_and_measure_rlib(
                    &pkg.manifest_path,
                    &metadata.target_directory,
                    &pkg.name,
                    None,
                )?;
                println!("  base: {}", ByteSize(base_size));
                if let Some(report) = &mut text_report_file {
                    writeln!(report, "Base size: {}", ByteSize(base_size))?;
                }

                target_json["base_size"] = json!(base_size);

                for feat in &available_features {
                    if args.skip_features.contains(feat) {
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
                    let sign = if diff >= 0 { '+' } else { '-' };

                    println!(
                        "  feature {:<15}: {} ({}{})",
                        feat,
                        ByteSize(size),
                        sign,
                        ByteSize(diff.unsigned_abs()),
                    );

                    if let Some(report) = &mut text_report_file {
                        writeln!(
                            report,
                            "Feature: {:<15} | Size: {} | Diff: {}{}",
                            feat,
                            ByteSize(size),
                            sign,
                            ByteSize(diff.unsigned_abs())
                        )?;
                    }

                    target_json["features"].as_array_mut().unwrap().push(json!({
                        "name": feat,
                        "size": size,
                        "diff": diff,
                        "diff_formatted": format!("{}{}", sign, ByteSize(diff.unsigned_abs())),
                        "size_formatted": ByteSize(size).to_string()
                    }));
                }

                package_json["targets"]
                    .as_array_mut()
                    .unwrap()
                    .push(target_json);
            }
        }

        json_report["packages"]
            .as_array_mut()
            .unwrap()
            .push(package_json);
    }

    if args.output_format == "json" || args.output_format == "both" {
        let mut json_file = fs::File::create(format!("{base_filename}.json"))?;
        writeln!(json_file, "{}", serde_json::to_string_pretty(&json_report)?)?;
    }

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
