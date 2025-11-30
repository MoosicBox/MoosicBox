//! Command-line interface for managing database schema migrations.
//!
//! This crate provides the `switchy-migrate` binary, a CLI tool for managing database schema
//! migrations with support for `PostgreSQL` and `SQLite` databases. It handles migration creation,
//! execution, rollback, validation, and status tracking.
//!
//! # Features
//!
//! * Create new migration files with timestamped directories
//! * Run pending migrations with various execution strategies (all, up-to, steps)
//! * Rollback migrations individually or in batches
//! * Validate migration checksums to detect file modifications
//! * Track migration status including failed and in-progress migrations
//! * Retry failed migrations
//! * Mark migrations as completed (for manual intervention scenarios)
//!
//! # Usage
//!
//! The binary is invoked as `switchy-migrate` with various subcommands:
//!
//! ```text
//! switchy-migrate create <name>              # Create a new migration
//! switchy-migrate status -d <db-url>          # Show migration status
//! switchy-migrate migrate -d <db-url>         # Run pending migrations
//! switchy-migrate rollback -d <db-url>        # Rollback migrations
//! switchy-migrate validate -d <db-url>        # Validate checksums
//! ```
//!
//! # Supported Databases
//!
//! * `SQLite`: `sqlite://path/to/db.sqlite` or `sqlite://:memory:`
//! * `PostgreSQL`: `postgresql://user:pass@host:port/database`
//!
//! # Environment Variables
//!
//! * `SWITCHY_DATABASE_URL`: Database connection URL
//! * `SWITCHY_MIGRATIONS_DIR`: Directory containing migration files (default: `./migrations`)
//! * `SWITCHY_MIGRATION_TABLE`: Name of migration tracking table (default: `__switchy_migrations`)

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clap::{Parser, Subcommand};
use std::{io::Write as _, path::PathBuf};
use thiserror::Error;

mod utils;

/// Error types for the CLI.
///
/// Represents all possible errors that can occur during CLI operations, including
/// migration execution errors, database connection failures, I/O errors, and
/// configuration problems.
#[derive(Debug, Error)]
pub enum CliError {
    /// Migration execution error.
    ///
    /// Wrapper for errors from the migration engine, including failed migrations,
    /// checksum validation failures, and migration state issues.
    #[error(transparent)]
    Migration(#[from] switchy_schema::MigrationError),
    /// Database connection or query error.
    ///
    /// Wrapper for database-level errors including connection failures, query
    /// execution errors, and transaction issues.
    #[error(transparent)]
    Database(#[from] switchy_database::DatabaseError),
    /// File system I/O error.
    ///
    /// Wrapper for standard I/O errors such as missing files, permission errors,
    /// or disk space issues when creating or reading migration files.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Configuration or validation error.
    ///
    /// Represents errors in command-line arguments, environment variables, or
    /// invalid database URLs. The string contains a human-readable error message.
    #[error("Configuration error: {0}")]
    Config(String),
}

type Result<T> = std::result::Result<T, CliError>;

/// CLI for managing database schema migrations
#[derive(Parser)]
#[command(name = "switchy-migrate")]
#[command(about = "A CLI tool for managing database schema migrations")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new migration file
    Create {
        /// Name for the migration
        name: String,
        /// Directory containing migrations
        #[arg(
            short,
            long,
            env = "SWITCHY_MIGRATIONS_DIR",
            default_value = "./migrations"
        )]
        migrations_dir: PathBuf,
    },
    /// Show migration status and pending migrations
    Status {
        /// Database connection URL
        #[arg(short, long, env = "SWITCHY_DATABASE_URL")]
        database_url: String,
        /// Directory containing migrations
        #[arg(
            short,
            long,
            env = "SWITCHY_MIGRATIONS_DIR",
            default_value = "./migrations"
        )]
        migrations_dir: PathBuf,
        /// Migration table name
        #[arg(
            long,
            env = "SWITCHY_MIGRATION_TABLE",
            default_value = "__switchy_migrations"
        )]
        migration_table: String,
        /// Show detailed status including failed and in-progress migrations
        #[arg(long)]
        show_failed: bool,
    },
    /// Run pending migrations
    Migrate {
        /// Database connection URL
        #[arg(short, long, env = "SWITCHY_DATABASE_URL")]
        database_url: String,
        /// Directory containing migrations
        #[arg(
            short,
            long,
            env = "SWITCHY_MIGRATIONS_DIR",
            default_value = "./migrations"
        )]
        migrations_dir: PathBuf,
        /// Migration table name
        #[arg(
            long,
            env = "SWITCHY_MIGRATION_TABLE",
            default_value = "__switchy_migrations"
        )]
        migration_table: String,
        /// Run migrations up to this specific migration ID
        #[arg(long)]
        up_to: Option<String>,
        /// Run only this many migrations
        #[arg(long)]
        steps: Option<usize>,
        /// Dry run - show what would be done without executing
        #[arg(long)]
        dry_run: bool,
        /// Force migration even if dirty state detected (dangerous)
        #[arg(long)]
        force: bool,
        /// Require checksum validation before running migrations
        #[arg(long)]
        require_checksum_validation: bool,
    },
    /// Rollback migrations
    Rollback {
        /// Database connection URL
        #[arg(short, long, env = "SWITCHY_DATABASE_URL")]
        database_url: String,
        /// Directory containing migrations
        #[arg(
            short,
            long,
            env = "SWITCHY_MIGRATIONS_DIR",
            default_value = "./migrations"
        )]
        migrations_dir: PathBuf,
        /// Migration table name
        #[arg(
            long,
            env = "SWITCHY_MIGRATION_TABLE",
            default_value = "__switchy_migrations"
        )]
        migration_table: String,
        /// Rollback to this migration ID (not including it)
        #[arg(long)]
        to: Option<String>,
        /// Number of migrations to rollback
        #[arg(long)]
        steps: Option<usize>,
        /// Rollback all migrations
        #[arg(long)]
        all: bool,
        /// Dry run - show what would be done without executing
        #[arg(long)]
        dry_run: bool,
    },
    /// Retry a failed migration
    Retry {
        /// Database connection URL
        #[arg(short, long, env = "SWITCHY_DATABASE_URL")]
        database_url: String,
        /// Directory containing migrations
        #[arg(
            short,
            long,
            env = "SWITCHY_MIGRATIONS_DIR",
            default_value = "./migrations"
        )]
        migrations_dir: PathBuf,
        /// Migration table name
        #[arg(
            long,
            env = "SWITCHY_MIGRATION_TABLE",
            default_value = "__switchy_migrations"
        )]
        migration_table: String,
        /// Migration ID to retry
        migration_id: String,
    },
    /// Mark a migration as completed (dangerous operation)
    MarkCompleted {
        /// Database connection URL
        #[arg(short, long, env = "SWITCHY_DATABASE_URL")]
        database_url: String,
        /// Directory containing migrations
        #[arg(
            short,
            long,
            env = "SWITCHY_MIGRATIONS_DIR",
            default_value = "./migrations"
        )]
        migrations_dir: PathBuf,
        /// Migration table name
        #[arg(
            long,
            env = "SWITCHY_MIGRATION_TABLE",
            default_value = "__switchy_migrations"
        )]
        migration_table: String,
        /// Migration ID to mark as completed
        migration_id: String,
        /// Force the operation without confirmation
        #[arg(long)]
        force: bool,
    },
    /// Mark all migrations as completed without executing them (VERY dangerous operation)
    MarkAllCompleted {
        /// Database connection URL
        #[arg(short, long, env = "SWITCHY_DATABASE_URL")]
        database_url: String,
        /// Directory containing migrations
        #[arg(
            short,
            long,
            env = "SWITCHY_MIGRATIONS_DIR",
            default_value = "./migrations"
        )]
        migrations_dir: PathBuf,
        /// Migration table name
        #[arg(
            long,
            env = "SWITCHY_MIGRATION_TABLE",
            default_value = "__switchy_migrations"
        )]
        migration_table: String,
        /// Also mark failed migrations as completed
        #[arg(long)]
        include_failed: bool,
        /// Also mark in-progress migrations as completed
        #[arg(long)]
        include_in_progress: bool,
        /// Mark ALL migrations regardless of state (most dangerous, implies --include-failed and --include-in-progress)
        #[arg(long)]
        all: bool,
        /// Drop and recreate the migration tracking table before marking (CRITICAL - deletes all migration history)
        #[arg(long)]
        drop: bool,
        /// Force the operation without confirmation
        #[arg(long)]
        force: bool,
    },
    /// Validate checksums of applied migrations
    Validate {
        /// Database connection URL
        #[arg(short, long, env = "SWITCHY_DATABASE_URL")]
        database_url: String,
        /// Directory containing migrations
        #[arg(
            short,
            long,
            env = "SWITCHY_MIGRATIONS_DIR",
            default_value = "./migrations"
        )]
        migrations_dir: PathBuf,
        /// Migration table name
        #[arg(
            long,
            env = "SWITCHY_MIGRATION_TABLE",
            default_value = "__switchy_migrations"
        )]
        migration_table: String,
        /// Exit with error code if mismatches found
        #[arg(long)]
        strict: bool,
        /// Show detailed checksum values
        #[arg(long)]
        verbose: bool,
    },
}

