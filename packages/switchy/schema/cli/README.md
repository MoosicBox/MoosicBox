# switchy_schema_cli

Command-line interface for managing database schema migrations with support for PostgreSQL and SQLite databases.

## Installation

```bash
cargo install switchy_schema_cli
```

The binary is installed as `switchy-migrate`.

## Supported Databases

- **SQLite**: `sqlite://path/to/db.sqlite` or `sqlite://:memory:`
- **PostgreSQL**: `postgresql://user:pass@host:port/database` or `postgres://user:pass@host:port/database`

## Environment Variables

- `SWITCHY_DATABASE_URL`: Database connection URL
- `SWITCHY_MIGRATIONS_DIR`: Directory containing migration files (default: `./migrations`)
- `SWITCHY_MIGRATION_TABLE`: Name of migration tracking table (default: `__switchy_migrations`)

## Commands

### create

Create a new migration file with timestamped directory containing `up.sql` and `down.sql` files.

```bash
switchy-migrate create <name>
switchy-migrate create add_users_table -m /custom/migrations
```

**Arguments:**
- `<name>`: Name for the migration

**Options:**
- `-m, --migrations-dir <PATH>`: Directory containing migrations (default: `./migrations`)

### status

Show migration status and pending migrations.

```bash
switchy-migrate status -d <database-url>
switchy-migrate status -d sqlite://db.sqlite --show-failed
```

**Options:**
- `-d, --database-url <URL>`: Database connection URL (required)
- `-m, --migrations-dir <PATH>`: Directory containing migrations (default: `./migrations`)
- `--migration-table <NAME>`: Migration table name (default: `__switchy_migrations`)
- `--show-failed`: Show detailed status including failed and in-progress migrations

### migrate

Run pending migrations.

```bash
switchy-migrate migrate -d <database-url>
switchy-migrate migrate -d sqlite://db.sqlite --dry-run
switchy-migrate migrate -d postgres://localhost/mydb --up-to 20231201000000_init
switchy-migrate migrate -d sqlite://db.sqlite --steps 3
```

**Options:**
- `-d, --database-url <URL>`: Database connection URL (required)
- `-m, --migrations-dir <PATH>`: Directory containing migrations (default: `./migrations`)
- `--migration-table <NAME>`: Migration table name (default: `__switchy_migrations`)
- `--up-to <ID>`: Run migrations up to this specific migration ID
- `--steps <N>`: Run only this many migrations
- `--dry-run`: Show what would be done without executing
- `--force`: Force migration even if dirty state detected (dangerous)
- `--require-checksum-validation`: Require checksum validation before running migrations

### rollback

Rollback migrations.

```bash
switchy-migrate rollback -d <database-url>
switchy-migrate rollback -d sqlite://db.sqlite --steps 2
switchy-migrate rollback -d postgres://localhost/mydb --to 20231201000000_init
switchy-migrate rollback -d sqlite://db.sqlite --all --dry-run
```

**Options:**
- `-d, --database-url <URL>`: Database connection URL (required)
- `-m, --migrations-dir <PATH>`: Directory containing migrations (default: `./migrations`)
- `--migration-table <NAME>`: Migration table name (default: `__switchy_migrations`)
- `--to <ID>`: Rollback to this migration ID (not including it)
- `--steps <N>`: Number of migrations to rollback (default: 1)
- `--all`: Rollback all migrations
- `--dry-run`: Show what would be done without executing

### retry

Retry a failed migration.

```bash
switchy-migrate retry -d <database-url> <migration-id>
switchy-migrate retry -d sqlite://db.sqlite 20231201000000_create_users
```

**Arguments:**
- `<migration-id>`: Migration ID to retry

**Options:**
- `-d, --database-url <URL>`: Database connection URL (required)
- `-m, --migrations-dir <PATH>`: Directory containing migrations (default: `./migrations`)
- `--migration-table <NAME>`: Migration table name (default: `__switchy_migrations`)

### mark-completed

Mark a migration as completed without executing it (dangerous operation).

```bash
switchy-migrate mark-completed -d <database-url> <migration-id>
switchy-migrate mark-completed -d sqlite://db.sqlite 20231201000000_init --force
```

**Arguments:**
- `<migration-id>`: Migration ID to mark as completed

**Options:**
- `-d, --database-url <URL>`: Database connection URL (required)
- `-m, --migrations-dir <PATH>`: Directory containing migrations (default: `./migrations`)
- `--migration-table <NAME>`: Migration table name (default: `__switchy_migrations`)
- `--force`: Force the operation without confirmation

### mark-all-completed

Mark all migrations as completed without executing them (very dangerous operation).

```bash
switchy-migrate mark-all-completed -d <database-url>
switchy-migrate mark-all-completed -d sqlite://db.sqlite --include-failed --force
switchy-migrate mark-all-completed -d postgres://localhost/mydb --drop --force
```

**Options:**
- `-d, --database-url <URL>`: Database connection URL (required)
- `-m, --migrations-dir <PATH>`: Directory containing migrations (default: `./migrations`)
- `--migration-table <NAME>`: Migration table name (default: `__switchy_migrations`)
- `--include-failed`: Also mark failed migrations as completed
- `--include-in-progress`: Also mark in-progress migrations as completed
- `--all`: Mark ALL migrations regardless of state (implies `--include-failed` and `--include-in-progress`)
- `--drop`: Drop and recreate the migration tracking table before marking (critical - deletes all migration history)
- `--force`: Force the operation without confirmation

### validate

Validate checksums of applied migrations.

```bash
switchy-migrate validate -d <database-url>
switchy-migrate validate -d sqlite://db.sqlite --strict --verbose
```

**Options:**
- `-d, --database-url <URL>`: Database connection URL (required)
- `-m, --migrations-dir <PATH>`: Directory containing migrations (default: `./migrations`)
- `--migration-table <NAME>`: Migration table name (default: `__switchy_migrations`)
- `--strict`: Exit with error code if mismatches found
- `--verbose`: Show detailed checksum values

## Migration File Structure

Migrations are organized in timestamped directories:

```
migrations/
  2024-01-15-120000_create_users/
    up.sql
    down.sql
  2024-01-16-093000_add_posts/
    up.sql
    down.sql
```

The `create` command automatically generates this structure with template files.

## License

MPL-2.0
