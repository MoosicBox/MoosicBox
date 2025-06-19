#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::{Parser, Subcommand, ValueEnum};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use strum::{EnumDiscriminants, EnumIter, IntoDiscriminant};
use toml::Value;

#[cfg(feature = "git-diff")]
mod git_diff;

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab_case")]
pub enum OutputType {
    Json,
    Raw,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter))]
#[strum_discriminants(name(CommandType))]
enum Commands {
    Dependencies {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        #[arg(long)]
        features: Option<String>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
    Environment {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        #[arg(long)]
        features: Option<String>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
    CiSteps {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        #[arg(long)]
        features: Option<String>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
    Features {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        #[arg(long)]
        offset: Option<u16>,

        #[arg(long)]
        max: Option<u16>,

        #[arg(long)]
        max_parallel: Option<u16>,

        #[arg(long)]
        chunked: Option<u16>,

        #[arg(short, long)]
        spread: bool,

        #[arg(long)]
        features: Option<String>,

        #[arg(long)]
        skip_features: Option<String>,

        #[arg(long)]
        required_features: Option<String>,

        /// List of changed files (paths relative to workspace root) - only include affected packages
        #[arg(long, value_delimiter = ',')]
        changed_files: Option<Vec<String>>,

        /// Git base commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,

        /// Git head commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
    WorkspaceDeps {
        /// Path to the workspace root
        workspace_root: PathBuf,
        /// Name of the target package
        package: String,
        /// Features to enable (optional)
        #[arg(long)]
        features: Option<Vec<String>>,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
        /// Include all potential workspace dependencies, regardless of feature activation
        #[arg(long)]
        all_potential_deps: bool,
    },
    GenerateDockerfile {
        /// Path to the workspace root
        workspace_root: PathBuf,
        /// Name of the target package to build
        package: String,
        /// Features to enable for the target package (optional)
        #[arg(long)]
        features: Option<Vec<String>>,
        /// Output path for the generated Dockerfile
        #[arg(long)]
        output: PathBuf,
        /// Docker base image for the builder stage
        #[arg(long, default_value = "rust:1-bookworm")]
        base_image: String,
        /// Docker base image for the final stage
        #[arg(long, default_value = "debian:bookworm-slim")]
        final_image: String,
        /// Port to expose in the container
        #[arg(long)]
        port: Option<u16>,
        /// Build arguments to pass to cargo build
        #[arg(long)]
        build_args: Option<String>,
        /// Generate dockerignore file alongside Dockerfile
        #[arg(long, default_value = "true")]
        generate_dockerignore: bool,
    },
    AffectedPackages {
        /// Path to the workspace root
        workspace_root: PathBuf,
        /// List of changed files (paths relative to workspace root)
        #[arg(long, value_delimiter = ',')]
        changed_files: Vec<String>,
        /// Package to check if affected (optional - if not provided, returns all affected packages)
        #[arg(long)]
        target_package: Option<String>,
        /// Git base commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,
        /// Git head commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,
        /// Output format
        #[arg(long, value_enum, default_value_t=OutputType::Json)]
        output: OutputType,
    },
}

#[derive(Debug, Serialize)]
struct PackageInfo {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct WorkspaceDepsResult {
    packages: Vec<PackageInfo>,
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None, None).expect("Failed to initialize logging");

    let args = Args::parse();

    let cmd_type = args.cmd.discriminant();

    match args.cmd {
        Commands::Dependencies {
            file,
            os,
            features: specific_features,
            output,
        }
        | Commands::Environment {
            file,
            os,
            features: specific_features,
            output,
        }
        | Commands::CiSteps {
            file,
            os,
            features: specific_features,
            output,
        } => {
            let path = PathBuf::from_str(&file)?;
            let cargo_path = path.join("Cargo.toml");
            log::debug!("Loading file '{}'", cargo_path.display());
            let source = std::fs::read_to_string(&cargo_path)?;
            let value: Value = toml::from_str(&source)?;

            let specific_features =
                specific_features.map(|x| x.split(',').map(str::to_string).collect_vec());

            let packages = if let Some(workspace_members) = value
                .get("workspace")
                .and_then(|x| x.get("members"))
                .and_then(|x| x.as_array())
                .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
            {
                let mut packages = vec![];

                for file in workspace_members {
                    let path = PathBuf::from_str(file)?;

                    packages.extend(process_configs(
                        &path,
                        None,
                        None,
                        None,
                        false,
                        specific_features.as_deref(),
                    )?);
                }

                packages
            } else {
                process_configs(&path, None, None, None, false, specific_features.as_deref())?
            };

            let dependencies = packages
                .iter()
                .filter(|x| {
                    os.as_deref().is_none_or(|os| {
                        x.get("os")
                            .is_some_and(|x| x.as_str().is_some_and(|x| x == os))
                    })
                })
                .filter_map(|x| {
                    x.get(match cmd_type {
                        CommandType::Dependencies => "dependencies",
                        CommandType::Environment => "env",
                        CommandType::CiSteps => "ciSteps",
                        CommandType::Features => unimplemented!(),
                        CommandType::WorkspaceDeps => unimplemented!(),
                        CommandType::GenerateDockerfile => unimplemented!(),
                        CommandType::AffectedPackages => unimplemented!(),
                    })
                    .and_then(|x| x.as_str())
                    .map(ToString::to_string)
                })
                .unique()
                .collect::<Vec<_>>();

            match output {
                OutputType::Json => {
                    println!("{}", serde_json::to_value(dependencies).unwrap());
                }
                OutputType::Raw => {
                    println!("{}", dependencies.join("\n"));
                }
            }
        }
        Commands::Features {
            file,
            os,
            offset,
            max,
            max_parallel,
            mut chunked,
            spread,
            features: specific_features,
            skip_features,
            required_features,
            changed_files,
            #[cfg(feature = "git-diff")]
            git_base,
            #[cfg(feature = "git-diff")]
            git_head,
            output,
        } => {
            let path = PathBuf::from_str(&file)?;
            let cargo_path = path.join("Cargo.toml");
            log::debug!("Loading file '{}'", cargo_path.display());
            let source = std::fs::read_to_string(&cargo_path)?;
            let value: Value = toml::from_str(&source)?;

            let specific_features =
                specific_features.map(|x| x.split(',').map(str::to_string).collect_vec());

            let skip_features =
                skip_features.map(|x| x.split(',').map(str::to_string).collect_vec());

            let required_features =
                required_features.map(|x| x.split(',').map(str::to_string).collect_vec());

            match output {
                OutputType::Json => {
                    let packages = loop {
                        let packages = if let Some(workspace_members) = value
                            .get("workspace")
                            .and_then(|x| x.get("members"))
                            .and_then(|x| x.as_array())
                            .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
                        {
                            let mut packages = vec![];

                            assert!(
                                output != OutputType::Raw,
                                "workspace Cargo.toml is not supported for raw output"
                            );

                            for file in workspace_members {
                                let path = PathBuf::from_str(file)?;

                                packages.extend(process_configs(
                                    &path,
                                    offset,
                                    max,
                                    chunked,
                                    spread,
                                    specific_features.as_deref(),
                                )?);
                            }

                            packages
                        } else {
                            process_configs(
                                &path,
                                offset,
                                max,
                                chunked,
                                spread,
                                specific_features.as_deref(),
                            )?
                        };

                        let packages = if let Some(os) = os.as_deref() {
                            packages
                                .into_iter()
                                .filter(|x| {
                                    x.get("os")
                                        .is_some_and(|x| x.as_str().is_some_and(|x| x == os))
                                })
                                .collect()
                        } else {
                            packages
                        };

                        // Filter by affected packages if changed_files is provided
                        let packages =
                            if let Some(changed_files) = &changed_files {
                                let affected_packages = {
                                    #[cfg(feature = "git-diff")]
                                    {
                                        // Check if git parameters are provided for external dependency analysis
                                        if let (Some(base), Some(head)) = (&git_base, &git_head) {
                                            log::debug!(
                                                "Using git diff analysis for external dependencies"
                                            );

                                            // Extract changed external dependencies from git diff
                                            let changed_external_deps =
                                                git_diff::extract_changed_dependencies_from_git(
                                                    &path,
                                                    base,
                                                    head,
                                                    changed_files,
                                                )?;

                                            // Use the enhanced function with external dependencies
                                            find_affected_packages_with_external_deps(
                                                &path,
                                                changed_files,
                                                Some(&changed_external_deps),
                                            )?
                                        } else {
                                            find_affected_packages(&path, changed_files)?
                                        }
                                    }
                                    #[cfg(not(feature = "git-diff"))]
                                    {
                                        find_affected_packages(&path, changed_files)?
                                    }
                                };

                                packages
                                    .into_iter()
                                    .filter(|pkg| {
                                        pkg.get("name").and_then(|n| n.as_str()).is_some_and(
                                            |name| affected_packages.contains(&name.to_string()),
                                        )
                                    })
                                    .collect()
                            } else {
                                packages
                            };

                        if let (Some(max_parallel), Some(chunked)) = (max_parallel, &mut chunked) {
                            if packages.len() > max_parallel as usize {
                                *chunked += 1;
                                continue;
                            }
                        }

                        break packages;
                    };

                    println!("{}", serde_json::to_value(packages).unwrap());
                }
                OutputType::Raw => {
                    let features = fetch_features(
                        &value,
                        offset,
                        max,
                        specific_features.as_deref(),
                        skip_features.as_deref(),
                        required_features.as_deref(),
                    );
                    assert!(
                        chunked.is_none(),
                        "chunked arg is not supported for raw output"
                    );
                    println!("{}", features.join("\n"));
                }
            }
        }
        Commands::WorkspaceDeps {
            workspace_root,
            package,
            features,
            format,
            all_potential_deps,
        } => {
            let workspace_path = PathBuf::from_str(&workspace_root.to_string_lossy())?;
            let enabled_features = features.map(|f| f.into_iter().collect::<Vec<_>>());
            let workspace_deps = find_workspace_dependencies(
                &workspace_path,
                &package,
                enabled_features.as_deref(),
                all_potential_deps,
            )?;

            match format.as_str() {
                "json" => {
                    let package_infos: Vec<PackageInfo> = workspace_deps
                        .into_iter()
                        .map(|(name, path)| PackageInfo { name, path })
                        .collect();
                    let result = WorkspaceDepsResult {
                        packages: package_infos,
                    };
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                "text" => {
                    for (name, _path) in workspace_deps {
                        println!("{name}");
                    }
                }
                _ => {
                    return Err(format!("Unsupported format: {format}").into());
                }
            }
        }
        Commands::GenerateDockerfile {
            workspace_root,
            package,
            features,
            output,
            base_image,
            final_image,
            port,
            build_args,
            generate_dockerignore,
        } => {
            let workspace_path = PathBuf::from_str(&workspace_root.to_string_lossy())?;
            let enabled_features = features.map(|f| f.into_iter().collect::<Vec<_>>());

            generate_dockerfile(
                &workspace_path,
                &package,
                enabled_features.as_deref(),
                &output,
                &base_image,
                &final_image,
                port,
                build_args.as_deref(),
                generate_dockerignore,
            )?;

            println!("Generated Dockerfile at: {}", output.display());
        }
        Commands::AffectedPackages {
            workspace_root,
            changed_files,
            target_package,
            #[cfg(feature = "git-diff")]
            git_base,
            #[cfg(feature = "git-diff")]
            git_head,
            output,
        } => {
            let workspace_path = PathBuf::from_str(&workspace_root.to_string_lossy())?;

            let affected_packages = {
                #[cfg(feature = "git-diff")]
                {
                    // Check if git parameters are provided for external dependency analysis
                    if let (Some(base), Some(head)) = (&git_base, &git_head) {
                        log::debug!("Using git diff analysis for external dependencies");

                        // Extract changed external dependencies from git diff
                        let changed_external_deps =
                            git_diff::extract_changed_dependencies_from_git(
                                &workspace_path,
                                base,
                                head,
                                &changed_files,
                            )?;

                        // Use the enhanced function with external dependencies
                        find_affected_packages_with_external_deps(
                            &workspace_path,
                            &changed_files,
                            Some(&changed_external_deps),
                        )?
                    } else {
                        find_affected_packages(&workspace_path, &changed_files)?
                    }
                }
                #[cfg(not(feature = "git-diff"))]
                {
                    find_affected_packages(&workspace_path, &changed_files)?
                }
            };

            match target_package {
                Some(package) => {
                    // Check if specific package is affected
                    let is_affected = affected_packages.contains(&package);
                    match output {
                        OutputType::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "package": package,
                                    "affected": is_affected,
                                    "all_affected": affected_packages
                                })
                            );
                        }
                        OutputType::Raw => {
                            println!("{is_affected}");
                        }
                    }
                }
                None => {
                    // Return all affected packages
                    match output {
                        OutputType::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "affected_packages": affected_packages
                                })
                            );
                        }
                        OutputType::Raw => {
                            for package in affected_packages {
                                println!("{package}");
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn find_workspace_dependencies(
    workspace_root: &Path,
    target_package: &str,
    enabled_features: Option<&[String]>,
    all_potential_deps: bool,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    use std::collections::{HashMap, HashSet, VecDeque};

    log::trace!("🔍 Finding workspace dependencies for package: {target_package}");
    if let Some(features) = enabled_features {
        log::trace!("📋 Enabled features: {features:?}");
    } else {
        log::trace!("📋 Using default features");
    }

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    log::trace!(
        "📂 Loading workspace from: {}",
        workspace_cargo_path.display()
    );
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
        .ok_or("No workspace members found")?;

    log::trace!("🏢 Found {} workspace members", workspace_members.len());

    // Create a map of package name -> package path for all workspace members
    let mut package_paths = HashMap::new();
    let mut package_dependencies: HashMap<String, Vec<String>> = HashMap::new();
    // Track packages that should not use default features
    let mut no_default_features: HashSet<String> = HashSet::new();

    for member_path in workspace_members {
        let full_path = workspace_root.join(member_path);
        let cargo_path = full_path.join("Cargo.toml");

        if !cargo_path.exists() {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = std::fs::read_to_string(&cargo_path)?;
        let value: Value = toml::from_str(&source)?;

        // Get package name
        if let Some(package_name) = value
            .get("package")
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
        {
            log::trace!("📦 Package name: {package_name} -> {member_path}");
            package_paths.insert(package_name.to_string(), member_path.to_string());

            // Get features for this package to resolve feature-conditional dependencies
            // For now, use simple feature resolution to avoid infinite loops
            #[allow(clippy::unnecessary_unwrap)]
            let package_features = if package_name == target_package && enabled_features.is_some() {
                // For the target package with explicit features, use those features
                enabled_features.unwrap().iter().cloned().collect()
            } else {
                // For other packages or target without explicit features, use empty set for now
                // This avoids the complex feature resolution that was causing infinite loops
                HashSet::new()
            };
            log::trace!("🎯 Active features for {package_name}: {package_features:?}");

            // Extract dependencies that are workspace members
            let mut deps = Vec::new();

            // Check regular dependencies
            if let Some(dependencies) = value.get("dependencies").and_then(|x| x.as_table()) {
                log::trace!(
                    "🔗 Checking {} regular dependencies for {}",
                    dependencies.len(),
                    package_name
                );
                for (dep_name, dep_value) in dependencies {
                    // Check if this is a workspace dependency and if it's enabled by features
                    if all_potential_deps {
                        // Include all workspace dependencies regardless of feature activation
                        if is_workspace_dependency(dep_value) {
                            log::trace!(
                                "  ✅ Adding workspace dependency (all-potential mode): {dep_name}"
                            );
                            deps.push(dep_name.clone());

                            // Store dependency feature information for potential use
                            no_default_features.insert(dep_name.clone());
                        }
                    } else if is_workspace_dependency_with_features(dep_value) {
                        log::trace!("  ✅ Adding regular dependency: {dep_name}");
                        deps.push(dep_name.clone());

                        // Store dependency feature information
                        let default_features = get_dependency_default_features(dep_value);
                        if default_features == Some(false) {
                            log::trace!(
                                "    🚫 Dependency {dep_name} specified with default-features = false"
                            );
                            no_default_features.insert(dep_name.clone());
                        }
                    } else {
                        log::trace!(
                            "  ⏸️  Skipping regular dependency (not activated): {dep_name}"
                        );
                    }
                }
            }

            // Check dev dependencies
            if let Some(dev_dependencies) = value.get("dev-dependencies").and_then(|x| x.as_table())
            {
                log::trace!(
                    "🔗 Checking {} dev dependencies for {}",
                    dev_dependencies.len(),
                    package_name
                );
                for (dep_name, dep_value) in dev_dependencies {
                    if all_potential_deps {
                        if is_workspace_dependency(dep_value) {
                            log::trace!(
                                "  ✅ Adding dev workspace dependency (all-potential mode): {dep_name}"
                            );
                            deps.push(dep_name.clone());

                            let default_features = get_dependency_default_features(dep_value);
                            if default_features == Some(false) {
                                no_default_features.insert(dep_name.clone());
                            }
                        }
                    } else if is_workspace_dependency_with_features(dep_value) {
                        log::trace!("  ✅ Adding dev dependency: {dep_name}");
                        deps.push(dep_name.clone());

                        let default_features = get_dependency_default_features(dep_value);
                        if default_features == Some(false) {
                            log::trace!(
                                "    🚫 Dev dependency {dep_name} specified with default-features = false"
                            );
                            no_default_features.insert(dep_name.clone());
                        }
                    } else {
                        log::trace!("  ⏸️  Skipping dev dependency (not activated): {dep_name}");
                    }
                }
            }

            // Check build dependencies
            if let Some(build_dependencies) =
                value.get("build-dependencies").and_then(|x| x.as_table())
            {
                log::trace!(
                    "🔗 Checking {} build dependencies for {}",
                    build_dependencies.len(),
                    package_name
                );
                for (dep_name, dep_value) in build_dependencies {
                    if all_potential_deps {
                        if is_workspace_dependency(dep_value) {
                            log::trace!(
                                "  ✅ Adding build workspace dependency (all-potential mode): {dep_name}"
                            );
                            deps.push(dep_name.clone());

                            let default_features = get_dependency_default_features(dep_value);
                            if default_features == Some(false) {
                                no_default_features.insert(dep_name.clone());
                            }
                        }
                    } else if is_workspace_dependency_with_features(dep_value) {
                        log::trace!("  ✅ Adding build dependency: {dep_name}");
                        deps.push(dep_name.clone());

                        let default_features = get_dependency_default_features(dep_value);
                        if default_features == Some(false) {
                            log::trace!(
                                "    🚫 Build dependency {dep_name} specified with default-features = false"
                            );
                            no_default_features.insert(dep_name.clone());
                        }
                    } else {
                        log::trace!("  ⏸️  Skipping build dependency (not activated): {dep_name}");
                    }
                }
            }

            // Check feature-activated dependencies
            let feature_deps = get_feature_dependencies(&value, &package_features);
            log::trace!(
                "🎭 Found {} feature-activated dependencies for {}",
                feature_deps.len(),
                package_name
            );
            for feature_dep in feature_deps {
                if !deps.contains(&feature_dep) {
                    if all_potential_deps {
                        // In all-potential mode, feature deps are already included above
                        log::trace!(
                            "  ⏸️  Skipping feature-activated dependency (already included in all-potential mode): {feature_dep}"
                        );
                    } else {
                        log::trace!("  ✅ Adding feature-activated dependency: {feature_dep}");
                        deps.push(feature_dep);
                    }
                }
            }

            log::trace!("📊 Final dependencies for {package_name}: {deps:?}");
            package_dependencies.insert(package_name.to_string(), deps);
        }
    }

    log::trace!("🚫 Packages that should not use default features: {no_default_features:?}");

    // Second pass: recalculate dependencies for packages that should not use default features
    for package_name in &no_default_features {
        if let Some(package_path) = package_paths.get(package_name) {
            let cargo_path = workspace_root.join(package_path).join("Cargo.toml");
            if cargo_path.exists() {
                log::trace!(
                    "🔄 Recalculating dependencies for {package_name} without default features"
                );
                let source = std::fs::read_to_string(&cargo_path)?;
                let value: Value = toml::from_str(&source)?;

                // Recalculate dependencies
                let mut deps = Vec::new();

                // Check regular dependencies
                if let Some(dependencies) = value.get("dependencies").and_then(|x| x.as_table()) {
                    for (dep_name, dep_value) in dependencies {
                        if is_workspace_dependency_with_features(dep_value) {
                            deps.push(dep_name.clone());
                        }
                    }
                }

                // Check dev dependencies
                if let Some(dev_dependencies) =
                    value.get("dev-dependencies").and_then(|x| x.as_table())
                {
                    for (dep_name, dep_value) in dev_dependencies {
                        if is_workspace_dependency_with_features(dep_value)
                            && !deps.contains(dep_name)
                        {
                            deps.push(dep_name.clone());
                        }
                    }
                }

                // Check build dependencies
                if let Some(build_dependencies) =
                    value.get("build-dependencies").and_then(|x| x.as_table())
                {
                    for (dep_name, dep_value) in build_dependencies {
                        if is_workspace_dependency_with_features(dep_value)
                            && !deps.contains(dep_name)
                        {
                            deps.push(dep_name.clone());
                        }
                    }
                }

                // Do NOT check feature-activated dependencies since we're not using default features

                log::trace!("🔄 Updated dependencies for {package_name} (no defaults): {deps:?}");
                package_dependencies.insert(package_name.clone(), deps);
            }
        }
    }

    // Now perform BFS to find all transitive dependencies
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut result_paths = Vec::new();

    // Start with the target package
    if !package_paths.contains_key(target_package) {
        return Err(format!("Package '{target_package}' not found in workspace").into());
    }

    log::trace!("🚀 Starting BFS from target package: {target_package}");
    queue.push_back(target_package.to_string());

    while let Some(current_package) = queue.pop_front() {
        if visited.contains(&current_package) {
            log::trace!("⏭️  Already visited: {current_package}");
            continue;
        }

        visited.insert(current_package.clone());
        log::trace!("🔄 Processing: {current_package}");

        // Add the package path to results (except for the original target)
        if current_package != target_package {
            if let Some(path) = package_paths.get(&current_package) {
                log::trace!("  ➕ Adding to result: {current_package} -> {path}");
                result_paths.push((current_package.clone(), path.clone()));
            }
        }

        // Add dependencies to queue
        let current_deps = if all_potential_deps {
            // In all-potential mode, dynamically calculate dependencies for each package
            // to ensure we get ALL possible workspace dependencies
            package_paths
                .get(&current_package)
                .and_then(|package_path| {
                    let cargo_path = workspace_root.join(package_path).join("Cargo.toml");
                    if cargo_path.exists() {
                        std::fs::read_to_string(&cargo_path)
                            .ok()
                            .and_then(|source| {
                                toml::from_str::<Value>(&source).ok().map(|value| {
                                    let mut all_deps = Vec::new();

                                    // Check all dependency types with all-potential logic
                                    for dep_section in
                                        ["dependencies", "dev-dependencies", "build-dependencies"]
                                    {
                                        if let Some(dependencies) =
                                            value.get(dep_section).and_then(|x| x.as_table())
                                        {
                                            for (dep_name, dep_value) in dependencies {
                                                if is_workspace_dependency(dep_value)
                                                    && package_paths.contains_key(dep_name)
                                                    && !all_deps.contains(dep_name)
                                                {
                                                    all_deps.push(dep_name.clone());
                                                }
                                            }
                                        }
                                    }

                                    log::trace!(
                                        "  🔗 Found {} all-potential dependencies for {}: {:?}",
                                        all_deps.len(),
                                        current_package,
                                        all_deps
                                    );
                                    all_deps
                                })
                            })
                    } else {
                        None
                    }
                })
        } else {
            // In normal mode, use the pre-calculated dependencies
            package_dependencies.get(&current_package).cloned()
        };

        if let Some(deps) = current_deps {
            log::trace!(
                "  🔗 Found {} dependencies for {}: {:?}",
                deps.len(),
                current_package,
                deps
            );
            for dep in deps {
                if !visited.contains(&dep) && package_paths.contains_key(&dep) {
                    log::trace!("    🔄 Queuing dependency: {dep}");
                    if no_default_features.contains(&dep) {
                        log::trace!("    🚫 Dependency {dep} will not use default features");
                    }
                    queue.push_back(dep.clone());
                } else if visited.contains(&dep) {
                    log::trace!("    ⏭️  Dependency already visited: {dep}");
                } else {
                    log::trace!("    ❓ Dependency not in workspace: {dep}");
                }
            }
        } else {
            log::trace!("  📭 No dependencies found for: {current_package}");
        }
    }

    // Sort for consistent output by package name
    result_paths.sort_by(|a, b| a.0.cmp(&b.0));

    log::trace!("🎉 Final result: {} packages", result_paths.len());
    log::trace!("📋 Package list: {result_paths:?}");

    Ok(result_paths)
}

#[allow(clippy::too_many_arguments)]
fn generate_dockerfile(
    workspace_root: &Path,
    target_package: &str,
    enabled_features: Option<&[String]>,
    output_path: &Path,
    base_image: &str,
    final_image: &str,
    port: Option<u16>,
    build_args: Option<&str>,
    generate_dockerignore: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get all potential dependencies for the target package (needed for Docker build compatibility)
    let mut dependencies =
        find_workspace_dependencies(workspace_root, target_package, enabled_features, true)?;

    // Add the target package itself to the dependencies list if not already present
    let default_target_path = format!(
        "packages/{}",
        target_package
            .strip_prefix("moosicbox_")
            .unwrap_or(target_package)
    );
    if !dependencies.iter().any(|(name, _)| name == target_package) {
        dependencies.push((target_package.to_string(), default_target_path.clone()));
    }

    // Get target package path
    let target_package_path = dependencies
        .iter()
        .find(|(name, _)| name == target_package)
        .map_or_else(|| default_target_path.as_str(), |(_, path)| path.as_str());

    // Create the Dockerfile content
    let dockerfile_content = generate_dockerfile_content(
        &dependencies,
        target_package,
        enabled_features,
        base_image,
        final_image,
        port,
        build_args,
        workspace_root,
        target_package_path,
    )?;

    // Write the Dockerfile
    std::fs::write(output_path, dockerfile_content)?;

    if generate_dockerignore {
        let dockerignore_content =
            generate_dockerignore_content(&dependencies, target_package, enabled_features)?;
        let dockerignore_path = output_path.with_extension("dockerignore");
        std::fs::write(dockerignore_path, dockerignore_content)?;
    }

    Ok(())
}

fn get_binary_name(
    workspace_root: &Path,
    target_package: &str,
    target_package_path: &str,
) -> String {
    let cargo_toml_path = workspace_root.join(target_package_path).join("Cargo.toml");

    if let Ok(content) = std::fs::read_to_string(&cargo_toml_path) {
        if let Ok(value) = toml::from_str::<Value>(&content) {
            // Check if there's a [[bin]] section with a specific name
            if let Some(bins) = value.get("bin").and_then(|x| x.as_array()) {
                if let Some(bin) = bins.first() {
                    if let Some(name) = bin.get("name").and_then(|x| x.as_str()) {
                        return name.to_string();
                    }
                }
            }
        }
    }

    // Fallback to package name with dashes converted to underscores
    target_package.replace('-', "_")
}

#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::cognitive_complexity
)]
fn generate_dockerfile_content(
    dependencies: &[(String, String)],
    target_package: &str,
    enabled_features: Option<&[String]>,
    base_image: &str,
    final_image: &str,
    port: Option<u16>,
    build_args: Option<&str>,
    workspace_root: &Path,
    target_package_path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::fmt::Write as _;

    let mut content = String::new();

    // Builder stage
    writeln!(
        content,
        "# Builder\nFROM {base_image} AS builder\nWORKDIR /app\n"
    )?;

    // APT configuration for faster downloads (early in build for caching)
    writeln!(content, "# APT configuration for faster downloads")?;
    content.push_str(
        "RUN echo 'Acquire::http::Timeout \"10\";' >>/etc/apt/apt.conf.d/httpproxy && \\\n",
    );
    writeln!(
        content,
        "  echo 'Acquire::ftp::Timeout \"10\";' >>/etc/apt/apt.conf.d/httpproxy\n"
    )?;

    // Collect and install system dependencies early for better caching
    let system_deps =
        collect_system_dependencies(workspace_root, dependencies, enabled_features, "ubuntu")?;

    if system_deps.is_empty() {
        // Fallback to basic dependencies if no clippier.toml found
        writeln!(
            content,
            "# Install basic build dependencies (early for better Docker layer caching)\n",
        )?;
        writeln!(content, "RUN apt-get update && apt-get -y install cmake\n")?;
    } else {
        writeln!(
            content,
            "# Install system dependencies (early for better Docker layer caching)"
        )?;
        writeln!(content, "RUN apt-get update && \\")?;

        // Parse and consolidate apt-get install commands
        let mut install_packages = std::collections::HashSet::new();
        let mut custom_commands = Vec::new();

        for dep in &system_deps {
            if dep.contains("apt-get install") {
                // Extract package names from apt-get install commands
                if let Some(packages_part) = dep.split("apt-get install").nth(1) {
                    for package in packages_part.split_whitespace() {
                        if !package.is_empty() && !package.starts_with('-') {
                            install_packages.insert(package.to_string());
                        }
                    }
                }
            } else if !dep.contains("apt-get update") {
                // Keep other custom commands
                custom_commands.push(dep);
            }
        }

        // Install all packages in one command
        if !install_packages.is_empty() {
            let mut packages: Vec<String> = install_packages.into_iter().collect();
            packages.sort();
            writeln!(content, "    apt-get -y install {}", packages.join(" "))?;
            if custom_commands.is_empty() {
                content.push('\n');
            } else {
                writeln!(content, " && \\")?;
            }
        }

        // Add custom commands
        for (i, cmd) in custom_commands.iter().enumerate() {
            if cmd.starts_with("sudo ") {
                // Remove sudo since we're already running as root in Docker
                let cmd_without_sudo = cmd.strip_prefix("sudo ").unwrap_or(cmd);
                writeln!(content, "    {cmd_without_sudo}")?;
            } else {
                writeln!(content, "    {cmd}")?;
            }

            if i < custom_commands.len() - 1 {
                writeln!(content, " && \\")?;
            } else {
                content.push('\n');
            }
        }

        content.push('\n');
    }

    // Copy workspace manifest files
    writeln!(
        content,
        "COPY Cargo.toml Cargo.toml\nCOPY Cargo.lock Cargo.lock\n"
    )?;

    // Generate workspace members list - create a simple list of quoted package names
    let members_list = dependencies
        .iter()
        .map(|(_, path)| format!("\"{path}\""))
        .collect::<Vec<_>>()
        .join(", ");

    // Modify Cargo.toml to include only needed packages using multi-line sed
    writeln!(
        content,
        "RUN sed -e '/^members = \\[/,/^\\]/c\\members = [{members_list}]' Cargo.toml > Cargo2.toml && mv Cargo2.toml Cargo.toml\n"
    )?;

    // Copy Cargo.toml files for all dependencies
    for (_, path) in dependencies {
        writeln!(content, "COPY {path}/Cargo.toml {path}/Cargo.toml")?;
    }
    content.push('\n');

    // Copy build.rs for target package if it exists
    writeln!(content, "# Copy build.rs for target package if it exists")?;

    // Check if build.rs exists for the target package
    let build_rs_path = workspace_root.join(target_package_path).join("build.rs");
    if build_rs_path.exists() {
        writeln!(
            content,
            "COPY {target_package_path}/build.rs {target_package_path}/build.rs"
        )?;
    }
    content.push('\n');

    // Handle special cases for packages with build.rs

    // Create temporary lib file for stubbing
    writeln!(content, "RUN touch temp_lib.rs\n")?;

    // Add lib path to packages for faster dependency builds (exclude target package)
    let packages_pattern = dependencies
        .iter()
        .filter(|(name, _)| name != target_package) // Exclude target package from lib stubbing
        .map(|(_, path)| path.as_str())
        .collect::<Vec<_>>()
        .join("|");

    if !packages_pattern.is_empty() {
        writeln!(
            content,
            "RUN for file in $(\\
    for file in packages/*/Cargo.toml; \\
      do printf \"$file\\n\"; \\
    done | grep -E \"^({packages_pattern})/Cargo.toml$\"); \\
    do printf \"\\n\\n[lib]\\npath=\\\"../../temp_lib.rs\\\"\" >> \"$file\"; \\
  done\n"
        )?;
    }

    // Handle nested packages (models, api, etc.) with correct relative paths (exclude target package)
    writeln!(content, "# Handle nested packages with correct lib paths")?;

    for (name, path) in dependencies {
        if name != target_package {
            // Check if this package has an existing [lib] section
            let cargo_toml_path = workspace_root.join(path).join("Cargo.toml");
            let has_existing_lib = std::fs::read_to_string(&cargo_toml_path)
                .is_ok_and(|toml_content| toml_content.contains("[lib]"));
            let depth = path.matches('/').count();

            if has_existing_lib {
                // Package has existing [lib] section - append path to it
                let relative_path = "../".repeat(depth + 1) + "temp_lib.rs";
                writeln!(
                    content,
                    "RUN if [ -f {path}/Cargo.toml ]; then \\
    sed -i '/^\\[lib\\]/a path = \"{relative_path}\"' \"{path}/Cargo.toml\"; \\
fi"
                )?;
            } else {
                // Package doesn't have [lib] section - add new one
                if depth > 1 {
                    let relative_path = "../".repeat(depth + 1) + "temp_lib.rs";
                    writeln!(
                        content,
                        "RUN printf \"\\n\\n[lib]\\npath=\\\"{relative_path}\\\"\" >> \"{path}/Cargo.toml\""
                    )?;
                }
            }
        }
    }
    content.push('\n');

    // Create main.rs stub for the target package
    writeln!(
        content,
        "RUN mkdir -p {target_package_path}/src && \\\n  echo 'fn main() {{}}' >{target_package_path}/src/main.rs\n"
    )?;

    // Get the actual binary name from Cargo.toml
    let binary_name = get_binary_name(workspace_root, target_package, target_package_path);

    // Environment variables
    writeln!(content, "# Environment setup")?;
    if let Some(build_args) = build_args {
        for arg in build_args.split(',') {
            let arg = arg.trim();
            writeln!(content, "ARG {arg}\nENV {arg}=${{{arg}}}")?;
        }
    }
    writeln!(
        content,
        "ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace\n"
    )?;

    // Initial cargo build (dependencies only)
    let features_arg = match enabled_features {
        Some(features) if !features.is_empty() => {
            format!("--no-default-features --features={}", features.join(","))
        }
        _ => String::new(),
    };

    writeln!(
        content,
        "RUN cargo build --package {target_package} --release {features_arg}\n"
    )?;

    // Copy actual source code
    writeln!(content, "COPY packages packages\n")?;

    // Remove old artifacts and rebuild with real code (if they exist)
    writeln!(content, "RUN rm -f target/release/deps/{binary_name}*")?;
    writeln!(
        content,
        "RUN cargo build --package {target_package} --release {features_arg}\n"
    )?;

    // Final stage
    writeln!(content, "# Final\nFROM {final_image}\n")?;

    // System setup for final image
    writeln!(
        content,
        "RUN echo 'Acquire::http::Timeout \"10\";' >>/etc/apt/apt.conf.d/httpproxy && \\",
    )?;
    writeln!(
        content,
        "  echo 'Acquire::ftp::Timeout \"10\";' >>/etc/apt/apt.conf.d/httpproxy"
    )?;

    // Install runtime dependencies in final image
    let mut runtime_packages = std::collections::HashSet::new();
    runtime_packages.insert("ca-certificates".to_string());
    runtime_packages.insert("curl".to_string());
    runtime_packages.insert("sqlite3".to_string());

    // Add runtime versions of build dependencies
    for dep in &system_deps {
        if dep.contains("libasound2-dev") {
            runtime_packages.insert("libasound2".to_string());
        }
        if dep.contains("libsqlite3-dev") {
            runtime_packages.insert("libsqlite3-0".to_string());
        }
        // Add more runtime dependency mappings as needed
    }

    let mut runtime_packages_vec: Vec<String> = runtime_packages.into_iter().collect();
    runtime_packages_vec.sort();
    writeln!(
        content,
        "RUN apt-get update && apt-get install -y {}",
        runtime_packages_vec.join(" ")
    )?;

    // Copy binary from builder
    writeln!(
        content,
        "COPY --from=builder /app/target/release/{binary_name} /"
    )?;

    // Expose port if specified
    if let Some(port) = port {
        writeln!(content, "EXPOSE {port}")?;
    }

    // Runtime environment
    if let Some(args) = build_args {
        for arg in args.split(',') {
            let arg = arg.trim();
            writeln!(content, "ARG {arg}\nENV {arg}=${{{arg}}}")?;
        }
    }
    writeln!(
        content,
        "ENV RUST_LOG=info,moosicbox=debug,moosicbox_middleware::api_logger=trace"
    )?;
    writeln!(content, "ENV MAX_THREADS=64")?;
    writeln!(content, "ENV ACTIX_WORKERS=32")?;

    // Final command
    if let Some(port) = port {
        writeln!(content, "CMD [\"./{binary_name}\", \"{port}\"]")?;
    } else {
        writeln!(content, "CMD [\"./{binary_name}\"]")?;
    }

    Ok(content)
}

fn generate_dockerignore_content(
    dependencies: &[(String, String)],
    _target_package: &str,
    _enabled_features: Option<&[String]>,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::fmt::Write as _;

    let mut content = String::new();

    // Exclude all packages first
    writeln!(content, "/packages/*\n")?;

    // Include only required packages
    for (_, path) in dependencies {
        writeln!(content, "!/{path}")?;
    }

    content.push('\n');

    Ok(content)
}

fn get_feature_dependencies(cargo_toml: &Value, enabled_features: &HashSet<String>) -> Vec<String> {
    let mut feature_deps = Vec::new();

    if let Some(features_table) = cargo_toml.get("features").and_then(|x| x.as_table()) {
        log::trace!(
            "🎭 Checking feature-activated dependencies for features: {enabled_features:?}"
        );

        for feature in enabled_features {
            if let Some(feature_deps_array) = features_table.get(feature).and_then(|x| x.as_array())
            {
                log::trace!(
                    "  🔍 Checking feature '{feature}' dependencies: {feature_deps_array:?}"
                );
                for dep in feature_deps_array {
                    if let Some(dep_str) = dep.as_str() {
                        // Handle "dep:dependency_name" syntax - this activates optional dependencies
                        if let Some(dep_name) = dep_str.strip_prefix("dep:") {
                            log::trace!("    🔗 Found dep: activation for '{dep_name}'");
                            // Check if this optional dependency is a workspace dependency
                            if let Some(dependencies) =
                                cargo_toml.get("dependencies").and_then(|x| x.as_table())
                            {
                                if let Some(dep_value) = dependencies.get(dep_name) {
                                    if is_workspace_dependency(dep_value) {
                                        log::trace!(
                                            "    ✅ Adding workspace dependency activated by feature '{feature}': {dep_name}"
                                        );
                                        feature_deps.push(dep_name.to_string());
                                    } else {
                                        log::trace!(
                                            "    ❌ Not a workspace dependency: {dep_name}"
                                        );
                                    }
                                } else {
                                    log::trace!(
                                        "    ❓ Dependency not found in dependencies section: {dep_name}"
                                    );
                                }
                            }
                        } else {
                            log::trace!("    🎯 Sub-feature activation: {dep_str}");
                        }
                    }
                }
            }
        }
    }

    log::trace!("🎭 Final feature-activated dependencies: {feature_deps:?}");
    feature_deps
}

fn is_workspace_dependency_with_features(dep_value: &Value) -> bool {
    match dep_value {
        Value::Table(table) => {
            // Must be a workspace dependency
            let is_workspace = table
                .get("workspace")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if !is_workspace {
                return false;
            }

            // Check if it's optional and if so, whether it's activated by features
            let is_optional = table
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if is_optional {
                // Optional dependencies are only included if explicitly activated by a feature
                // This is handled in get_feature_dependencies, so we return false here
                false
            } else {
                // Non-optional workspace dependencies are always included
                true
            }
        }
        _ => false,
    }
}

fn is_workspace_dependency(dep_value: &Value) -> bool {
    match dep_value {
        Value::Table(table) => table.get("workspace") == Some(&Value::Boolean(true)),
        _ => false,
    }
}

fn process_configs(
    path: &Path,
    offset: Option<u16>,
    max: Option<u16>,
    chunked: Option<u16>,
    spread: bool,
    specific_features: Option<&[String]>,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, Box<dyn std::error::Error>> {
    log::debug!("Loading file '{}'", path.display());
    let cargo_path = path.join("Cargo.toml");
    let source = std::fs::read_to_string(cargo_path)?;
    let value: Value = toml::from_str(&source)?;

    let conf_path = path.join("clippier.toml");
    let conf = if conf_path.is_file() {
        let source = std::fs::read_to_string(conf_path)?;
        let value: ClippierConf = toml::from_str(&source)?;
        Some(value)
    } else {
        None
    };

    log::debug!("{} conf={conf:?}", path.display());

    let configs = conf.as_ref().map_or_else(
        || {
            vec![ClippierConfiguration {
                os: "ubuntu".to_string(),
                dependencies: None,
                env: None,
                cargo: None,
                name: None,
                ci_steps: None,
                skip_features: None,
                required_features: None,
                nightly: None,
            }]
        },
        |x| x.config.clone(),
    );

    let mut packages = vec![];

    if let Some(name) = value
        .get("package")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .map(str::to_string)
    {
        for config in configs {
            let features = fetch_features(
                &value,
                offset,
                max,
                specific_features,
                config.skip_features.as_deref(),
                config.required_features.as_deref(),
            );
            let features = process_features(
                features,
                conf.as_ref()
                    .and_then(|x| x.parallelization.as_ref().map(|x| x.chunked))
                    .or(chunked),
                spread,
            );
            match &features {
                FeaturesList::Chunked(x) => {
                    for features in x {
                        packages.push(create_map(
                            conf.as_ref(),
                            &config,
                            path.to_str().unwrap(),
                            &name,
                            config.required_features.as_deref(),
                            features,
                        )?);
                    }
                }
                FeaturesList::NotChunked(x) => {
                    packages.push(create_map(
                        conf.as_ref(),
                        &config,
                        path.to_str().unwrap(),
                        &name,
                        config.required_features.as_deref(),
                        x,
                    )?);
                }
            }
        }
    }

    Ok(packages)
}

#[allow(clippy::too_many_lines)]
fn create_map(
    conf: Option<&ClippierConf>,
    config: &ClippierConfiguration,
    file: &str,
    name: &str,
    required_features: Option<&[String]>,
    features: &[String],
) -> Result<serde_json::Map<String, serde_json::Value>, Box<dyn std::error::Error>> {
    let mut map = serde_json::Map::new();
    map.insert("os".to_string(), serde_json::to_value(&config.os)?);
    map.insert("path".to_string(), serde_json::to_value(file)?);
    map.insert(
        "name".to_string(),
        serde_json::to_value(config.name.as_deref().unwrap_or(name))?,
    );
    map.insert("features".to_string(), features.into());
    map.insert("requiredFeatures".to_string(), required_features.into());
    map.insert(
        "nightly".to_string(),
        config
            .nightly
            .or_else(|| conf.as_ref().and_then(|x| x.nightly))
            .unwrap_or_default()
            .into(),
    );

    if let Some(dependencies) = &config.dependencies {
        let matches = dependencies
            .iter()
            .filter(|x| {
                let target_features = match x {
                    DependencyFilteredByFeatures::Command { features, .. }
                    | DependencyFilteredByFeatures::Toolchain { features, .. } => features,
                };

                target_features.as_ref().is_none_or(|f| {
                    f.iter()
                        .any(|required| features.iter().any(|x| x == required))
                })
            })
            .collect::<Vec<_>>();

        if !matches.is_empty() {
            let dependencies = matches
                .iter()
                .filter_map(|x| match x {
                    DependencyFilteredByFeatures::Command { command, .. } => Some(command),
                    DependencyFilteredByFeatures::Toolchain { .. } => None,
                })
                .map(String::as_str)
                .collect::<Vec<_>>();

            if !dependencies.is_empty() {
                map.insert(
                    "dependencies".to_string(),
                    serde_json::to_value(dependencies.join("\n"))?,
                );
            }

            let toolchains = matches
                .iter()
                .filter_map(|x| match x {
                    DependencyFilteredByFeatures::Toolchain { toolchain, .. } => Some(toolchain),
                    DependencyFilteredByFeatures::Command { .. } => None,
                })
                .map(String::as_str)
                .collect::<Vec<_>>();

            if !toolchains.is_empty() {
                map.insert(
                    "toolchains".to_string(),
                    serde_json::to_value(toolchains.join("\n"))?,
                );
            }
        }
    }

    let mut env = conf
        .and_then(|x| x.env.as_ref())
        .cloned()
        .unwrap_or_default();
    env.extend(config.env.clone().unwrap_or_default());

    let matches = env
        .iter()
        .filter(|(_k, v)| match v {
            ClippierEnv::Value(..) => true,
            ClippierEnv::FilteredValue { features: f, .. } => f.as_ref().is_none_or(|f| {
                f.iter()
                    .any(|required| features.iter().any(|x| x == required))
            }),
        })
        .map(|(k, v)| {
            (
                k,
                match v {
                    ClippierEnv::Value(value) | ClippierEnv::FilteredValue { value, .. } => value,
                },
            )
        })
        .collect::<Vec<_>>();

    if !matches.is_empty() {
        map.insert(
            "env".to_string(),
            serde_json::to_value(
                matches
                    .iter()
                    .map(|(k, v)| serde_json::to_value(v).map(|v| format!("{k}={v}")))
                    .collect::<Result<Vec<_>, _>>()?
                    .join("\n"),
            )?,
        );
    }

    let mut cargo: Vec<_> = conf
        .and_then(|x| x.cargo.as_ref())
        .cloned()
        .unwrap_or_default()
        .into();
    let config_cargo: Vec<_> = config.cargo.clone().unwrap_or_default().into();
    cargo.extend(config_cargo);

    if !cargo.is_empty() {
        map.insert("cargo".to_string(), serde_json::to_value(cargo.join(" "))?);
    }

    let mut ci_steps: Vec<_> = conf
        .and_then(|x| x.ci_steps.as_ref())
        .cloned()
        .unwrap_or_default()
        .into();
    let config_ci_steps: Vec<_> = config.ci_steps.clone().unwrap_or_default().into();
    ci_steps.extend(config_ci_steps);

    let ci_steps = ci_steps
        .iter()
        .filter(|x| {
            let target_features = match x {
                DependencyFilteredByFeatures::Command { features, .. }
                | DependencyFilteredByFeatures::Toolchain { features, .. } => features,
            };

            target_features.as_ref().is_none_or(|f| {
                f.iter()
                    .any(|required| features.iter().any(|x| x == required))
            })
        })
        .collect::<Vec<_>>();

    if !ci_steps.is_empty() {
        let dependencies = ci_steps
            .iter()
            .filter_map(|x| match x {
                DependencyFilteredByFeatures::Command { command, .. } => Some(command),
                DependencyFilteredByFeatures::Toolchain { .. } => None,
            })
            .map(String::as_str)
            .collect::<Vec<_>>();

        if !dependencies.is_empty() {
            map.insert(
                "ciSteps".to_string(),
                serde_json::to_value(dependencies.join("\n"))?,
            );
        }

        let toolchains = ci_steps
            .iter()
            .filter_map(|x| match x {
                DependencyFilteredByFeatures::Toolchain { toolchain, .. } => Some(toolchain),
                DependencyFilteredByFeatures::Command { .. } => None,
            })
            .map(String::as_str)
            .collect::<Vec<_>>();

        if !toolchains.is_empty() {
            map.insert(
                "ciToolchains".to_string(),
                serde_json::to_value(toolchains.join("\n"))?,
            );
        }
    }

    Ok(map)
}

enum FeaturesList {
    Chunked(Vec<Vec<String>>),
    NotChunked(Vec<String>),
}

impl TryFrom<FeaturesList> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: FeaturesList) -> Result<Self, Self::Error> {
        Ok(match value {
            FeaturesList::Chunked(x) => serde_json::to_value(x)?,
            FeaturesList::NotChunked(x) => serde_json::to_value(x)?,
        })
    }
}

fn process_features(features: Vec<String>, chunked: Option<u16>, spread: bool) -> FeaturesList {
    if let Some(chunked) = chunked {
        let count = features.len();

        FeaturesList::Chunked(if count <= chunked as usize {
            vec![features]
        } else if spread && count > 1 {
            split(&features, chunked as usize)
                .map(<[String]>::to_vec)
                .collect::<Vec<_>>()
        } else {
            features
                .into_iter()
                .chunks(chunked as usize)
                .into_iter()
                .map(Iterator::collect)
                .collect::<Vec<_>>()
        })
    } else {
        FeaturesList::NotChunked(features)
    }
}

fn fetch_features(
    value: &Value,
    offset: Option<u16>,
    max: Option<u16>,
    specific_features: Option<&[String]>,
    skip_features: Option<&[String]>,
    required_features: Option<&[String]>,
) -> Vec<String> {
    value.get("features").map_or_else(Vec::new, |features| {
        features.as_table().map_or_else(Vec::new, |features| {
            let offset = offset.unwrap_or_default().into();
            let feature_count = features.keys().len() - offset;
            features
                .keys()
                .filter(|x| !x.starts_with('_'))
                .filter(|x| specific_features.as_ref().is_none_or(|s| s.contains(x)))
                .filter(|x| skip_features.as_ref().is_none_or(|s| !s.contains(x)))
                .filter(|x| required_features.as_ref().is_none_or(|s| !s.contains(x)))
                .skip(offset)
                .take(max.map_or(feature_count, |x| std::cmp::min(feature_count, x as usize)))
                .cloned()
                .collect::<Vec<_>>()
        })
    })
}

pub fn split<T>(slice: &[T], n: usize) -> impl Iterator<Item = &[T]> {
    let len = slice.len() / n;
    let rem = slice.len() % n;
    let len = if rem != 0 { len + 1 } else { len };
    let len = slice.len() / len;
    let rem = slice.len() % len;
    Split { slice, len, rem }
}

struct Split<'a, T> {
    slice: &'a [T],
    len: usize,
    rem: usize,
}

impl<'a, T> Iterator for Split<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.is_empty() {
            return None;
        }
        let mut len = self.len;
        if self.rem > 0 {
            len += 1;
            self.rem -= 1;
        }
        let (chunk, rest) = self.slice.split_at(len);
        self.slice = rest;
        Some(chunk)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DependencyFilteredByFeatures {
    Command {
        command: String,
        features: Option<Vec<String>>,
    },
    Toolchain {
        toolchain: String,
        features: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ClippierEnv {
    Value(String),
    FilteredValue {
        value: String,
        features: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VecOrItem<T> {
    Value(T),
    Values(Vec<T>),
}

impl<T> From<VecOrItem<T>> for Vec<T> {
    fn from(value: VecOrItem<T>) -> Self {
        match value {
            VecOrItem::Value(x) => vec![x],
            VecOrItem::Values(x) => x,
        }
    }
}

impl<T> Default for VecOrItem<T> {
    fn default() -> Self {
        Self::Values(vec![])
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ClippierConfiguration {
    ci_steps: Option<VecOrItem<DependencyFilteredByFeatures>>,
    cargo: Option<VecOrItem<String>>,
    env: Option<HashMap<String, ClippierEnv>>,
    dependencies: Option<Vec<DependencyFilteredByFeatures>>,
    os: String,
    skip_features: Option<Vec<String>>,
    required_features: Option<Vec<String>>,
    name: Option<String>,
    nightly: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ParallelizationConfig {
    chunked: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ClippierConf {
    ci_steps: Option<VecOrItem<DependencyFilteredByFeatures>>,
    cargo: Option<VecOrItem<String>>,
    config: Vec<ClippierConfiguration>,
    env: Option<HashMap<String, ClippierEnv>>,
    parallelization: Option<ParallelizationConfig>,
    nightly: Option<bool>,
}

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn find_affected_packages(
    workspace_root: &Path,
    changed_files: &[String],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    #[cfg(feature = "git-diff")]
    {
        find_affected_packages_with_external_deps(workspace_root, changed_files, None)
    }
    #[cfg(not(feature = "git-diff"))]
    {
        find_affected_packages_basic(workspace_root, changed_files)
    }
}

#[cfg(not(feature = "git-diff"))]
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn find_affected_packages_basic(
    workspace_root: &Path,
    changed_files: &[String],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    use std::collections::{HashMap, HashSet, VecDeque};

    log::trace!("🔍 Finding affected packages for changed files: {changed_files:?}");

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
        .ok_or("No workspace members found")?;

    log::trace!("🏢 Found {} workspace members", workspace_members.len());

    // Create a map of package name -> package path and package_path -> package name
    let mut package_name_to_path = HashMap::new();
    let mut package_path_to_name = HashMap::new();
    let mut package_dependencies: HashMap<String, Vec<String>> = HashMap::new();

    for member_path in workspace_members {
        let full_path = workspace_root.join(member_path);
        let cargo_path = full_path.join("Cargo.toml");

        if !cargo_path.exists() {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = std::fs::read_to_string(&cargo_path)?;
        let value: Value = toml::from_str(&source)?;

        // Get package name
        if let Some(package_name) = value
            .get("package")
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
        {
            log::trace!("📦 Package name: {package_name} -> {member_path}");
            package_name_to_path.insert(package_name.to_string(), member_path.to_string());
            package_path_to_name.insert(member_path.to_string(), package_name.to_string());

            // Extract dependencies that are workspace members
            let mut deps = Vec::new();

            // Check regular dependencies
            if let Some(dependencies) = value.get("dependencies").and_then(|x| x.as_table()) {
                for (dep_name, dep_value) in dependencies {
                    if is_workspace_dependency(dep_value) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            // Check dev dependencies
            if let Some(dev_dependencies) = value.get("dev-dependencies").and_then(|x| x.as_table())
            {
                for (dep_name, dep_value) in dev_dependencies {
                    if is_workspace_dependency(dep_value) && !deps.contains(dep_name) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            // Check build dependencies
            if let Some(build_dependencies) =
                value.get("build-dependencies").and_then(|x| x.as_table())
            {
                for (dep_name, dep_value) in build_dependencies {
                    if is_workspace_dependency(dep_value) && !deps.contains(dep_name) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            log::trace!("📊 Dependencies for {package_name}: {deps:?}");
            package_dependencies.insert(package_name.to_string(), deps);
        }
    }

    // Find packages directly affected by changed files
    let mut directly_affected_packages = HashSet::new();

    for changed_file in changed_files {
        let changed_path = PathBuf::from(changed_file);

        // Check if the changed file belongs to a workspace package
        for (package_path, package_name) in &package_path_to_name {
            let package_path_buf = PathBuf::from(package_path);

            // Check if the changed file is within this package's directory
            if changed_path.starts_with(&package_path_buf) {
                log::trace!("📝 File {changed_file} affects package {package_name}");
                directly_affected_packages.insert(package_name.clone());
            }
        }
    }

    log::trace!("🎯 Directly affected packages: {directly_affected_packages:?}");

    // Now find all packages that depend on the directly affected packages (transitive dependencies)
    let mut all_affected_packages = directly_affected_packages.clone();
    let mut queue = VecDeque::new();

    // Add all directly affected packages to the queue
    for package in &directly_affected_packages {
        queue.push_back(package.clone());
    }

    // Build reverse dependency map (package -> packages that depend on it)
    let mut reverse_deps: HashMap<String, Vec<String>> = HashMap::new();
    for (package, deps) in &package_dependencies {
        for dep in deps {
            reverse_deps
                .entry(dep.clone())
                .or_default()
                .push(package.clone());
        }
    }

    // Process the queue to find all transitive dependents
    while let Some(current_package) = queue.pop_front() {
        if let Some(dependents) = reverse_deps.get(&current_package) {
            for dependent in dependents {
                if !all_affected_packages.contains(dependent) {
                    log::trace!(
                        "🔄 Package {dependent} depends on affected package {current_package}"
                    );
                    all_affected_packages.insert(dependent.clone());
                    queue.push_back(dependent.clone());
                }
            }
        }
    }

    let mut result: Vec<String> = all_affected_packages.into_iter().collect();
    result.sort();

    log::trace!("🏁 Final affected packages: {result:?}");

    Ok(result)
}

#[cfg(feature = "git-diff")]
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn find_affected_packages_with_external_deps(
    workspace_root: &Path,
    changed_files: &[String],
    external_deps: Option<&[String]>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    use std::collections::{HashMap, HashSet, VecDeque};

    log::trace!("🔍 Finding affected packages for changed files: {changed_files:?}");

    // First, load the workspace and get all members
    let workspace_cargo_path = workspace_root.join("Cargo.toml");
    let workspace_source = std::fs::read_to_string(&workspace_cargo_path)?;
    let workspace_value: Value = toml::from_str(&workspace_source)?;

    let workspace_members = workspace_value
        .get("workspace")
        .and_then(|x| x.get("members"))
        .and_then(|x| x.as_array())
        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
        .ok_or("No workspace members found")?;

    log::trace!("🏢 Found {} workspace members", workspace_members.len());

    // Create a map of package name -> package path and package_path -> package name
    let mut package_path_to_name = HashMap::new();
    let mut package_dependencies: HashMap<String, Vec<String>> = HashMap::new();

    for member_path in workspace_members {
        let full_path = workspace_root.join(member_path);
        let cargo_path = full_path.join("Cargo.toml");

        if !cargo_path.exists() {
            log::trace!("⚠️  Skipping {member_path}: Cargo.toml not found");
            continue;
        }

        log::trace!("📄 Processing package: {member_path}");
        let source = std::fs::read_to_string(&cargo_path)?;
        let value: Value = toml::from_str(&source)?;

        // Get package name
        if let Some(package_name) = value
            .get("package")
            .and_then(|x| x.get("name"))
            .and_then(|x| x.as_str())
        {
            log::trace!("📦 Package name: {package_name} -> {member_path}");
            package_path_to_name.insert(member_path.to_string(), package_name.to_string());

            // Extract dependencies that are workspace members
            let mut deps = Vec::new();

            // Check regular dependencies
            if let Some(dependencies) = value.get("dependencies").and_then(|x| x.as_table()) {
                for (dep_name, dep_value) in dependencies {
                    if is_workspace_dependency(dep_value) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            // Check dev dependencies
            if let Some(dev_dependencies) = value.get("dev-dependencies").and_then(|x| x.as_table())
            {
                for (dep_name, dep_value) in dev_dependencies {
                    if is_workspace_dependency(dep_value) && !deps.contains(dep_name) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            // Check build dependencies
            if let Some(build_dependencies) =
                value.get("build-dependencies").and_then(|x| x.as_table())
            {
                for (dep_name, dep_value) in build_dependencies {
                    if is_workspace_dependency(dep_value) && !deps.contains(dep_name) {
                        deps.push(dep_name.clone());
                    }
                }
            }

            log::trace!("📊 Dependencies for {package_name}: {deps:?}");
            package_dependencies.insert(package_name.to_string(), deps);
        }
    }

    // Find packages directly affected by changed files
    let mut directly_affected_packages = HashSet::new();

    for changed_file in changed_files {
        let changed_path = PathBuf::from(changed_file);

        // Check if the changed file belongs to a workspace package
        for (package_path, package_name) in &package_path_to_name {
            let package_path_buf = PathBuf::from(package_path);

            // Check if the changed file is within this package's directory
            if changed_path.starts_with(&package_path_buf) {
                log::trace!("📝 File {changed_file} affects package {package_name}");
                directly_affected_packages.insert(package_name.clone());
            }
        }
    }

    // Add packages affected by external dependency changes
    #[cfg(feature = "git-diff")]
    if let Some(external_deps) = external_deps {
        if !external_deps.is_empty() {
            log::trace!("🔍 Processing external dependency changes: {external_deps:?}");

            // Build external dependency map
            let workspace_members: Vec<String> = package_path_to_name.keys().cloned().collect();
            let external_dep_map =
                git_diff::build_external_dependency_map(workspace_root, &workspace_members)?;

            // Find packages affected by external dependencies
            let externally_affected =
                git_diff::find_packages_affected_by_external_deps(&external_dep_map, external_deps);

            log::trace!("📦 Packages affected by external dependencies: {externally_affected:?}");

            for package in externally_affected {
                directly_affected_packages.insert(package);
            }
        }
    }

    log::trace!("🎯 Directly affected packages: {directly_affected_packages:?}");

    // Now find all packages that depend on the directly affected packages (transitive dependencies)
    let mut all_affected_packages = directly_affected_packages.clone();
    let mut queue = VecDeque::new();

    // Add all directly affected packages to the queue
    for package in &directly_affected_packages {
        queue.push_back(package.clone());
    }

    // Build reverse dependency map (package -> packages that depend on it)
    let mut reverse_deps: HashMap<String, Vec<String>> = HashMap::new();
    for (package, deps) in &package_dependencies {
        for dep in deps {
            reverse_deps
                .entry(dep.clone())
                .or_default()
                .push(package.clone());
        }
    }

    // Process the queue to find all transitive dependents
    while let Some(current_package) = queue.pop_front() {
        if let Some(dependents) = reverse_deps.get(&current_package) {
            for dependent in dependents {
                if !all_affected_packages.contains(dependent) {
                    log::trace!(
                        "🔄 Package {dependent} depends on affected package {current_package}"
                    );
                    all_affected_packages.insert(dependent.clone());
                    queue.push_back(dependent.clone());
                }
            }
        }
    }

    let mut result: Vec<String> = all_affected_packages.into_iter().collect();
    result.sort();

    log::trace!("🏁 Final affected packages: {result:?}");

    Ok(result)
}

fn get_dependency_default_features(dep_value: &Value) -> Option<bool> {
    match dep_value {
        Value::Table(table) => {
            // Check for "default-features" key
            if let Some(default_features) = table.get("default-features") {
                return default_features.as_bool();
            }
            // Check for legacy "default_features" key (underscore variant)
            if let Some(default_features) = table.get("default_features") {
                return default_features.as_bool();
            }
            // Default is true if not specified
            None
        }
        _ => None,
    }
}

fn collect_system_dependencies(
    workspace_root: &Path,
    dependencies: &[(String, String)],
    enabled_features: Option<&[String]>,
    target_os: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut all_deps = std::collections::HashSet::new();

    // Convert features to comma-separated string for the dependencies command
    let features_str = enabled_features.map(|f| f.join(",")).unwrap_or_default();

    for (_, package_path) in dependencies {
        let path = workspace_root.join(package_path);

        // Skip if no clippier.toml exists for this package
        let clippier_path = path.join("clippier.toml");
        if !clippier_path.exists() {
            continue;
        }

        // Use the existing process_configs function to get dependencies
        let specific_features = if features_str.is_empty() {
            None
        } else {
            Some(
                features_str
                    .split(',')
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            )
        };

        let packages =
            process_configs(&path, None, None, None, false, specific_features.as_deref())?;

        // Extract system dependencies
        for package in packages {
            if let Some(os) = package.get("os").and_then(|v| v.as_str()) {
                if os == target_os {
                    if let Some(deps) = package.get("dependencies").and_then(|v| v.as_str()) {
                        for dep in deps.lines() {
                            if !dep.trim().is_empty() {
                                all_deps.insert(dep.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Convert to sorted vector for consistent output
    let mut result: Vec<String> = all_deps.into_iter().collect();
    result.sort();
    Ok(result)
}