#[allow(clippy::too_many_lines)]
#[switchy_async::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Create {
            name,
            migrations_dir,
        } => create_migration(&name, &migrations_dir),
        Commands::Status {
            database_url,
            migrations_dir,
            migration_table,
            show_failed,
        } => show_status(database_url, migrations_dir, migration_table, show_failed).await,
        Commands::Migrate {
            database_url,
            migrations_dir,
            migration_table,
            up_to,
            steps,
            dry_run,
            force,
            require_checksum_validation,
        } => {
            run_migrations(
                database_url,
                migrations_dir,
                migration_table,
                up_to,
                steps,
                dry_run,
                force,
                require_checksum_validation,
            )
            .await
        }
        Commands::Rollback {
            database_url,
            migrations_dir,
            migration_table,
            to,
            steps,
            all,
            dry_run,
        } => {
            rollback_migrations(
                database_url,
                migrations_dir,
                migration_table,
                to,
                steps,
                all,
                dry_run,
            )
            .await
        }
        Commands::Retry {
            database_url,
            migrations_dir,
            migration_table,
            migration_id,
        } => retry_migration(database_url, migrations_dir, migration_table, migration_id).await,
        Commands::MarkCompleted {
            database_url,
            migrations_dir,
            migration_table,
            migration_id,
            force,
        } => {
            mark_migration_completed(
                database_url,
                migrations_dir,
                migration_table,
                migration_id,
                force,
            )
            .await
        }
        Commands::MarkAllCompleted {
            database_url,
            migrations_dir,
            migration_table,
            include_failed,
            include_in_progress,
            all,
            drop,
            force,
        } => {
            mark_all_migrations_completed(
                database_url,
                migrations_dir,
                migration_table,
                include_failed,
                include_in_progress,
                all,
                drop,
                force,
            )
            .await
        }
        Commands::Validate {
            database_url,
            migrations_dir,
            migration_table,
            strict,
            verbose,
        } => {
            validate_checksums(
                database_url,
                migrations_dir,
                migration_table,
                strict,
                verbose,
            )
            .await
        }
    }
}

/// Create a new migration file
fn create_migration(name: &str, migrations_dir: &PathBuf) -> Result<()> {
    // Validate migration name
    if name.is_empty() {
        return Err(CliError::Config(
            "Migration name cannot be empty".to_string(),
        ));
    }

    // Generate timestamp prefix (YYYY-MM-DD-HHMMSS)
    let timestamp = chrono::Utc::now().format("%Y-%m-%d-%H%M%S").to_string();
    let migration_id = format!("{timestamp}_{name}");

    // Create migrations directory if it doesn't exist
    if !migrations_dir.exists() {
        switchy_fs::sync::create_dir_all(migrations_dir)?;
        println!("Created migrations directory: {}", migrations_dir.display());
    }

    // Create migration subdirectory
    let migration_path = migrations_dir.join(&migration_id);
    if migration_path.exists() {
        return Err(CliError::Config(format!(
            "Migration directory already exists: {}",
            migration_path.display()
        )));
    }

    switchy_fs::sync::create_dir_all(&migration_path)?;

    // Create up.sql file
    let up_path = migration_path.join("up.sql");
    let up_content = format!(
        r"-- Migration: {name}
-- Created at: {}
--
-- Add your forward migration SQL here
-- This file will be executed when running migrations

",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );

    let mut up_file = switchy_fs::sync::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&up_path)?;

    up_file.write_all(up_content.as_bytes())?;

    // Create down.sql file
    let down_path = migration_path.join("down.sql");
    let down_content = format!(
        r"-- Rollback: {name}
-- Created at: {}
--
-- Add your rollback migration SQL here
-- This file will be executed when rolling back migrations
-- Should reverse the changes made in up.sql

",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );

    let mut down_file = switchy_fs::sync::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&down_path)?;

    down_file.write_all(down_content.as_bytes())?;

    println!("Created migration: {migration_id}");
    println!("  Up:   {}", up_path.display());
    println!("  Down: {}", down_path.display());

    Ok(())
}

