#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use clap::{Parser, Subcommand};
use std::{io::Write as _, path::PathBuf};
use thiserror::Error;

mod utils;

/// Error types for the CLI
#[derive(Debug, Error)]
pub enum CliError {
    /// Migration error
    #[error(transparent)]
    Migration(#[from] switchy_schema::MigrationError),
    /// Database connection error
    #[error(transparent)]
    Database(#[from] switchy_database::DatabaseError),
    /// IO error
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Configuration error
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
}

#[tokio::main]
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
        } => show_status(database_url, migrations_dir, migration_table).await,
        Commands::Migrate {
            database_url,
            migrations_dir,
            migration_table,
            up_to,
            steps,
            dry_run,
        } => {
            run_migrations(
                database_url,
                migrations_dir,
                migration_table,
                up_to,
                steps,
                dry_run,
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
async fn show_status(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
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

    for migration in &migrations {
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

    println!();
    println!("Summary:");
    println!("  Applied: {applied_count}");
    println!("  Pending: {pending_count}");
    println!("  Total:   {}", migrations.len());

    if pending_count > 0 {
        println!();
        println!("Run 'switchy-migrate migrate' to apply pending migrations.");
    }

    Ok(())
}

/// Run pending migrations
async fn run_migrations(
    database_url: String,
    migrations_dir: PathBuf,
    migration_table: String,
    up_to: Option<String>,
    steps: Option<usize>,
    dry_run: bool,
) -> Result<()> {
    use switchy_schema::runner::{ExecutionStrategy, MigrationRunner};

    // Validate arguments
    if up_to.is_some() && steps.is_some() {
        return Err(CliError::Config(
            "Cannot specify both --up-to and --steps".to_string(),
        ));
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

    // Create migration runner with directory source
    let mut runner = MigrationRunner::new_directory(&migrations_dir)
        .with_table_name(migration_table.clone())
        .with_strategy(strategy);

    if dry_run {
        runner = runner.dry_run();
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

    if pending_before.is_empty() {
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
        println!("Applying {} migration(s):", pending_before.len());

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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use switchy_fs::TempDir;

    #[test]
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

    #[test]
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

    #[test]
    fn test_cli_error_display() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let cli_error = CliError::Io(io_error);

        let error_string = format!("{cli_error}");
        assert!(error_string.contains("File not found"));
    }

    #[test]
    fn test_config_error() {
        let config_error = CliError::Config("Invalid configuration".to_string());
        let error_string = format!("{config_error}");
        assert_eq!(error_string, "Configuration error: Invalid configuration");
    }

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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
        ]);

        match cli.command {
            Commands::Status {
                database_url,
                migrations_dir,
                migration_table,
            } => {
                assert_eq!(database_url, "postgres://user:pass@localhost:5432/mydb");
                assert_eq!(migrations_dir, PathBuf::from("/app/migrations"));
                assert_eq!(migration_table, "schema_versions");
            }
            _ => panic!("Expected Status command"),
        }
    }

    #[test]
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

        // Check directory name contains migration name
        assert!(migration_dir_name.contains("test_migration"));

        // Check directory name starts with timestamp (date format YYYY-MM-DD-HHMMSS)
        assert!(
            migration_dir_name.starts_with("2025-"),
            "Migration directory should start with year: {migration_dir_name}"
        );

        // Check that up.sql and down.sql files exist inside the migration directory
        let migration_dir_path = migration_dir_entry.path();
        let up_sql = migration_dir_path.join("up.sql");
        let down_sql = migration_dir_path.join("down.sql");

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
}
