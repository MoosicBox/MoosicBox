# switchy-migrate

A command-line interface for managing database schema migrations using the `switchy_schema` library.

## Installation

Build from source:

```bash
cargo build -p switchy_schema_cli
```

The binary will be available at `target/debug/switchy-migrate` (or `target/release/switchy-migrate` for release builds).

## Usage

### Create a new migration

```bash
switchy-migrate create create_users_table
```

This creates a new migration with timestamped directory and up/down SQL files.

### Check migration status

```bash
switchy-migrate status --database-url sqlite:./database.db
```

Shows which migrations have been applied and which are pending.

### Run pending migrations

```bash
switchy-migrate migrate --database-url sqlite:./database.db
```

Applies all pending migrations to the database.

### Rollback migrations

```bash
# Rollback the most recent migration
switchy-migrate rollback --database-url sqlite:./database.db

# Rollback multiple migrations
switchy-migrate rollback --database-url sqlite:./database.db --steps 3

# Rollback to a specific migration (exclusive)
switchy-migrate rollback --database-url sqlite:./database.db --to 2025-01-01-120000_initial_schema
```

### Mark migrations as completed (dangerous operations)

```bash
# Mark a single migration as completed
switchy-migrate mark-completed 2025-09-01-151110_create_users_table --database-url sqlite:./app.db

# Mark untracked migrations as completed (default, safest)
switchy-migrate mark-all-completed --database-url sqlite:./app.db

# Also mark failed migrations as completed
switchy-migrate mark-all-completed --include-failed --database-url sqlite:./app.db

# Also mark in-progress migrations as completed
switchy-migrate mark-all-completed --include-in-progress --database-url sqlite:./app.db

# Mark ALL migrations as completed (VERY dangerous)
switchy-migrate mark-all-completed --all --database-url sqlite:./app.db
```

**⚠️ WARNING:** These operations bypass migration execution and can cause:
- Database schema inconsistencies
- Failed future migrations
- Data corruption

**Default behavior** (`mark-all-completed` without flags):
- ✅ Safe: Only marks untracked migrations as completed
- ⏭️ Preserves: Failed and in-progress migration states
- 💡 Use for: Initializing tracking for existing databases

**With flags** (`--include-failed`, `--include-in-progress`, `--all`):
- ⚠️ Dangerous: Changes migration states
- 🔄 Updates: Failed/in-progress migrations to completed
- 💡 Use for: Recovery scenarios only

All commands require interactive confirmation unless `--force` is used. Dangerous scopes require double confirmation.

## Supported Databases

- **SQLite**: `sqlite:./database.db` or `sqlite:` for in-memory
- **PostgreSQL**: `postgresql://user:password@localhost:5432/dbname`

## Environment Variables

- `SWITCHY_DATABASE_URL` - Database connection URL
- `SWITCHY_MIGRATIONS_DIR` - Directory containing migrations (default: `./migrations`)
- `SWITCHY_MIGRATION_TABLE` - Migration tracking table name (default: `__switchy_migrations`)

## Migration File Structure

Migrations are organized in directories:

```
migrations/
├── 2025-09-01-151110_create_users_table/
│   ├── up.sql    # Forward migration
│   └── down.sql  # Rollback migration (optional)
└── 2025-09-01-151120_add_user_email/
    ├── up.sql
    └── down.sql
```

## Examples

### Complete workflow

```bash
# Create a new migration
switchy-migrate create create_users_table

# Edit the generated SQL files
# migrations/2025-09-01-151110_create_users_table/up.sql
# migrations/2025-09-01-151110_create_users_table/down.sql

# Check status
switchy-migrate status --database-url sqlite:./app.db

# Apply migrations
switchy-migrate migrate --database-url sqlite:./app.db

# Check status again
switchy-migrate status --database-url sqlite:./app.db
```

### Dry run

```bash
# See what would be migrated without applying
switchy-migrate migrate --database-url sqlite:./app.db --dry-run
```

### Partial migrations

```bash
# Apply only the next 2 migrations
switchy-migrate migrate --database-url sqlite:./app.db --steps 2

# Apply migrations up to a specific one
switchy-migrate migrate --database-url sqlite:./app.db --up-to 2025-09-01-151120_add_user_email
```

### Marking Migrations with Different Scopes

```bash
# Scenario 1: Initialize tracking for existing database
# Safe - only marks new migrations
switchy-migrate mark-all-completed --database-url sqlite:./app.db

# Scenario 2: Multiple migrations failed, you fixed them manually
# Marks failed migrations as completed
switchy-migrate mark-all-completed --include-failed --database-url sqlite:./app.db

# Scenario 3: Migration process crashed, but migrations actually completed
# Marks in-progress migrations as completed
switchy-migrate mark-all-completed --include-in-progress --database-url sqlite:./app.db

# Scenario 4: Complete reset of tracking table
# Marks everything as completed (most dangerous)
switchy-migrate mark-all-completed --all --force --database-url sqlite:./app.db
```

## Safety Features

- Rollback operations require user confirmation
- Mark-completed operations have progressive confirmation levels:
  - Default scope (pending only): Single confirmation
  - Dangerous scopes (include-failed/in-progress): Double confirmation
  - All scope: Double confirmation with extreme warnings
- Danger-level-aware warnings adapt to selected scope
- Database connections are validated before operations
- Migration ordering is deterministic (alphabetical by ID)
- Comprehensive error reporting with detailed summaries
- Support for dry-run operations
- Failed and in-progress states preserved by default