/// Show migration status
#[allow(clippy::too_many_lines)]
async fn show_status(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
    show_failed: bool,
) -> Result<()> {
    use switchy_schema::runner::MigrationRunner;

    // Connect to database
    let db = utils::database::connect(&database_url).await?;

    // Create migration runner with directory source
    let runner =
        MigrationRunner::new_directory(&migrations_dir).with_table_name(migration_table.clone());

    // Get migration information
    let migrations = runner.list_migrations(&*db).await?;

    if migrations.is_empty() {
        println!(
            "No migrations found in directory: {}",
            migrations_dir.display()
        );
        return Ok(());
    }

    // Display status
    println!("Migration Status");
    println!("================");
    println!("Migrations directory: {}", migrations_dir.display());
    println!("Migration table: {migration_table}");
    println!();

    let mut applied_count = 0;
    let mut pending_count = 0;
    let mut failed_count = 0;
    let mut in_progress_count = 0;

    for migration in &migrations {
        if show_failed {
            // Enhanced status display with colors
            use colored::Colorize;
            use switchy_schema::migration::MigrationStatus;

            let (status_symbol, status_text, color_fn): (_, _, fn(&str) -> colored::ColoredString) =
                match migration.status {
                    Some(MigrationStatus::Completed) => {
                        applied_count += 1;
                        ("✓", "Completed", |s| s.green())
                    }
                    Some(MigrationStatus::Failed) => {
                        failed_count += 1;
                        ("✗", "Failed", |s| s.red())
                    }
                    Some(MigrationStatus::InProgress) => {
                        in_progress_count += 1;
                        ("⚠", "In Progress", |s| s.yellow())
                    }
                    None => {
                        pending_count += 1;
                        ("-", "Pending", |s| s.normal())
                    }
                };

            println!(
                "{} {:<11} {}",
                status_symbol,
                color_fn(status_text),
                migration.id
            );

            // Show description if available
            if let Some(description) = &migration.description
                && !description.is_empty()
            {
                println!("               {description}");
            }

            // Show timestamps for applied migrations
            if migration.applied {
                if let Some(run_on) = migration.run_on {
                    println!(
                        "               Started: {}",
                        run_on.format("%Y-%m-%d %H:%M:%S")
                    );
                }
                if let Some(finished_on) = migration.finished_on {
                    println!(
                        "               Finished: {}",
                        finished_on.format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }

            // Show failure reason for failed migrations
            if let Some(failure_reason) = &migration.failure_reason {
                println!(
                    "               {}",
                    format!("Error: {failure_reason}").red()
                );
            }
        } else {
            // Original simple status display
            let status = if migration.applied { "✓" } else { "✗" };
            let applied_text = if migration.applied {
                applied_count += 1;
                "Applied"
            } else {
                pending_count += 1;
                "Pending"
            };

            println!("{status} {:<8} {}", applied_text, migration.id);

            if let Some(description) = &migration.description
                && !description.is_empty()
            {
                println!("             {description}");
            }
        }
    }

    println!();
    println!("Summary:");
    if show_failed {
        use colored::Colorize;
        println!("  {}: {applied_count}", "Completed".green());
        println!("  {}: {failed_count}", "Failed".red());
        println!("  {}: {in_progress_count}", "In Progress".yellow());
        println!("  Pending: {pending_count}");
        println!("  Total:   {}", migrations.len());

        // Show warnings for problematic states
        if in_progress_count > 0 {
            println!();
            println!("{}", "⚠️  WARNING: Found migrations in progress - this may indicate interrupted operations".yellow());
        }
        if failed_count > 0 {
            println!();
            println!("{}", "❌ ERROR: Found failed migrations - use 'switchy-migrate retry <migration_id>' to retry".red());
        }
    } else {
        println!("  Applied: {applied_count}");
        println!("  Pending: {pending_count}");
        println!("  Total:   {}", migrations.len());
    }

    if pending_count > 0 {
        println!();
        println!("Run 'switchy-migrate migrate' to apply pending migrations.");
    }

    Ok(())
}

/// Run pending migrations
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn run_migrations(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
    up_to: Option<String>,
    steps: Option<usize>,
    dry_run: bool,
    force: bool,
    require_checksum_validation: bool,
) -> Result<()> {
    use switchy_schema::runner::{ChecksumConfig, ExecutionStrategy, MigrationRunner};

    // Validate arguments
    if up_to.is_some() && steps.is_some() {
        return Err(CliError::Config(
            "Cannot specify both --up-to and --steps".to_string(),
        ));
    }

    // Check environment variable with CLI priority
    let require_validation = require_checksum_validation
        || std::env::var("MIGRATION_REQUIRE_CHECKSUM_VALIDATION")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

    // Warn if CLI overrides env var
    if require_checksum_validation && std::env::var("MIGRATION_REQUIRE_CHECKSUM_VALIDATION").is_ok()
    {
        println!(
            "Warning: CLI flag --require-checksum-validation overrides MIGRATION_REQUIRE_CHECKSUM_VALIDATION environment variable"
        );
    }

    // Connect to database
    let db = utils::database::connect(&database_url).await?;

    // Determine execution strategy
    let strategy = match (up_to.clone(), steps) {
        (Some(id), None) => ExecutionStrategy::UpTo(id),
        (None, Some(n)) => ExecutionStrategy::Steps(n),
        (None, None) => {
            if dry_run {
                ExecutionStrategy::DryRun
            } else {
                ExecutionStrategy::All
            }
        }
        (Some(_), Some(_)) => unreachable!(), // Already validated above
    };

    // Configure checksum validation
    let config = ChecksumConfig { require_validation };

    // Create migration runner with directory source
    let mut runner = MigrationRunner::new_directory(&migrations_dir)
        .with_table_name(migration_table.clone())
        .with_strategy(strategy)
        .with_checksum_config(config);

    if dry_run {
        runner = runner.dry_run();
    }

    if force {
        use colored::Colorize;
        println!(
            "{}",
            "⚠️  WARNING: Bypassing dirty state check - this may cause data corruption!"
                .yellow()
                .bold()
        );
        println!("Proceeding with --force flag...");
        println!();
        runner = runner.with_allow_dirty(true);
    }

    // Show what we're about to do
    let action = if dry_run { "Dry run" } else { "Running" };
    println!("{action} migrations from: {}", migrations_dir.display());

    // Show strategy description
    let strategy_desc = match (up_to.as_ref(), steps, dry_run) {
        (Some(id), None, false) => format!("Strategy: Apply migrations up to '{id}'"),
        (None, Some(n), false) => format!("Strategy: Apply next {n} migration(s)"),
        (None, None, true) => "Strategy: Dry run (validate only)".to_string(),
        (None, None, false) => "Strategy: Apply all pending migrations".to_string(),
        _ => unreachable!(), // Already validated above
    };
    println!("{strategy_desc}");

    println!("Migration table: {migration_table}");
    println!();

    // Get migration status before running
    let migrations_before = runner.list_migrations(&*db).await?;
    let pending_before: Vec<_> = migrations_before.iter().filter(|m| !m.applied).collect();

    // If strict mode is enabled, validate checksums even when no migrations are pending
    if require_validation && !migrations_before.is_empty() {
        let mismatches = runner.validate_checksums(&*db).await?;
        if !mismatches.is_empty() {
            use colored::Colorize;
            eprintln!("{}", "Checksum validation failed!".red().bold());
            eprintln!("The following migrations have been modified since they were applied:");
            eprintln!();
            for mismatch in &mismatches {
                eprintln!("  Migration: {}", mismatch.migration_id.yellow());
                eprintln!("    - {} script checksum mismatch", mismatch.checksum_type);
                eprintln!("      Expected: {}", mismatch.stored_checksum);
                eprintln!("      Actual:   {}", mismatch.current_checksum);
                eprintln!();
            }
            eprintln!(
                "Migration integrity check failed. Aborting to prevent potential data corruption."
            );
            eprintln!("If you're certain the changes are safe, you can:");
            eprintln!("  1. Run without --require-checksum-validation flag");
            eprintln!("  2. Use 'validate' command with --update flag to update checksums");
            return Err(
                switchy_schema::MigrationError::ChecksumValidationFailed { mismatches }.into(),
            );
        }
    }

    if pending_before.is_empty() && !require_validation {
        println!("No pending migrations found.");
        return Ok(());
    }

    if dry_run {
        println!("Would apply {} migration(s):", pending_before.len());
        for migration in &pending_before {
            println!("  • {}", migration.id);
        }
        println!();
        println!("Dry run completed. No changes were made.");
    } else {
        if pending_before.is_empty() {
            println!("No pending migrations to apply. Running checksum validation only.");
        } else {
            println!("Applying {} migration(s):", pending_before.len());
        }

        // Run migrations
        runner.run(&*db).await?;

        // Show results
        let migrations_after = runner.list_migrations(&*db).await?;
        let applied_count = migrations_after.iter().filter(|m| m.applied).count()
            - migrations_before.iter().filter(|m| m.applied).count();

        println!("Successfully applied {applied_count} migration(s).");
    }

    Ok(())
}

/// Rollback migrations
async fn rollback_migrations(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
    to: Option<String>,
    steps: Option<usize>,
    all: bool,
    dry_run: bool,
) -> Result<()> {
    use std::io::{self, Write};
    use switchy_schema::runner::{MigrationRunner, RollbackStrategy};

    // Validate arguments
    let strategy_count = [steps.is_some(), to.is_some(), all]
        .iter()
        .filter(|&&x| x)
        .count();
    if strategy_count > 1 {
        return Err(CliError::Config(
            "Cannot specify multiple rollback strategies (--steps, --to, --all)".to_string(),
        ));
    }

    // Connect to database
    let db = utils::database::connect(&database_url).await?;

    // Determine rollback strategy
    let strategy = if all {
        RollbackStrategy::All
    } else if let Some(target_id) = to {
        RollbackStrategy::DownTo(target_id)
    } else if let Some(n) = steps {
        if n == 1 {
            RollbackStrategy::Last
        } else {
            RollbackStrategy::Steps(n)
        }
    } else {
        // Default to rolling back 1 migration
        RollbackStrategy::Last
    };

    // Create migration runner with directory source
    let mut runner =
        MigrationRunner::new_directory(&migrations_dir).with_table_name(migration_table.clone());

    if dry_run {
        runner = runner.dry_run();
    }

    // Show what we're about to do
    let action = if dry_run {
        "Dry run rollback"
    } else {
        "Rolling back migrations"
    };
    println!("{action} from: {}", migrations_dir.display());

    match &strategy {
        RollbackStrategy::Last => println!("Strategy: Rollback the most recent migration"),
        RollbackStrategy::DownTo(id) => {
            println!("Strategy: Rollback to (but not including) '{id}'");
        }
        RollbackStrategy::Steps(n) => println!("Strategy: Rollback {n} migration(s)"),
        RollbackStrategy::All => println!("Strategy: Rollback all applied migrations"),
    }

    println!("Migration table: {migration_table}");
    println!();

    // Get migration status before rollback
    let migrations_before = runner.list_migrations(&*db).await?;
    let applied_before: Vec<_> = migrations_before.iter().filter(|m| m.applied).collect();

    if applied_before.is_empty() {
        println!("No applied migrations found to rollback.");
        return Ok(());
    }

    // Show which migrations would be affected
    println!("Applied migrations (most recent first):");
    let mut applied_reversed = applied_before.clone();
    applied_reversed.reverse(); // Show in reverse chronological order

    for (i, migration) in applied_reversed.iter().enumerate() {
        let marker = match &strategy {
            RollbackStrategy::Last if i == 0 => " ← will rollback",
            RollbackStrategy::Steps(n) if i < *n => " ← will rollback",
            RollbackStrategy::DownTo(target_id) if migration.id > *target_id => " ← will rollback",
            RollbackStrategy::All => " ← will rollback",
            _ => "",
        };

        println!("  {} {}{marker}", i + 1, migration.id);
    }

    println!();

    if dry_run {
        println!("Dry run completed. No changes would be made.");
        println!("Run without --dry-run to execute the rollback.");
    } else {
        // Confirm before proceeding
        println!("⚠️  WARNING: Rolling back migrations may result in data loss!");
        print!("Are you sure you want to proceed? (y/N): ");
        io::stdout().flush().map_err(CliError::Io)?;

        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(CliError::Io)?;

        let confirmed = input.trim().to_lowercase();
        if confirmed != "y" && confirmed != "yes" {
            println!("Rollback cancelled.");
            return Ok(());
        }

        // Perform rollback
        println!("Rolling back migrations...");
        runner.rollback(&*db, strategy).await?;

        // Show results
        let migrations_after = runner.list_migrations(&*db).await?;
        let applied_after_count = migrations_after.iter().filter(|m| m.applied).count();
        let rollback_count = applied_before.len() - applied_after_count;

        println!("Successfully rolled back {rollback_count} migration(s).");
    }

    Ok(())
}

/// Retry a failed migration
async fn retry_migration(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
    migration_id: String,
) -> Result<()> {
    use colored::Colorize;
    use switchy_schema::runner::MigrationRunner;

    // Connect to database
    let db = utils::database::connect(&database_url).await?;

    // Create migration runner with directory source
    let runner = MigrationRunner::new_directory(&migrations_dir).with_table_name(migration_table);

    println!("Retrying migration: {}", migration_id.cyan());
    println!("Migrations directory: {}", migrations_dir.display());
    println!();

    // Retry the migration
    match runner.retry_migration(&*db, &migration_id).await {
        Ok(()) => {
            println!(
                "{} Successfully retried migration '{migration_id}'",
                "✓".green()
            );
        }
        Err(e) => {
            println!(
                "{} Failed to retry migration '{migration_id}': {e}",
                "✗".red()
            );
            return Err(CliError::Migration(e));
        }
    }

    Ok(())
}

/// Mark a migration as completed (dangerous operation)
async fn mark_migration_completed(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
    migration_id: String,
    force: bool,
) -> Result<()> {
    use colored::Colorize;
    use dialoguer::Confirm;
    use switchy_schema::runner::MigrationRunner;

    // Connect to database
    let db = utils::database::connect(&database_url).await?;

    // Create migration runner with directory source
    let runner = MigrationRunner::new_directory(&migrations_dir).with_table_name(migration_table);

    println!(
        "{} Marking migration as completed: {}",
        "⚠️".yellow(),
        migration_id.cyan()
    );
    println!("Migrations directory: {}", migrations_dir.display());
    println!();

    // Show warning unless force flag is used
    if !force {
        println!(
            "{}",
            "⚠️  WARNING: This is a dangerous operation!"
                .yellow()
                .bold()
        );
        println!("Marking a migration as completed without running it can lead to:");
        println!("- Database schema inconsistencies");
        println!("- Failed future migrations");
        println!("- Data corruption");
        println!();

        let confirmed = Confirm::new()
            .with_prompt("Are you sure you want to mark this migration as completed?")
            .default(false)
            .interact()
            .map_err(|e| CliError::Config(format!("Failed to get user confirmation: {e}")))?;

        if !confirmed {
            println!("Operation cancelled.");
            return Ok(());
        }
    }

    // Mark the migration as completed
    match runner.mark_migration_completed(&*db, &migration_id).await {
        Ok(message) => {
            println!("{} {}", "✓".green(), message);
        }
        Err(e) => {
            println!(
                "{} Failed to mark migration '{migration_id}' as completed: {e}",
                "✗".red()
            );
            return Err(CliError::Migration(e));
        }
    }

    Ok(())
}

/// Mark all migrations as completed without executing them
#[allow(
    clippy::fn_params_excessive_bools,
    clippy::too_many_lines,
    clippy::too_many_arguments
)]
async fn mark_all_migrations_completed(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
    include_failed: bool,
    include_in_progress: bool,
    all: bool,
    drop: bool,
    force: bool,
) -> Result<()> {
    use colored::Colorize;
    use dialoguer::Confirm;
    use switchy_schema::runner::{MarkCompletedScope, MigrationRunner};

    // Connect to database
    let db = utils::database::connect(&database_url).await?;

    // Create migration runner with directory source
    let runner =
        MigrationRunner::new_directory(&migrations_dir).with_table_name(migration_table.clone());

    // Determine scope based on flags
    let scope = if all || include_failed && include_in_progress {
        MarkCompletedScope::All
    } else if include_failed {
        MarkCompletedScope::IncludeFailed
    } else if include_in_progress {
        MarkCompletedScope::IncludeInProgress
    } else {
        MarkCompletedScope::PendingOnly
    };

    // Determine danger level for warnings
    let danger_level = if drop {
        "CRITICAL"
    } else {
        match scope {
            MarkCompletedScope::PendingOnly => "MODERATE",
            MarkCompletedScope::IncludeFailed | MarkCompletedScope::IncludeInProgress => "HIGH",
            MarkCompletedScope::All => "EXTREME",
        }
    };

    println!("{} Marking migrations as completed", "⚠️".yellow().bold());
    println!("Migrations directory: {}", migrations_dir.display());
    println!("Scope: {scope:?}");
    if drop {
        println!("Drop table: {} (CRITICAL)", "YES".red().bold());
    }
    println!("Danger level: {}", danger_level.red().bold());
    println!();

    // Show warnings unless force flag is used
    if !force {
        if drop {
            println!(
                "{}",
                "🔥 CRITICAL: THIS WILL DELETE ALL MIGRATION HISTORY! 🔥"
                    .red()
                    .bold()
            );
            println!("{}", "█".repeat(70).red());
            println!();
            println!(
                "{}",
                "⚠️  ALL DATA IN THE MIGRATION TABLE WILL BE PERMANENTLY DELETED:"
                    .red()
                    .bold()
            );
            println!(
                "  {} Migration execution status (completed/failed/in-progress)",
                "✗".red()
            );
            println!("  {} Execution timestamps (when migrations ran)", "✗".red());
            println!("  {} Failure reasons and error messages", "✗".red());
            println!("  {} Stored checksums for validation", "✗".red());
            println!();
            println!("{}", "This operation will:".yellow().bold());
            println!(
                "  1️⃣  {} the entire '{}' table",
                "DROP".red().bold(),
                migration_table.cyan()
            );
            println!(
                "  2️⃣  {} a fresh migration tracking table",
                "CREATE".green(),
            );
            println!(
                "  3️⃣  {} all source migrations as completed with new checksums",
                "MARK".green()
            );
            println!("{}", "█".repeat(70).red());
            println!();
            println!("{}", "⚠️  THIS CANNOT BE UNDONE!".red().bold());
            println!();
            println!("Only use this if:");
            println!(
                "  {} The migration tracking table is corrupted",
                "•".yellow()
            );
            println!(
                "  {} The table schema is incompatible with the current code",
                "•".yellow()
            );
            println!(
                "  {} You need to completely reset migration history",
                "•".yellow()
            );
            println!();

            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "{} Type 'yes' if you want to {} the migration table and start fresh",
                    "⚠️".red(),
                    "DELETE ALL HISTORY".red().bold()
                ))
                .default(false)
                .interact()
                .map_err(|e| CliError::Config(format!("Failed to get user confirmation: {e}")))?;

            if !confirmed {
                println!("Operation cancelled.");
                return Ok(());
            }

            println!();
            let double_confirm = Confirm::new()
                .with_prompt(format!(
                    "{} Are you {} sure? This will {} all migration history!",
                    "⚠️".red(),
                    "ABSOLUTELY".red().bold(),
                    "PERMANENTLY DELETE".red().bold()
                ))
                .default(false)
                .interact()
                .map_err(|e| CliError::Config(format!("Failed to get user confirmation: {e}")))?;

            if !double_confirm {
                println!("Operation cancelled.");
                return Ok(());
            }
        } else {
            match scope {
                MarkCompletedScope::PendingOnly => {
                    println!(
                        "{}",
                        "⚠️  WARNING: This will mark untracked migrations as completed!"
                            .yellow()
                            .bold()
                    );
                    println!("This is relatively safe but can still lead to issues if:");
                    println!(
                        "  {} Database schema doesn't match migrations",
                        "•".yellow()
                    );
                    println!(
                        "  {} Migrations haven't been manually applied",
                        "•".yellow()
                    );
                    println!();
                    println!("This operation will:");
                    println!("  {} Mark untracked migrations as completed", "✓".green());
                    println!("  {} Leave completed migrations unchanged", "⏭".blue());
                    println!("  {} Leave failed migrations unchanged", "⏭".blue());
                    println!("  {} Leave in-progress migrations unchanged", "⏭".blue());
                }
                MarkCompletedScope::IncludeFailed => {
                    println!(
                        "{}",
                        "⚠️  DANGER: This will mark untracked AND FAILED migrations as completed!"
                            .red()
                            .bold()
                    );
                    println!("{}", "═".repeat(70).red());
                    println!("This operation will:");
                    println!("  {} Mark untracked migrations as completed", "✓".green());
                    println!("  {} Mark FAILED migrations as completed", "⚠".yellow());
                    println!("  {} Leave completed migrations unchanged", "⏭".blue());
                    println!("  {} Leave in-progress migrations unchanged", "⏭".blue());
                    println!("{}", "═".repeat(70).red());
                    println!();
                    println!("Use this only if:");
                    println!("  {} Failed migrations were manually fixed", "•".yellow());
                    println!(
                        "  {} You want to skip multiple failed migrations",
                        "•".yellow()
                    );
                }
                MarkCompletedScope::IncludeInProgress => {
                    println!(
                    "{}",
                    "⚠️  DANGER: This will mark untracked AND IN-PROGRESS migrations as completed!"
                        .red()
                        .bold()
                );
                    println!("{}", "═".repeat(70).red());
                    println!("This operation will:");
                    println!("  {} Mark untracked migrations as completed", "✓".green());
                    println!(
                        "  {} Mark IN-PROGRESS migrations as completed",
                        "⚠".yellow()
                    );
                    println!("  {} Leave completed migrations unchanged", "⏭".blue());
                    println!("  {} Leave failed migrations unchanged", "⏭".blue());
                    println!("{}", "═".repeat(70).red());
                    println!();
                    println!("Use this only if:");
                    println!(
                        "  {} A migration process was interrupted/crashed",
                        "•".yellow()
                    );
                    println!(
                        "  {} In-progress migrations actually completed",
                        "•".yellow()
                    );
                }
                MarkCompletedScope::All => {
                    println!(
                        "{}",
                        "🚨 EXTREME DANGER: THIS WILL MARK ALL MIGRATIONS AS COMPLETED! 🚨"
                            .red()
                            .bold()
                    );
                    println!("{}", "═".repeat(70).red());
                    println!("This operation will:");
                    println!("  {} Mark untracked migrations as completed", "✓".green());
                    println!("  {} Mark FAILED migrations as completed", "⚠".red());
                    println!("  {} Mark IN-PROGRESS migrations as completed", "⚠".red());
                    println!("  {} Leave completed migrations unchanged", "⏭".blue());
                    println!("{}", "═".repeat(70).red());
                    println!();
                    println!("This can lead to:");
                    println!("  {} Database schema inconsistencies", "✗".red());
                    println!("  {} Failed future migrations", "✗".red());
                    println!("  {} Data corruption", "✗".red());
                    println!("  {} Application crashes", "✗".red());
                }
            }

            println!();
            println!("Only use this if:");
            println!(
                "  {} You're initializing a tracking table for an existing database",
                "•".cyan()
            );
            println!(
                "  {} You've manually applied migrations and need to sync",
                "•".cyan()
            );
            println!(
                "  {} You're recovering from schema table corruption",
                "•".cyan()
            );
            println!();

            let prompt = match scope {
                MarkCompletedScope::PendingOnly => {
                    "Are you sure you want to mark untracked migrations as completed?"
                }
                MarkCompletedScope::IncludeFailed | MarkCompletedScope::IncludeInProgress => {
                    "Are you SURE you want to proceed with this dangerous operation?"
                }
                MarkCompletedScope::All => {
                    "Are you ABSOLUTELY CERTAIN you want to mark ALL migrations as completed?"
                }
            };

            let confirmed = Confirm::new()
                .with_prompt(prompt)
                .default(false)
                .interact()
                .map_err(|e| CliError::Config(format!("Failed to get user confirmation: {e}")))?;

            if !confirmed {
                println!("Operation cancelled.");
                return Ok(());
            }
        }

        // Double confirmation for dangerous scopes
        if matches!(
            scope,
            MarkCompletedScope::All
                | MarkCompletedScope::IncludeFailed
                | MarkCompletedScope::IncludeInProgress
        ) {
            let double_confirmed = Confirm::new()
                .with_prompt("Last chance: Proceed?")
                .default(false)
                .interact()
                .map_err(|e| CliError::Config(format!("Failed to get user confirmation: {e}")))?;

            if !double_confirmed {
                println!("Operation cancelled.");
                return Ok(());
            }
        }
    }

    // Drop table if requested
    if drop {
        println!();
        println!("{} Dropping migration tracking table...", "⚙".yellow());
        runner
            .drop_tracking_table(&*db)
            .await
            .map_err(CliError::Migration)?;
        println!("{} Table dropped successfully", "✓".green());

        println!(
            "{} Creating fresh migration tracking table...",
            "⚙".yellow()
        );
        runner
            .ensure_tracking_table_exists(&*db)
            .await
            .map_err(CliError::Migration)?;
        println!("{} Fresh table created", "✓".green());
        println!();
    }

    // Mark migrations as completed
    match runner.mark_all_migrations_completed(&*db, scope).await {
        Ok(summary) => {
            println!();
            println!("{} Operation completed successfully!", "✓".green().bold());
            println!();
            println!("Summary:");
            println!("  Total migrations found:              {}", summary.total);
            println!(
                "  Already completed:                   {}",
                summary.already_completed
            );
            println!(
                "  Newly marked as completed:           {}",
                summary.newly_marked.to_string().green()
            );

            if summary.failed_marked > 0 {
                println!(
                    "  Failed → Completed:                  {}",
                    summary.failed_marked.to_string().yellow().bold()
                );
            }
            if summary.in_progress_marked > 0 {
                println!(
                    "  In-Progress → Completed:             {}",
                    summary.in_progress_marked.to_string().yellow().bold()
                );
            }
            if summary.failed_skipped > 0 {
                println!(
                    "  Failed migrations skipped:           {}",
                    summary.failed_skipped.to_string().blue()
                );
            }
            if summary.in_progress_skipped > 0 {
                println!(
                    "  In-Progress migrations skipped:      {}",
                    summary.in_progress_skipped.to_string().blue()
                );
            }
        }
        Err(e) => {
            println!("{} Failed to mark migrations as completed: {e}", "✗".red());
            return Err(CliError::Migration(e));
        }
    }

    Ok(())
}

