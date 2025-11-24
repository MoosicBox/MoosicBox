//! Clippier - Rust workspace analysis and automation CLI tool.
//!
//! This binary provides a command-line interface for workspace analysis, CI/CD generation,
//! dependency management, and feature validation in Rust workspace projects.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use clippier::{
    OutputType, handle_affected_packages_command, handle_ci_steps_command,
    handle_dependencies_command, handle_environment_command, handle_features_command,
    handle_generate_dockerfile_command, handle_packages_command,
    handle_validate_feature_propagation_command, handle_workspace_deps_command, print_human_output,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
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

        /// Randomize features before chunking/spreading (useful for CI to test different feature combinations)
        #[arg(long)]
        randomize: bool,

        /// Seed for randomization (enables deterministic randomization when provided)
        #[arg(long)]
        seed: Option<u64>,

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

        /// Include reasoning for why each package is affected in the JSON output (only works with --changed-files)
        #[arg(long)]
        include_reasoning: bool,

        /// List of specific packages to process (comma-separated)
        #[arg(long, value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Glob patterns to ignore when detecting affected packages (e.g., "**/*.md", "*.txt")
        /// Can be specified multiple times. Use "!" prefix for negation (e.g., "!important.md")
        #[arg(long, action = clap::ArgAction::Append)]
        ignore: Vec<String>,

        /// Skip packages matching criteria (format: property<op>value, e.g., "package.publish=false")
        /// Can be specified multiple times. ANY match causes package to be skipped.
        #[arg(long, action = clap::ArgAction::Append)]
        skip_if: Vec<String>,

        /// Only include packages matching criteria (format: property<op>value, e.g., "categories@=audio")
        /// Can be specified multiple times. ALL criteria must match (AND logic between properties).
        #[arg(long, action = clap::ArgAction::Append)]
        include_if: Vec<String>,

        /// Lua transform scripts to apply to the generated matrix (can be specified multiple times)
        #[cfg(feature = "_transforms")]
        #[arg(long, action = clap::ArgAction::Append)]
        transform_scripts: Vec<PathBuf>,

        /// Enable trace mode for transform debugging
        #[cfg(feature = "_transforms")]
        #[arg(long)]
        transform_trace: bool,

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
        /// Path to the workspace root OR git URL
        workspace_root: PathBuf,
        /// Name of the target package to build
        package: String,
        /// Git reference (branch/tag/commit) when using git URL
        #[arg(long, default_value = "master")]
        git_ref: String,
        /// Features to enable for the target package (optional)
        #[arg(long)]
        features: Option<Vec<String>>,
        /// Do not activate the `default` feature
        #[arg(long)]
        no_default_features: bool,
        /// Output path for the generated Dockerfile
        #[arg(long)]
        output: PathBuf,
        /// Docker base image for the builder stage
        #[arg(long, default_value = "rust:1-bookworm")]
        base_image: String,
        /// Docker base image for the final stage
        #[arg(long, default_value = "debian:bookworm-slim")]
        final_image: String,
        /// Arguments to pass to the binary in the CMD instruction
        #[arg(long, action = clap::ArgAction::Append)]
        arg: Vec<String>,
        /// Build arguments to pass to cargo build
        #[arg(long)]
        build_args: Option<String>,
        /// Generate dockerignore file alongside Dockerfile
        #[arg(long, default_value = "true")]
        generate_dockerignore: bool,
        /// Environment variables to include in the generated Dockerfile (format: KEY=VALUE)
        #[arg(long, action = clap::ArgAction::Append)]
        env: Vec<String>,
        /// Environment variables to set during the build process (format: KEY=VALUE)
        #[arg(long, action = clap::ArgAction::Append)]
        build_env: Vec<String>,
        /// Specify the binary name to build and use in the Dockerfile (overrides automatic detection)
        #[arg(long)]
        bin: Option<String>,
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
        /// Include reasoning for why each package is affected in the JSON output
        #[arg(long)]
        include_reasoning: bool,
        /// Glob patterns to ignore when detecting affected packages (e.g., "**/*.md", "*.txt")
        /// Can be specified multiple times. Use "!" prefix for negation (e.g., "!important.md")
        #[arg(long, action = clap::ArgAction::Append)]
        ignore: Vec<String>,
        /// Output format
        #[arg(long, value_enum, default_value_t=OutputType::Json)]
        output: OutputType,
    },
    ValidateFeaturePropagation {
        /// Features to validate (comma-separated, e.g., "fail-on-warnings,cpal")
        /// If not specified, validates all matching features
        #[arg(long, value_delimiter = ',')]
        features: Option<Vec<String>>,

        /// Features to skip during validation (comma-separated, supports glob patterns)
        /// Supports wildcards (* and ?) and negation (! prefix)
        /// Examples: "default,test-*", "*-codec", "*,!fail-on-warnings"
        /// If not specified, defaults to skipping "default" feature and features starting with "_"
        /// Use empty string to skip nothing: --skip-features ""
        #[arg(long, value_delimiter = ',')]
        skip_features: Option<Vec<String>>,

        /// Path to package or workspace (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Only validate workspace packages (ignore external dependencies)
        #[arg(long, default_value_t = true)]
        workspace_only: bool,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,

        /// Exit with error code if validation fails (for CI)
        #[arg(long, default_value_t = true)]
        fail_on_error: bool,

        /// Require strict optional dependency propagation syntax
        /// When enabled, optional dependencies MUST use `dep?/feature` syntax
        /// When disabled (default), accepts both `dep?/feature` and `dep/feature`
        #[arg(long, default_value_t = false)]
        strict_optional: bool,

        /// Allow specific missing propagations (format: "[package:]feature:dependency")
        /// Can be specified multiple times. Package is optional and defaults to all packages.
        /// Examples: `"fail-on-warnings:tcp"`, `"server:async:sync_dep"`
        #[arg(long, action = clap::ArgAction::Append)]
        allow_missing: Vec<String>,

        /// Allow specific incorrect propagations (format: "[package:]feature:entry")
        /// Can be specified multiple times
        #[arg(long, action = clap::ArgAction::Append)]
        allow_incorrect: Vec<String>,

        /// Suppress all validation for specific packages (supports wildcards)
        /// Can be specified multiple times
        #[arg(long, action = clap::ArgAction::Append)]
        ignore_package: Vec<String>,

        /// Suppress validation for specific features globally (supports wildcards)
        /// Can be specified multiple times
        #[arg(long, action = clap::ArgAction::Append)]
        ignore_feature: Vec<String>,

        /// Load overrides from clippier.toml configuration files
        #[arg(long, default_value_t = true)]
        use_config_overrides: bool,

        /// Load overrides from Cargo.toml metadata
        #[arg(long, default_value_t = true)]
        use_cargo_metadata_overrides: bool,

        /// Warn about expired overrides
        #[arg(long, default_value_t = true)]
        warn_expired: bool,

        /// Fail validation if expired overrides exist
        #[arg(long, default_value_t = false)]
        fail_on_expired: bool,

        /// Show verbose override information
        #[arg(long, default_value_t = false)]
        verbose_overrides: bool,
    },
    Packages {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        /// List of specific packages to process (comma-separated)
        #[arg(long, value_delimiter = ',')]
        packages: Option<Vec<String>>,

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

        /// Include reasoning for why each package is affected in the JSON output
        #[arg(long)]
        include_reasoning: bool,

        /// Maximum number of packages in matrix
        #[arg(long)]
        max_parallel: Option<u16>,

        /// Glob patterns to ignore when detecting affected packages (e.g., "**/*.md", "*.txt")
        /// Can be specified multiple times. Use "!" prefix for negation (e.g., "!important.md")
        #[arg(long, action = clap::ArgAction::Append)]
        ignore: Vec<String>,

        /// Skip packages matching criteria (format: property<op>value, e.g., "package.publish=false")
        /// Can be specified multiple times. ANY match causes package to be skipped.
        #[arg(long, action = clap::ArgAction::Append)]
        skip_if: Vec<String>,

        /// Only include packages matching criteria (format: property<op>value, e.g., "categories@=audio")
        /// Can be specified multiple times. ALL criteria must match (AND logic between properties).
        #[arg(long, action = clap::ArgAction::Append)]
        include_if: Vec<String>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Json)]
        output: OutputType,
    },
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None, None).expect("Failed to initialize logging");

    let args = Args::parse();

    let result = match args.cmd {
        Commands::Dependencies {
            file,
            os,
            features,
            output,
        } => handle_dependencies_command(&file, os.as_deref(), features.as_deref(), output)?,
        Commands::Environment {
            file,
            os,
            features,
            output,
        } => handle_environment_command(&file, os.as_deref(), features.as_deref(), output)?,
        Commands::CiSteps {
            file,
            os,
            features,
            output,
        } => handle_ci_steps_command(&file, os.as_deref(), features.as_deref(), output)?,
        Commands::Features {
            file,
            os,
            offset,
            max,
            max_parallel,
            chunked,
            spread,
            randomize,
            seed,
            features,
            skip_features,
            required_features,
            changed_files,
            #[cfg(feature = "git-diff")]
            git_base,
            #[cfg(feature = "git-diff")]
            git_head,
            include_reasoning,
            packages,
            ignore,
            skip_if,
            include_if,
            #[cfg(feature = "_transforms")]
            transform_scripts,
            #[cfg(feature = "_transforms")]
            transform_trace,
            output,
        } => handle_features_command(
            &file,
            os.as_deref(),
            offset,
            max,
            max_parallel,
            chunked,
            spread,
            randomize,
            seed,
            features.as_deref(),
            skip_features.as_deref(),
            required_features.as_deref(),
            packages.as_deref(),
            changed_files.as_deref(),
            #[cfg(feature = "git-diff")]
            git_base.as_deref(),
            #[cfg(feature = "git-diff")]
            git_head.as_deref(),
            include_reasoning,
            if ignore.is_empty() {
                None
            } else {
                Some(&ignore)
            },
            &skip_if,
            &include_if,
            #[cfg(feature = "_transforms")]
            &transform_scripts,
            #[cfg(feature = "_transforms")]
            transform_trace,
            output,
        )?,
        Commands::WorkspaceDeps {
            workspace_root,
            package,
            features,
            format,
            all_potential_deps,
        } => handle_workspace_deps_command(
            &workspace_root,
            &package,
            features.as_deref(),
            &format,
            all_potential_deps,
        )?,
        Commands::GenerateDockerfile {
            workspace_root,
            package,
            git_ref,
            features,
            no_default_features,
            output,
            base_image,
            final_image,
            arg,
            build_args,
            generate_dockerignore,
            env,
            build_env,
            bin,
        } => handle_generate_dockerfile_command(
            &workspace_root,
            &package,
            &git_ref,
            features.as_deref(),
            no_default_features,
            &output,
            &base_image,
            &final_image,
            &arg,
            build_args.as_deref(),
            generate_dockerignore,
            &env,
            &build_env,
            bin.as_deref(),
        )?,
        Commands::AffectedPackages {
            workspace_root,
            changed_files,
            target_package,
            #[cfg(feature = "git-diff")]
            git_base,
            #[cfg(feature = "git-diff")]
            git_head,
            include_reasoning,
            ignore,
            output,
        } => handle_affected_packages_command(
            &workspace_root,
            &changed_files,
            target_package.as_deref(),
            #[cfg(feature = "git-diff")]
            git_base.as_deref(),
            #[cfg(feature = "git-diff")]
            git_head.as_deref(),
            include_reasoning,
            if ignore.is_empty() {
                None
            } else {
                Some(&ignore)
            },
            output,
        )?,
        Commands::ValidateFeaturePropagation {
            features,
            skip_features,
            path,
            workspace_only,
            output,
            fail_on_error,
            strict_optional,
            allow_missing,
            allow_incorrect,
            ignore_package,
            ignore_feature,
            use_config_overrides,
            use_cargo_metadata_overrides,
            warn_expired,
            fail_on_expired,
            verbose_overrides,
        } => {
            let result = handle_validate_feature_propagation_command(
                features,
                skip_features,
                path,
                workspace_only,
                output,
                strict_optional,
                &allow_missing,
                &allow_incorrect,
                &ignore_package,
                &ignore_feature,
                use_config_overrides,
                use_cargo_metadata_overrides,
                warn_expired,
                fail_on_expired,
                verbose_overrides,
            )?;

            match output {
                OutputType::Raw => print_human_output(&result),
                OutputType::Json => println!("{}", serde_json::to_string(&result)?),
            }

            if fail_on_error
                && (!result.errors.is_empty()
                    || (fail_on_expired
                        && result
                            .override_summary
                            .as_ref()
                            .is_some_and(|s| s.expired > 0)))
            {
                std::process::exit(1);
            }

            return Ok(()); // Early return since we handle output ourselves
        }
        Commands::Packages {
            file,
            os,
            packages,
            changed_files,
            #[cfg(feature = "git-diff")]
            git_base,
            #[cfg(feature = "git-diff")]
            git_head,
            include_reasoning,
            max_parallel,
            ignore,
            skip_if,
            include_if,
            output,
        } => handle_packages_command(
            &file,
            os.as_deref(),
            packages.as_deref(),
            changed_files.as_deref(),
            #[cfg(feature = "git-diff")]
            git_base.as_deref(),
            #[cfg(feature = "git-diff")]
            git_head.as_deref(),
            #[cfg(feature = "git-diff")]
            include_reasoning,
            max_parallel,
            #[cfg(feature = "git-diff")]
            Some(&ignore),
            &skip_if,
            &include_if,
            output,
        )?,
    };

    if !result.is_empty() {
        println!("{result}");
    }

    Ok(())
}