/// Validate checksums of applied migrations
async fn validate_checksums(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
    strict: bool,
    verbose: bool,
) -> Result<()> {
    use colored::Colorize;
    use switchy_schema::runner::MigrationRunner;

    // Connect to database
    let db = utils::database::connect(&database_url).await?;

    // Create migration runner with directory source
    let runner =
        MigrationRunner::new_directory(&migrations_dir).with_table_name(migration_table.clone());

    println!(
        "\
        Validating migration checksums\n\
        ------------------------------\n\
        Migrations directory: {}\n\
        Migration table: {migration_table}\n",
        migrations_dir.display()
    );

    // Validate checksums
    let mismatches = runner.validate_checksums(&*db).await?;

    if mismatches.is_empty() {
        println!("{} All migration checksums are valid!", "✓".green());
        return Ok(());
    }

    // Display mismatches
    println!(
        "{} Found {} checksum mismatch(es):\n",
        "✗".red(),
        mismatches.len()
    );

    for mismatch in &mismatches {
        println!(
            "  {} Migration: {}\n\
            Checksum type: {} migration\n\
            {}\n",
            "•".yellow(),
            mismatch.migration_id.cyan(),
            match mismatch.checksum_type {
                switchy_schema::ChecksumType::Up => "UP".green(),
                switchy_schema::ChecksumType::Down => "DOWN".blue(),
            },
            if verbose {
                format!(
                    "    Stored:  {}\n    Current: {}",
                    mismatch.stored_checksum, mismatch.current_checksum,
                )
            } else {
                String::new()
            }
        );
    }

    println!(
        "{}\n\
        This could indicate:\n\
        - Accidental modification of migration files\n\
        - Different migration content between environments\n\
        - Potential database schema inconsistencies\n",
        "⚠️  WARNING: Migration files have been modified after being applied!"
            .yellow()
            .bold()
    );

    if strict {
        println!("{} Exiting with error due to --strict flag", "✗".red());
        return Err(CliError::Migration(
            switchy_schema::MigrationError::ChecksumValidationFailed { mismatches },
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use switchy_fs::TempDir;

    #[test_log::test]
    fn test_cli_parsing_create_command() {
        let cli = Cli::parse_from(["switchy-migrate", "create", "test_migration"]);

        match cli.command {
            Commands::Create {
                name,
                migrations_dir,
            } => {
                assert_eq!(name, "test_migration");
                assert_eq!(migrations_dir, PathBuf::from("./migrations"));
            }
            _ => panic!("Expected Create command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_rollback_command() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "rollback",
            "--database-url",
            "sqlite://test.db",
            "--steps",
            "3",
        ]);

        match cli.command {
            Commands::Rollback {
                database_url,
                migrations_dir,
                migration_table,
                to,
                steps,
                all,
                dry_run,
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert_eq!(migrations_dir, PathBuf::from("./migrations"));
                assert_eq!(migration_table, "__switchy_migrations");
                assert_eq!(to, None);
                assert_eq!(steps, Some(3));
                assert!(!all);
                assert!(!dry_run);
            }
            _ => panic!("Expected Rollback command"),
        }
    }

    #[test_log::test]
    fn test_cli_error_display() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let cli_error = CliError::Io(io_error);

        let error_string = format!("{cli_error}");
        assert!(error_string.contains("File not found"));
    }

    #[test_log::test]
    fn test_config_error() {
        let config_error = CliError::Config("Invalid configuration".to_string());
        let error_string = format!("{config_error}");
        assert_eq!(error_string, "Configuration error: Invalid configuration");
    }

    #[test_log::test]
    fn test_cli_parsing_rollback_with_dry_run() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "rollback",
            "--database-url",
            "sqlite://test.db",
            "--steps",
            "2",
            "--dry-run",
        ]);

        match cli.command {
            Commands::Rollback {
                database_url,
                migrations_dir,
                migration_table,
                to,
                steps,
                all,
                dry_run,
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert_eq!(migrations_dir, PathBuf::from("./migrations"));
                assert_eq!(migration_table, "__switchy_migrations");
                assert_eq!(to, None);
                assert_eq!(steps, Some(2));
                assert!(!all);
                assert!(dry_run);
            }
            _ => panic!("Expected Rollback command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_rollback_all_dry_run() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "rollback",
            "--database-url",
            "postgres://localhost/test",
            "--all",
            "--dry-run",
        ]);

        match cli.command {
            Commands::Rollback { all, dry_run, .. } => {
                assert!(all);
                assert!(dry_run);
            }
            _ => panic!("Expected Rollback command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_rollback_to_migration() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "rollback",
            "--database-url",
            "sqlite://test.db",
            "--to",
            "20231201000000_init",
        ]);

        match cli.command {
            Commands::Rollback { to, dry_run, .. } => {
                assert_eq!(to, Some("20231201000000_init".to_string()));
                assert!(!dry_run);
            }
            _ => panic!("Expected Rollback command"),
        }
    }

    #[test_log::test]
    fn test_create_migration_with_short_flag() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "create",
            "add_users_table",
            "-m",
            "/custom/path",
        ]);

        match cli.command {
            Commands::Create {
                name,
                migrations_dir,
            } => {
                assert_eq!(name, "add_users_table");
                assert_eq!(migrations_dir, PathBuf::from("/custom/path"));
            }
            _ => panic!("Expected Create command"),
        }
    }

    #[test_log::test]
    fn test_status_with_all_custom_options() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "status",
            "-d",
            "postgres://user:pass@localhost:5432/mydb",
            "-m",
            "/app/migrations",
            "--migration-table",
            "schema_versions",
            "--show-failed",
        ]);

        match cli.command {
            Commands::Status {
                database_url,
                migrations_dir,
                migration_table,
                show_failed,
            } => {
                assert_eq!(database_url, "postgres://user:pass@localhost:5432/mydb");
                assert_eq!(migrations_dir, PathBuf::from("/app/migrations"));
                assert_eq!(migration_table, "schema_versions");
                assert!(show_failed);
            }
            _ => panic!("Expected Status command"),
        }
    }

    #[test_log::test]
    fn test_create_migration_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let migrations_dir = temp_dir.path().to_path_buf();

        // Test creating a migration file
        let result = create_migration("test_migration", &migrations_dir);
        assert!(
            result.is_ok(),
            "Failed to create migration: {:?}",
            result.err()
        );

        // Check that migration directory was created
        let entries: Vec<_> = switchy_fs::sync::read_dir_sorted(&migrations_dir)
            .expect("Failed to read migrations directory")
            .into_iter()
            .collect();

        assert_eq!(entries.len(), 1, "Should create 1 migration directory");

        // Get the migration directory
        let migration_dir_entry = entries
            .into_iter()
            .next()
            .expect("Failed to read directory entry");
        let migration_dir_name = migration_dir_entry
            .file_name()
            .to_string_lossy()
            .to_string();

        assert!(
            migration_dir_name.contains("test_migration"),
            "Migration directory should contain the name"
        );

        // Check files exist
        let migrations_dir = migrations_dir.join(migration_dir_name);
        let up_sql = migrations_dir.join("up.sql");
        let down_sql = migrations_dir.join("down.sql");

        assert!(switchy_fs::exists(&up_sql), "up.sql should exist");
        assert!(switchy_fs::exists(&down_sql), "down.sql should exist");

        // Check file contents
        let up_content = switchy_fs::sync::read_to_string(&up_sql).expect("Failed to read up.sql");
        let down_content =
            switchy_fs::sync::read_to_string(&down_sql).expect("Failed to read down.sql");

        assert!(up_content.contains("-- Add your forward migration SQL here"));
        assert!(down_content.contains("-- Add your rollback migration SQL here"));
        assert!(up_content.contains("Migration: test_migration"));
        assert!(down_content.contains("Rollback: test_migration"));
    }

    #[test_log::test]
    fn test_cli_parsing_retry_command() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "retry",
            "--database-url",
            "sqlite://test.db",
            "001_create_users",
        ]);

        match cli.command {
            Commands::Retry {
                database_url,
                migrations_dir,
                migration_table,
                migration_id,
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert_eq!(migrations_dir, PathBuf::from("./migrations"));
                assert_eq!(migration_table, "__switchy_migrations");
                assert_eq!(migration_id, "001_create_users");
            }
            _ => panic!("Expected Retry command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_retry_with_custom_paths() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "retry",
            "--database-url",
            "postgres://localhost/test",
            "--migrations-dir",
            "/app/migrations",
            "--migration-table",
            "custom_migrations",
            "002_add_indexes",
        ]);

        match cli.command {
            Commands::Retry {
                database_url,
                migrations_dir,
                migration_table,
                migration_id,
            } => {
                assert_eq!(database_url, "postgres://localhost/test");
                assert_eq!(migrations_dir, PathBuf::from("/app/migrations"));
                assert_eq!(migration_table, "custom_migrations");
                assert_eq!(migration_id, "002_add_indexes");
            }
            _ => panic!("Expected Retry command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_mark_completed_command() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "mark-completed",
            "--database-url",
            "sqlite://test.db",
            "001_create_users",
        ]);

        match cli.command {
            Commands::MarkCompleted {
                database_url,
                migrations_dir,
                migration_table,
                migration_id,
                force,
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert_eq!(migrations_dir, PathBuf::from("./migrations"));
                assert_eq!(migration_table, "__switchy_migrations");
                assert_eq!(migration_id, "001_create_users");
                assert!(!force);
            }
            _ => panic!("Expected MarkCompleted command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_mark_completed_with_force() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "mark-completed",
            "--database-url",
            "postgres://localhost/test",
            "--force",
            "002_add_indexes",
        ]);

        match cli.command {
            Commands::MarkCompleted {
                database_url,
                migration_id,
                force,
                ..
            } => {
                assert_eq!(database_url, "postgres://localhost/test");
                assert_eq!(migration_id, "002_add_indexes");
                assert!(force);
            }
            _ => panic!("Expected MarkCompleted command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_migrate_with_force() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "migrate",
            "--database-url",
            "sqlite://test.db",
            "--force",
        ]);

        match cli.command {
            Commands::Migrate {
                database_url,
                force,
                ..
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert!(force);
            }
            _ => panic!("Expected Migrate command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_migrate_force_with_other_options() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "migrate",
            "--database-url",
            "postgres://localhost/test",
            "--force",
            "--dry-run",
            "--steps",
            "3",
        ]);

        match cli.command {
            Commands::Migrate {
                database_url,
                force,
                dry_run,
                steps,
                ..
            } => {
                assert_eq!(database_url, "postgres://localhost/test");
                assert!(force);
                assert!(dry_run);
                assert_eq!(steps, Some(3));
            }
            _ => panic!("Expected Migrate command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_status_show_failed() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "status",
            "--database-url",
            "sqlite://test.db",
            "--show-failed",
        ]);

        match cli.command {
            Commands::Status {
                database_url,
                show_failed,
                ..
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert!(show_failed);
            }
            _ => panic!("Expected Status command"),
        }
    }

    // CLI command execution tests
    #[switchy_async::test]
    async fn test_retry_command_error_handling() {
        // Test retry with invalid database URL should give clear error
        let result = retry_migration(
            "invalid://database/url".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            "001_test_retry".to_string(),
        )
        .await;

        assert!(result.is_err(), "Should fail with invalid database URL");

        // The error should be properly formatted
        match result {
            Err(CliError::Config(message)) => {
                assert!(message.contains("Unsupported database scheme: invalid"));
            }
            Err(other) => panic!("Unexpected error type: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[switchy_async::test]
    async fn test_mark_completed_error_handling() {
        // Test mark_completed with invalid database should fail gracefully
        let result = mark_migration_completed(
            "invalid://database/url".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            "001_test".to_string(),
            true, // force = true to skip confirmation
        )
        .await;

        assert!(result.is_err(), "Should fail with invalid database URL");

        // Should be a proper error type
        match result {
            Err(CliError::Config(message)) => {
                assert!(message.contains("Unsupported database scheme: invalid"));
            }
            Err(other) => panic!("Unexpected error type: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[switchy_async::test]
    async fn test_migrate_with_force_error_handling() {
        // Test that migrate with force flag handles errors properly
        let result = run_migrations(
            "invalid://database/url".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            None,
            None,
            false,
            true,  // force = true
            false, // require_checksum_validation = false
        )
        .await;

        // Should be a proper error type
        match result {
            Err(CliError::Config(message)) => {
                assert!(message.contains("Unsupported database scheme: invalid"));
            }
            Err(other) => panic!("Unexpected error type: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_validate_command() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "validate",
            "--database-url",
            "sqlite://test.db",
            "--migrations-dir",
            "/custom/migrations",
            "--migration-table",
            "custom_migrations",
        ]);

        match cli.command {
            Commands::Validate {
                database_url,
                migrations_dir,
                migration_table,
                strict,
                verbose,
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert_eq!(migrations_dir, PathBuf::from("/custom/migrations"));
                assert_eq!(migration_table, "custom_migrations");
                assert!(!strict);
                assert!(!verbose);
            }
            _ => panic!("Expected Validate command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_validate_with_flags() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "validate",
            "--database-url",
            "sqlite://test.db",
            "--strict",
            "--verbose",
        ]);

        match cli.command {
            Commands::Validate {
                database_url,
                migrations_dir,
                migration_table,
                strict,
                verbose,
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert_eq!(migrations_dir, PathBuf::from("./migrations")); // default
                assert_eq!(migration_table, "__switchy_migrations"); // default
                assert!(strict);
                assert!(verbose);
            }
            _ => panic!("Expected Validate command"),
        }
    }

    #[test_log::test]
    fn test_validate_command_default_values() {
        let cli = Cli::parse_from(["switchy-migrate", "validate", "-d", "sqlite://memory"]);

        match cli.command {
            Commands::Validate {
                database_url,
                migrations_dir,
                migration_table,
                strict,
                verbose,
            } => {
                assert_eq!(database_url, "sqlite://memory");
                assert_eq!(migrations_dir, PathBuf::from("./migrations"));
                assert_eq!(migration_table, "__switchy_migrations");
                assert!(!strict);
                assert!(!verbose);
            }
            _ => panic!("Expected Validate command"),
        }
    }

    #[test_log::test]
    fn test_create_migration_with_empty_name() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let migrations_dir = temp_dir.path().to_path_buf();

        // Test creating a migration with empty name
        let result = create_migration("", &migrations_dir);

        assert!(result.is_err(), "Should fail with empty migration name");
        match result {
            Err(CliError::Config(message)) => {
                assert_eq!(message, "Migration name cannot be empty");
            }
            _ => panic!("Expected Config error for empty name"),
        }
    }

    #[test_log::test]
    fn test_create_migration_creates_directory_structure() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let migrations_dir = temp_dir.path().to_path_buf();

        // Create a migration
        let result = create_migration("add_users_table", &migrations_dir);
        assert!(result.is_ok(), "Failed to create migration");

        // Verify the migrations directory was created
        assert!(
            switchy_fs::exists(&migrations_dir),
            "Migrations directory should exist"
        );

        // Find the created migration directory
        let entries: Vec<_> = switchy_fs::sync::read_dir_sorted(&migrations_dir)
            .expect("Failed to read migrations directory")
            .into_iter()
            .collect();

        assert_eq!(entries.len(), 1, "Should create exactly one migration");

        let created_migration_dir = entries[0].path();

        // Verify both SQL files exist in the migration directory
        let up_sql = created_migration_dir.join("up.sql");
        let down_sql = created_migration_dir.join("down.sql");
        assert!(switchy_fs::exists(&up_sql), "up.sql should exist");
        assert!(switchy_fs::exists(&down_sql), "down.sql should exist");
    }

    #[test_log::test]
    fn test_create_migration_with_special_characters_in_name() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let migrations_dir = temp_dir.path().to_path_buf();

        // Test with various special characters that should be valid in filenames
        let names = vec![
            "add_users-table",
            "update_schema_v2",
            "fix-bug-123",
            "add_column_with_underscores",
        ];

        for name in names {
            let result = create_migration(name, &migrations_dir);
            assert!(result.is_ok(), "Should create migration with name '{name}'");
        }

        // Verify all migrations were created
        assert_eq!(
            switchy_fs::sync::read_dir_sorted(&migrations_dir)
                .expect("Failed to read migrations directory")
                .len(),
            4,
            "Should create migrations for all valid names"
        );
    }

    #[test_log::test]
    fn test_create_migration_file_content() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let migrations_dir = temp_dir.path().to_path_buf();

        let migration_name = "add_products_table";
        let result = create_migration(migration_name, &migrations_dir);
        assert!(result.is_ok(), "Failed to create migration");

        // Find the migration directory
        let entries: Vec<_> = switchy_fs::sync::read_dir_sorted(&migrations_dir)
            .expect("Failed to read migrations directory")
            .into_iter()
            .collect();

        let migration_path = entries[0].path();

        // Read and verify up.sql content
        let up_content = switchy_fs::sync::read_to_string(migration_path.join("up.sql"))
            .expect("Failed to read up.sql");
        assert!(
            up_content.contains(&format!("-- Migration: {migration_name}")),
            "up.sql should contain migration name in header"
        );
        assert!(
            up_content.contains("-- Add your forward migration SQL here"),
            "up.sql should contain instruction comment"
        );
        assert!(
            up_content.contains("-- This file will be executed when running migrations"),
            "up.sql should contain usage description"
        );

        // Read and verify down.sql content
        let down_content = switchy_fs::sync::read_to_string(migration_path.join("down.sql"))
            .expect("Failed to read down.sql");
        assert!(
            down_content.contains(&format!("-- Rollback: {migration_name}")),
            "down.sql should contain migration name in header"
        );
        assert!(
            down_content.contains("-- Add your rollback migration SQL here"),
            "down.sql should contain instruction comment"
        );
        assert!(
            down_content.contains("-- Should reverse the changes made in up.sql"),
            "down.sql should contain reversal instruction"
        );
    }

    #[test_log::test]
    fn test_cli_parsing_mark_all_completed_pending_only() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "mark-all-completed",
            "--database-url",
            "sqlite://test.db",
        ]);

        match cli.command {
            Commands::MarkAllCompleted {
                database_url,
                migrations_dir,
                migration_table,
                include_failed,
                include_in_progress,
                all,
                drop,
                force,
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert_eq!(migrations_dir, PathBuf::from("./migrations"));
                assert_eq!(migration_table, "__switchy_migrations");
                assert!(!include_failed);
                assert!(!include_in_progress);
                assert!(!all);
                assert!(!drop);
                assert!(!force);
            }
            _ => panic!("Expected MarkAllCompleted command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_mark_all_completed_with_all_flags() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "mark-all-completed",
            "--database-url",
            "postgres://localhost/test",
            "--include-failed",
            "--include-in-progress",
            "--all",
            "--drop",
            "--force",
        ]);

        match cli.command {
            Commands::MarkAllCompleted {
                database_url,
                include_failed,
                include_in_progress,
                all,
                drop,
                force,
                ..
            } => {
                assert_eq!(database_url, "postgres://localhost/test");
                assert!(include_failed);
                assert!(include_in_progress);
                assert!(all);
                assert!(drop);
                assert!(force);
            }
            _ => panic!("Expected MarkAllCompleted command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_migrate_with_require_checksum_validation() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "migrate",
            "--database-url",
            "sqlite://test.db",
            "--require-checksum-validation",
        ]);

        match cli.command {
            Commands::Migrate {
                database_url,
                require_checksum_validation,
                ..
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert!(require_checksum_validation);
            }
            _ => panic!("Expected Migrate command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_migrate_with_up_to() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "migrate",
            "--database-url",
            "postgres://localhost/test",
            "--up-to",
            "20231201000000_target_migration",
        ]);

        match cli.command {
            Commands::Migrate {
                database_url,
                up_to,
                steps,
                ..
            } => {
                assert_eq!(database_url, "postgres://localhost/test");
                assert_eq!(up_to, Some("20231201000000_target_migration".to_string()));
                assert_eq!(steps, None);
            }
            _ => panic!("Expected Migrate command"),
        }
    }

    #[test_log::test]
    fn test_cli_parsing_migrate_with_steps() {
        let cli = Cli::parse_from([
            "switchy-migrate",
            "migrate",
            "--database-url",
            "sqlite://test.db",
            "--steps",
            "5",
        ]);

        match cli.command {
            Commands::Migrate {
                database_url,
                up_to,
                steps,
                ..
            } => {
                assert_eq!(database_url, "sqlite://test.db");
                assert_eq!(up_to, None);
                assert_eq!(steps, Some(5));
            }
            _ => panic!("Expected Migrate command"),
        }
    }

    #[test_log::test]
    fn test_cli_error_from_migration_error() {
        let migration_error =
            switchy_schema::MigrationError::Discovery("test_migration not found".to_string());
        let cli_error = CliError::Migration(migration_error);

        let error_string = format!("{cli_error}");
        assert!(error_string.contains("test_migration"));
    }

    #[test_log::test]
    fn test_cli_error_from_io_error() {
        let io_error = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Access denied to migration file",
        );
        let cli_error = CliError::Io(io_error);

        let error_string = format!("{cli_error}");
        assert!(error_string.contains("Access denied") || error_string.contains("migration"));
    }

    #[test_log::test]
    fn test_cli_error_from_validation_error() {
        let migration_error =
            switchy_schema::MigrationError::Validation("Invalid migration format".to_string());
        let cli_error = CliError::Migration(migration_error);

        let error_string = format!("{cli_error}");
        assert!(error_string.contains("validation") || error_string.contains("Invalid"));
    }

    // Tests for internal validation logic
    #[switchy_async::test]
    async fn test_run_migrations_conflicting_strategies() {
        // Test that specifying both up_to and steps is rejected
        let result = run_migrations(
            "sqlite://:memory:".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            Some("target_migration".to_string()), // up_to
            Some(5),                              // steps - conflicting with up_to
            false,                                // dry_run
            false,                                // force
            false,                                // require_checksum_validation
        )
        .await;

        assert!(result.is_err(), "Should fail with conflicting strategies");
        match result {
            Err(CliError::Config(message)) => {
                assert!(
                    message.contains("Cannot specify both --up-to and --steps"),
                    "Error message should indicate conflicting options: {message}"
                );
            }
            Err(other) => panic!("Expected Config error, got: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[switchy_async::test]
    async fn test_rollback_migrations_conflicting_strategies() {
        // Test that specifying multiple rollback strategies is rejected
        let result = rollback_migrations(
            "sqlite://:memory:".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            Some("target_migration".to_string()), // to
            Some(3),                              // steps - conflicting with to
            false,                                // all
            true,                                 // dry_run
        )
        .await;

        assert!(
            result.is_err(),
            "Should fail with conflicting rollback strategies"
        );
        match result {
            Err(CliError::Config(message)) => {
                assert!(
                    message.contains("Cannot specify multiple rollback strategies"),
                    "Error message should indicate conflicting strategies: {message}"
                );
            }
            Err(other) => panic!("Expected Config error, got: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[switchy_async::test]
    async fn test_rollback_migrations_all_with_steps_conflict() {
        // Test that --all and --steps conflict
        let result = rollback_migrations(
            "sqlite://:memory:".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            None,    // to
            Some(2), // steps
            true,    // all - conflicting with steps
            true,    // dry_run
        )
        .await;

        assert!(
            result.is_err(),
            "Should fail when --all and --steps are both specified"
        );
        match result {
            Err(CliError::Config(message)) => {
                assert!(
                    message.contains("Cannot specify multiple rollback strategies"),
                    "Error message should indicate conflicting strategies: {message}"
                );
            }
            Err(other) => panic!("Expected Config error, got: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[switchy_async::test]
    async fn test_rollback_migrations_all_with_to_conflict() {
        // Test that --all and --to conflict
        let result = rollback_migrations(
            "sqlite://:memory:".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            Some("target_migration".to_string()), // to - conflicting with all
            None,                                 // steps
            true,                                 // all
            true,                                 // dry_run
        )
        .await;

        assert!(
            result.is_err(),
            "Should fail when --all and --to are both specified"
        );
        match result {
            Err(CliError::Config(message)) => {
                assert!(
                    message.contains("Cannot specify multiple rollback strategies"),
                    "Error message should indicate conflicting strategies: {message}"
                );
            }
            Err(other) => panic!("Expected Config error, got: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[switchy_async::test]
    async fn test_validate_checksums_error_handling() {
        // Test that validate_checksums properly handles invalid database
        let result = validate_checksums(
            "invalid://database/url".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            false, // strict
            false, // verbose
        )
        .await;

        assert!(result.is_err(), "Should fail with invalid database URL");
        match result {
            Err(CliError::Config(message)) => {
                assert!(
                    message.contains("Unsupported database scheme: invalid"),
                    "Error message should indicate unsupported scheme: {message}"
                );
            }
            Err(other) => panic!("Expected Config error, got: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[switchy_async::test]
    async fn test_show_status_error_handling() {
        // Test that show_status properly handles invalid database
        let result = show_status(
            "invalid://database/url".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            false, // show_failed
        )
        .await;

        assert!(result.is_err(), "Should fail with invalid database URL");
        match result {
            Err(CliError::Config(message)) => {
                assert!(
                    message.contains("Unsupported database scheme: invalid"),
                    "Error message should indicate unsupported scheme: {message}"
                );
            }
            Err(other) => panic!("Expected Config error, got: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[switchy_async::test]
    async fn test_mark_all_migrations_completed_error_handling() {
        // Test that mark_all_migrations_completed properly handles invalid database
        let result = mark_all_migrations_completed(
            "invalid://database/url".to_string(),
            PathBuf::from("./migrations"),
            "__switchy_migrations".to_string(),
            false, // include_failed
            false, // include_in_progress
            false, // all
            false, // drop
            true,  // force
        )
        .await;

        assert!(result.is_err(), "Should fail with invalid database URL");
        match result {
            Err(CliError::Config(message)) => {
                assert!(
                    message.contains("Unsupported database scheme: invalid"),
                    "Error message should indicate unsupported scheme: {message}"
                );
            }
            Err(other) => panic!("Expected Config error, got: {other:?}"),
            Ok(()) => panic!("Expected error but got success"),
        }
    }

    #[test_log::test]
    fn test_create_migration_in_nested_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nested_migrations_dir = temp_dir.path().join("deeply").join("nested").join("path");

        // Create a migration in a deeply nested directory that doesn't exist yet
        let result = create_migration("nested_test", &nested_migrations_dir);
        assert!(
            result.is_ok(),
            "Should create nested directory structure: {:?}",
            result.err()
        );

        // Verify the entire path was created
        assert!(
            switchy_fs::exists(&nested_migrations_dir),
            "Nested migrations directory should be created"
        );

        // Verify migration files exist
        let entries: Vec<_> = switchy_fs::sync::read_dir_sorted(&nested_migrations_dir)
            .expect("Failed to read migrations directory")
            .into_iter()
            .collect();

        assert_eq!(entries.len(), 1, "Should create exactly one migration");

        let migration_dir = entries[0].path();
        assert!(
            switchy_fs::exists(migration_dir.join("up.sql")),
            "up.sql should exist in nested location"
        );
        assert!(
            switchy_fs::exists(migration_dir.join("down.sql")),
            "down.sql should exist in nested location"
        );
    }

    #[test_log::test]
    fn test_create_migration_timestamp_format() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let migrations_dir = temp_dir.path().to_path_buf();

        // Create a migration
        let result = create_migration("timestamp_test", &migrations_dir);
        assert!(result.is_ok(), "Failed to create migration");

        // Get the created migration directory name
        let entries: Vec<_> = switchy_fs::sync::read_dir_sorted(&migrations_dir)
            .expect("Failed to read migrations directory")
            .into_iter()
            .collect();

        let migration_name = entries[0].file_name().to_string_lossy().to_string();

        // Verify timestamp format: YYYY-MM-DD-HHMMSS_name
        // The format should be: 2024-01-15-143022_timestamp_test
        let parts: Vec<&str> = migration_name.splitn(2, '_').collect();
        assert_eq!(
            parts.len(),
            2,
            "Migration name should have timestamp_name format"
        );

        let timestamp_part = parts[0];
        let name_part = parts[1];

        // Verify timestamp has correct structure (YYYY-MM-DD-HHMMSS = 17 chars)
        assert_eq!(
            timestamp_part.len(),
            17,
            "Timestamp should be 17 characters (YYYY-MM-DD-HHMMSS)"
        );

        // Verify the name part matches what we provided
        assert_eq!(
            name_part, "timestamp_test",
            "Name part should match the provided name"
        );

        // Verify timestamp contains dashes in correct positions
        assert_eq!(
            timestamp_part.chars().nth(4),
            Some('-'),
            "Should have dash after year"
        );
        assert_eq!(
            timestamp_part.chars().nth(7),
            Some('-'),
            "Should have dash after month"
        );
        assert_eq!(
            timestamp_part.chars().nth(10),
            Some('-'),
            "Should have dash after day"
        );
    }
}
